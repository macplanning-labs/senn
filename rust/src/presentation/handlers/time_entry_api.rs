/// presentation/handlers/time_entry_api.rs — Time Entry JSON API
///
/// Django /api/v1/time-entries/ と挙動を一致させるハンドラー。
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::access::{Action, ResourceRef, Viewer};
use crate::domain::models::time_entry_api::*;
use crate::infrastructure::access::{
    facts_repo,
    shadow::{self, Mode, Resource},
};
use crate::infrastructure::repositories::time_entry_repo;
use crate::presentation::extractors::authorize;
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::presentation::state::AppState;

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

// アクセス制御の再設計(D-5。設計書 §5.2・§6.1)。時間記録は「親のチケットが見えること」。
// 削除は、本人か、そのチームの設定を管理できる人。切り替えはチケットと同じ `ACCESS_ENFORCE_TICKET`。
// 今の判定には確認が無い。

fn db_error(e: anyhow::Error) -> axum::response::Response {
    tracing::error!("DB operation failed: {:?}", e);
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            detail: "サーバーエラーが発生しました".to_string(),
        }),
    )
        .into_response()
}

/// 親のチケットへの判定
async fn ticket_gate(
    state: &AppState,
    viewer: &Viewer,
    ticket_id: i32,
    action: Action,
    route: &'static str,
) -> Result<Option<ResourceRef>, axum::response::Response> {
    let facts = facts_repo::facts_for_ticket(&state.pool, ticket_id)
        .await
        .map_err(db_error)?;
    authorize::gate(
        &state.pool,
        viewer,
        facts.as_ref(),
        action,
        Resource::Ticket,
        ticket_id as i64,
        Ok(()),
        route,
    )?;
    Ok(facts)
}

// =============================================================================
// Time Entries
// =============================================================================

/// タイムエントリー一覧 GET /api/v1/time-entries/
pub async fn time_entry_list(
    State(state): State<AppState>,
    viewer: Viewer,
    Query(params): Query<TimeEntryListQuery>,
) -> impl IntoResponse {
    let page = params.page.unwrap_or(1).max(1);
    const PAGE_SIZE: i64 = 50;

    // 新しい判定(on)では SQL で絞る(親のチケットが見える物だけ。件数・ページ送りも合わせる)
    let scope = (shadow::mode(Resource::Ticket) == Mode::On).then(|| viewer.scope());

    // タイムエントリー一覧取得
    let entries = match time_entry_repo::find_all_time_entries(
        &state.pool,
        page,
        params.ticket,
        params.user,
        scope.as_ref(),
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

    // 試運転: 見えなくなる行を記録する(親のチケットの情報は、出てきたチケットの分だけ読む)
    let entries = if shadow::mode(Resource::Ticket) == Mode::Shadow {
        let mut facts = std::collections::HashMap::new();
        for e in &entries {
            if facts.contains_key(&e.ticket) {
                continue;
            }
            match facts_repo::facts_for_ticket(&state.pool, e.ticket).await {
                Ok(Some(f)) => {
                    facts.insert(e.ticket, f);
                }
                Ok(None) => {}
                Err(e) => return db_error(e),
            }
        }
        authorize::filter_list(
            &state.pool,
            &viewer,
            Resource::Ticket,
            "GET /api/v1/time-entries/",
            entries,
            |e| e.id as i64,
            |e| {
                facts
                    .get(&e.ticket)
                    .cloned()
                    .unwrap_or(ResourceRef::Ticket {
                        team: None,
                        project_id: None,
                        author_id: None,
                        assignee_ids: vec![],
                    })
            },
        )
    } else {
        entries
    };

    // 件数取得
    let count = match time_entry_repo::count_time_entries(
        &state.pool,
        params.ticket,
        params.user,
        scope.as_ref(),
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
    viewer: Viewer,
    Json(body): Json<TimeEntryCreateIn>,
) -> impl IntoResponse {
    let auth = AuthUser {
        user_id: match viewer.require_user_id() {
            Ok(id) => id,
            Err(resp) => return resp,
        },
    };
    if let Err(resp) = ticket_gate(
        &state,
        &viewer,
        body.ticket,
        Action::Write,
        "POST /api/v1/time-entries/",
    )
    .await
    {
        return resp;
    }
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
                detail:
                    "startTimeとendTimeの両方、またはdurationMinutesのいずれかを指定してください"
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
    viewer: Viewer,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    const ROUTE: &str = "DELETE /api/v1/time-entries/{id}/";
    match time_entry_repo::find_time_entry_by_id(&state.pool, id).await {
        Ok(Some(entry)) => {
            // 親のチケットが見えること。本人以外は、そのチームの設定を管理できること
            let facts = match ticket_gate(&state, &viewer, entry.ticket, Action::Read, ROUTE).await
            {
                Ok(f) => f,
                Err(resp) => return resp,
            };
            if viewer.user_id() != Some(entry.user.id) {
                let team = match facts {
                    Some(ResourceRef::Ticket { team: Some(t), .. }) => Some(ResourceRef::Team(t)),
                    // チームの無いチケットは、本人だけ(判定材料が無いので、on では 404)
                    _ => None,
                };
                if let Err(resp) = authorize::gate(
                    &state.pool,
                    &viewer,
                    team.as_ref(),
                    Action::ManageSettings,
                    Resource::Ticket,
                    entry.ticket as i64,
                    Ok(()),
                    ROUTE,
                ) {
                    return resp;
                }
            }
        }
        Ok(None) => {}
        Err(e) => return db_error(e),
    }
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
    viewer: Viewer,
) -> impl IntoResponse {
    // 本人の記録だけを返す(閲覧者の確認だけ)
    let user_id = match viewer.require_user_id() {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    match time_entry_repo::get_today_time_entries(&state.pool, user_id).await {
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
