/// presentation/handlers/wiki_api.rs — Wiki JSON API
///
/// Django /api/v1/wiki/* と挙動を一致させるハンドラー。
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::domain::access::{Action, ResourceRef, Viewer};
use crate::domain::models::wiki_api::*;
use crate::infrastructure::access::{
    facts_repo,
    shadow::{self, Mode, Resource},
};
use crate::infrastructure::repositories::wiki_api_repo;
use crate::presentation::extractors::authorize;
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::presentation::state::AppState;

#[derive(Deserialize)]
pub struct ListQuery {
    pub project: Option<i32>,
    pub team: Option<i32>,
    pub category: Option<String>,
    pub search: Option<String>,
    pub page: Option<i64>,
}

#[derive(Serialize)]
pub struct PaginatedWikiOut {
    pub count: i64,
    pub next: Option<String>,
    pub previous: Option<String>,
    pub results: Vec<WikiPageListOut>,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub detail: String,
}

const NO_WIKI_WRITE_PERMISSION: &str = "このWikiを変更する権限がありません";

fn forbidden() -> axum::response::Response {
    (
        StatusCode::FORBIDDEN,
        Json(ErrorResponse {
            detail: NO_WIKI_WRITE_PERMISSION.to_string(),
        }),
    )
        .into_response()
}

fn server_error() -> axum::response::Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            detail: "サーバーエラーが発生しました".to_string(),
        }),
    )
        .into_response()
}

/// 指定した所属（プロジェクト／チーム）に書き込めるか。作成と、移動先の確認に使う。
pub(crate) async fn authorize_scope_write(
    pool: &sqlx::PgPool,
    user_id: i32,
    project: Option<i32>,
    team: Option<i32>,
) -> Result<(), axum::response::Response> {
    match wiki_api_repo::can_write_scope(pool, user_id, project, team).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(forbidden()),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            Err(server_error())
        }
    }
}

/// 既存ページへの書き込み権限（編集・紐付け・削除）。
/// ページが無ければ 404。`delete` が true のとき、どこにも属さない共有ページは staff か作成者だけ。
pub(crate) async fn authorize_page_write(
    pool: &sqlx::PgPool,
    user_id: i32,
    page_id: i32,
    delete: bool,
) -> Result<wiki_api_repo::WikiScope, axum::response::Response> {
    let scope = match wiki_api_repo::find_scope(pool, page_id).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    detail: "見つかりません".to_string(),
                }),
            )
                .into_response());
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return Err(server_error());
        }
    };
    authorize_scope_write(pool, user_id, scope.project, scope.team).await?;
    if delete && scope.project.is_none() && scope.team.is_none() && scope.author_id != user_id {
        let is_staff: bool = sqlx::query_scalar("SELECT is_staff FROM accounts_user WHERE id = $1")
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| {
                tracing::error!("DB operation failed: {:?}", e);
                server_error()
            })?
            .unwrap_or(false);
        if !is_staff {
            return Err(forbidden());
        }
    }
    Ok(scope)
}

// アクセス制御の再設計(D-4)。切り替えは `ACCESS_ENFORCE_WIKI`。
// 閲覧・書き込み: チームの Wiki はチームが見える人、プロジェクトの Wiki はプロジェクトの参加チームが見える人
// (プロジェクト単位の所属では見えない)、全体の Wiki は Full Member。
// 削除: 作成者か、チームの設定を管理できる人(全体の Wiki は作成者かシステム管理者)。

fn db_error(e: anyhow::Error) -> axum::response::Response {
    tracing::error!("DB operation failed: {:?}", e);
    server_error()
}

/// 作成先・移動先への書き込みの判定(今の判定は `authorize_scope_write`)
async fn scope_write_gate(
    state: &AppState,
    viewer: &Viewer,
    user_id: i32,
    project: Option<i32>,
    team: Option<i32>,
    route: &'static str,
) -> Result<(), axum::response::Response> {
    let legacy = authorize_scope_write(&state.pool, user_id, project, team).await;
    let facts = facts_repo::facts_for_wiki_scope(&state.pool, team, project, Some(user_id))
        .await
        .map_err(db_error)?;
    authorize::gate(
        &state.pool,
        viewer,
        facts.as_ref(),
        Action::Create,
        Resource::Wiki,
        0,
        legacy,
        route,
    )
}

