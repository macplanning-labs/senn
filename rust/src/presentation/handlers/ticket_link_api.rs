/// presentation/handlers/ticket_link_api.rs — チケット参照リンク API
///
/// GET    /api/v1/tickets/{ticket_key}/links/     → 一覧
/// POST   /api/v1/tickets/{ticket_key}/links/     → 追加
/// DELETE /api/v1/tickets/{ticket_key}/links/{link_id}/ → 削除

use axum::{
    extract::{State, Path},
    response::{IntoResponse, Response},
    http::StatusCode,
    Json,
    Extension,
};
use serde::{Deserialize, Serialize};

use crate::presentation::state::AppState;
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::infrastructure::repositories::{ticket_repo, ticket_link_repo};

#[derive(Debug, Serialize)]
pub struct TicketLinkOut {
    pub id: i32,
    pub url: String,
    pub title: Option<String>,
    #[serde(rename = "createdBy")]
    pub created_by: CreatedByInfo,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct CreatedByInfo {
    pub id: i32,
    #[serde(rename = "displayName")]
    pub display_name: String,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub detail: String,
}

#[derive(Debug, Deserialize)]
pub struct TicketLinkCreateIn {
    pub url: String,
    #[serde(default)]
    pub title: Option<String>,
}

fn to_out(l: crate::domain::models::ticket_link::TicketLink) -> TicketLinkOut {
    TicketLinkOut {
        id: l.id,
        url: l.url,
        title: l.title,
        created_by: CreatedByInfo { id: l.created_by_id, display_name: l.created_by_name.unwrap_or_default() },
        created_at: l.created_at.to_rfc3339(),
    }
}

/// GET /api/v1/tickets/{ticket_key}/links/
pub async fn list_links(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(ticket_key): Path<String>,
) -> Response {
    let ticket_id = match ticket_repo::resolve_ticket_id(&state.pool, &ticket_key).await {
        Ok(Some(id)) => id,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(ErrorResponse { detail: "見つかりません".to_string() })).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() })).into_response();
        }
    };

    match ticket_link_repo::find_by_ticket(&state.pool, ticket_id).await {
        Ok(links) => (StatusCode::OK, Json(links.into_iter().map(to_out).collect::<Vec<_>>())).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() })).into_response()
        }
    }
}

/// POST /api/v1/tickets/{ticket_key}/links/
pub async fn add_link(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(ticket_key): Path<String>,
    Json(body): Json<TicketLinkCreateIn>,
) -> Response {
    if body.url.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(ErrorResponse { detail: "url は必須です".to_string() })).into_response();
    }

    let ticket_id = match ticket_repo::resolve_ticket_id(&state.pool, &ticket_key).await {
        Ok(Some(id)) => id,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(ErrorResponse { detail: "見つかりません".to_string() })).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() })).into_response();
        }
    };

    match ticket_link_repo::create(&state.pool, ticket_id, &body.url, body.title.as_deref(), auth.user_id).await {
        Ok(id) => match ticket_link_repo::find_by_id(&state.pool, id).await {
            Ok(Some(l)) => (StatusCode::CREATED, Json(to_out(l))).into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() })).into_response(),
        },
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() })).into_response()
        }
    }
}

/// DELETE /api/v1/tickets/{ticket_key}/links/{link_id}/
pub async fn delete_link(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path((ticket_key, link_id)): Path<(String, i32)>,
) -> Response {
    let ticket_id = match ticket_repo::resolve_ticket_id(&state.pool, &ticket_key).await {
        Ok(Some(id)) => id,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(ErrorResponse { detail: "見つかりません".to_string() })).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() })).into_response();
        }
    };

    match ticket_link_repo::delete(&state.pool, link_id, ticket_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, Json(ErrorResponse { detail: "見つかりません".to_string() })).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() })).into_response()
        }
    }
}
