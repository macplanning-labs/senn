/// presentation/handlers/search_api.rs — グローバル検索 JSON API
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;

use crate::domain::access::Viewer;
use crate::infrastructure::repositories::search_repo;
use crate::presentation::state::AppState;

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
    pub limit: Option<i64>,
}

/// GET /api/v1/search/?q=<query>&limit=10
pub async fn search(
    State(state): State<AppState>,
    viewer: Viewer,
    Query(params): Query<SearchQuery>,
) -> impl IntoResponse {
    let user_id = match viewer.require_user_id() {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    let query = params.q.unwrap_or_default().trim().to_string();
    let limit = params.limit.unwrap_or(10).min(50);

    if query.chars().count() < 2 {
        return (StatusCode::OK, Json(serde_json::json!({"results": []}))).into_response();
    }

    match search_repo::global_search(&state.pool, user_id, &viewer.scope(), &query, limit).await {
        Ok(results) => (
            StatusCode::OK,
            Json(serde_json::json!({"results": results})),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"detail": "サーバーエラーが発生しました"})),
            )
                .into_response()
        }
    }
}
