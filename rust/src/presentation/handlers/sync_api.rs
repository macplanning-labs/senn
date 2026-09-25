//! Sync API handlers for local-first synchronization

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use serde::Deserialize;

use crate::{
    domain::models::sync_api::{SyncCursor, SyncError},
    infrastructure::repositories::sync_repo,
    presentation::middleware::jwt_auth::AuthUser,
    AppState,
};

#[derive(Deserialize)]
pub struct SyncQuery {
    cursor: Option<String>,
    limit: Option<i64>,
}

/// GET /api/v1/sync/tickets/
pub async fn sync_tickets(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Query(params): Query<SyncQuery>,
) -> Result<impl IntoResponse, SyncApiError> {
    let cursor = match params.cursor {
        Some(c) => match SyncCursor::decode(&c) {
            Ok(cursor) => Some(cursor),
            Err(_) => return Err(SyncApiError::InvalidCursor),
        },
        None => None,
    };

    let limit = params.limit.unwrap_or(500);

    match sync_repo::sync_tickets(&state.pool, auth.user_id, cursor, limit).await {
        Ok(page) => Ok(Json(page)),
        Err(SyncError::Expired) => Err(SyncApiError::CursorExpired),
        Err(SyncError::Db(err)) => {
            tracing::error!("sync failed: {:?}", err);
            Err(SyncApiError::InternalError)
        }
    }
}

/// GET /api/v1/sync/projects/
pub async fn sync_projects(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Query(params): Query<SyncQuery>,
) -> Result<impl IntoResponse, SyncApiError> {
    let cursor = match params.cursor {
        Some(c) => match SyncCursor::decode(&c) {
            Ok(cursor) => Some(cursor),
            Err(_) => return Err(SyncApiError::InvalidCursor),
        },
        None => None,
    };

    let limit = params.limit.unwrap_or(500);

    match sync_repo::sync_projects(&state.pool, auth.user_id, cursor, limit).await {
        Ok(page) => Ok(Json(page)),
        Err(SyncError::Expired) => Err(SyncApiError::CursorExpired),
        Err(SyncError::Db(err)) => {
            tracing::error!("sync failed: {:?}", err);
            Err(SyncApiError::InternalError)
        }
    }
}

#[derive(Debug)]
pub enum SyncApiError {
    InvalidCursor,
    CursorExpired,
    InternalError,
}

impl IntoResponse for SyncApiError {
    fn into_response(self) -> axum::response::Response {
        match self {
            SyncApiError::InvalidCursor => (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "detail": "invalid cursor" })),
            )
                .into_response(),
            SyncApiError::CursorExpired => (
                StatusCode::GONE,
                Json(serde_json::json!({ "detail": "cursor expired" })),
            )
                .into_response(),
            SyncApiError::InternalError => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "detail": "internal server error" })),
            )
                .into_response(),
        }
    }
}
