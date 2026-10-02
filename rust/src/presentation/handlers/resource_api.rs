/// presentation/handlers/resource_api.rs — JSON リソース API
///
/// Djangoの /api/v1/projects/, /api/v1/categories/, /api/v1/milestones/, /api/v1/labels/
/// と挙動を一致させるハンドラー。
/// Phase 3: リソース CRUD API。
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::access::{sees_team, Action, ResourceRef, Viewer};
use crate::domain::models::holiday::Holiday;
use crate::domain::models::resource_api::*;
use crate::infrastructure::access::{
    facts_repo,
    shadow::{self, Mode, Resource},
};
use crate::infrastructure::repositories::holiday_repo;
use crate::infrastructure::repositories::project_activity_repo;
use crate::infrastructure::repositories::project_hierarchy_repo;
use crate::infrastructure::repositories::project_team_repo;
use crate::infrastructure::repositories::resource_repo;
use crate::infrastructure::repositories::team_repo;
use crate::infrastructure::repositories::ticket_repo;
use crate::infrastructure::repositories::user_repo;
use crate::presentation::extractors::authorize;
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::presentation::state::AppState;
use serde_json::json;

// =============================================================================
// 権限確認
// =============================================================================

const NO_PROJECT_EDIT_PERMISSION: &str = "このプロジェクトを変更する権限がありません";

/// PUT / PATCH の権限確認。ハンドラの最初に呼ぶ。権限がなければ返すべき応答を Err で返す。
pub(crate) async fn authorize_project_edit(
    pool: &sqlx::PgPool,
    user_id: i32,
    project_id: i32,
) -> Result<(), axum::response::Response> {
    let caller = match user_repo::find_by_id(pool, user_id).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    detail: "ユーザーが見つかりません".to_string(),
                }),
            )
                .into_response());
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response());
        }
    };
    match project_team_repo::can_edit(pool, project_id, user_id, caller.is_staff).await {
        Ok(true) => Ok(()),
        Ok(false) => Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                detail: NO_PROJECT_EDIT_PERMISSION.to_string(),
            }),
        )
            .into_response()),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response())
        }
    }
}

// =============================================================================
// リクエスト構造体
// =============================================================================

#[derive(Deserialize)]
pub struct ProjectListQuery {
    pub page: Option<i64>,
    #[serde(rename = "parentProjectId")]
    pub parent_project_id: Option<String>,
    #[serde(rename = "roadmapId")]
    pub roadmap_id: Option<i32>,
    #[serde(rename = "relatedTo")]
    pub related_to: Option<i32>,
}

#[derive(Deserialize)]
pub struct MilestoneListQuery {
    pub project: Option<i32>,
    pub page: Option<i64>,
}

#[derive(Deserialize)]
pub struct LabelListQuery {
    pub project: Option<i32>,
    pub team: Option<i32>,
}

// =============================================================================
// レスポンス構造体
// =============================================================================

#[derive(Serialize)]
pub struct PaginatedProjectsOut {
    pub count: i64,
    pub next: Option<String>,
    pub previous: Option<String>,
    pub results: Vec<ProjectOut>,
}

#[derive(Serialize)]
pub struct PaginatedMilestonesOut {
    pub count: i64,
    pub next: Option<String>,
    pub previous: Option<String>,
    pub results: Vec<MilestoneOut>,
}

/// DRFの本物のカーソルトークンとは異なる簡略実装
/// フロントエンドは `.results` のみ実際に参照していることを確認済み
#[derive(Serialize)]
pub struct PaginatedLabelsOut {
    pub next: Option<String>,
    pub previous: Option<String>,
    pub results: Vec<LabelOut>,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub detail: String,
}

/// 新しい判定(`on`)のとき、プロジェクトの参加チームから、閲覧者に見えないチームを除く(D-3)
fn hide_invisible_teams(viewer: &Viewer, project: &mut ProjectOut) {
    if shadow::mode(Resource::Project) == Mode::On {
        let before = project.teams.len();
        project
            .teams
            .retain(|t| sees_team(viewer, &viewer.team_facts_for_read(t.id)));
        project.hidden_team_count = (before - project.teams.len()) as i64;
    }
}

// =============================================================================
// Projects
// =============================================================================

/// プロジェクト一覧 GET /api/v1/projects/
#[utoipa::path(
    get,
    path = "/api/v1/projects/",
    tag = "projects",
    responses(
        (status = 200, description = "プロジェクト一覧を返す")
    )
)]
pub async fn project_list(
    State(state): State<AppState>,
    viewer: Viewer,
    Query(params): Query<ProjectListQuery>,
) -> impl IntoResponse {
    let user_id = match viewer.require_user_id() {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    // 新しい判定(`on`)では、SQL で見えるプロジェクトだけを読む(件数・ページ送りも合わせる)
    let scope = (shadow::mode(Resource::Project) == Mode::On).then(|| viewer.scope());
    let page = params.page.unwrap_or(1).max(1);
    const PAGE_SIZE: i64 = 50;

    // パラメータをバリデーション
    let parent_project_id = match &params.parent_project_id {
        None => Ok(None),
        Some(s) if s == "none" => Ok(Some(None)), // ルートのみ
        Some(s) => s
            .parse::<i32>()
            .map(|id| Some(Some(id)))
            .map_err(|_| StatusCode::BAD_REQUEST),
    };

    let parent_project_id = match parent_project_id {
        Ok(v) => v,
        Err(status) => {
            return (
                status,
                Json(ErrorResponse {
                    detail: "Invalid parentProjectId parameter".to_string(),
                }),
            )
                .into_response();
        }
    };

    let filter = crate::domain::models::resource_api::ProjectListFilter {
        parent_project_id: params.parent_project_id,
        roadmap_id: params.roadmap_id,
        related_to: params.related_to,
    };

    // プロジェクト一覧取得
    let projects = match resource_repo::find_all_projects(
        &state.pool,
        page,
        Some(user_id),
        &filter,
        scope.as_ref(),
    )
    .await
    {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response();
        }
    };

    // 試運転: 新しい判定で見えなくなる行を記録する(今の一覧は全件なので、新しく見える行は無い)
    let mut projects = authorize::filter_list(
        &state.pool,
        &viewer,
        Resource::Project,
        "GET /api/v1/projects/",
        projects,
        |p| p.id as i64,
        |p| ResourceRef::Project {
            project_id: p.id,
            teams: p
                .teams
                .iter()
                .map(|t| viewer.team_facts_for_read(t.id))
                .collect(),
        },
    );
    for p in &mut projects {
        hide_invisible_teams(&viewer, p);
    }

    // 件数取得
    let count = match resource_repo::count_projects(&state.pool, scope.as_ref()).await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response();
        }
    };

    let has_next = page * PAGE_SIZE < count;
    let has_previous = page > 1;
    let next = if has_next {
        Some(format!("?page={}", page + 1))
    } else {
        None
    };
    let previous = if has_previous {
        Some(if page == 2 {
            "?".to_string()
        } else {
            format!("?page={}", page - 1)
        })
    } else {
        None
    };

    (
        StatusCode::OK,
        Json(PaginatedProjectsOut {
            count,
            next,
            previous,
            results: projects,
        }),
    )
        .into_response()
}

