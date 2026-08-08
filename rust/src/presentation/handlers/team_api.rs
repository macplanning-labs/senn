/// presentation/handlers/team_api.rs — チーム JSON API ハンドラー
///
/// Teams (m_team) と Team Memberships (t_team_membership) の CRUD API

use axum::{
    extract::{State, Path, Query},
    response::IntoResponse,
    http::StatusCode,
    Json,
    Extension,
};
use serde::{Deserialize, Serialize};

use crate::presentation::state::AppState;
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::infrastructure::repositories::team_repo;
use crate::domain::models::team_api::*;

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

/// チーム一覧 GET /api/v1/teams/
pub async fn team_list(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Query(params): Query<TeamListQuery>,
) -> impl IntoResponse {
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
    let next = if has_next { Some(format!("?page={}", page + 1)) } else { None };
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
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match team_repo::find_team_by_id(&state.pool, id).await {
        Ok(Some(team)) => (StatusCode::OK, Json(team)).into_response(),
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
    Extension(_auth): Extension<AuthUser>,
    Json(body): Json<TeamWriteIn>,
) -> impl IntoResponse {
    match team_repo::create_team(&state.pool, &body).await {
        Ok(team_id) => {
            // 作成したチームを返す
            match team_repo::find_team_by_id(&state.pool, team_id).await {
                Ok(Some(team)) => (StatusCode::CREATED, Json(team)).into_response(),
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

/// チーム更新 PUT /api/v1/teams/{id}/
pub async fn team_update(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
    Json(body): Json<TeamWriteIn>,
) -> impl IntoResponse {
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

/// チーム削除 DELETE /api/v1/teams/{id}/
pub async fn team_delete(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
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
// Team Members
// =============================================================================

/// メンバー一覧 GET /api/v1/teams/{id}/members/
pub async fn team_members_list(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(team_id): Path<i32>,
) -> impl IntoResponse {
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
    Extension(_auth): Extension<AuthUser>,
    Path(team_id): Path<i32>,
    Json(body): Json<TeamMembershipCreateIn>,
) -> impl IntoResponse {
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

    // メンバー追加
    match team_repo::add_team_member(&state.pool, team_id, body.user_id, &body.role).await {
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
pub async fn team_members_remove(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path((team_id, user_id)): Path<(i32, i32)>,
) -> impl IntoResponse {
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
