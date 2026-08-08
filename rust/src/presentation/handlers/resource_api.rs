/// presentation/handlers/resource_api.rs — JSON リソース API
///
/// Djangoの /api/v1/projects/, /api/v1/categories/, /api/v1/milestones/, /api/v1/labels/
/// と挙動を一致させるハンドラー。
/// Phase 3: リソース CRUD API。

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
use crate::infrastructure::repositories::resource_repo;
use crate::domain::models::resource_api::*;

// =============================================================================
// リクエスト構造体
// =============================================================================

#[derive(Deserialize)]
pub struct ProjectListQuery {
    pub page: Option<i64>,
}

#[derive(Deserialize)]
pub struct MilestoneListQuery {
    pub project: Option<i32>,
    pub page: Option<i64>,
}

#[derive(Deserialize)]
pub struct LabelListQuery {
    pub project: Option<i32>,
}

// =============================================================================
// レスポンス構造体
// =============================================================================

#[derive(Serialize)]
pub struct PaginatedProjectsOut {
    pub count: i64,
    pub next: Option<String>,
    pub previous: Option<String>,
    pub results: Vec<ProjectOut>,
}

#[derive(Serialize)]
pub struct PaginatedMilestonesOut {
    pub count: i64,
    pub next: Option<String>,
    pub previous: Option<String>,
    pub results: Vec<MilestoneOut>,
}

/// DRFの本物のカーソルトークンとは異なる簡略実装
/// フロントエンドは `.results` のみ実際に参照していることを確認済み
#[derive(Serialize)]
pub struct PaginatedLabelsOut {
    pub next: Option<String>,
    pub previous: Option<String>,
    pub results: Vec<LabelOut>,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub detail: String,
}

// =============================================================================
// Projects
// =============================================================================

