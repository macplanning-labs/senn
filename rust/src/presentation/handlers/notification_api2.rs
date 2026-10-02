/// presentation/handlers/notification_api2.rs — JSON 通知 API
///
/// Djangoの /api/v1/notifications/* と挙動を一致させるハンドラー。
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Serialize;

use crate::domain::access::Viewer;
use crate::domain::models::notification_api::NotificationOut;
use crate::infrastructure::access::shadow;
use crate::infrastructure::repositories::notification_repo2;
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::presentation::state::AppState;

#[derive(Serialize)]
pub struct NotificationListOut {
    pub results: Vec<NotificationOut>,
}

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
pub struct DismissedResponse {
    pub dismissed: i64,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub detail: String,
}

/// 試運転: 新しい規則なら一覧から外れる通知の参照先(チケット)を記録する
async fn record_hidden_notifications(
    state: &AppState,
    user_id: i32,
    scope: &crate::domain::access::Scope,
) {
    let all = notification_repo2::find_all_for_user(&state.pool, user_id, None).await;
    let visible = notification_repo2::find_all_for_user(&state.pool, user_id, Some(scope)).await;
    let (Ok(all), Ok(visible)) = (all, visible) else {
        tracing::warn!("[認可/試運転] 通知の差分の計算に失敗(操作は続行)");
        return;
    };
    let keep: std::collections::HashSet<i32> = visible.iter().map(|n| n.id).collect();
    let diffs = all
        .iter()
        .filter(|n| !keep.contains(&n.id))
        .filter_map(|n| n.ticket.map(|t| (shadow::Direction::NewlyHidden, t as i64)))
        .collect();
    shadow::record(
        &state.pool,
        shadow::Resource::Ticket,
        Some(user_id),
        "GET /api/v1/notifications/",
        diffs,
    );
}

/// GET /api/v1/notifications/ — 通知一覧
pub async fn list(State(state): State<AppState>, viewer: Viewer) -> impl IntoResponse {
    let user_id = match viewer.require_user_id() {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    // 参照先が見える通知だけに絞るか(アクセス制御の再設計 C-5。試運転のスイッチに従う)
    let scope = viewer.scope();
    let use_scope = match shadow::mode(shadow::Resource::Ticket) {
        shadow::Mode::On => Some(&scope),
        shadow::Mode::Shadow => {
            record_hidden_notifications(&state, user_id, &scope).await;
            None
        }
        shadow::Mode::Off => None,
    };
    match notification_repo2::find_all_for_user(&state.pool, user_id, use_scope).await {
        Ok(notifications) => (
            StatusCode::OK,
            Json(NotificationListOut {
                results: notifications,
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

/// POST /api/v1/notifications/{id}/read/ — 通知を既読に
pub async fn mark_read(
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
    match notification_repo2::mark_read(&state.pool, id, auth.user_id).await {
        Ok(true) => (
            StatusCode::OK,
            Json(StatusResponse {
                status: "read".to_string(),
            }),
        )
            .into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "通知が見つかりません".to_string(),
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

/// POST /api/v1/notifications/read_all/ — 全通知を既読に
pub async fn mark_all_read(State(state): State<AppState>, viewer: Viewer) -> impl IntoResponse {
    let auth = AuthUser {
        user_id: match viewer.require_user_id() {
            Ok(id) => id,
            Err(resp) => return resp,
        },
    };
    match notification_repo2::mark_all_read(&state.pool, auth.user_id).await {
        Ok(count) => (
            StatusCode::OK,
            Json(MarkedReadResponse { marked_read: count }),
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

/// GET /api/v1/notifications/unread_count/ — 未読数
pub async fn unread_count(State(state): State<AppState>, viewer: Viewer) -> impl IntoResponse {
    let user_id = match viewer.require_user_id() {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    let scope = viewer.scope();
    let use_scope = (shadow::mode(shadow::Resource::Ticket) == shadow::Mode::On).then_some(&scope);
    match notification_repo2::unread_count(&state.pool, user_id, use_scope).await {
        Ok(count) => (StatusCode::OK, Json(UnreadCountResponse { count })).into_response(),
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

/// DELETE /api/v1/notifications/{id}/ — 通知をdismiss
pub async fn dismiss(
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
    match notification_repo2::dismiss(&state.pool, id, auth.user_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "通知が見つかりません".to_string(),
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

/// POST /api/v1/notifications/dismiss_read/ — 既読通知を全てdismiss
pub async fn dismiss_all_read(State(state): State<AppState>, viewer: Viewer) -> impl IntoResponse {
    let auth = AuthUser {
        user_id: match viewer.require_user_id() {
            Ok(id) => id,
            Err(resp) => return resp,
        },
    };
    match notification_repo2::dismiss_all_read(&state.pool, auth.user_id).await {
        Ok(count) => (
            StatusCode::OK,
            Json(DismissedResponse {
                dismissed: count as i64,
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
