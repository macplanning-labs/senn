//! Sync API handlers for local-first synchronization

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;

use crate::{
    domain::access::Viewer,
    domain::models::sync_api::{SyncAccessOut, SyncCursor, SyncError},
    infrastructure::access::shadow::{self, Mode, Resource},
    infrastructure::repositories::sync_repo,
    AppState,
};

/// 同期の見える範囲(アクセス制御の再設計 E-1。`sync_repo::sync_access`)
async fn access_of(state: &AppState, viewer: &Viewer) -> Result<SyncAccessOut, SyncApiError> {
    if viewer.user_id().is_none() {
        return Err(SyncApiError::Unauthorized);
    }
    sync_repo::sync_access(&state.pool, viewer)
        .await
        .map_err(|e| {
            tracing::error!("sync access failed: {:?}", e);
            SyncApiError::InternalError
        })
}

#[derive(Deserialize)]
pub struct SyncQuery {
    cursor: Option<String>,
    limit: Option<i64>,
}

/// GET /api/v1/sync/tickets/
pub async fn sync_tickets(
    State(state): State<AppState>,
    viewer: Viewer,
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
    let access = access_of(&state, &viewer).await?;

    match sync_repo::sync_tickets(&state.pool, access, cursor, limit).await {
        Ok(page) => Ok(Json(page)),
        Err(SyncError::Expired) => Err(SyncApiError::CursorExpired),
        Err(SyncError::Db(err)) => {
            tracing::error!("sync failed: {:?}", err);
            Err(SyncApiError::InternalError)
        }
    }
}

/// GET /api/v1/sync/comments/
pub async fn sync_comments(
    State(state): State<AppState>,
    viewer: Viewer,
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
    let access = access_of(&state, &viewer).await?;
    match sync_repo::sync_comments(
        &state.pool,
        access,
        cursor,
        limit,
        &state.config.wip_ai_api_user,
    )
    .await
    {
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
    viewer: Viewer,
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
    let Some(user_id) = viewer.user_id() else {
        return Err(SyncApiError::Unauthorized);
    };
    // 新しい判定(on)だけ、見えないプロジェクトを deleted として返す(off / shadow は今のまま)
    let filter = if shadow::mode(Resource::Sync) == Mode::On {
        Some(access_of(&state, &viewer).await?)
    } else {
        None
    };

    match sync_repo::sync_projects(&state.pool, user_id, filter.as_ref(), cursor, limit).await {
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
    /// 人の閲覧者でない(キー経由の同期はフェーズ F まで受け付けない)
    Unauthorized,
    InvalidCursor,
    CursorExpired,
    InternalError,
}

impl IntoResponse for SyncApiError {
    fn into_response(self) -> axum::response::Response {
        match self {
            SyncApiError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "detail": "認証が必要です" })),
            )
                .into_response(),
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