/// プロジェクト一覧 GET /api/v1/projects/
pub async fn project_list(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Query(params): Query<ProjectListQuery>,
) -> impl IntoResponse {
    let page = params.page.unwrap_or(1).max(1);
    const PAGE_SIZE: i64 = 50;

    // プロジェクト一覧取得
    let projects = match resource_repo::find_all_projects(&state.pool, page).await {
        Ok(p) => p,
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
    let count = match resource_repo::count_projects(&state.pool).await {
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
    let next = if has_next { Some(format!("?page={}", page + 1)) } else { None };
    let previous = if has_previous {
        Some(if page == 2 {
            "?".to_string()
        } else {
            format!("?page={}", page - 1)
        })
    } else {
        None
    };

    (
        StatusCode::OK,
        Json(PaginatedProjectsOut {
            count,
            next,
            previous,
            results: projects,
        }),
    )
        .into_response()
}

/// プロジェクト詳細 GET /api/v1/projects/{id}/
pub async fn project_detail(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match resource_repo::find_project_by_id(&state.pool, id).await {
        Ok(Some(project)) => (StatusCode::OK, Json(project)).into_response(),
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

/// プロジェクト作成 POST /api/v1/projects/
pub async fn project_create(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Json(body): Json<ProjectWriteIn>,
) -> impl IntoResponse {
    match resource_repo::create_project(&state.pool, &body).await {
        Ok(project_id) => {
            // 作成したプロジェクトを返す
            match resource_repo::find_project_by_id(&state.pool, project_id).await {
                Ok(Some(project)) => (StatusCode::CREATED, Json(project)).into_response(),
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

/// プロジェクト更新 PUT /api/v1/projects/{id}/
pub async fn project_update(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
    Json(body): Json<ProjectWriteIn>,
) -> impl IntoResponse {
    match resource_repo::update_project(&state.pool, id, &body).await {
        Ok(true) => {
            // 更新後のプロジェクトを返す
            match resource_repo::find_project_by_id(&state.pool, id).await {
                Ok(Some(project)) => (StatusCode::OK, Json(project)).into_response(),
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

/// プロジェクト削除 DELETE /api/v1/projects/{id}/
pub async fn project_delete(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    use resource_repo::DeleteProjectResult;
    match resource_repo::delete_project(&state.pool, id).await {
        Ok(DeleteProjectResult::Deleted) => StatusCode::NO_CONTENT.into_response(),
        Ok(DeleteProjectResult::NotFound) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "見つかりません".to_string(),
            }),
        )
            .into_response(),
        Ok(DeleteProjectResult::HasTickets) => (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                detail: "チケットが存在するプロジェクトは削除できません".to_string(),
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

// =============================================================================
// Categories
// =============================================================================

/// カテゴリー一覧 GET /api/v1/categories/
pub async fn category_list(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
) -> impl IntoResponse {
    match resource_repo::find_all_categories(&state.pool).await {
        Ok(categories) => (StatusCode::OK, Json(categories)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(Vec::<CategoryOut>::new()),
            )
                .into_response()
        }
    }
}

/// カテゴリー詳細 GET /api/v1/categories/{id}/
pub async fn category_detail(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match resource_repo::find_category_by_id(&state.pool, id).await {
        Ok(Some(category)) => (StatusCode::OK, Json(category)).into_response(),
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

/// カテゴリー作成 POST /api/v1/categories/
pub async fn category_create(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Json(body): Json<CategoryWriteIn>,
) -> impl IntoResponse {
    match resource_repo::create_category(&state.pool, &body).await {
        Ok(category_id) => {
            // 作成したカテゴリーを返す
            match resource_repo::find_category_by_id(&state.pool, category_id).await {
                Ok(Some(category)) => (StatusCode::CREATED, Json(category)).into_response(),
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

/// カテゴリー更新 PUT /api/v1/categories/{id}/
pub async fn category_update(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
    Json(body): Json<CategoryWriteIn>,
) -> impl IntoResponse {
    match resource_repo::update_category(&state.pool, id, &body).await {
        Ok(true) => {
            // 更新後のカテゴリーを返す
            match resource_repo::find_category_by_id(&state.pool, id).await {
                Ok(Some(category)) => (StatusCode::OK, Json(category)).into_response(),
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

/// カテゴリー削除 DELETE /api/v1/categories/{id}/
pub async fn category_delete(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match resource_repo::delete_category(&state.pool, id).await {
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

// =============================================================================
// Milestones
// =============================================================================

/// マイルストーン一覧 GET /api/v1/milestones/
pub async fn milestone_list(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Query(params): Query<MilestoneListQuery>,
) -> impl IntoResponse {
    let page = params.page.unwrap_or(1).max(1);
    const PAGE_SIZE: i64 = 50;

    // マイルストーン一覧取得
    let milestones = match resource_repo::find_all_milestones(&state.pool, params.project, page).await {
        Ok(m) => m,
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
    let count = match resource_repo::count_milestones(&state.pool, params.project).await {
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
    let next = if has_next { Some(format!("?page={}", page + 1)) } else { None };
    let previous = if has_previous {
        Some(if page == 2 {
            "?".to_string()
        } else {
            format!("?page={}", page - 1)
        })
    } else {
        None
    };

    (
        StatusCode::OK,
        Json(PaginatedMilestonesOut {
            count,
            next,
            previous,
            results: milestones,
        }),
    )
        .into_response()
}

/// マイルストーン詳細 GET /api/v1/milestones/{id}/
pub async fn milestone_detail(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match resource_repo::find_milestone_by_id(&state.pool, id).await {
        Ok(Some(milestone)) => (StatusCode::OK, Json(milestone)).into_response(),
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

/// マイルストーン作成 POST /api/v1/milestones/
pub async fn milestone_create(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Json(body): Json<MilestoneWriteIn>,
) -> impl IntoResponse {
    match resource_repo::create_milestone(&state.pool, &body).await {
        Ok(milestone_id) => {
            // 作成したマイルストーンを返す
            match resource_repo::find_milestone_by_id(&state.pool, milestone_id).await {
                Ok(Some(milestone)) => (StatusCode::CREATED, Json(milestone)).into_response(),
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

/// マイルストーン更新 PUT /api/v1/milestones/{id}/
pub async fn milestone_update(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
    Json(body): Json<MilestoneWriteIn>,
) -> impl IntoResponse {
    match resource_repo::update_milestone(&state.pool, id, &body).await {
        Ok(true) => {
            // 更新後のマイルストーンを返す
            match resource_repo::find_milestone_by_id(&state.pool, id).await {
                Ok(Some(milestone)) => (StatusCode::OK, Json(milestone)).into_response(),
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

/// マイルストーン削除 DELETE /api/v1/milestones/{id}/
pub async fn milestone_delete(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match resource_repo::delete_milestone(&state.pool, id).await {
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

// =============================================================================
// Labels
// =============================================================================

/// ラベル一覧 GET /api/v1/labels/
pub async fn label_list(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Query(params): Query<LabelListQuery>,
) -> impl IntoResponse {
    match resource_repo::find_all_labels(&state.pool, params.project).await {
        Ok(labels) => (
            StatusCode::OK,
            Json(PaginatedLabelsOut {
                next: None,
                previous: None,
                results: labels,
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

/// ラベル詳細 GET /api/v1/labels/{id}/
pub async fn label_detail(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match resource_repo::find_label_by_id(&state.pool, id).await {
        Ok(Some(label)) => (StatusCode::OK, Json(label)).into_response(),
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

/// ラベル作成 POST /api/v1/labels/
pub async fn label_create(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Json(body): Json<LabelWriteIn>,
) -> impl IntoResponse {
    match resource_repo::create_label(&state.pool, &body).await {
        Ok(label_id) => {
            // 作成したラベルを返す
            match resource_repo::find_label_by_id(&state.pool, label_id).await {
                Ok(Some(label)) => (StatusCode::CREATED, Json(label)).into_response(),
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

/// ラベル更新 PUT /api/v1/labels/{id}/
pub async fn label_update(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
    Json(body): Json<LabelWriteIn>,
) -> impl IntoResponse {
    match resource_repo::update_label(&state.pool, id, &body).await {
        Ok(true) => {
            // 更新後のラベルを返す
            match resource_repo::find_label_by_id(&state.pool, id).await {
                Ok(Some(label)) => (StatusCode::OK, Json(label)).into_response(),
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

/// ラベル削除 DELETE /api/v1/labels/{id}/
pub async fn label_delete(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match resource_repo::delete_label(&state.pool, id).await {
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
