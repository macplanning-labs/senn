/// presentation/handlers/cycle_api.rs — JSON サイクル API
///
/// Djangoの /api/v1/cycles/* (基本CRUD) と挙動を一致させるハンドラー。
/// progress/complete/velocity/burndown は含めない。

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
use crate::infrastructure::repositories::cycle_repo;
use crate::domain::models::cycle_api::CycleWriteIn;

#[derive(Deserialize)]
pub struct ListQuery {
    pub project: Option<i32>,
    pub status: Option<String>,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub detail: String,
}

/// GET /api/v1/cycles/?project=<id>&status=<s> — サイクル一覧
pub async fn list(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Query(params): Query<ListQuery>,
) -> impl IntoResponse {
    match cycle_repo::find_all_cycles(
        &state.pool,
        params.project,
        params.status.as_deref(),
    )
    .await
    {
        Ok(cycles) => (StatusCode::OK, Json(cycles)).into_response(),
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

/// GET /api/v1/cycles/{id}/ — サイクル詳細
pub async fn detail(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match cycle_repo::find_cycle_by_id(&state.pool, id).await {
        Ok(Some(cycle)) => (StatusCode::OK, Json(cycle)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "サイクルが見つかりません".to_string(),
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

/// POST /api/v1/cycles/ — サイクル作成
pub async fn create(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(input): Json<CycleWriteIn>,
) -> impl IntoResponse {
    match cycle_repo::create_cycle(&state.pool, &input, auth.user_id).await {
        Ok(id) => {
            // 作成したサイクルを返す
            match cycle_repo::find_cycle_by_id(&state.pool, id).await {
                Ok(Some(cycle)) => (StatusCode::CREATED, Json(cycle)).into_response(),
                Ok(None) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "作成されたサイクルが見つかりません".to_string(),
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
        Err(e) => {
            // バリデーションエラーの場合
            if e.to_string().contains("開始日は終了日より前") {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        detail: "開始日は終了日より前でなければなりません。".to_string(),
                    }),
                )
                    .into_response()
            } else {
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
}

/// PUT /api/v1/cycles/{id}/ — サイクル更新
pub async fn update(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
    Json(input): Json<CycleWriteIn>,
) -> impl IntoResponse {
    match cycle_repo::update_cycle(&state.pool, id, &input).await {
        Ok(true) => {
            // 更新したサイクルを返す
            match cycle_repo::find_cycle_by_id(&state.pool, id).await {
                Ok(Some(cycle)) => (StatusCode::OK, Json(cycle)).into_response(),
                Ok(None) => (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        detail: "サイクルが見つかりません".to_string(),
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
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "サイクルが見つかりません".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            // バリデーションエラーの場合
            if e.to_string().contains("開始日は終了日より前") {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        detail: "開始日は終了日より前でなければなりません。".to_string(),
                    }),
                )
                    .into_response()
            } else {
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
}

/// DELETE /api/v1/cycles/{id}/ — サイクル削除
pub async fn delete(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match cycle_repo::delete_cycle(&state.pool, id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "サイクルが見つかりません".to_string(),
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
