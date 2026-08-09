/// presentation/handlers/triage_api.rs — トリアージ依頼 JSON API
///
/// Django /api/v1/triage-requests/* と挙動を一致させるハンドラー。

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
use crate::infrastructure::repositories::triage_repo;
use crate::domain::models::triage_api::*;

#[derive(Deserialize)]
pub struct ListQuery {
    pub status: Option<String>,
    pub change_type: Option<String>,
    pub page: Option<i64>,
}

#[derive(Serialize)]
pub struct PaginatedTriageOut {
    pub count: i64,
    pub next: Option<String>,
    pub previous: Option<String>,
    pub results: Vec<TriageRequestOut>,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub detail: String,
}

/// GET /api/v1/triage-requests/
pub async fn list(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Query(params): Query<ListQuery>,
) -> impl IntoResponse {
    let page = params.page.unwrap_or(1).max(1);
    const PAGE_SIZE: i64 = 50;

    let items = match triage_repo::find_all(
        &state.pool,
        params.status.as_deref(),
        params.change_type.as_deref(),
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

    let count = match triage_repo::count_all(&state.pool, params.status.as_deref(), params.change_type.as_deref()).await {
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
        Json(PaginatedTriageOut { count, next, previous, results: items }),
    )
        .into_response()
}

/// GET /api/v1/triage-requests/{id}/
pub async fn detail(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match triage_repo::find_by_id(&state.pool, id).await {
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

/// POST /api/v1/triage-requests/
pub async fn create(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<TriageRequestWriteIn>,
) -> impl IntoResponse {
    match triage_repo::create(&state.pool, &body, auth.user_id).await {
        Ok(id) => match triage_repo::find_by_id(&state.pool, id).await {
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

/// PATCH /api/v1/triage-requests/{id}/
pub async fn update(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
    Json(body): Json<TriageRequestUpdateIn>,
) -> impl IntoResponse {
    match triage_repo::update(&state.pool, id, &body).await {
        Ok(true) => match triage_repo::find_by_id(&state.pool, id).await {
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

/// DELETE /api/v1/triage-requests/{id}/
pub async fn delete(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match triage_repo::delete(&state.pool, id).await {
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

/// POST /api/v1/triage-requests/{id}/approve/
pub async fn approve(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<i32>,
    Json(body): Json<TriageApproveIn>,
) -> impl IntoResponse {
    let status = match triage_repo::get_status(&state.pool, id).await {
        Ok(Some(s)) => s,
        Ok(None) => {
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
    };

    if status != "pending" {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { detail: "この依頼は既にレビュー済みです。".to_string() }),
        )
            .into_response();
    }

    match triage_repo::approve(&state.pool, id, auth.user_id, &body.comment, body.project_id).await {
        Ok(Some(_)) => match triage_repo::find_by_id(&state.pool, id).await {
            Ok(Some(item)) => (StatusCode::OK, Json(item)).into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() }),
            )
                .into_response(),
        },
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

/// POST /api/v1/triage-requests/{id}/reject/
pub async fn reject(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<i32>,
    Json(body): Json<TriageReviewIn>,
) -> impl IntoResponse {
    let status = match triage_repo::get_status(&state.pool, id).await {
        Ok(Some(s)) => s,
        Ok(None) => {
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
    };

    if status != "pending" {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { detail: "この依頼は既にレビュー済みです。".to_string() }),
        )
            .into_response();
    }

    match triage_repo::reject(&state.pool, id, auth.user_id, &body.comment).await {
        Ok(true) => match triage_repo::find_by_id(&state.pool, id).await {
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
