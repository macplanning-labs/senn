/// presentation/handlers/saved_view_api.rs — Saved View JSON API ハンドラー
///
/// t_saved_view テーブル用。個人用チケット一覧フィルタ。

use axum::{
    extract::{State, Path},
    response::IntoResponse,
    http::StatusCode,
    Json,
    Extension,
};
use serde::Serialize;

use crate::presentation::state::AppState;
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::infrastructure::repositories::{saved_view_repo, membership_repo, resource_repo};
use crate::domain::models::saved_view_api::{
    SavedViewCreateIn, SavedViewUpdateIn,
};

#[derive(Serialize)]
pub struct ErrorResponse {
    pub detail: String,
}

/// プロジェクトメンバーまたはプロジェクトオーナーならアクセス可。
async fn ensure_project_access(
    state: &AppState,
    project_id: i32,
    user_id: i32,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let is_member = membership_repo::check_membership_exists(&state.pool, project_id, user_id)
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
    let is_owner = resource_repo::is_project_owner(&state.pool, project_id, user_id)
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
    if is_owner {
        return Ok(());
    }
    Err((
        StatusCode::FORBIDDEN,
        Json(ErrorResponse {
            detail: "このプロジェクトのメンバーではありません".to_string(),
        }),
    ))
}

fn validate_view_name(name: &str) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let trimmed_len = name.trim().chars().count();
    if trimmed_len == 0 || trimmed_len > 100 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                detail: "名前は1文字以上100文字以下である必要があります".to_string(),
            }),
        ));
    }
    Ok(())
}

/// GET /api/v1/projects/{project_id}/saved-views/ — 自分の Saved View 一覧
pub async fn list(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(project_id): Path<i64>,
) -> impl IntoResponse {
    let owner_id: i64 = auth.user_id.into();

    if let Err(resp) = ensure_project_access(&state, project_id as i32, auth.user_id).await {
        return resp.into_response();
    }

    // 一覧取得
    match saved_view_repo::list_by_project_and_owner(&state.pool, project_id, owner_id)
        .await
    {
        Ok(views) => (StatusCode::OK, Json(views)).into_response(),
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

/// POST /api/v1/projects/{project_id}/saved-views/ — Saved View 作成
pub async fn create(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(project_id): Path<i64>,
    Json(input): Json<SavedViewCreateIn>,
) -> impl IntoResponse {
    let owner_id: i64 = auth.user_id.into();
    let name = input.name.trim();

    if let Err(resp) = validate_view_name(name) {
        return resp.into_response();
    }
    if let Err(resp) = ensure_project_access(&state, project_id as i32, auth.user_id).await {
        return resp.into_response();
    }

    // 作成
    match saved_view_repo::create(
        &state.pool,
        project_id,
        owner_id,
        name,
        &input.filters,
    )
    .await
    {
        Ok(view) => (StatusCode::CREATED, Json(view)).into_response(),
        Err(e) => {
            // UNIQUE 制約違反チェック
            if e.to_string().contains("duplicate") || e.to_string().contains("UNIQUE") {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        detail: "同じ名前のビューが既にあります".to_string(),
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

/// PATCH /api/v1/saved-views/{id}/ — Saved View 更新
pub async fn update(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<i64>,
    Json(input): Json<SavedViewUpdateIn>,
) -> impl IntoResponse {
    let owner_id: i64 = auth.user_id.into();
    let trimmed_name = input.name.as_ref().map(|n| n.trim().to_string());

    if let Some(ref name) = trimmed_name {
        if let Err(resp) = validate_view_name(name) {
            return resp.into_response();
        }
    }

    // 更新（所有者以外は repo 側で None → 404）
    match saved_view_repo::update(
        &state.pool,
        id,
        owner_id,
        trimmed_name.as_deref(),
        input.filters.as_ref(),
    )
    .await
    {
        Ok(Some(view)) => (StatusCode::OK, Json(view)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "Saved View が見つかりません".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            // UNIQUE 制約違反チェック
            if e.to_string().contains("duplicate") || e.to_string().contains("UNIQUE") {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        detail: "同じ名前のビューが既にあります".to_string(),
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

/// DELETE /api/v1/saved-views/{id}/ — Saved View 削除
pub async fn delete(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let owner_id: i64 = auth.user_id.into();

    match saved_view_repo::delete(&state.pool, id, owner_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "Saved View が見つかりません".to_string(),
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