/// プロジェクト詳細 GET /api/v1/projects/{id}/
#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}/",
    tag = "projects",
    params(
        ("id" = i32, Path, description = "プロジェクトID")
    ),
    responses(
        (status = 200, description = "プロジェクト詳細を返す"),
        (status = 404, description = "プロジェクトが見つからない")
    )
)]
pub async fn project_detail(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    let user_id = match viewer.require_user_id() {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    // 今の判定には確認が無い(見つからなければ、下の取得で 404)
    if let Err(resp) = authorize::gate_project(
        &state.pool,
        &viewer,
        id,
        Action::Read,
        Ok(()),
        "GET /api/v1/projects/{id}/",
    )
    .await
    {
        return resp;
    }
    match resource_repo::find_project_by_id(&state.pool, id, Some(user_id)).await {
        Ok(Some(mut project)) => {
            hide_invisible_teams(&viewer, &mut project);
            (StatusCode::OK, Json(project)).into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "見つかりません".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// 依存グラフの中のチケットを絞る範囲。チケットの見え方のスイッチ(`ACCESS_ENFORCE_TICKET`)が `on` のときだけ
fn dependency_graph_scope(viewer: &Viewer) -> Option<crate::domain::access::Scope> {
    (shadow::mode(Resource::Ticket) == Mode::On).then(|| viewer.scope())
}

/// GET /api/v1/projects/{id}/dependencies/ — 依存関係フロー可視化用の一括取得(ノード+エッジ)
pub async fn project_dependency_graph(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    // 今の判定には確認が無い。グラフの中のチケットは、チケットの新しい判定(`on`)で見える物だけにする(D-3)
    if let Err(resp) = authorize::gate_project(
        &state.pool,
        &viewer,
        id,
        Action::Read,
        Ok(()),
        "GET /api/v1/projects/{id}/dependencies/",
    )
    .await
    {
        return resp;
    }
    let graph = match ticket_repo::find_dependency_graph_for_project(&state.pool, id).await {
        Ok(graph) => match dependency_graph_scope(&viewer) {
            Some(scope) => ticket_repo::filter_dependency_graph(&state.pool, graph, &scope).await,
            None => Ok(graph),
        },
        Err(e) => Err(e),
    };
    match graph {
        Ok(graph) => (StatusCode::OK, Json(graph)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// GET /api/v1/teams/{id}/dependencies/ — チーム版依存関係フロー可視化
pub async fn team_dependency_graph(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    let auth = AuthUser {
        user_id: match viewer.require_user_id() {
            Ok(id) => id,
            Err(resp) => return resp,
        },
    };
    let caller = match user_repo::find_by_id(&state.pool, auth.user_id).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    detail: "ユーザーが見つかりません".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response();
        }
    };

    let can_access = if caller.is_staff {
        true
    } else {
        match team_repo::check_team_membership_exists(&state.pool, id, auth.user_id).await {
            Ok(is_member) => is_member,
            Err(e) => {
                tracing::error!("DB operation failed: {:?}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response();
            }
        }
    };

    let legacy = if can_access {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                detail: "このチームにアクセスする権限がありません".to_string(),
            }),
        )
            .into_response())
    };
    if let Err(resp) = authorize::gate_team(
        &state.pool,
        &viewer,
        id,
        Action::Read,
        legacy,
        "GET /api/v1/teams/{id}/dependencies/",
    )
    .await
    {
        return resp;
    }

    let graph = match ticket_repo::find_dependency_graph_for_team(&state.pool, id).await {
        Ok(graph) => match dependency_graph_scope(&viewer) {
            Some(scope) => ticket_repo::filter_dependency_graph(&state.pool, graph, &scope).await,
            None => Ok(graph),
        },
        Err(e) => Err(e),
    };
    match graph {
        Ok(graph) => (StatusCode::OK, Json(graph)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct ProjectTeamAddIn {
    #[serde(rename = "teamId")]
    pub team_id: i32,
}

/// 参加チーム追加 POST /api/v1/projects/{id}/teams/
pub async fn project_team_add(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
    Json(body): Json<ProjectTeamAddIn>,
) -> impl IntoResponse {
    let auth = AuthUser {
        user_id: match viewer.require_user_id() {
            Ok(id) => id,
            Err(resp) => return resp,
        },
    };
    match resource_repo::find_project_by_id(&state.pool, id, None).await {
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    detail: "見つかりません".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response();
        }
        Ok(Some(_)) => {}
    }

    // 変更権限(管理者/オーナー/参加チームの管理者)と、自分が所属するチームのみ追加可
    if let Err(resp) = super::project_team_api::authorize_add(
        &state,
        &viewer,
        id,
        body.team_id,
        "POST /api/v1/projects/{id}/teams/",
    )
    .await
    {
        return resp;
    }

    match resource_repo::add_project_team(&state.pool, id, body.team_id).await {
        Ok(true) => match resource_repo::find_project_by_id(&state.pool, id, None).await {
            Ok(Some(mut project)) => {
                let team_name = project
                    .teams
                    .iter()
                    .find(|t| t.id == body.team_id)
                    .map(|t| t.name.clone());
                project_activity_repo::record_best_effort(
                    &state.pool,
                    id,
                    Some(auth.user_id),
                    "team_added",
                    json!({"team_id": body.team_id, "team_name": team_name}),
                )
                .await;
                hide_invisible_teams(&viewer, &mut project);
                (StatusCode::OK, Json(project)).into_response()
            }
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response(),
        },
        Ok(false) => match resource_repo::find_project_by_id(&state.pool, id, None).await {
            // 既に参加済み → 冪等に 200
            Ok(Some(mut project)) => {
                hide_invisible_teams(&viewer, &mut project);
                (StatusCode::OK, Json(project)).into_response()
            }
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response(),
        },
        Err(e) => {
            tracing::error!("add_project_team failed: {:?}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    detail: e.to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// 参加チーム削除 DELETE /api/v1/projects/{id}/teams/{team_id}/
pub async fn project_team_remove(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, team_id)): Path<(i32, i32)>,
) -> impl IntoResponse {
    let auth = AuthUser {
        user_id: match viewer.require_user_id() {
            Ok(id) => id,
            Err(resp) => return resp,
        },
    };
    let project_before = match resource_repo::find_project_by_id(&state.pool, id, None).await {
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    detail: "見つかりません".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response();
        }
        Ok(Some(p)) => p,
    };

    // 変更権限があり、そのチームのチケット・サイクルがこのプロジェクトに残っていないこと
    if let Err(resp) = super::project_team_api::authorize_remove(
        &state,
        &viewer,
        id,
        team_id,
        "DELETE /api/v1/projects/{id}/teams/{team_id}/",
    )
    .await
    {
        return resp;
    }

    match resource_repo::remove_project_team(&state.pool, id, team_id).await {
        Ok(true) => {
            let team_name = project_before
                .teams
                .iter()
                .find(|t| t.id == team_id)
                .map(|t| t.name.clone());
            project_activity_repo::record_best_effort(
                &state.pool,
                id,
                Some(auth.user_id),
                "team_removed",
                json!({"team_id": team_id, "team_name": team_name}),
            )
            .await;
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "参加チームが見つかりません".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("cannot remove the last team") {
                (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(ErrorResponse {
                        detail: "最後の参加チームは削除できません".to_string(),
                    }),
                )
                    .into_response()
            } else {
                tracing::error!("remove_project_team failed: {:?}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response()
            }
        }
    }
}

/// プロジェクト作成 POST /api/v1/projects/
pub async fn project_create(
    State(state): State<AppState>,
    viewer: Viewer,
    headers: HeaderMap,
    Json(body): Json<ProjectWriteIn>,
) -> impl IntoResponse {
    let auth = AuthUser {
        user_id: match viewer.require_user_id() {
            Ok(id) => id,
            Err(resp) => return resp,
        },
    };
    // 冪等キー（詳細設計 §2.5）: 同じキーで作成済みなら、その行を 200 で返す
    let client_request_id = match super::idempotency::parse_key(&headers) {
        Ok(k) => k,
        Err(resp) => return resp,
    };
    if let Some(key) = client_request_id {
        if let Some(resp) = existing_project_for_key(&state, key, auth.user_id).await {
            return resp;
        }
    }

    // 新しい判定(D-3): 参加させるすべてのチームが見えること(Guest は不可)。今の判定には確認が無い。
    // チームの指定が無い場合は、作成の検証(400)に任せる
    if !body.team_ids.is_empty() {
        let teams = match facts_repo::facts_for_teams(&state.pool, &body.team_ids).await {
            Ok(t) => t,
            Err(e) => {
                tracing::error!("DB operation failed: {:?}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response();
            }
        };
        let target = ResourceRef::Project {
            project_id: 0,
            teams,
        };
        if let Err(resp) = authorize::gate(
            &state.pool,
            &viewer,
            Some(&target),
            Action::Create,
            Resource::Project,
            0,
            Ok(()),
            "POST /api/v1/projects/",
        ) {
            return resp;
        }
    }

    // アーカイブ済みのチームは、新しいプロジェクトの担当にできない
    if let Ok(true) = crate::infrastructure::repositories::team_archive_repo::any_archived(
        &state.pool,
        &body.team_ids,
    )
    .await
    {
        return (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                detail:
                    "アーカイブ済みのチームは、プロジェクトの担当にできません(先に復元してください)"
                        .to_string(),
            }),
        )
            .into_response();
    }

    match resource_repo::create_project(&state.pool, &body, Some(auth.user_id), client_request_id)
        .await
    {
        Ok(project_id) => {
            // 作成したプロジェクトを返す
            match resource_repo::find_project_by_id(&state.pool, project_id, Some(auth.user_id))
                .await
            {
                Ok(Some(project)) => {
                    project_activity_repo::record_best_effort(
                        &state.pool,
                        project.id,
                        Some(auth.user_id),
                        "project_created",
                        json!({"name": project.name, "prefix": project.prefix}),
                    )
                    .await;
                    (StatusCode::CREATED, Json(project)).into_response()
                }
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response(),
            }
        }
        Err(e) => {
            // 同時に同じキーで作成された場合は、先に作られた行を返す
            if let Some(key) = client_request_id {
                if super::idempotency::is_unique_violation(&e) {
                    if let Some(resp) = existing_project_for_key(&state, key, auth.user_id).await {
                        return resp;
                    }
                }
            }

            tracing::error!("DB operation failed: {:?}", e);
            let detail = e.to_string();
            if detail.contains("team_ids") || detail.contains("at least one") {
                return (StatusCode::BAD_REQUEST, Json(ErrorResponse { detail })).into_response();
            }
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// 冪等キーで作成済みのプロジェクトを探し、あれば応答（200 か、別の利用者なら 409）を返す。
async fn existing_project_for_key(
    state: &AppState,
    key: Uuid,
    user_id: i32,
) -> Option<axum::response::Response> {
    let found = sqlx::query_as::<_, (i32, Option<i32>)>(
        "SELECT id::int4, owner_id::int4 FROM tickets_project WHERE client_request_id = $1",
    )
    .bind(key)
    .fetch_optional(&state.pool)
    .await;
    let (project_id, owner_id) = match found {
        Ok(Some(row)) => row,
        Ok(None) => return None,
        Err(e) => {
            tracing::error!("idempotency lookup failed: {:?}", e);
            return Some(
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response(),
            );
        }
    };
    if owner_id.is_some_and(|o| o != user_id) {
        return Some(super::idempotency::conflict_response());
    }
    match resource_repo::find_project_by_id(&state.pool, project_id, Some(user_id)).await {
        Ok(Some(project)) => Some((StatusCode::OK, Json(project)).into_response()),
        other => {
            tracing::error!("idempotency fetch failed: {:?}", other.err());
            Some(
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response(),
            )
        }
    }
}

/// プロジェクト更新 PUT /api/v1/projects/{id}/
pub async fn project_update(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
    Json(body): Json<ProjectWriteIn>,
) -> impl IntoResponse {
    let auth = AuthUser {
        user_id: match viewer.require_user_id() {
            Ok(id) => id,
            Err(resp) => return resp,
        },
    };
    let legacy = authorize_project_edit(&state.pool, auth.user_id, id).await;
    if let Err(resp) = authorize::gate_project(
        &state.pool,
        &viewer,
        id,
        Action::Write,
        legacy,
        "PUT /api/v1/projects/{id}/",
    )
    .await
    {
        return resp;
    }
    // Activity 用に変更前の状態を取っておく(取得できなくても更新自体は続ける)
    let old = resource_repo::find_project_by_id(&state.pool, id, None)
        .await
        .ok()
        .flatten();
    match resource_repo::update_project(&state.pool, id, &body).await {
        Ok(true) => {
            // 更新後のプロジェクトを返す
            match resource_repo::find_project_by_id(&state.pool, id, None).await {
                Ok(Some(mut project)) => {
                    if let Some(old) = &old {
                        project_activity_repo::record_project_changes(
                            &state.pool,
                            Some(auth.user_id),
                            old,
                            &project,
                        )
                        .await;
                    }
                    hide_invisible_teams(&viewer, &mut project);
                    (StatusCode::OK, Json(project)).into_response()
                }
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response(),
            }
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "見つかりません".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// プロジェクト部分更新 PATCH /api/v1/projects/{id}/
pub async fn project_patch(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
    Json(body): Json<ProjectPatchIn>,
) -> impl IntoResponse {
    let auth = AuthUser {
        user_id: match viewer.require_user_id() {
            Ok(id) => id,
            Err(resp) => return resp,
        },
    };
    let legacy = authorize_project_edit(&state.pool, auth.user_id, id).await;
    if let Err(resp) = authorize::gate_project(
        &state.pool,
        &viewer,
        id,
        Action::Write,
        legacy,
        "PATCH /api/v1/projects/{id}/",
    )
    .await
    {
        return resp;
    }
    const ALLOWED_PROJECT_STATUSES: [&str; 4] = ["planned", "in_progress", "paused", "completed"];
    if let Some(ref status) = body.status {
        if !ALLOWED_PROJECT_STATUSES.contains(&status.as_str()) {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    detail: format!(
                        "status must be one of: {}",
                        ALLOWED_PROJECT_STATUSES.join(", ")
                    ),
                }),
            )
                .into_response();
        }
    }

    // 親の変更が指定されている場合、先に処理する(失敗したら他のフィールドは適用しない)
    if let Some(new_parent) = body.parent_project_id {
        if let Err(resp) = super::project_structure_api::change_parent(
            &state,
            &viewer,
            id,
            new_parent,
            "PATCH /api/v1/projects/{id}/",
        )
        .await
        {
            return resp;
        }
        // 親だけの変更ならここで完了。ほかのフィールドが一緒にあれば、続けて適用する。
        if body.cycle_auto_complete.is_none()
            && body.cycle_auto_create_next.is_none()
            && body.status.is_none()
            && body.priority.is_none()
        {
            return StatusCode::NO_CONTENT.into_response();
        }
    }

    let old = resource_repo::find_project_by_id(&state.pool, id, None)
        .await
        .ok()
        .flatten();
    match resource_repo::patch_project_settings(&state.pool, id, &body).await {
        Ok(_) => {
            if let Some(old) = &old {
                if let Ok(Some(new)) =
                    resource_repo::find_project_by_id(&state.pool, id, None).await
                {
                    project_activity_repo::record_project_changes(
                        &state.pool,
                        Some(auth.user_id),
                        old,
                        &new,
                    )
                    .await;
                }
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => {
            tracing::error!("プロジェクト設定更新失敗: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// プロジェクト削除 DELETE /api/v1/projects/{id}/
pub async fn project_delete(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    let auth = AuthUser {
        user_id: match viewer.require_user_id() {
            Ok(id) => id,
            Err(resp) => return resp,
        },
    };
    use resource_repo::DeleteProjectResult;

    let caller = match user_repo::find_by_id(&state.pool, auth.user_id).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    detail: "ユーザーが見つかりません".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response();
        }
    };

    let can_delete = if caller.is_staff {
        true
    } else {
        match resource_repo::get_project_owner_id(&state.pool, id).await {
            Ok(Some(owner_id)) => owner_id == auth.user_id,
            Ok(None) => false,
            Err(e) => {
                tracing::error!("DB operation failed: {:?}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response();
            }
        }
    };

    let legacy = if can_delete {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                detail: "オーナー以外はプロジェクトを削除できません".to_string(),
            }),
        )
            .into_response())
    };
    // 新しい判定(D-3): 参加チームのどれかの設定を管理できること
    if let Err(resp) = authorize::gate_project(
        &state.pool,
        &viewer,
        id,
        Action::Delete,
        legacy,
        "DELETE /api/v1/projects/{id}/",
    )
    .await
    {
        return resp;
    }

    match resource_repo::delete_project(&state.pool, id).await {
        Ok(DeleteProjectResult::Deleted) => StatusCode::NO_CONTENT.into_response(),
        Ok(DeleteProjectResult::NotFound) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "見つかりません".to_string(),
            }),
        )
            .into_response(),
        Ok(DeleteProjectResult::HasTickets) => (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                detail: "チケットが存在するプロジェクトは削除できません".to_string(),
            }),
        )
            .into_response(),
        Ok(DeleteProjectResult::HasChildren(child_names)) => (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                detail: format!(
                    "子プロジェクトが残っているため削除できません: {}",
                    child_names.join(", ")
                ),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

// =============================================================================
// マスタ類の認可(アクセス制御の再設計 D-4)
// =============================================================================
//
// 切り替えは `ACCESS_ENFORCE_MASTER`(カテゴリ・休日・ラベル・マイルストーン)。今の判定には確認が無い。
// - カテゴリ・休日・全体のラベル・プロジェクトの無いマイルストーン: 閲覧は全員、変更はシステム管理者だけ
// - チーム・プロジェクトのラベル: 閲覧はチーム・プロジェクトが見える人、変更はチームの設定を管理できる人
// - プロジェクトのマイルストーン: 閲覧はプロジェクトが見える人、変更はプロジェクトに書き込める人

fn server_error_response(e: anyhow::Error) -> axum::response::Response {
    tracing::error!("DB operation failed: {:?}", e);
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            detail: "サーバーエラーが発生しました".to_string(),
        }),
    )
        .into_response()
}

/// 共通のマスタ(カテゴリ・休日)の変更の判定。閲覧は全員が可能なので、判定しない(`_viewer` のみ受け取る)
#[allow(clippy::result_large_err)] // 認可の失敗は、そのまま応答として返す
fn master_gate(
    state: &AppState,
    viewer: &Viewer,
    action: Action,
    id: i32,
    route: &'static str,
) -> Result<(), axum::response::Response> {
    authorize::gate(
        &state.pool,
        viewer,
        Some(&ResourceRef::GlobalMaster),
        action,
        Resource::Master,
        id as i64,
        Ok(()),
        route,
    )
}

/// チーム・プロジェクト・全体のどれかに属する物(ラベル・マイルストーン)の判定。切り替えは `Master`
#[allow(clippy::too_many_arguments)]
async fn scoped_gate(
    state: &AppState,
    viewer: &Viewer,
    team_id: Option<i32>,
    project_id: Option<i32>,
    action: Action,
    on_scoped: Action,
    id: i32,
    route: &'static str,
) -> Result<(), axum::response::Response> {
    authorize::gate_scoped(
        &state.pool,
        viewer,
        team_id,
        project_id,
        action,
        on_scoped,
        Resource::Master,
        id,
        route,
    )
    .await
}

/// 新しい判定(`on`)で、一覧を SQL で絞るための見える範囲
fn master_scope(viewer: &Viewer) -> Option<crate::domain::access::Scope> {
    (shadow::mode(Resource::Master) == Mode::On).then(|| viewer.scope())
}

// =============================================================================
// Categories
// =============================================================================

/// カテゴリー一覧 GET /api/v1/categories/
pub async fn category_list(State(state): State<AppState>, _viewer: Viewer) -> impl IntoResponse {
    match resource_repo::find_all_categories(&state.pool).await {
        Ok(categories) => (StatusCode::OK, Json(categories)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(Vec::<CategoryOut>::new()),
            )
                .into_response()
        }
    }
}

/// カテゴリー詳細 GET /api/v1/categories/{id}/
pub async fn category_detail(
    State(state): State<AppState>,
    _viewer: Viewer,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match resource_repo::find_category_by_id(&state.pool, id).await {
        Ok(Some(category)) => (StatusCode::OK, Json(category)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "見つかりません".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// カテゴリー作成 POST /api/v1/categories/
pub async fn category_create(
    State(state): State<AppState>,
    viewer: Viewer,
    Json(body): Json<CategoryWriteIn>,
) -> impl IntoResponse {
    if let Err(resp) = master_gate(
        &state,
        &viewer,
        Action::Create,
        0,
        "POST /api/v1/categories/",
    ) {
        return resp;
    }
    match resource_repo::create_category(&state.pool, &body).await {
        Ok(category_id) => {
            // 作成したカテゴリーを返す
            match resource_repo::find_category_by_id(&state.pool, category_id).await {
                Ok(Some(category)) => (StatusCode::CREATED, Json(category)).into_response(),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response(),
            }
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// カテゴリー更新 PUT /api/v1/categories/{id}/
pub async fn category_update(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
    Json(body): Json<CategoryWriteIn>,
) -> impl IntoResponse {
    if let Err(resp) = master_gate(
        &state,
        &viewer,
        Action::Write,
        id,
        "PUT /api/v1/categories/{id}/",
    ) {
        return resp;
    }
    match resource_repo::update_category(&state.pool, id, &body).await {
        Ok(true) => {
            // 更新後のカテゴリーを返す
            match resource_repo::find_category_by_id(&state.pool, id).await {
                Ok(Some(category)) => (StatusCode::OK, Json(category)).into_response(),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response(),
            }
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "見つかりません".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// カテゴリー削除 DELETE /api/v1/categories/{id}/
pub async fn category_delete(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    if let Err(resp) = master_gate(
        &state,
        &viewer,
        Action::Delete,
        id,
        "DELETE /api/v1/categories/{id}/",
    ) {
        return resp;
    }
    match resource_repo::delete_category(&state.pool, id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "見つかりません".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

// =============================================================================
// Milestones
// =============================================================================

/// マイルストーン一覧 GET /api/v1/milestones/
pub async fn milestone_list(
    State(state): State<AppState>,
    viewer: Viewer,
    Query(params): Query<MilestoneListQuery>,
) -> impl IntoResponse {
    let page = params.page.unwrap_or(1).max(1);
    const PAGE_SIZE: i64 = 50;

    let scope = master_scope(&viewer);
    // マイルストーン一覧取得
    let milestones =
        match resource_repo::find_all_milestones(&state.pool, params.project, page, scope.as_ref())
            .await
        {
            Ok(m) => m,
            Err(e) => {
                tracing::error!("DB operation failed: {:?}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response();
            }
        };

    // 件数取得
    // 試運転: 新しい判定で見えなくなる行を記録する(参加チームの情報は、出てきたプロジェクトの分だけ読む)
    let mut project_facts = std::collections::HashMap::new();
    if shadow::mode(Resource::Master) == Mode::Shadow {
        let mut ids: Vec<i32> = milestones.iter().filter_map(|m| m.project).collect();
        ids.sort_unstable();
        ids.dedup();
        for pid in ids {
            match facts_repo::facts_for_project(&state.pool, pid).await {
                Ok(Some(f)) => {
                    project_facts.insert(pid, f);
                }
                Ok(None) => {}
                Err(e) => return server_error_response(e),
            }
        }
    }
    let milestones = authorize::filter_list(
        &state.pool,
        &viewer,
        Resource::Master,
        "GET /api/v1/milestones/",
        milestones,
        |m| m.id as i64,
        |m| match m.project {
            None => ResourceRef::GlobalMaster,
            Some(pid) => project_facts
                .get(&pid)
                .cloned()
                .unwrap_or(ResourceRef::Project {
                    project_id: pid,
                    teams: vec![],
                }),
        },
    );

    let count =
        match resource_repo::count_milestones(&state.pool, params.project, scope.as_ref()).await {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("DB operation failed: {:?}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response();
            }
        };

    let has_next = page * PAGE_SIZE < count;
    let has_previous = page > 1;
    let next = if has_next {
        Some(format!("?page={}", page + 1))
    } else {
        None
    };
    let previous = if has_previous {
        Some(if page == 2 {
            "?".to_string()
        } else {
            format!("?page={}", page - 1)
        })
    } else {
        None
    };

    (
        StatusCode::OK,
        Json(PaginatedMilestonesOut {
            count,
            next,
            previous,
            results: milestones,
        }),
    )
        .into_response()
}

/// マイルストーン詳細 GET /api/v1/milestones/{id}/
pub async fn milestone_detail(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match resource_repo::find_milestone_by_id(&state.pool, id).await {
        Ok(Some(milestone)) => {
            if let Err(resp) = scoped_gate(
                &state,
                &viewer,
                None,
                milestone.project,
                Action::Read,
                Action::Read,
                id,
                "GET /api/v1/milestones/{id}/",
            )
            .await
            {
                return resp;
            }
            (StatusCode::OK, Json(milestone)).into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "見つかりません".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// マイルストーン作成 POST /api/v1/milestones/
pub async fn milestone_create(
    State(state): State<AppState>,
    viewer: Viewer,
    Json(body): Json<MilestoneWriteIn>,
) -> impl IntoResponse {
    let auth = AuthUser {
        user_id: match viewer.require_user_id() {
            Ok(id) => id,
            Err(resp) => return resp,
        },
    };
    if let Err(resp) = scoped_gate(
        &state,
        &viewer,
        None,
        body.project,
        Action::Create,
        Action::Write,
        0,
        "POST /api/v1/milestones/",
    )
    .await
    {
        return resp;
    }
    match resource_repo::create_milestone(&state.pool, &body).await {
        Ok(milestone_id) => {
            // 作成したマイルストーンを返す
            match resource_repo::find_milestone_by_id(&state.pool, milestone_id).await {
                Ok(Some(milestone)) => {
                    if let Some(project_id) = milestone.project {
                        project_activity_repo::record_best_effort(
                            &state.pool,
                            project_id,
                            Some(auth.user_id),
                            "milestone_created",
                            json!({
                                "milestone_id": milestone.id,
                                "name": milestone.name,
                                "due_date": milestone.due_date.map(|d| d.to_string()),
                            }),
                        )
                        .await;
                    }
                    (StatusCode::CREATED, Json(milestone)).into_response()
                }
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response(),
            }
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// マイルストーン更新 PUT /api/v1/milestones/{id}/
pub async fn milestone_update(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
    Json(body): Json<MilestoneWriteIn>,
) -> impl IntoResponse {
    let auth = AuthUser {
        user_id: match viewer.require_user_id() {
            Ok(id) => id,
            Err(resp) => return resp,
        },
    };
    let old = resource_repo::find_milestone_by_id(&state.pool, id)
        .await
        .ok()
        .flatten();
    // 今のマイルストーンと、移し先(プロジェクトの変更)の両方で判定する
    const ROUTE: &str = "PUT /api/v1/milestones/{id}/";
    let current = old.as_ref().map(|m| m.project);
    for project in current.into_iter().chain([body.project]) {
        if let Err(resp) = scoped_gate(
            &state,
            &viewer,
            None,
            project,
            Action::Write,
            Action::Write,
            id,
            ROUTE,
        )
        .await
        {
            return resp;
        }
    }
    match resource_repo::update_milestone(&state.pool, id, &body).await {
        Ok(true) => {
            // 更新後のマイルストーンを返す
            match resource_repo::find_milestone_by_id(&state.pool, id).await {
                Ok(Some(milestone)) => {
                    if let (Some(project_id), Some(old)) = (milestone.project, &old) {
                        let mut changes = Vec::new();
                        if old.name != milestone.name {
                            changes.push(
                                json!({"field": "name", "from": old.name, "to": milestone.name}),
                            );
                        }
                        if old.due_date != milestone.due_date {
                            changes.push(json!({
                                "field": "dueDate",
                                "from": old.due_date.map(|d| d.to_string()),
                                "to": milestone.due_date.map(|d| d.to_string()),
                            }));
                        }
                        if !changes.is_empty() {
                            project_activity_repo::record_best_effort(
                                &state.pool,
                                project_id,
                                Some(auth.user_id),
                                "milestone_updated",
                                json!({"milestone_id": milestone.id, "name": milestone.name, "changes": changes}),
                            )
                            .await;
                        }
                    }
                    (StatusCode::OK, Json(milestone)).into_response()
                }
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response(),
            }
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "見つかりません".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// マイルストーン削除 DELETE /api/v1/milestones/{id}/
pub async fn milestone_delete(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    let auth = AuthUser {
        user_id: match viewer.require_user_id() {
            Ok(id) => id,
            Err(resp) => return resp,
        },
    };
    let old = resource_repo::find_milestone_by_id(&state.pool, id)
        .await
        .ok()
        .flatten();
    if let Some(m) = &old {
        if let Err(resp) = scoped_gate(
            &state,
            &viewer,
            None,
            m.project,
            Action::Delete,
            Action::Write,
            id,
            "DELETE /api/v1/milestones/{id}/",
        )
        .await
        {
            return resp;
        }
    }
    match resource_repo::delete_milestone(&state.pool, id).await {
        Ok(true) => {
            if let Some(old) = &old {
                if let Some(project_id) = old.project {
                    project_activity_repo::record_best_effort(
                        &state.pool,
                        project_id,
                        Some(auth.user_id),
                        "milestone_deleted",
                        json!({"milestone_id": old.id, "name": old.name}),
                    )
                    .await;
                }
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "見つかりません".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

// =============================================================================
// Labels
// =============================================================================

/// ラベル一覧 GET /api/v1/labels/
pub async fn label_list(
    State(state): State<AppState>,
    viewer: Viewer,
    Query(params): Query<LabelListQuery>,
) -> impl IntoResponse {
    const ROUTE: &str = "GET /api/v1/labels/";
    let mode = shadow::mode(Resource::Master);
    let scope = master_scope(&viewer);
    let labels = match resource_repo::find_all_labels(
        &state.pool,
        params.project,
        params.team,
        scope.as_ref(),
    )
    .await
    {
        Ok(l) => l,
        Err(e) => return server_error_response(e),
    };
    // 試運転: 同じ条件を新しい判定で読み、見えなくなる行を記録する(今の一覧は全件なので、新しく見える行は無い)。
    // ラベルは SQL の規則(Guest の全体のラベルは使用範囲だけ。§7.4)を含めて比べるため、SQL の結果どうしで比べる
    if mode == Mode::Shadow {
        match resource_repo::find_all_labels(
            &state.pool,
            params.project,
            params.team,
            Some(&viewer.scope()),
        )
        .await
        {
            Ok(new) => {
                let cur: Vec<i64> = labels.iter().map(|l| l.id as i64).collect();
                let new: Vec<i64> = new.iter().map(|l| l.id as i64).collect();
                shadow::record(
                    &state.pool,
                    Resource::Master,
                    viewer.user_id(),
                    ROUTE,
                    shadow::diff_ids(&cur, &new),
                );
            }
            Err(e) => tracing::warn!(
                "[認可/試運転] ラベルの差分の計算に失敗(一覧は続行): {:?}",
                e
            ),
        }
    }
    (
        StatusCode::OK,
        Json(PaginatedLabelsOut {
            next: None,
            previous: None,
            results: labels,
        }),
    )
        .into_response()
}

/// ラベル詳細 GET /api/v1/labels/{id}/
pub async fn label_detail(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match resource_repo::find_label_by_id(&state.pool, id).await {
        Ok(Some(label)) => {
            if let Err(resp) = scoped_gate(
                &state,
                &viewer,
                label.team_id,
                label.project,
                Action::Read,
                Action::Read,
                id,
                "GET /api/v1/labels/{id}/",
            )
            .await
            {
                return resp;
            }
            (StatusCode::OK, Json(label)).into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "見つかりません".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// ラベル作成 POST /api/v1/labels/
pub async fn label_create(
    State(state): State<AppState>,
    viewer: Viewer,
    Json(body): Json<LabelWriteIn>,
) -> impl IntoResponse {
    if body.project.is_none() && body.team_id.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                detail: "project または teamId を指定してください".to_string(),
            }),
        )
            .into_response();
    }
    if let Err(resp) = scoped_gate(
        &state,
        &viewer,
        body.team_id,
        body.project,
        Action::Create,
        Action::ManageSettings,
        0,
        "POST /api/v1/labels/",
    )
    .await
    {
        return resp;
    }
    match resource_repo::create_label(&state.pool, &body).await {
        Ok(label_id) => {
            // 作成したラベルを返す
            match resource_repo::find_label_by_id(&state.pool, label_id).await {
                Ok(Some(label)) => (StatusCode::CREATED, Json(label)).into_response(),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response(),
            }
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// ラベル更新 PUT /api/v1/labels/{id}/
pub async fn label_update(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
    Json(body): Json<LabelWriteIn>,
) -> impl IntoResponse {
    // 今のラベルと、移し先(チーム・プロジェクトの変更)の両方で判定する
    const ROUTE: &str = "PUT /api/v1/labels/{id}/";
    let current = match resource_repo::find_label_by_id(&state.pool, id).await {
        Ok(l) => l,
        Err(e) => return server_error_response(e),
    };
    let targets = current
        .map(|l| (l.team_id, l.project))
        .into_iter()
        .chain([(body.team_id, body.project)]);
    for (team_id, project_id) in targets {
        if let Err(resp) = scoped_gate(
            &state,
            &viewer,
            team_id,
            project_id,
            Action::Write,
            Action::ManageSettings,
            id,
            ROUTE,
        )
        .await
        {
            return resp;
        }
    }
    match resource_repo::update_label(&state.pool, id, &body).await {
        Ok(true) => {
            // 更新後のラベルを返す
            match resource_repo::find_label_by_id(&state.pool, id).await {
                Ok(Some(label)) => (StatusCode::OK, Json(label)).into_response(),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response(),
            }
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "見つかりません".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// ラベル削除 DELETE /api/v1/labels/{id}/
pub async fn label_delete(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match resource_repo::find_label_by_id(&state.pool, id).await {
        Ok(Some(l)) => {
            if let Err(resp) = scoped_gate(
                &state,
                &viewer,
                l.team_id,
                l.project,
                Action::Delete,
                Action::ManageSettings,
                id,
                "DELETE /api/v1/labels/{id}/",
            )
            .await
            {
                return resp;
            }
        }
        Ok(None) => {}
        Err(e) => return server_error_response(e),
    }
    match resource_repo::delete_label(&state.pool, id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "見つかりません".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

// =============================================================================
// Holidays
// =============================================================================

#[derive(Deserialize)]
pub struct HolidayBulkAddIn {
    pub year: i32,
}

/// 休日一覧 GET /api/v1/holidays/
pub async fn holiday_list(State(state): State<AppState>, _viewer: Viewer) -> impl IntoResponse {
    match holiday_repo::find_all(&state.pool).await {
        Ok(holidays) => (StatusCode::OK, Json(holidays)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(Vec::<Holiday>::new()),
            )
                .into_response()
        }
    }
}

/// 指定年の祝日を一括追加 POST /api/v1/holidays/bulk-add/
pub async fn holiday_bulk_add(
    State(state): State<AppState>,
    viewer: Viewer,
    Json(body): Json<HolidayBulkAddIn>,
) -> impl IntoResponse {
    if let Err(resp) = master_gate(
        &state,
        &viewer,
        Action::Create,
        0,
        "POST /api/v1/holidays/bulk-add/",
    ) {
        return resp;
    }
    // holidays.rs::bulk_add と同一の固定10件ロジックをそのまま移植する
    let holidays: Vec<(chrono::NaiveDate, String)> = vec![
        (
            chrono::NaiveDate::from_ymd_opt(body.year, 1, 1).unwrap(),
            "元日".to_string(),
        ),
        (
            chrono::NaiveDate::from_ymd_opt(body.year, 2, 11).unwrap(),
            "建国記念の日".to_string(),
        ),
        (
            chrono::NaiveDate::from_ymd_opt(body.year, 2, 23).unwrap(),
            "天皇誕生日".to_string(),
        ),
        (
            chrono::NaiveDate::from_ymd_opt(body.year, 4, 29).unwrap(),
            "昭和の日".to_string(),
        ),
        (
            chrono::NaiveDate::from_ymd_opt(body.year, 5, 3).unwrap(),
            "憲法記念日".to_string(),
        ),
        (
            chrono::NaiveDate::from_ymd_opt(body.year, 5, 4).unwrap(),
            "みどりの日".to_string(),
        ),
        (
            chrono::NaiveDate::from_ymd_opt(body.year, 5, 5).unwrap(),
            "こどもの日".to_string(),
        ),
        (
            chrono::NaiveDate::from_ymd_opt(body.year, 8, 11).unwrap(),
            "山の日".to_string(),
        ),
        (
            chrono::NaiveDate::from_ymd_opt(body.year, 11, 3).unwrap(),
            "文化の日".to_string(),
        ),
        (
            chrono::NaiveDate::from_ymd_opt(body.year, 11, 23).unwrap(),
            "勤労感謝の日".to_string(),
        ),
    ];
    match holiday_repo::bulk_add(&state.pool, &holidays).await {
        Ok(count) => (StatusCode::OK, Json(serde_json::json!({ "added": count }))).into_response(),
        Err(e) => {
            tracing::error!(
                "[祝日/一括追加] 処理=祝日追加 結果=失敗 影響=祝日が登録されていない | {}",
                e
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "祝日の追加に失敗しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// 休日削除 DELETE /api/v1/holidays/{id}/
pub async fn holiday_delete(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    if let Err(resp) = master_gate(
        &state,
        &viewer,
        Action::Delete,
        id,
        "DELETE /api/v1/holidays/{id}/",
    ) {
        return resp;
    }
    match holiday_repo::delete(&state.pool, id).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            tracing::error!("[祝日/削除] 処理=祝日削除 結果=失敗 影響=削除が実行されていない holiday_id={} | {}", id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "祝日の削除に失敗しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::authorize_project_edit;
    use crate::test_support;
    use axum::http::StatusCode;

    async fn set_role(pool: &sqlx::PgPool, team_id: i32, user_id: i32, role: &str) {
        sqlx::query(
            "INSERT INTO t_team_membership (team_id, user_id, role, joined_at) VALUES ($1, $2, $3, NOW())",
        )
        .bind(team_id as i64)
        .bind(user_id as i64)
        .bind(role)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn authorize_a1_non_member_returns_403() {
        // A1: 参加していないチームの人
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let owner = test_support::create_test_user(&pool, "a1o").await;
        let outsider = test_support::create_test_user(&pool, "a1x").await;
        let project = test_support::create_test_project(&pool, "A1", owner).await;

        let result = authorize_project_edit(&pool, outsider, project).await;
        assert!(result.is_err(), "権限なしは Err を返すべき");
        // レスポンスの status が 403 か確認
        if let Err(response) = result {
            let status_code = response.status();
            assert_eq!(
                status_code,
                StatusCode::FORBIDDEN,
                "403 Forbidden が返されるべき"
            );
        }
    }

    #[tokio::test]
    async fn authorize_a2_team_member_returns_ok() {
        // A2: 参加チームの一般メンバー
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let owner = test_support::create_test_user(&pool, "a2o").await;
        let member = test_support::create_test_user(&pool, "a2m").await;
        let project = test_support::create_test_project(&pool, "A2", owner).await;
        let team = sqlx::query_scalar::<_, i32>(
            "SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1",
        )
        .bind(project as i64)
        .fetch_one(&pool)
        .await
        .unwrap();
        set_role(&pool, team, member, "member").await;

        let result = authorize_project_edit(&pool, member, project).await;
        assert!(result.is_ok(), "参加チームメンバーは Ok を返すべき");
    }

    #[tokio::test]
    async fn authorize_a3_nonexistent_user_returns_401() {
        // A3: 存在しない利用者ID
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let owner = test_support::create_test_user(&pool, "a3o").await;
        let project = test_support::create_test_project(&pool, "A3", owner).await;

        let nonexistent_user_id = i32::MAX;
        let result = authorize_project_edit(&pool, nonexistent_user_id, project).await;
        assert!(result.is_err(), "存在しないユーザーは Err を返すべき");
        if let Err(response) = result {
            let status_code = response.status();
            assert_eq!(
                status_code,
                StatusCode::UNAUTHORIZED,
                "401 Unauthorized が返されるべき"
            );
        }
    }
}
