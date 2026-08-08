/// presentation/handlers/membership_api.rs — プロジェクトメンバーシップ JSON API ハンドラー
///
/// tickets_project_membership テーブル の CRUD API

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
use crate::infrastructure::repositories::membership_repo;
use crate::domain::models::membership_api::*;

// =============================================================================
// リクエスト構造体
// =============================================================================

#[derive(Deserialize)]
pub struct MembershipListQuery {
    pub project: Option<i32>,
    pub page: Option<i64>,
}

// =============================================================================
// レスポンス構造体
// =============================================================================

#[derive(Serialize)]
pub struct PaginatedMembershipsOut {
    pub count: i64,
    pub next: Option<String>,
    pub previous: Option<String>,
    pub results: Vec<MembershipOut>,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub detail: String,
}

// =============================================================================
// Memberships
// =============================================================================

/// メンバーシップ一覧 GET /api/v1/memberships/
pub async fn membership_list(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Query(params): Query<MembershipListQuery>,
) -> impl IntoResponse {
    let page = params.page.unwrap_or(1).max(1);
    const PAGE_SIZE: i64 = 50;

    // メンバーシップ一覧取得
    let memberships = match membership_repo::find_all_memberships(&state.pool, params.project, page).await {
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
    let count = match membership_repo::count_memberships(&state.pool, params.project).await {
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
        Json(PaginatedMembershipsOut {
            count,
            next,
            previous,
            results: memberships,
        }),
    )
        .into_response()
}

/// メンバーシップ詳細 GET /api/v1/memberships/{id}/
pub async fn membership_detail(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match membership_repo::find_membership_by_id(&state.pool, id).await {
        Ok(Some(membership)) => (StatusCode::OK, Json(membership)).into_response(),
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

/// メンバーシップ作成 POST /api/v1/memberships/
pub async fn membership_create(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<MembershipCreateIn>,
) -> impl IntoResponse {
    // (project, user) の重複チェック
    match membership_repo::check_membership_exists(&state.pool, body.project, body.user_id).await {
        Ok(true) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    detail: "このユーザーは既にプロジェクトに属しています".to_string(),
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

    // メンバーシップ作成
    match membership_repo::create_membership(&state.pool, &body, auth.user_id).await {
        Ok(membership_id) => {
            // 作成したメンバーシップを返す
            match membership_repo::find_membership_by_id(&state.pool, membership_id).await {
                Ok(Some(membership)) => (StatusCode::CREATED, Json(membership)).into_response(),
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

/// メンバーシップ更新 PATCH /api/v1/memberships/{id}/
pub async fn membership_update(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
    Json(body): Json<MembershipUpdateIn>,
) -> impl IntoResponse {
    match membership_repo::update_membership(&state.pool, id, &body).await {
        Ok(true) => {
            // 更新後のメンバーシップを返す
            match membership_repo::find_membership_by_id(&state.pool, id).await {
                Ok(Some(membership)) => (StatusCode::OK, Json(membership)).into_response(),
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

/// メンバーシップ削除 DELETE /api/v1/memberships/{id}/
pub async fn membership_delete(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    // 削除対象の user_id を取得
    let membership_user_id = match membership_repo::get_membership_user_id(&state.pool, id).await {
        Ok(Some(uid)) => uid,
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
    };

    // 自分自身を削除できないようにチェック
    if membership_user_id == auth.user_id {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                detail: "自分自身を削除することはできません".to_string(),
            }),
        )
            .into_response();
    }

    // メンバーシップ削除
    match membership_repo::delete_membership(&state.pool, id).await {
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
