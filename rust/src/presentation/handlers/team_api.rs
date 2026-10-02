/// presentation/handlers/team_api.rs — チーム JSON API ハンドラー
///
/// Teams (m_team) と Team Memberships (t_team_membership) の CRUD API
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::domain::access::Viewer;
use crate::domain::models::team_api::*;
use crate::infrastructure::repositories::team_repo;
use crate::presentation::handlers::access_guard;
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::presentation::state::AppState;

// =============================================================================
// リクエスト構造体
// =============================================================================

#[derive(Deserialize)]
pub struct TeamListQuery {
    pub page: Option<i64>,
}

// =============================================================================
// レスポンス構造体
// =============================================================================

#[derive(Serialize)]
pub struct PaginatedTeamsOut {
    pub count: i64,
    pub next: Option<String>,
    pub previous: Option<String>,
    pub results: Vec<TeamOut>,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub detail: String,
}

// =============================================================================
// Teams
// =============================================================================

/// チームの閲覧の判定(今の判定は無い = 許可)
async fn team_read_check(
    state: &AppState,
    viewer: &Viewer,
    team_id: i32,
    route: &'static str,
) -> Option<Result<(), axum::response::Response>> {
    let facts =
        match crate::infrastructure::access::facts_repo::facts_for_team(&state.pool, team_id).await
        {
            Ok(f) => f.map(crate::domain::access::ResourceRef::Team),
            Err(e) => {
                tracing::error!("facts_for_team failed: {:?}", e);
                return Some(Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response()));
            }
        };
    crate::presentation::extractors::authorize::enforce(
        &state.pool,
        viewer,
        facts.as_ref(),
        crate::domain::access::Action::Read,
        crate::infrastructure::access::shadow::Resource::Team,
        team_id as i64,
        true,
        route,
    )
}

/// チーム一覧 GET /api/v1/teams/
/// 閲覧者が Owner の操作(`Action::ManageOwners`)をできるチームの ID。画面のボタン表示に使う(DEMO-000170)。
/// 判定は API と同じ関数(`policy::can`)で行う。読み込みに失敗したら空(ボタンを出さない側に倒す)
async fn owner_operation_team_ids(
    pool: &sqlx::PgPool,
    viewer: &Viewer,
    team_ids: &[i32],
) -> std::collections::HashSet<i32> {
    use crate::domain::access::{can, Action, ResourceRef};
    match crate::infrastructure::access::facts_repo::facts_for_teams(pool, team_ids).await {
        Ok(facts) => facts
            .into_iter()
            .filter(|f| can(viewer, Action::ManageOwners, &ResourceRef::Team(*f)).is_allowed())
            .map(|f| f.team_id)
            .collect(),
        Err(e) => {
            tracing::error!("facts_for_teams failed: {:?}", e);
            std::collections::HashSet::new()
        }
    }
}

