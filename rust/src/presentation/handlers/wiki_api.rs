/// presentation/handlers/wiki_api.rs — Wiki JSON API
///
/// Django /api/v1/wiki/* と挙動を一致させるハンドラー。

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
use crate::infrastructure::repositories::wiki_api_repo;
use crate::domain::models::wiki_api::*;

#[derive(Deserialize)]
pub struct ListQuery {
    pub project: Option<i32>,
    pub category: Option<String>,
    pub search: Option<String>,
    pub page: Option<i64>,
}

#[derive(Serialize)]
pub struct PaginatedWikiOut {
    pub count: i64,
    pub next: Option<String>,
    pub previous: Option<String>,
    pub results: Vec<WikiPageListOut>,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub detail: String,
}

/// GET /api/v1/wiki/
#[utoipa::path(
    get,
    path = "/api/v1/wiki/",
    tag = "wiki",
    responses(
        (status = 200, description = "Wikiページ一覧を返す")
    )
)]
pub async fn list(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Query(params): Query<ListQuery>,
) -> impl IntoResponse {
    let page = params.page.unwrap_or(1).max(1);
    const PAGE_SIZE: i64 = 50;

    let items = match wiki_api_repo::find_all(
        &state.pool,
        params.project,
        params.category.as_deref(),
        params.search.as_deref(),
        page,
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() }),
            )
                .into_response();
        }
    };

    let count = match wiki_api_repo::count_all(
        &state.pool,
        params.project,
        params.category.as_deref(),
        params.search.as_deref(),
    )
    .await
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() }),
            )
                .into_response();
        }
    };

    let has_next = page * PAGE_SIZE < count;
    let has_previous = page > 1;
    let next = if has_next { Some(format!("?page={}", page + 1)) } else { None };
    let previous = if has_previous { Some(format!("?page={}", page - 1)) } else { None };

    (
        StatusCode::OK,
        Json(PaginatedWikiOut { count, next, previous, results: items }),
    )
        .into_response()
}

/// GET /api/v1/wiki/{id}/
#[utoipa::path(
    get,
    path = "/api/v1/wiki/{id}/",
    tag = "wiki",
    params(
        ("id" = i32, Path, description = "WikiページID")
    ),
    responses(
        (status = 200, description = "Wikiページ詳細を返す"),
        (status = 404, description = "ページが見つからない")
    )
)]
pub async fn detail(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match wiki_api_repo::find_by_id(&state.pool, id).await {
        Ok(Some(item)) => (StatusCode::OK, Json(item)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse { detail: "見つかりません".to_string() }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() }),
            )
                .into_response()
        }
    }
}

/// POST /api/v1/wiki/
pub async fn create(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<WikiPageCreateIn>,
) -> impl IntoResponse {
    match wiki_api_repo::create(&state.pool, &body, auth.user_id).await {
        Ok(id) => match wiki_api_repo::find_by_id(&state.pool, id).await {
            Ok(Some(item)) => (StatusCode::CREATED, Json(item)).into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() }),
            )
                .into_response(),
        },
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() }),
            )
                .into_response()
        }
    }
}

/// PUT/PATCH /api/v1/wiki/{id}/
pub async fn update(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<i32>,
    Json(body): Json<WikiPageUpdateIn>,
) -> impl IntoResponse {
    match wiki_api_repo::update(&state.pool, id, &body, auth.user_id).await {
        Ok(true) => match wiki_api_repo::find_by_id(&state.pool, id).await {
            Ok(Some(item)) => (StatusCode::OK, Json(item)).into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() }),
            )
                .into_response(),
        },
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse { detail: "見つかりません".to_string() }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() }),
            )
                .into_response()
        }
    }
}

/// DELETE /api/v1/wiki/{id}/
pub async fn delete(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match wiki_api_repo::delete(&state.pool, id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse { detail: "見つかりません".to_string() }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() }),
            )
                .into_response()
        }
    }
}

/// GET /api/v1/wiki/{id}/revisions/
pub async fn revisions(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match wiki_api_repo::find_revisions(&state.pool, id).await {
        Ok(items) => (StatusCode::OK, Json(items)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() }),
            )
                .into_response()
        }
    }
}

#[derive(Serialize)]
struct LinkActionOut {
    detail: String,
    ticket_id: i32,
}

/// POST /api/v1/wiki/{id}/link-ticket/
pub async fn link_ticket(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
    Json(body): Json<LinkTicketIn>,
) -> impl IntoResponse {
    match wiki_api_repo::page_exists(&state.pool, id).await {
        Ok(true) => {}
        Ok(false) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse { detail: "見つかりません".to_string() }),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() }),
            )
                .into_response();
        }
    }

    match wiki_api_repo::ticket_exists(&state.pool, body.ticket_id).await {
        Ok(true) => {}
        Ok(false) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse { detail: "Ticket not found".to_string() }),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() }),
            )
                .into_response();
        }
    }

    match wiki_api_repo::link_ticket(&state.pool, id, body.ticket_id).await {
        Ok(()) => (
            StatusCode::OK,
            Json(LinkActionOut { detail: "linked".to_string(), ticket_id: body.ticket_id }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() }),
            )
                .into_response()
        }
    }
}

/// POST /api/v1/wiki/{id}/unlink-ticket/
pub async fn unlink_ticket(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
    Json(body): Json<LinkTicketIn>,
) -> impl IntoResponse {
    match wiki_api_repo::unlink_ticket(&state.pool, id, body.ticket_id).await {
        Ok(()) => (
            StatusCode::OK,
            Json(LinkActionOut { detail: "unlinked".to_string(), ticket_id: body.ticket_id }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() }),
            )
                .into_response()
        }
    }
}
