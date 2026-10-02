/// presentation/handlers/triage_api.rs — トリアージ依頼 JSON API
///
/// Django /api/v1/triage-requests/* と挙動を一致させるハンドラー。
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::domain::access::{Action, ResourceRef, Viewer};
use crate::domain::models::triage_api::*;
use crate::infrastructure::access::{facts_repo, shadow::Resource};
use crate::infrastructure::repositories::{team_repo, triage_repo};
use crate::presentation::extractors::authorize;
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::presentation::state::AppState;

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

/// チームへのアクセスの確認(D-5)
///
/// 今の判定: チームのメンバーだけ(403)。新しい判定: チームが見えること(Full Member は Public の
/// どのチームでも。チームの物の規則はサイクルと同じ)。切り替えは `ACCESS_ENFORCE_TEAM`。
async fn ensure_team_access(
    state: &AppState,
    viewer: &Viewer,
    team_id: i32,
    user_id: i32,
    action: Action,
    id: i32,
    route: &'static str,
) -> Result<(), axum::response::Response> {
    let server_error = |e: anyhow::Error| {
        tracing::error!("DB operation failed: {:?}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                detail: "サーバーエラーが発生しました".to_string(),
            }),
        )
            .into_response()
    };
    let is_member = team_repo::check_team_membership_exists(&state.pool, team_id, user_id)
        .await
        .map_err(server_error)?;
    let legacy = if is_member {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                detail: "このチームのメンバーではありません".to_string(),
            }),
        )
            .into_response())
    };
    let facts = facts_repo::facts_for_team(&state.pool, team_id)
        .await
        .map_err(server_error)?
        .map(|team| ResourceRef::Cycle { team });
    authorize::gate(
        &state.pool,
        viewer,
        facts.as_ref(),
        action,
        Resource::Team,
        id as i64,
        legacy,
        route,
    )
}

/// 既存の依頼のチームへのアクセスの確認(チームの無い依頼は、今の判定では確認なし。新しい判定では 404)
async fn ensure_triage_team_access(
    state: &AppState,
    viewer: &Viewer,
    item: &TriageRequestOut,
    user_id: i32,
    action: Action,
    route: &'static str,
) -> Result<(), axum::response::Response> {
    match item.team {
        Some(team_id) => {
            ensure_team_access(state, viewer, team_id, user_id, action, item.id, route).await
        }
        None => authorize::gate(
            &state.pool,
            viewer,
            None,
            action,
            Resource::Team,
            item.id as i64,
            Ok(()),
            route,
        ),
    }
}

/// GET /api/v1/triage-requests/
pub async fn list(
    State(state): State<AppState>,
    viewer: Viewer,
    Query(params): Query<ListQuery>,
) -> impl IntoResponse {
    let auth = AuthUser {
        user_id: match viewer.require_user_id() {
            Ok(id) => id,
            Err(resp) => return resp,
        },
    };
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

    if let Err(resp) = ensure_team_access(
        &state,
        &viewer,
        team_id,
        auth.user_id,
        Action::Read,
        0,
        "GET /api/v1/triage-requests/",
    )
    .await
    {
        return resp;
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
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
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
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
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
        Json(PaginatedTriageOut {
            count,
            next,
            previous,
            results: items,
        }),
    )
        .into_response()
}