pub async fn team_list(
    State(state): State<AppState>,
    viewer: Viewer,
    Query(params): Query<TeamListQuery>,
) -> impl IntoResponse {
    let auth = AuthUser {
        user_id: match viewer.require_user_id() {
            Ok(id) => id,
            Err(resp) => return resp,
        },
    };
    let page = params.page.unwrap_or(1).max(1);
    const PAGE_SIZE: i64 = 50;

    // チーム一覧取得
    let teams = match team_repo::find_all_teams(&state.pool, page).await {
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
    // チームの一覧の絞り込み(アクセス制御の再設計 D-1。試運転のスイッチに従う)
    let mut teams = crate::presentation::extractors::authorize::filter_list(
        &state.pool,
        &viewer,
        crate::infrastructure::access::shadow::Resource::Team,
        "GET /api/v1/teams/",
        teams,
        |team| team.id as i64,
        |team| crate::domain::access::ResourceRef::Team(viewer.team_facts_for_read(team.id)),
    );

    // 閲覧者が管理できる(アーカイブ・復元できる)チームに印を付ける。
    // 画面が、押しても拒否されるボタンを出さないための情報(最終判定は各APIが行う)。
    // 取得に失敗しても一覧は返す(全て false = ボタンを出さない側に倒す)。
    let is_staff = matches!(
        crate::infrastructure::repositories::user_repo::find_by_id(&state.pool, auth.user_id).await,
        Ok(Some(u)) if u.is_staff
    );
    match crate::infrastructure::repositories::team_archive_repo::manageable_team_ids(
        &state.pool,
        auth.user_id,
        is_staff,
    )
    .await
    {
        Ok(ids) => {
            for t in teams.iter_mut() {
                t.viewer_can_manage = ids.contains(&t.id);
                access_guard::hide_webhook_unless_manager(t, t.viewer_can_manage);
            }
        }
        Err(e) => tracing::error!("manageable_team_ids failed: {:?}", e),
    }
    // 参加済みか(G-2。サイドバーは参加済みのチームだけを出す)
    for t in teams.iter_mut() {
        t.viewer_is_member = viewer.team_membership(t.id).is_some();
    }
    // Owner の操作ができるか(画面のボタン表示。判定の関数と同じ値。DEMO-000170)
    let ids: Vec<i32> = teams.iter().map(|t| t.id).collect();
    let owner_ops = owner_operation_team_ids(&state.pool, &viewer, &ids).await;
    for t in teams.iter_mut() {
        t.viewer_can_manage_owners = owner_ops.contains(&t.id);
    }

    // 件数取得
    let count = match team_repo::count_teams(&state.pool).await {
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
        Json(PaginatedTeamsOut {
            count,
            next,
            previous,
            results: teams,
        }),
    )
        .into_response()
}

/// チーム詳細 GET /api/v1/teams/{id}/
pub async fn team_detail(
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
    // チームの閲覧(アクセス制御の再設計 D-1。今は誰でも見られる)
    if let Some(Err(resp)) = team_read_check(&state, &viewer, id, "GET /api/v1/teams/{id}/").await {
        return resp;
    }
    match team_repo::find_team_by_id(&state.pool, id).await {
        Ok(Some(mut team)) => {
            // 判定に失敗したときは、管理者ではない側に倒す(秘密情報を出さない)
            let can_manage = access_guard::can_manage_team(&state.pool, auth.user_id, id)
                .await
                .unwrap_or(false);
            access_guard::hide_webhook_unless_manager(&mut team, can_manage);
            team.viewer_is_member = viewer.team_membership(id).is_some();
            team.viewer_can_manage = can_manage;
            team.viewer_can_manage_owners = owner_operation_team_ids(&state.pool, &viewer, &[id])
                .await
                .contains(&id);
            (StatusCode::OK, Json(team)).into_response()
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

/// チーム作成 POST /api/v1/teams/
pub async fn team_create(
    State(state): State<AppState>,
    viewer: Viewer,
    Json(body): Json<TeamWriteIn>,
) -> impl IntoResponse {
    let auth = AuthUser {
        user_id: match viewer.require_user_id() {
            Ok(id) => id,
            Err(resp) => return resp,
        },
    };
    // Guest・キー経由はチームを作れない(アクセス制御の再設計 D-1。試運転のスイッチに従う)
    {
        use crate::infrastructure::access::shadow::{self, Mode, Resource};
        let denied = viewer.role == crate::domain::access::Role::Guest || viewer.principal.is_key();
        match shadow::mode(Resource::Team) {
            Mode::On if denied => {
                return (
                    StatusCode::FORBIDDEN,
                    Json(ErrorResponse {
                        detail: "チームを作成する権限がありません".to_string(),
                    }),
                )
                    .into_response();
            }
            Mode::Shadow if denied => shadow::record(
                &state.pool,
                Resource::Team,
                viewer.user_id(),
                "POST /api/v1/teams/",
                vec![(shadow::Direction::NewlyHidden, 0)],
            ),
            _ => {}
        }
    }
    // 作成時の公開区分(G-2。省略時は public)
    let private = match body.visibility.as_deref() {
        None | Some("public") => false,
        Some("private") => true,
        Some(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    detail: "visibility は public か private を指定してください".to_string(),
                }),
            )
                .into_response();
        }
    };
    match team_repo::create_team(&state.pool, &body).await {
        Ok(team_id) => {
            if private {
                if let Err(e) =
                    sqlx::query("UPDATE m_team SET visibility = 'private' WHERE id = $1::int8")
                        .bind(team_id as i64)
                        .execute(&state.pool)
                        .await
                {
                    tracing::error!("Failed to set team visibility: {:?}", e);
                }
            }
            // 作成者を管理者としてチームメンバーに追加する。これを怠ると、
            // 作成者自身がそのチームのチケット一覧を一切閲覧できなくなる
            // (push_ticket_access_sqlはis_staffかチームメンバーのみ許可するため)。
            if let Err(e) =
                team_repo::add_team_member(&state.pool, team_id, auth.user_id, "admin").await
            {
                tracing::error!("Failed to add team creator as member: {:?}", e);
            }

            // 作成したチームを返す
            match team_repo::find_team_by_id(&state.pool, team_id).await {
                Ok(Some(mut team)) => {
                    // 作成者は Owner として所属している
                    team.viewer_is_member = true;
                    team.viewer_can_manage = true;
                    team.viewer_can_manage_owners = true;
                    (StatusCode::CREATED, Json(team)).into_response()
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
            let error_msg = e.to_string();
            tracing::error!("Create team failed: {:?}", e);
            if error_msg.contains("Prefix")
                || error_msg.contains("別のチームで使われています")
                || error_msg.contains("uq_m_team_prefix_upper")
                || error_msg.contains("duplicate key")
            {
                let detail = if error_msg.contains("uq_m_team_prefix_upper")
                    || error_msg.contains("duplicate key")
                {
                    "このPrefixは別のチームで使われています".to_string()
                } else {
                    error_msg
                };
                (StatusCode::BAD_REQUEST, Json(ErrorResponse { detail })).into_response()
            } else {
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

/// チーム更新 PUT /api/v1/teams/{id}/
pub async fn team_update(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
    Json(body): Json<TeamWriteIn>,
) -> impl IntoResponse {
    // 認可チェック: チーム管理者のみ
    if let Err(response) = access_guard::require_team_manager_v(
        &state.pool,
        &viewer,
        id,
        crate::domain::access::Action::ManageSettings,
        "PUT /api/v1/teams/{id}/",
    )
    .await
    {
        return response.into_response();
    }

    // アーカイブ済みのチームは閲覧専用(設定も固定)。変更するには、先に復元する
    if let Ok(true) =
        crate::infrastructure::repositories::team_archive_repo::is_archived(&state.pool, id).await
    {
        return (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                detail: "アーカイブ済みのチームは変更できません(先に復元してください)".to_string(),
            }),
        )
            .into_response();
    }
    match team_repo::update_team(&state.pool, id, &body).await {
        Ok(true) => {
            // 更新後のチームを返す
            match team_repo::find_team_by_id(&state.pool, id).await {
                Ok(Some(team)) => (StatusCode::OK, Json(team)).into_response(),
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
            let error_msg = e.to_string();
            tracing::error!("Update team failed: {:?}", e);
            if error_msg.contains("Prefix")
                || error_msg.contains("別のチームで使われています")
                || error_msg.contains("uq_m_team_prefix_upper")
                || error_msg.contains("duplicate key")
            {
                let detail = if error_msg.contains("uq_m_team_prefix_upper")
                    || error_msg.contains("duplicate key")
                {
                    "このPrefixは別のチームで使われています".to_string()
                } else {
                    error_msg
                };
                (StatusCode::BAD_REQUEST, Json(ErrorResponse { detail })).into_response()
            } else {
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

/// チーム削除 DELETE /api/v1/teams/{id}/
pub async fn team_delete(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    // 認可チェック: チーム管理者のみ
    if let Err(response) = access_guard::require_team_manager_v(
        &state.pool,
        &viewer,
        id,
        // チームの削除は Owner の操作(設計書 §4.3)
        crate::domain::access::Action::Delete,
        "DELETE /api/v1/teams/{id}/",
    )
    .await
    {
        return response.into_response();
    }
    // 削除は Owner の操作。今の判定の救済規則(管理者のいないチームはメンバーが管理できる)は及ばないので、
    // 試運転を通さず、すぐに適用する(設計書 §4.3。DEMO-000169)
    if let Err(response) = access_guard::require_owner_operation(&state.pool, &viewer, id).await {
        return response;
    }

    match team_repo::delete_team(&state.pool, id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "見つかりません".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            if let Some(dep) = e.downcast_ref::<team_repo::TeamHasDependents>() {
                // 何が何件残っているかを伝える(例: 「プロジェクト2件が残っているため…」)
                (
                    StatusCode::CONFLICT,
                    Json(ErrorResponse {
                        detail: dep.user_message(),
                    }),
                )
                    .into_response()
            } else {
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
}

// =============================================================================
// Team Members
// =============================================================================

/// メンバー一覧 GET /api/v1/teams/{id}/members/
pub async fn team_members_list(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(team_id): Path<i32>,
) -> impl IntoResponse {
    // チームの閲覧(アクセス制御の再設計 D-1。今は誰でも見られる)
    if let Some(Err(resp)) =
        team_read_check(&state, &viewer, team_id, "GET /api/v1/teams/{id}/members/").await
    {
        return resp;
    }
    match team_repo::find_team_members(&state.pool, team_id).await {
        Ok(members) => (StatusCode::OK, Json(members)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(Vec::<TeamMembershipOut>::new()),
            )
                .into_response()
        }
    }
}

/// メンバー追加 POST /api/v1/teams/{id}/members/
pub async fn team_members_add(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(team_id): Path<i32>,
    Json(body): Json<TeamMembershipCreateIn>,
) -> impl IntoResponse {
    // 認可チェック: チーム管理者のみ
    if let Err(response) = access_guard::require_team_manager_v(
        &state.pool,
        &viewer,
        team_id,
        crate::domain::access::Action::ManageMembers,
        "POST /api/v1/teams/{id}/members/",
    )
    .await
    {
        return response.into_response();
    }

    // ユーザー存在チェック
    match team_repo::check_user_exists(&state.pool, body.user_id).await {
        Ok(false) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    detail: "指定されたユーザーが見つかりません".to_string(),
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
        Ok(true) => {}
    }

    // 重複チェック
    match team_repo::check_team_membership_exists(&state.pool, team_id, body.user_id).await {
        Ok(true) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    detail: "このユーザーは既にチームに属しています".to_string(),
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
        Ok(false) => {}
    }

    // Role 正規化（admin / member のみ。leader → admin）
    let Some(role) = crate::domain::models::team_api::normalize_team_role(&body.role) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                detail: "role は admin または member のみです".to_string(),
            }),
        )
            .into_response();
    };

    // Owner(admin)を付ける・Guest を入れるのは、Owner とシステム管理者だけ(試運転を通さず、すぐに適用。
    // 設計書 §4.3・詳細設計書 §5.4。DEMO-000169)。Guest は Owner にできない
    let target_is_guest: bool =
        match sqlx::query_scalar("SELECT is_guest FROM accounts_user WHERE id = $1::int8")
            .bind(body.user_id as i64)
            .fetch_one(&state.pool)
            .await
        {
            Ok(g) => g,
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
    if role == "admin" && target_is_guest {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                detail: "Guest は Owner にできません".to_string(),
            }),
        )
            .into_response();
    }
    if role == "admin" || target_is_guest {
        if let Err(response) =
            access_guard::require_owner_operation(&state.pool, &viewer, team_id).await
        {
            return response;
        }
    }

    // メンバー追加
    match team_repo::add_team_member(&state.pool, team_id, body.user_id, role).await {
        Ok(membership_id) => {
            // 追加したメンバーシップを返す
            match team_repo::get_team_member_by_id(&state.pool, membership_id).await {
                Ok(Some(member)) => (StatusCode::CREATED, Json(member)).into_response(),
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

/// メンバー削除 DELETE /api/v1/teams/{team_id}/members/{user_id}/
/// メンバーの削除で、対象が Owner のときの確認(Owner の解除と同じ規則)
async fn guard_owner_removal(
    pool: &sqlx::PgPool,
    viewer: &Viewer,
    team_id: i32,
    target: i32,
    is_self: bool,
) -> Result<(), axum::response::Response> {
    let server_error = |e: &dyn std::fmt::Debug| {
        tracing::error!("DB operation failed: {:?}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                detail: "サーバーエラーが発生しました".to_string(),
            }),
        )
            .into_response()
    };
    let is_owner: Option<bool> = sqlx::query_scalar(
        "SELECT role = 'admin' FROM t_team_membership
         WHERE team_id = $1::int8 AND user_id = $2::int8 AND scoped_project_id IS NULL",
    )
    .bind(team_id as i64)
    .bind(target as i64)
    .fetch_optional(pool)
    .await
    .map_err(|e| server_error(&e))?;
    if is_owner != Some(true) {
        return Ok(());
    }
    if !is_self {
        access_guard::require_owner_operation(pool, viewer, team_id).await?;
    }
    let facts = crate::infrastructure::access::facts_repo::facts_for_team(pool, team_id)
        .await
        .map_err(|e| server_error(&e))?;
    if let Some(f) = facts {
        if f.settings_policy == crate::domain::access::SettingsPolicy::Owners && f.owner_count <= 1
        {
            return Err((
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "detail": "最後の Owner は外せません。先に別の人を Owner にしてください",
                    "code": "last_owner",
                })),
            )
                .into_response());
        }
    }
    Ok(())
}

pub async fn team_members_remove(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((team_id, user_id)): Path<(i32, i32)>,
) -> impl IntoResponse {
    let auth = AuthUser {
        user_id: match viewer.require_user_id() {
            Ok(id) => id,
            Err(resp) => return resp,
        },
    };
    // 管理者は誰でも外せる。管理者でなければ、自分自身がチームを抜ける場合だけ許可する。
    if auth.user_id != user_id {
        if let Err(response) = access_guard::require_team_manager_v(
            &state.pool,
            &viewer,
            team_id,
            crate::domain::access::Action::ManageMembers,
            "DELETE /api/v1/teams/{id}/members/{user_id}/",
        )
        .await
        {
            return response;
        }
    }

    // Owner を外す: 他人なら Owner とシステム管理者だけ(すぐに適用)。owners の方針で最後の Owner は外せない
    // (`team_access_api::leave` と同じ条件。DEMO-000169)
    if let Err(response) = guard_owner_removal(
        &state.pool,
        &viewer,
        team_id,
        user_id,
        auth.user_id == user_id,
    )
    .await
    {
        return response;
    }

    match team_repo::remove_team_member(&state.pool, team_id, user_id).await {
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
// L2: Project ゲスト(scoped_project_id 付き t_team_membership)
// =============================================================================

/// Projectゲスト一覧 GET /api/v1/teams/{id}/guests/
pub async fn team_guests_list(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(team_id): Path<i32>,
) -> impl IntoResponse {
    // チームの閲覧(アクセス制御の再設計 D-1。今は誰でも見られる)
    if let Some(Err(resp)) =
        team_read_check(&state, &viewer, team_id, "GET /api/v1/teams/{id}/guests/").await
    {
        return resp;
    }
    match team_repo::find_team_guests(&state.pool, team_id).await {
        Ok(guests) => (StatusCode::OK, Json(guests)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(Vec::<TeamGuestOut>::new()),
            )
                .into_response()
        }
    }
}

/// Projectゲスト追加 POST /api/v1/teams/{id}/guests/
pub async fn team_guests_add(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(team_id): Path<i32>,
    Json(body): Json<TeamGuestCreateIn>,
) -> impl IntoResponse {
    // 認可チェック: チーム管理者のみ
    if let Err(response) = access_guard::require_team_manager_v(
        &state.pool,
        &viewer,
        team_id,
        // Guest の追加は Owner の操作(設計書 §4.3。Linear: 方針にかかわらず Owner だけ)
        crate::domain::access::Action::Invite,
        "POST /api/v1/teams/{id}/guests/",
    )
    .await
    {
        return response.into_response();
    }
    // 今の判定の救済規則は、Guest の追加には及ばない(すぐに適用。DEMO-000169)
    if let Err(response) =
        access_guard::require_owner_operation(&state.pool, &viewer, team_id).await
    {
        return response;
    }

    // ユーザー存在チェック
    match team_repo::check_user_exists(&state.pool, body.user_id).await {
        Ok(false) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    detail: "指定されたユーザーが見つかりません".to_string(),
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
        Ok(true) => {}
    }

    // 指定Projectがこのチームの所有Projectであることを確認
    match team_repo::project_belongs_to_team(&state.pool, body.project_id, team_id).await {
        Ok(false) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    detail: "指定されたProjectはこのチームの所有ではありません".to_string(),
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
        Ok(true) => {}
    }

    // 既にチーム全体のメンバーなら、限定ゲストとして追加する意味が無い
    match team_repo::check_team_membership_exists(&state.pool, team_id, body.user_id).await {
        Ok(true) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    detail: "このユーザーは既にチーム全体のメンバーです(ゲスト追加は不要です)"
                        .to_string(),
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
        Ok(false) => {}
    }

    // 重複チェック(同じProjectへの重複ゲスト登録)
    match team_repo::check_team_guest_exists(&state.pool, team_id, body.user_id, body.project_id)
        .await
    {
        Ok(true) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    detail: "このユーザーは既にこのProjectのゲストです".to_string(),
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
        Ok(false) => {}
    }

    match team_repo::add_team_guest(
        &state.pool,
        team_id,
        body.user_id,
        body.project_id,
        body.end_date,
    )
    .await
    {
        Ok(_membership_id) => match team_repo::find_team_guests(&state.pool, team_id).await {
            Ok(guests) => (StatusCode::CREATED, Json(guests)).into_response(),
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

/// Projectゲスト削除 DELETE /api/v1/teams/{team_id}/guests/{membership_id}/
pub async fn team_guests_remove(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((team_id, membership_id)): Path<(i32, i32)>,
) -> impl IntoResponse {
    // 認可チェック: チーム管理者のみ
    if let Err(response) = access_guard::require_team_manager_v(
        &state.pool,
        &viewer,
        team_id,
        crate::domain::access::Action::ManageMembers,
        "DELETE /api/v1/teams/{id}/guests/{id}/",
    )
    .await
    {
        return response.into_response();
    }

    match team_repo::remove_team_guest(&state.pool, team_id, membership_id).await {
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

/// メンバーの API の Owner・Guest の確認(DEMO-000169)
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        add_test_team_member, create_test_team, create_test_user, test_pool, test_state,
        test_viewer,
    };
    use sqlx::PgPool;

    async fn role_of(pool: &PgPool, team: i32, user: i32) -> Option<String> {
        sqlx::query_scalar(
            "SELECT role FROM t_team_membership WHERE team_id = $1::int8 AND user_id = $2::int8 AND scoped_project_id IS NULL",
        )
        .bind(team as i64)
        .bind(user as i64)
        .fetch_optional(pool)
        .await
        .unwrap()
    }

    async fn add(pool: &PgPool, as_user: i32, team: i32, target: i32, role: &str) -> StatusCode {
        team_members_add(
            State(test_state(pool).await),
            test_viewer(pool, as_user).await,
            Path(team),
            Json(TeamMembershipCreateIn {
                user_id: target,
                role: role.to_string(),
            }),
        )
        .await
        .into_response()
        .status()
    }

    async fn remove(pool: &PgPool, as_user: i32, team: i32, target: i32) -> StatusCode {
        team_members_remove(
            State(test_state(pool).await),
            test_viewer(pool, as_user).await,
            Path((team, target)),
        )
        .await
        .into_response()
        .status()
    }

    async fn make_guest(pool: &PgPool, user: i32) {
        sqlx::query("UPDATE accounts_user SET is_guest = true WHERE id = $1::int8")
            .bind(user as i64)
            .execute(pool)
            .await
            .unwrap();
    }

    /// 管理者のいないチームで、メンバーが、自分や他人を admin(Owner)として追加できない
    /// (今の判定の救済規則は、Owner の指名には及ばない)
    #[tokio::test]
    async fn member_of_ownerless_team_cannot_grant_owner() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let team = create_test_team(&pool, "tm-none").await;
        let m = create_test_user(&pool, "tm-m").await;
        let friend = create_test_user(&pool, "tm-f").await;
        add_test_team_member(&pool, team, m, "member").await;

        assert_eq!(
            add(&pool, m, team, friend, "admin").await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(role_of(&pool, team, friend).await, None);
        // Owner 以外の追加は、今までどおり(救済規則)
        assert_eq!(
            add(&pool, m, team, friend, "member").await,
            StatusCode::CREATED
        );
    }

    /// 管理者のいないチームでも、メンバーでない人は管理できない(今の判定の救済規則を、メンバーに限る)
    #[tokio::test]
    async fn outsider_cannot_manage_ownerless_team() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let team = create_test_team(&pool, "tm-out").await;
        let m = create_test_user(&pool, "tm-m").await;
        let outsider = create_test_user(&pool, "tm-o").await;
        add_test_team_member(&pool, team, m, "member").await;

        assert_eq!(
            add(&pool, outsider, team, outsider, "admin").await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            add(&pool, outsider, team, outsider, "member").await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(role_of(&pool, team, outsider).await, None);
    }

    /// Guest は admin(Owner)にできない。Guest の追加は Owner だけ
    #[tokio::test]
    async fn guest_cannot_be_owner() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let team = create_test_team(&pool, "tm-g").await;
        let owner = create_test_user(&pool, "tm-o").await;
        let guest = create_test_user(&pool, "tm-g").await;
        add_test_team_member(&pool, team, owner, "admin").await;
        make_guest(&pool, guest).await;

        assert_eq!(
            add(&pool, owner, team, guest, "admin").await,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(role_of(&pool, team, guest).await, None);
    }

    /// 新しい規則に切り替えた後(members の方針ではメンバーも ManageMembers が通る)も、
    /// Owner でない人は、他人の Owner を外せない(今は試運転のため、今の判定が先に止める)
    #[tokio::test]
    async fn member_cannot_remove_other_owner_even_under_new_rules() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let team = create_test_team(&pool, "tm-rm").await;
        let owner = create_test_user(&pool, "tm-o").await;
        let m = create_test_user(&pool, "tm-m").await;
        add_test_team_member(&pool, team, owner, "admin").await;
        add_test_team_member(&pool, team, m, "member").await;
        let viewer = test_viewer(&pool, m).await;

        let denied = guard_owner_removal(&pool, &viewer, team, owner, false).await;
        assert_eq!(
            denied.err().map(|r| r.status()),
            Some(StatusCode::FORBIDDEN)
        );
        // Owner でない人を外すときは、この確認は関係しない
        assert!(guard_owner_removal(&pool, &viewer, team, m, true)
            .await
            .is_ok());
    }

    /// Owner のいないチームでも、メンバーはチームを削除できない(削除は Owner の操作。今の判定の救済規則は及ばない)
    #[tokio::test]
    async fn member_of_ownerless_team_cannot_delete_team() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let team = create_test_team(&pool, "tm-del").await;
        let m = create_test_user(&pool, "tm-m").await;
        add_test_team_member(&pool, team, m, "member").await;
        let sa = create_test_user(&pool, "tm-sa").await;
        sqlx::query("UPDATE accounts_user SET is_staff = true WHERE id = $1::int8")
            .bind(sa as i64)
            .execute(&pool)
            .await
            .unwrap();

        let delete = |user: i32| {
            let pool = pool.clone();
            async move {
                team_delete(
                    State(test_state(&pool).await),
                    test_viewer(&pool, user).await,
                    Path(team),
                )
                .await
                .into_response()
                .status()
            }
        };
        assert_eq!(delete(m).await, StatusCode::FORBIDDEN);
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM m_team WHERE id = $1::int8)")
                .bind(team as i64)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(exists, "チームは残っている");
        assert_eq!(delete(sa).await, StatusCode::NO_CONTENT);
    }

    /// Owner のいないチームでも、メンバーはプロジェクトの Guest を追加できない(Guest の追加は Owner の操作)
    #[tokio::test]
    async fn member_of_ownerless_team_cannot_add_project_guest() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let m = create_test_user(&pool, "tm-m").await;
        let target = create_test_user(&pool, "tm-g").await;
        let project = crate::test_support::create_test_project(&pool, "TMG", m).await;
        let team: i32 = sqlx::query_scalar(
            "SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1::int8",
        )
        .bind(project as i64)
        .fetch_one(&pool)
        .await
        .unwrap();
        add_test_team_member(&pool, team, m, "member").await;

        let status = team_guests_add(
            State(test_state(&pool).await),
            test_viewer(&pool, m).await,
            Path(team),
            Json(TeamGuestCreateIn {
                user_id: target,
                project_id: project,
                end_date: None,
            }),
        )
        .await
        .into_response()
        .status();
        assert_eq!(status, StatusCode::FORBIDDEN);
        let added: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM t_team_membership WHERE team_id = $1::int8 AND user_id = $2::int8",
        )
        .bind(team as i64)
        .bind(target as i64)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(added, 0);
    }

    /// 画面のボタン表示(viewerCanManageOwners)と、API の判定(Owner の指名)が一致する(DEMO-000170)。
    /// 一覧・詳細のどちらの応答でも、役割ごとに「出る ⇔ 実際に通る」
    #[tokio::test]
    async fn viewer_can_manage_owners_matches_the_api() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let team = create_test_team(&pool, "tm-flag").await;
        let owner = create_test_user(&pool, "tm-fo").await;
        let member = create_test_user(&pool, "tm-fm").await;
        let outsider = create_test_user(&pool, "tm-fx").await;
        let sa = create_test_user(&pool, "tm-fsa").await;
        add_test_team_member(&pool, team, owner, "admin").await;
        add_test_team_member(&pool, team, member, "member").await;
        sqlx::query("UPDATE accounts_user SET is_staff = true WHERE id = $1::int8")
            .bind(sa as i64)
            .execute(&pool)
            .await
            .unwrap();
        // Owner のいないチーム(古い規則では、メンバーが管理できる)
        let ownerless = create_test_team(&pool, "tm-flag0").await;
        add_test_team_member(&pool, ownerless, member, "member").await;

        async fn flag_from_detail(pool: &PgPool, user: i32, team: i32) -> bool {
            let resp = team_detail(
                State(test_state(pool).await),
                test_viewer(pool, user).await,
                Path(team),
            )
            .await
            .into_response();
            let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap();
            let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
            v["viewerCanManageOwners"]
                .as_bool()
                .expect("viewerCanManageOwners が無い")
        }
        async fn flag_from_list(pool: &PgPool, user: i32, team: i32) -> bool {
            // 一覧はページ分けされ、テスト用 DB には他のテストのチームもあるので、見つかるまでページを進める
            for page in 1..=1000 {
                let resp = team_list(
                    State(test_state(pool).await),
                    test_viewer(pool, user).await,
                    axum::extract::Query(TeamListQuery { page: Some(page) }),
                )
                .await
                .into_response();
                let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
                    .await
                    .unwrap();
                let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
                let rows = v["results"].as_array().cloned().unwrap_or_default();
                if rows.is_empty() {
                    break;
                }
                if let Some(r) = rows.iter().find(|r| r["id"] == serde_json::json!(team)) {
                    return r["viewerCanManageOwners"]
                        .as_bool()
                        .expect("viewerCanManageOwners が無い");
                }
            }
            panic!("チーム一覧に、対象のチームが見つからない");
        }
        async fn api_allows(pool: &PgPool, user: i32, team: i32, target: i32) -> bool {
            // 指名の可否だけを見る(通ったら元に戻す)
            let status = crate::presentation::handlers::team_access_api::add_owner(
                State(test_state(pool).await),
                test_viewer(pool, user).await,
                Path((team, target)),
            )
            .await
            .status();
            if status == StatusCode::NO_CONTENT {
                sqlx::query("UPDATE t_team_membership SET role = 'member' WHERE team_id = $1::int8 AND user_id = $2::int8")
                    .bind(team as i64)
                    .bind(target as i64)
                    .execute(pool)
                    .await
                    .unwrap();
            }
            status == StatusCode::NO_CONTENT
        }

        for (who, user, t, expected) in [
            ("Owner", owner, team, true),
            ("メンバー", member, team, false),
            ("メンバーでない人", outsider, team, false),
            ("システム管理者", sa, team, true),
            ("Owner のいないチームのメンバー", member, ownerless, false),
            ("Owner のいないチームのシステム管理者", sa, ownerless, true),
        ] {
            let api = api_allows(&pool, user, t, member).await;
            assert_eq!(api, expected, "{who}: API の判定が想定と違う");
            assert_eq!(
                flag_from_detail(&pool, user, t).await,
                api,
                "{who}: 詳細の表示と API が食い違う"
            );
            assert_eq!(
                flag_from_list(&pool, user, t).await,
                api,
                "{who}: 一覧の表示と API が食い違う"
            );
        }
    }

    /// 最後の Owner は、メンバーの API からも外せない(leave と同じ: owners の方針のとき)
    #[tokio::test]
    async fn last_owner_cannot_be_removed() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let team = create_test_team(&pool, "tm-last").await;
        sqlx::query("UPDATE m_team SET settings_policy = 'owners' WHERE id = $1::int8")
            .bind(team as i64)
            .execute(&pool)
            .await
            .unwrap();
        let owner = create_test_user(&pool, "tm-o").await;
        let second = create_test_user(&pool, "tm-2").await;
        add_test_team_member(&pool, team, owner, "admin").await;

        assert_eq!(
            remove(&pool, owner, team, owner).await,
            StatusCode::CONFLICT
        );
        assert_eq!(role_of(&pool, team, owner).await.as_deref(), Some("admin"));

        // Owner が 2 人なら外せる
        add_test_team_member(&pool, team, second, "admin").await;
        assert_eq!(
            remove(&pool, second, team, owner).await,
            StatusCode::NO_CONTENT
        );
        assert_eq!(role_of(&pool, team, owner).await, None);
    }
}
