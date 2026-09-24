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
use crate::infrastructure::repositories::{triage_repo, team_repo};
use crate::domain::models::triage_api::*;

#[derive(Deserialize)]
pub struct ListQuery {
    pub team: Option<i32>,
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

async fn ensure_team_access(
    state: &AppState,
    team_id: i32,
    user_id: i32,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let is_member = team_repo::check_team_membership_exists(&state.pool, team_id, user_id)
        .await
        .map_err(|e| {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
        })?;
    if is_member {
        return Ok(());
    }
    Err((
        StatusCode::FORBIDDEN,
        Json(ErrorResponse {
            detail: "このチームのメンバーではありません".to_string(),
        }),
    ))
}

async fn ensure_triage_team_access(
    state: &AppState,
    item: &TriageRequestOut,
    user_id: i32,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if let Some(team_id) = item.team {
        ensure_team_access(state, team_id, user_id).await
    } else {
        Ok(())
    }
}

/// GET /api/v1/triage-requests/
pub async fn list(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Query(params): Query<ListQuery>,
) -> impl IntoResponse {
    let team_id = match params.team {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    detail: "team パラメータが必要です".to_string(),
                }),
            )
                .into_response();
        }
    };

    if let Err(resp) = ensure_team_access(&state, team_id, auth.user_id).await {
        return resp.into_response();
    }

    let page = params.page.unwrap_or(1).max(1);
    const PAGE_SIZE: i64 = 50;

    let items = match triage_repo::find_all(
        &state.pool,
        Some(team_id),
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

    let count = match triage_repo::count_all(
        &state.pool,
        Some(team_id),
        params.status.as_deref(),
        params.change_type.as_deref(),
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
    let mut query_suffix = format!("team={team_id}");
    if let Some(ref status) = params.status {
        query_suffix.push_str(&format!("&status={status}"));
    }
    if let Some(ref change_type) = params.change_type {
        query_suffix.push_str(&format!("&change_type={change_type}"));
    }
    let next = if has_next {
        Some(format!("?{query_suffix}&page={}", page + 1))
    } else {
        None
    };
    let previous = if has_previous {
        Some(format!("?{query_suffix}&page={}", page - 1))
    } else {
        None
    };

    (
        StatusCode::OK,
        Json(PaginatedTriageOut { count, next, previous, results: items }),
    )
        .into_response()
}

/// GET /api/v1/triage-requests/{id}/
pub async fn detail(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match triage_repo::find_by_id(&state.pool, id).await {
        Ok(Some(item)) => {
            if let Err(resp) = ensure_triage_team_access(&state, &item, auth.user_id).await {
                return resp.into_response();
            }
            (StatusCode::OK, Json(item)).into_response()
        }
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
    let team_id = match body.team {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    detail: "team が必要です".to_string(),
                }),
            )
                .into_response();
        }
    };

    if let Err(resp) = ensure_team_access(&state, team_id, auth.user_id).await {
        return resp.into_response();
    }

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
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<i32>,
    Json(body): Json<TriageRequestUpdateIn>,
) -> impl IntoResponse {
    let existing = match triage_repo::find_by_id(&state.pool, id).await {
        Ok(Some(item)) => item,
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

    if let Err(resp) = ensure_triage_team_access(&state, &existing, auth.user_id).await {
        return resp.into_response();
    }

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
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    let existing = match triage_repo::find_by_id(&state.pool, id).await {
        Ok(Some(item)) => item,
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

    if let Err(resp) = ensure_triage_team_access(&state, &existing, auth.user_id).await {
        return resp.into_response();
    }

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
    let existing = match triage_repo::find_by_id(&state.pool, id).await {
        Ok(Some(item)) => item,
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

    if let Err(resp) = ensure_triage_team_access(&state, &existing, auth.user_id).await {
        return resp.into_response();
    }

    if existing.status != "pending" {
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
    let existing = match triage_repo::find_by_id(&state.pool, id).await {
        Ok(Some(item)) => item,
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

    if let Err(resp) = ensure_triage_team_access(&state, &existing, auth.user_id).await {
        return resp.into_response();
    }

    if existing.status != "pending" {
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
