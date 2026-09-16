/// presentation/handlers/chat_integration_api.rs — チャット通知連携 JSON API
///
/// chat-integrations CRUD(JWT認証必須)。

use axum::{
    extract::{State, Path, Query},
    response::IntoResponse,
    http::StatusCode,
    Json,
    Extension,
};
use serde::Deserialize;
use serde_json::json;

use crate::presentation::state::AppState;
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::infrastructure::repositories::chat_integration_repo;
use crate::domain::models::chat_integration_api::*;

fn err(detail: &str) -> serde_json::Value {
    json!({"detail": detail})
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub project: Option<i32>,
    pub team: Option<i32>,
}

/// GET /api/v1/chat-integrations/?project=<id> または ?team=<id>
pub async fn list(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Query(params): Query<ListQuery>,
) -> impl IntoResponse {
    match chat_integration_repo::find_all(&state.pool, params.project, params.team).await {
        Ok(items) => (StatusCode::OK, Json(items)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

/// POST /api/v1/chat-integrations/
pub async fn create(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<ChatIntegrationWriteIn>,
) -> impl IntoResponse {
    match chat_integration_repo::create(&state.pool, &body, auth.user_id).await {
        Ok(id) => match chat_integration_repo::find_by_id(&state.pool, id).await {
            Ok(Some(item)) => (StatusCode::CREATED, Json(item)).into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response(),
        },
        Err(e) if e.to_string().contains("exactly one of project or team") => {
            (StatusCode::BAD_REQUEST, Json(err("project か team のどちらか一方が必要です"))).into_response()
        }
        Err(e) if e.to_string().contains("invalid provider") => {
            (StatusCode::BAD_REQUEST, Json(err("provider が不正です"))).into_response()
        }
        Err(e) if e.to_string().contains("requires") || e.to_string().contains("webhook_url is required") => {
            (StatusCode::BAD_REQUEST, Json(err(&e.to_string()))).into_response()
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

/// PATCH /api/v1/chat-integrations/{id}/
pub async fn update(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
    Json(body): Json<ChatIntegrationUpdateIn>,
) -> impl IntoResponse {
    match chat_integration_repo::update(&state.pool, id, &body).await {
        Ok(true) => match chat_integration_repo::find_by_id(&state.pool, id).await {
            Ok(Some(item)) => (StatusCode::OK, Json(item)).into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response(),
        },
        Ok(false) => (StatusCode::NOT_FOUND, Json(err("見つかりません"))).into_response(),
        Err(e) if e.to_string().contains("exactly one of project or team") => {
            (StatusCode::BAD_REQUEST, Json(err("project か team のどちらか一方が必要です"))).into_response()
        }
        Err(e) if e.to_string().contains("invalid provider") => {
            (StatusCode::BAD_REQUEST, Json(err("provider が不正です"))).into_response()
        }
        Err(e) if e.to_string().contains("requires") || e.to_string().contains("webhook_url is required") => {
            (StatusCode::BAD_REQUEST, Json(err(&e.to_string()))).into_response()
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

/// DELETE /api/v1/chat-integrations/{id}/
pub async fn delete(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match chat_integration_repo::delete(&state.pool, id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, Json(err("見つかりません"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}