/// 既存ページへの操作の判定(今の判定は `authorize_page_write`)。ページの所属を返す
async fn page_gate(
    state: &AppState,
    viewer: &Viewer,
    user_id: i32,
    page_id: i32,
    action: Action,
    route: &'static str,
) -> Result<wiki_api_repo::WikiScope, axum::response::Response> {
    let (legacy, scope) = match action {
        Action::Read => (Ok(()), None),
        _ => match authorize_page_write(&state.pool, user_id, page_id, action == Action::Delete)
            .await
        {
            Ok(s) => (Ok(()), Some(s)),
            Err(resp) => (Err(resp), None),
        },
    };
    let facts = facts_repo::facts_for_wiki(&state.pool, page_id)
        .await
        .map_err(db_error)?;
    authorize::gate(
        &state.pool,
        viewer,
        facts.as_ref(),
        action,
        Resource::Wiki,
        page_id as i64,
        legacy,
        route,
    )?;
    match scope {
        Some(s) => Ok(s),
        // 今の判定を使わなかった(閲覧)か、新しい判定だけが許した(on)場合は、所属を読み直す
        None => match wiki_api_repo::find_scope(&state.pool, page_id).await {
            Ok(Some(s)) => Ok(s),
            Ok(None) => Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    detail: "見つかりません".to_string(),
                }),
            )
                .into_response()),
            Err(e) => Err(db_error(e)),
        },
    }
}

