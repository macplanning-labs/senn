/// presentation/handlers/time_entry_api.rs — Time Entry JSON API
///
/// Django /api/v1/time-entries/ と挙動を一致させるハンドラー。

use axum::{
    extract::{State, Path, Query},
    response::IntoResponse,
    http::StatusCode,
    Json,
    Extension,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::presentation::state::AppState;
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::infrastructure::repositories::time_entry_repo;
use crate::domain::models::time_entry_api::*;

// =============================================================================
// リクエスト構造体
// =============================================================================

#[derive(Deserialize)]
pub struct TimeEntryListQuery {
    pub ticket: Option<i32>,
    pub user: Option<i32>,
    pub page: Option<i64>,
}

// =============================================================================
// レスポンス構造体
// =============================================================================

#[derive(Serialize)]
pub struct PaginatedTimeEntriesOut {
    pub count: i64,
    pub next: Option<String>,
    pub previous: Option<String>,
    pub results: Vec<TimeEntryOut>,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub detail: String,
}

// =============================================================================
// Time Entries
// =============================================================================

/// タイムエントリー一覧 GET /api/v1/time-entries/
pub async fn time_entry_list(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Query(params): Query<TimeEntryListQuery>,
) -> impl IntoResponse {
    let page = params.page.unwrap_or(1).max(1);
    const PAGE_SIZE: i64 = 50;

    // タイムエントリー一覧取得
    let entries = match time_entry_repo::find_all_time_entries(
        &state.pool,
        page,
        params.ticket,
        params.user,
    )
    .await
    {
        Ok(e) => e,
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
    let count = match time_entry_repo::count_time_entries(&state.pool, params.ticket, params.user)
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

    let mut query_params = String::new();
    if let Some(ticket) = params.ticket {
        query_params.push_str(&format!("ticket={}", ticket));
    }
    if let Some(user) = params.user {
        if !query_params.is_empty() {
            query_params.push('&');
        }
        query_params.push_str(&format!("user={}", user));
    }

    let next = if has_next {
        if query_params.is_empty() {
            Some(format!("?page={}", page + 1))
        } else {
            Some(format!("?{}&page={}", query_params, page + 1))
        }
    } else {
        None
    };

    let previous = if has_previous {
        if page == 2 {
            if query_params.is_empty() {
                Some("?".to_string())
            } else {
                Some(format!("?{}", query_params))
            }
        } else {
            if query_params.is_empty() {
                Some(format!("?page={}", page - 1))
            } else {
                Some(format!("?{}&page={}", query_params, page - 1))
            }
        }
    } else {
        None
    };

    (
        StatusCode::OK,
        Json(PaginatedTimeEntriesOut {
            count,
            next,
            previous,
            results: entries,
        }),
    )
        .into_response()
}

/// タイムエントリー作成 POST /api/v1/time-entries/
pub async fn time_entry_create(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<TimeEntryCreateIn>,
) -> impl IntoResponse {
    // バリデーション: (startTime と endTime が両方あり) または (durationMinutes > 0)
    let duration_minutes = if let (Some(start), Some(end)) = (body.start_time, body.end_time) {
        // startTime と endTime が両方指定されている場合
        if let Some(specified_duration) = body.duration_minutes {
            // 指定された値を使用
            specified_duration
        } else {
            // 計算
            let duration = end.signed_duration_since(start);
            let minutes = duration.num_minutes() as i32;
            minutes.max(1)
        }
    } else if let Some(dur) = body.duration_minutes {
        // durationMinutes が指定されている場合
        if dur <= 0 {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    detail: "durationMinutesは0より大きい値を指定してください".to_string(),
                }),
            )
                .into_response();
        }
        dur
    } else {
        // どちらも指定されていない場合はエラー
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                detail: "startTimeとendTimeの両方、またはdurationMinutesのいずれかを指定してください"
                    .to_string(),
            }),
        )
            .into_response();
    };

    match time_entry_repo::create_time_entry(
        &state.pool,
        body.ticket,
        auth.user_id,
        body.description,
        body.start_time,
        body.end_time,
        duration_minutes,
    )
    .await
    {
        Ok(entry_id) => {
            // 作成したタイムエントリーを返す
            match time_entry_repo::find_time_entry_by_id(&state.pool, entry_id).await {
                Ok(Some(entry)) => (StatusCode::CREATED, Json(entry)).into_response(),
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

/// タイムエントリー削除 DELETE /api/v1/time-entries/{id}/
pub async fn time_entry_delete(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match time_entry_repo::delete_time_entry(&state.pool, id).await {
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

/// 今日の自分の作業時間サマリー GET /api/v1/time-entries/my-today/
pub async fn time_entry_my_today(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
) -> impl IntoResponse {
    match time_entry_repo::get_today_time_entries(&state.pool, auth.user_id).await {
        Ok(today) => (StatusCode::OK, Json(today)).into_response(),
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