/// GET /api/v1/triage-requests/{id}/
pub async fn detail(
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
    match triage_repo::find_by_id(&state.pool, id).await {
        Ok(Some(item)) => {
            if let Err(resp) = ensure_triage_team_access(
                &state,
                &viewer,
                &item,
                auth.user_id,
                Action::Read,
                "GET /api/v1/triage-requests/{id}/",
            )
            .await
            {
                return resp;
            }
            (StatusCode::OK, Json(item)).into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "見つかりません".to_string(),
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

/// POST /api/v1/triage-requests/
pub async fn create(
    State(state): State<AppState>,
    viewer: Viewer,
    Json(body): Json<TriageRequestWriteIn>,
) -> impl IntoResponse {
    let auth = AuthUser {
        user_id: match viewer.require_user_id() {
            Ok(id) => id,
            Err(resp) => return resp,
        },
    };
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

    if let Err(resp) = ensure_team_access(
        &state,
        &viewer,
        team_id,
        auth.user_id,
        Action::Create,
        0,
        "POST /api/v1/triage-requests/",
    )
    .await
    {
        return resp;
    }

    match triage_repo::create(&state.pool, &body, auth.user_id).await {
        Ok(id) => match triage_repo::find_by_id(&state.pool, id).await {
            Ok(Some(item)) => (StatusCode::CREATED, Json(item)).into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response(),
        },
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

/// PATCH /api/v1/triage-requests/{id}/
pub async fn update(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
    Json(body): Json<TriageRequestUpdateIn>,
) -> impl IntoResponse {
    let auth = AuthUser {
        user_id: match viewer.require_user_id() {
            Ok(id) => id,
            Err(resp) => return resp,
        },
    };
    let existing = match triage_repo::find_by_id(&state.pool, id).await {
        Ok(Some(item)) => item,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    detail: "見つかりません".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response();
        }
    };

    if let Err(resp) = ensure_triage_team_access(
        &state,
        &viewer,
        &existing,
        auth.user_id,
        Action::Write,
        "PATCH /api/v1/triage-requests/{id}/",
    )
    .await
    {
        return resp;
    }

    match triage_repo::update(&state.pool, id, &body).await {
        Ok(true) => match triage_repo::find_by_id(&state.pool, id).await {
            Ok(Some(item)) => (StatusCode::OK, Json(item)).into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response(),
        },
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "見つかりません".to_string(),
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

/// DELETE /api/v1/triage-requests/{id}/
pub async fn delete(
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
    let existing = match triage_repo::find_by_id(&state.pool, id).await {
        Ok(Some(item)) => item,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    detail: "見つかりません".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response();
        }
    };

    if let Err(resp) = ensure_triage_team_access(
        &state,
        &viewer,
        &existing,
        auth.user_id,
        Action::Delete,
        "DELETE /api/v1/triage-requests/{id}/",
    )
    .await
    {
        return resp;
    }

    match triage_repo::delete(&state.pool, id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "見つかりません".to_string(),
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

/// POST /api/v1/triage-requests/{id}/approve/
pub async fn approve(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
    Json(body): Json<TriageApproveIn>,
) -> impl IntoResponse {
    let auth = AuthUser {
        user_id: match viewer.require_user_id() {
            Ok(id) => id,
            Err(resp) => return resp,
        },
    };
    let existing = match triage_repo::find_by_id(&state.pool, id).await {
        Ok(Some(item)) => item,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    detail: "見つかりません".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response();
        }
    };

    if let Err(resp) = ensure_triage_team_access(
        &state,
        &viewer,
        &existing,
        auth.user_id,
        Action::Write,
        "POST /api/v1/triage-requests/{id}/approve/",
    )
    .await
    {
        return resp;
    }

    if existing.status != "pending" {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                detail: "この依頼は既にレビュー済みです。".to_string(),
            }),
        )
            .into_response();
    }

    match triage_repo::approve(
        &state.pool,
        id,
        auth.user_id,
        &body.comment,
        body.project_id,
    )
    .await
    {
        Ok(Some(_)) => match triage_repo::find_by_id(&state.pool, id).await {
            Ok(Some(item)) => (StatusCode::OK, Json(item)).into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response(),
        },
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "見つかりません".to_string(),
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

/// POST /api/v1/triage-requests/{id}/reject/
pub async fn reject(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
    Json(body): Json<TriageReviewIn>,
) -> impl IntoResponse {
    let auth = AuthUser {
        user_id: match viewer.require_user_id() {
            Ok(id) => id,
            Err(resp) => return resp,
        },
    };
    let existing = match triage_repo::find_by_id(&state.pool, id).await {
        Ok(Some(item)) => item,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    detail: "見つかりません".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response();
        }
    };

    if let Err(resp) = ensure_triage_team_access(
        &state,
        &viewer,
        &existing,
        auth.user_id,
        Action::Write,
        "POST /api/v1/triage-requests/{id}/reject/",
    )
    .await
    {
        return resp;
    }

    if existing.status != "pending" {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                detail: "この依頼は既にレビュー済みです。".to_string(),
            }),
        )
            .into_response();
    }

    match triage_repo::reject(&state.pool, id, auth.user_id, &body.comment).await {
        Ok(true) => match triage_repo::find_by_id(&state.pool, id).await {
            Ok(Some(item)) => (StatusCode::OK, Json(item)).into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response(),
        },
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "見つかりません".to_string(),
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
