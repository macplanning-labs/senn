/// presentation/handlers/workflow_status_api.rs — Workflow Status JSON API
///
/// Django /api/v1/workflow-statuses/ と挙動を一致させるハンドラー。

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
use crate::infrastructure::repositories::workflow_status_repo;
use crate::domain::models::workflow_status_api::*;

// =============================================================================
// リクエスト構造体
// =============================================================================

#[derive(Deserialize)]
pub struct WorkflowStatusListQuery {
    pub project: Option<i32>,
    pub page: Option<i64>,
}

// =============================================================================
// レスポンス構造体
// =============================================================================

#[derive(Serialize)]
pub struct PaginatedWorkflowStatusesOut {
    pub count: i64,
    pub next: Option<String>,
    pub previous: Option<String>,
    pub results: Vec<WorkflowStatusOut>,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub detail: String,
}

// =============================================================================
// Workflow Statuses
// =============================================================================

/// ワークフロー状態一覧 GET /api/v1/workflow-statuses/
pub async fn workflow_status_list(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Query(params): Query<WorkflowStatusListQuery>,
) -> impl IntoResponse {
    let page = params.page.unwrap_or(1).max(1);
    const PAGE_SIZE: i64 = 50;

    // ワークフロー状態一覧取得
    let statuses = match workflow_status_repo::find_all_workflow_statuses(
        &state.pool,
        page,
        params.project,
    )
    .await
    {
        Ok(s) => s,
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

    // 件数取得
    let count = match workflow_status_repo::count_workflow_statuses(&state.pool, params.project)
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

    let next = if has_next {
        if let Some(project) = params.project {
            Some(format!("?project={}&page={}", project, page + 1))
        } else {
            Some(format!("?page={}", page + 1))
        }
    } else {
        None
    };

    let previous = if has_previous {
        if page == 2 {
            if let Some(project) = params.project {
                Some(format!("?project={}", project))
            } else {
                Some("?".to_string())
            }
        } else {
            if let Some(project) = params.project {
                Some(format!("?project={}&page={}", project, page - 1))
            } else {
                Some(format!("?page={}", page - 1))
            }
        }
    } else {
        None
    };

    (
        StatusCode::OK,
        Json(PaginatedWorkflowStatusesOut {
            count,
            next,
            previous,
            results: statuses,
        }),
    )
        .into_response()
}

/// ワークフロー状態詳細 GET /api/v1/workflow-statuses/{id}/
pub async fn workflow_status_detail(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match workflow_status_repo::find_workflow_status_by_id(&state.pool, id).await {
        Ok(Some(status)) => (StatusCode::OK, Json(status)).into_response(),
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

/// ワークフロー状態作成 POST /api/v1/workflow-statuses/
pub async fn workflow_status_create(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Json(body): Json<WorkflowStatusWriteIn>,
) -> impl IntoResponse {
    match workflow_status_repo::create_workflow_status(&state.pool, &body).await {
        Ok(status_id) => {
            // 作成したワークフロー状態を返す
            match workflow_status_repo::find_workflow_status_by_id(&state.pool, status_id).await {
                Ok(Some(status)) => (StatusCode::CREATED, Json(status)).into_response(),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response(),
            }
        }
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

/// ワークフロー状態更新 PUT /api/v1/workflow-statuses/{id}/ (フル更新)
pub async fn workflow_status_update(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
    Json(body): Json<WorkflowStatusWriteIn>,
) -> impl IntoResponse {
    match workflow_status_repo::update_workflow_status(&state.pool, id, &body).await {
        Ok(true) => {
            // 更新後のワークフロー状態を返す
            match workflow_status_repo::find_workflow_status_by_id(&state.pool, id).await {
                Ok(Some(status)) => (StatusCode::OK, Json(status)).into_response(),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response(),
            }
        }
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

/// ワークフロー状態部分更新 PATCH /api/v1/workflow-statuses/{id}/ (部分更新)
pub async fn workflow_status_partial_update(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
    Json(body): Json<crate::domain::models::workflow_status_api::WorkflowStatusUpdateIn>,
) -> impl IntoResponse {
    match workflow_status_repo::partial_update_workflow_status(&state.pool, id, &body).await {
        Ok(true) => {
            // 更新後のワークフロー状態を返す
            match workflow_status_repo::find_workflow_status_by_id(&state.pool, id).await {
                Ok(Some(status)) => (StatusCode::OK, Json(status)).into_response(),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response(),
            }
        }
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

/// ワークフロー状態削除 DELETE /api/v1/workflow-statuses/{id}/
pub async fn workflow_status_delete(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match workflow_status_repo::delete_workflow_status(&state.pool, id).await {
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

/// ワークフロー状態並び順一括更新 POST /api/v1/workflow-statuses/reorder/
pub async fn workflow_status_reorder(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Json(body): Json<ReorderIn>,
) -> impl IntoResponse {
    match workflow_status_repo::reorder_workflow_statuses(&state.pool, body.order).await {
        Ok(_) => (
            StatusCode::OK,
            Json(ReorderOut {
                detail: "並び順を更新しました".to_string(),
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