/// GET /api/v1/wiki/
#[utoipa::path(
    get,
    path = "/api/v1/wiki/",
    tag = "wiki",
    responses(
        (status = 200, description = "Wikiページ一覧を返す")
    )
)]
pub async fn list(
    State(state): State<AppState>,
    viewer: Viewer,
    Query(params): Query<ListQuery>,
) -> impl IntoResponse {
    let page = params.page.unwrap_or(1).max(1);
    const PAGE_SIZE: i64 = 50;
    // 新しい判定(on)では SQL で絞る(件数・ページ送りも合わせる)
    let scope = (shadow::mode(Resource::Wiki) == Mode::On).then(|| viewer.scope());

    let items = match wiki_api_repo::find_all(
        &state.pool,
        params.project,
        params.team,
        params.category.as_deref(),
        params.search.as_deref(),
        page,
        scope.as_ref(),
    )
    .await
    {
        Ok(v) => v,
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

    // 試運転: 見えなくなる行を記録する(今の一覧は全件なので、新しく見える行は無い)
    let items = if shadow::mode(Resource::Wiki) == Mode::Shadow {
        let mut facts = std::collections::HashMap::new();
        for w in &items {
            match facts_repo::facts_for_wiki(&state.pool, w.id).await {
                Ok(Some(f)) => {
                    facts.insert(w.id, f);
                }
                Ok(None) => {}
                Err(e) => return db_error(e),
            }
        }
        authorize::filter_list(
            &state.pool,
            &viewer,
            Resource::Wiki,
            "GET /api/v1/wiki/",
            items,
            |w| w.id as i64,
            |w| {
                facts
                    .get(&w.id)
                    .cloned()
                    .unwrap_or(ResourceRef::GlobalMaster)
            },
        )
    } else {
        items
    };

    let count = match wiki_api_repo::count_all(
        &state.pool,
        params.project,
        params.team,
        params.category.as_deref(),
        params.search.as_deref(),
        scope.as_ref(),
    )
    .await
    {
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
        Some(format!("?page={}", page - 1))
    } else {
        None
    };

    (
        StatusCode::OK,
        Json(PaginatedWikiOut {
            count,
            next,
            previous,
            results: items,
        }),
    )
        .into_response()
}

/// GET /api/v1/wiki/{id}/
#[utoipa::path(
    get,
    path = "/api/v1/wiki/{id}/",
    tag = "wiki",
    params(
        ("id" = i32, Path, description = "WikiページID")
    ),
    responses(
        (status = 200, description = "Wikiページ詳細を返す"),
        (status = 404, description = "ページが見つからない")
    )
)]
pub async fn detail(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    // 今の判定には確認が無い(見つからなければ、下の取得で 404)
    let facts = match facts_repo::facts_for_wiki(&state.pool, id).await {
        Ok(f) => f,
        Err(e) => return db_error(e),
    };
    if let Some(Err(resp)) = authorize::enforce(
        &state.pool,
        &viewer,
        facts.as_ref(),
        Action::Read,
        Resource::Wiki,
        id as i64,
        true,
        "GET /api/v1/wiki/{id}/",
    ) {
        return resp;
    }
    match wiki_api_repo::find_by_id(&state.pool, id).await {
        Ok(Some(item)) => (StatusCode::OK, Json(item)).into_response(),
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

/// POST /api/v1/wiki/
pub async fn create(
    State(state): State<AppState>,
    viewer: Viewer,
    Json(body): Json<WikiPageCreateIn>,
) -> impl IntoResponse {
    let auth = AuthUser {
        user_id: match viewer.require_user_id() {
            Ok(id) => id,
            Err(resp) => return resp,
        },
    };
    if let Err(resp) = scope_write_gate(
        &state,
        &viewer,
        auth.user_id,
        body.project,
        body.team,
        "POST /api/v1/wiki/",
    )
    .await
    {
        return resp;
    }
    match wiki_api_repo::create(&state.pool, &body, auth.user_id).await {
        Ok(id) => match wiki_api_repo::find_by_id(&state.pool, id).await {
            Ok(Some(item)) => (StatusCode::CREATED, Json(item)).into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response(),
        },
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

/// PUT/PATCH /api/v1/wiki/{id}/
pub async fn update(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
    Json(body): Json<WikiPageUpdateIn>,
) -> impl IntoResponse {
    let auth = AuthUser {
        user_id: match viewer.require_user_id() {
            Ok(id) => id,
            Err(resp) => return resp,
        },
    };
    const ROUTE: &str = "PUT /api/v1/wiki/{id}/";
    let scope = match page_gate(&state, &viewer, auth.user_id, id, Action::Write, ROUTE).await {
        Ok(s) => s,
        Err(resp) => return resp,
    };
    let new_project = body.project.or(scope.project);
    let new_team = body.team.or(scope.team);
    if (new_project, new_team) != (scope.project, scope.team) {
        if let Err(resp) =
            scope_write_gate(&state, &viewer, auth.user_id, new_project, new_team, ROUTE).await
        {
            return resp;
        }
    }
    match wiki_api_repo::update(&state.pool, id, &body, auth.user_id).await {
        Ok(true) => match wiki_api_repo::find_by_id(&state.pool, id).await {
            Ok(Some(item)) => (StatusCode::OK, Json(item)).into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response(),
        },
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

/// DELETE /api/v1/wiki/{id}/
pub async fn delete(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    let user_id = match viewer.require_user_id() {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    if let Err(resp) = page_gate(
        &state,
        &viewer,
        user_id,
        id,
        Action::Delete,
        "DELETE /api/v1/wiki/{id}/",
    )
    .await
    {
        return resp;
    }
    match wiki_api_repo::delete(&state.pool, id).await {
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

/// GET /api/v1/wiki/{id}/revisions/
pub async fn revisions(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    let user_id = match viewer.require_user_id() {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    if let Err(resp) = page_gate(
        &state,
        &viewer,
        user_id,
        id,
        Action::Read,
        "GET /api/v1/wiki/{id}/revisions/",
    )
    .await
    {
        return resp;
    }
    match wiki_api_repo::find_revisions(&state.pool, id).await {
        Ok(items) => (StatusCode::OK, Json(items)).into_response(),
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

#[derive(Serialize)]
struct LinkActionOut {
    detail: String,
    ticket_id: i32,
}

/// POST /api/v1/wiki/{id}/link-ticket/
pub async fn link_ticket(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
    Json(body): Json<LinkTicketIn>,
) -> impl IntoResponse {
    let auth = AuthUser {
        user_id: match viewer.require_user_id() {
            Ok(id) => id,
            Err(resp) => return resp,
        },
    };
    const ROUTE: &str = "POST /api/v1/wiki/{id}/link-ticket/";
    if let Err(resp) = page_gate(&state, &viewer, auth.user_id, id, Action::Write, ROUTE).await {
        return resp;
    }

    match wiki_api_repo::page_exists(&state.pool, id).await {
        Ok(true) => {}
        Ok(false) => {
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
    }

    match wiki_api_repo::ticket_exists(&state.pool, body.ticket_id).await {
        Ok(true) => {}
        Ok(false) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    detail: "Ticket not found".to_string(),
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
    }

    // 紐付けるチケットが見えること(今の判定は所属の確認。新しい判定はチケットの閲覧)
    let legacy = match crate::infrastructure::repositories::membership_repo::check_ticket_access(
        &state.pool,
        body.ticket_id,
        auth.user_id,
    )
    .await
    {
        Ok(true) => Ok(()),
        Ok(false) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "Ticket not found".to_string(),
            }),
        )
            .into_response()),
        Err(e) => return db_error(e),
    };
    let ticket_facts = match facts_repo::facts_for_ticket(&state.pool, body.ticket_id).await {
        Ok(f) => f,
        Err(e) => return db_error(e),
    };
    if let Err(resp) = authorize::gate(
        &state.pool,
        &viewer,
        ticket_facts.as_ref(),
        Action::Read,
        Resource::Ticket,
        body.ticket_id as i64,
        legacy,
        ROUTE,
    ) {
        return resp;
    }

    match wiki_api_repo::link_ticket(&state.pool, id, body.ticket_id).await {
        Ok(()) => (
            StatusCode::OK,
            Json(LinkActionOut {
                detail: "linked".to_string(),
                ticket_id: body.ticket_id,
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

/// POST /api/v1/wiki/{id}/unlink-ticket/
pub async fn unlink_ticket(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
    Json(body): Json<LinkTicketIn>,
) -> impl IntoResponse {
    let user_id = match viewer.require_user_id() {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    if let Err(resp) = page_gate(
        &state,
        &viewer,
        user_id,
        id,
        Action::Write,
        "POST /api/v1/wiki/{id}/unlink-ticket/",
    )
    .await
    {
        return resp;
    }
    match wiki_api_repo::unlink_ticket(&state.pool, id, body.ticket_id).await {
        Ok(()) => (
            StatusCode::OK,
            Json(LinkActionOut {
                detail: "unlinked".to_string(),
                ticket_id: body.ticket_id,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;

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

    async fn set_staff(pool: &sqlx::PgPool, user_id: i32) {
        sqlx::query("UPDATE accounts_user SET is_staff = true WHERE id = $1")
            .bind(user_id as i64)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn create_wiki_page(
        pool: &sqlx::PgPool,
        project: Option<i32>,
        team: Option<i32>,
        author_id: i32,
    ) -> i32 {
        let suffix = test_support::unique_suffix();
        let title = format!("wiki_{}", suffix);
        let slug = format!("wiki-{}", suffix);
        sqlx::query_scalar::<_, i32>(
            "INSERT INTO wiki_page (project_id, team_id, title, slug, category, content, author_id, last_editor_id, created_at, updated_at)
             VALUES ($1, $2, $3, $4, 'general', 'content', $5, $5, NOW(), NOW())
             RETURNING id::int4"
        )
        .bind(project.map(|p| p as i64))
        .bind(team.map(|t| t as i64))
        .bind(&title)
        .bind(&slug)
        .bind(author_id as i64)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn a1_authorize_page_write_non_member_returns_403() {
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let owner = test_support::create_test_user(&pool, "a1o").await;
        let outsider = test_support::create_test_user(&pool, "a1x").await;
        let project = test_support::create_test_project(&pool, "A1", owner).await;

        let page_id = create_wiki_page(&pool, Some(project), None, owner).await;

        let result = authorize_page_write(&pool, outsider, page_id, false).await;
        assert!(result.is_err(), "権限なしは Err を返すべき");
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
    async fn a2_authorize_page_write_team_member_returns_ok() {
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

        let page_id = create_wiki_page(&pool, Some(project), None, owner).await;

        let result = authorize_page_write(&pool, member, page_id, false).await;
        assert!(result.is_ok(), "参加チームメンバーは Ok を返すべき");
    }

    #[tokio::test]
    async fn a3_authorize_page_write_nonexistent_page_returns_404() {
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let user = test_support::create_test_user(&pool, "a3").await;

        let result = authorize_page_write(&pool, user, i32::MAX, false).await;
        assert!(result.is_err(), "存在しないページは Err を返すべき");
        if let Err(response) = result {
            let status_code = response.status();
            assert_eq!(
                status_code,
                StatusCode::NOT_FOUND,
                "404 Not Found が返されるべき"
            );
        }
    }

    #[tokio::test]
    async fn a4_delete_shared_page_non_author_non_staff_returns_403() {
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let author = test_support::create_test_user(&pool, "a4a").await;
        let other = test_support::create_test_user(&pool, "a4o").await;

        let page_id = create_wiki_page(&pool, None, None, author).await;

        let result = authorize_page_write(&pool, other, page_id, true).await;
        assert!(
            result.is_err(),
            "共有ページ削除は作成者でない非staff は Err を返すべき"
        );
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
    async fn a5_delete_shared_page_author_returns_ok() {
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let author = test_support::create_test_user(&pool, "a5a").await;

        let page_id = create_wiki_page(&pool, None, None, author).await;

        let result = authorize_page_write(&pool, author, page_id, true).await;
        assert!(result.is_ok(), "共有ページ削除は作成者は Ok を返すべき");
    }

    #[tokio::test]
    async fn a6_delete_shared_page_staff_returns_ok() {
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let author = test_support::create_test_user(&pool, "a6a").await;
        let staff = test_support::create_test_user(&pool, "a6s").await;
        set_staff(&pool, staff).await;

        let page_id = create_wiki_page(&pool, None, None, author).await;

        let result = authorize_page_write(&pool, staff, page_id, true).await;
        assert!(result.is_ok(), "共有ページ削除は staff は Ok を返すべき");
    }

    #[tokio::test]
    async fn a7_delete_project_page_team_member_returns_ok() {
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let owner = test_support::create_test_user(&pool, "a7o").await;
        let member = test_support::create_test_user(&pool, "a7m").await;
        let project = test_support::create_test_project(&pool, "A7", owner).await;
        let team = sqlx::query_scalar::<_, i32>(
            "SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1",
        )
        .bind(project as i64)
        .fetch_one(&pool)
        .await
        .unwrap();
        set_role(&pool, team, member, "member").await;

        let page_id = create_wiki_page(&pool, Some(project), None, owner).await;

        let result = authorize_page_write(&pool, member, page_id, true).await;
        assert!(
            result.is_ok(),
            "プロジェクト所属ページ削除は所属チームメンバーは Ok を返すべき"
        );
    }

    #[tokio::test]
    async fn a8_authorize_scope_write_non_member_project_returns_403() {
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let owner = test_support::create_test_user(&pool, "a8o").await;
        let outsider = test_support::create_test_user(&pool, "a8x").await;
        let project = test_support::create_test_project(&pool, "A8", owner).await;

        let result = authorize_scope_write(&pool, outsider, Some(project), None).await;
        assert!(result.is_err(), "権限なしプロジェクトは Err を返すべき");
        if let Err(response) = result {
            let status_code = response.status();
            assert_eq!(
                status_code,
                StatusCode::FORBIDDEN,
                "403 Forbidden が返されるべき"
            );
        }
    }
}
