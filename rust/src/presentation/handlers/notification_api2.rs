/// presentation/handlers/notification_api2.rs — JSON 通知 API
///
/// Djangoの /api/v1/notifications/* と挙動を一致させるハンドラー。

use axum::{
    extract::{State, Path},
    response::IntoResponse,
    http::StatusCode,
    Json,
    Extension,
};
use serde::Serialize;

use crate::presentation::state::AppState;
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::infrastructure::repositories::notification_repo2;

#[derive(Serialize)]
pub struct StatusResponse {
    pub status: String,
}

#[derive(Serialize)]
pub struct MarkedReadResponse {
    pub marked_read: i64,
}

#[derive(Serialize)]
pub struct UnreadCountResponse {
    pub count: i64,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub detail: String,
}

/// GET /api/v1/notifications/ — 通知一覧
pub async fn list(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
) -> impl IntoResponse {
    match notification_repo2::find_all_for_user(&state.pool, auth.user_id).await {
        Ok(notifications) => (StatusCode::OK, Json(notifications)).into_response(),
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

/// POST /api/v1/notifications/{id}/read/ — 通知を既読に
pub async fn mark_read(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match notification_repo2::mark_read(&state.pool, id, auth.user_id).await {
        Ok(true) => (StatusCode::OK, Json(StatusResponse { status: "read".to_string() })).into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "通知が見つかりません".to_string(),
            }),
        ).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            ).into_response()
        }
    }
}

/// POST /api/v1/notifications/read_all/ — 全通知を既読に
pub async fn mark_all_read(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
) -> impl IntoResponse {
    match notification_repo2::mark_all_read(&state.pool, auth.user_id).await {
        Ok(count) => (StatusCode::OK, Json(MarkedReadResponse { marked_read: count })).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            ).into_response()
        }
    }
}

/// GET /api/v1/notifications/unread_count/ — 未読数
pub async fn unread_count(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
) -> impl IntoResponse {
    match notification_repo2::unread_count(&state.pool, auth.user_id).await {
        Ok(count) => (StatusCode::OK, Json(UnreadCountResponse { count })).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            ).into_response()
        }
    }
}
