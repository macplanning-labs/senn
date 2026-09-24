/// presentation/handlers/tickets_api.rs — JSON チケット API
///
/// Djangoの /api/v1/tickets/* と挙動を一致させるハンドラー。
/// Phase 2: チケットの作成・読み取り・更新・削除。
use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::domain::models::ticket_api::*;
use crate::domain::services::notification_service;
use crate::infrastructure::repositories::{
    membership_repo, project_repo, resource_repo, ticket_repo, user_repo, workflow_status_repo,
};
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::presentation::state::AppState;

// =============================================================================
// リクエスト構造体
// =============================================================================

#[derive(Deserialize)]
pub struct ListQuery {
    pub status: Option<String>,
    pub status__in: Option<String>,
    pub priority: Option<String>,
    pub priority__in: Option<String>,
    pub assignees: Option<i32>,
    pub project: Option<i32>,
    pub project__prefix: Option<String>,
    pub milestone: Option<i32>,
    pub cycle: Option<i32>,
    pub category: Option<i32>,
    pub labels: Option<i32>,
    pub parent: Option<i32>,
    pub parent__isnull: Option<bool>,
    pub due_date__gte: Option<String>,
    pub due_date__lte: Option<String>,
    pub due_date__isnull: Option<bool>,
    pub search: Option<String>,
    pub ordering: Option<String>,
    pub page: Option<i64>,
    pub team_slug: Option<String>,
}

/// Djangoの rest_framework.pagination.PageNumberPagination と同じレスポンス形状。
/// フロントエンド(KanbanBoard.tsx, TicketTable.tsx等)は `.results` を前提にしており、
/// 配列そのままでは壊れるため必ずこの形でラップすること。
#[derive(Serialize)]
pub struct PaginatedTicketsOut {
    pub count: i64,
    pub next: Option<String>,
    pub previous: Option<String>,
    pub results: Vec<TicketListOut>,
}

#[derive(Deserialize)]
pub struct AnchorIn {
    pub start: i32,
    pub end: i32,
    pub quote: String,
}

#[derive(Deserialize)]
pub struct AddCommentIn {
    pub body: String,
    pub anchor: Option<AnchorIn>,
    #[serde(alias = "parentCommentId")]
    pub parent_comment_id: Option<i32>,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub detail: String,
}

/// Team だけのチケットは Team マスタ → ワークスペース既定。無い slug は 400。
/// Project 付きは従来どおり文字列のまま（既存マスタと食い違っても壊さない）。
async fn reject_unknown_team_status(
    pool: &sqlx::PgPool,
    project_id: Option<i32>,
    team_id: Option<i32>,
    status: &str,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if project_id.is_some() {
        return Ok(());
    }
    match workflow_status_repo::status_slug_exists_for_scope(pool, None, team_id, status).await {
        Ok(true) => Ok(()),
        Ok(false) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                detail: "このチームに存在しないステータスです。チームのワークフロー設定を確認してください"
                    .to_string(),
            }),
        )),
        Err(e) => {
            tracing::error!("status validation failed: {:?}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            ))
        }
    }
}

async fn ticket_scope_by_key(
    pool: &sqlx::PgPool,
    ticket_key: &str,
) -> Result<Option<(Option<i32>, Option<i32>)>, (StatusCode, Json<ErrorResponse>)> {
    sqlx::query_as::<_, (Option<i32>, Option<i32>)>(
        "SELECT project_id::int4, team_id::int4 FROM tickets_ticket WHERE ticket_key = $1",
    )
    .bind(ticket_key)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        tracing::error!("DB operation failed: {:?}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                detail: "サーバーエラーが発生しました".to_string(),
            }),
        )
    })
}

// =============================================================================
// ハンドラー実装
// =============================================================================

/// チケット一覧 GET /api/v1/tickets/
#[utoipa::path(
    get,
    path = "/api/v1/tickets/",
    tag = "tickets",
    responses(
        (status = 200, description = "チケット一覧を返す")
    )
)]
pub async fn list(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Query(params): Query<ListQuery>,
) -> impl IntoResponse {
    // フィルタ条件を構築
    let mut filter = ticket_repo::ApiTicketFilter::default();

    // status フィルタ
    if let Some(status_str) = params.status {
        filter.status = Some(vec![status_str]);
    }
    if let Some(status_in) = params.status__in {
        filter.status = Some(status_in.split(',').map(|s| s.to_string()).collect());
    }

    // priority フィルタ
    if let Some(priority_str) = params.priority {
        filter.priority = Some(vec![priority_str]);
    }
    if let Some(priority_in) = params.priority__in {
        filter.priority = Some(priority_in.split(',').map(|s| s.to_string()).collect());
    }

    filter.assignees = params.assignees;
    filter.project = params.project;
    filter.project_prefix = params.project__prefix;
    filter.team_slug = params.team_slug;
    filter.milestone = params.milestone;
    filter.cycle = params.cycle;
    filter.category = params.category;
    filter.labels = params.labels;
    filter.parent = params.parent;
    filter.parent_isnull = params.parent__isnull;
    filter.user_id = Some(auth.user_id);

    // due_date
    if let Some(due_gte) = params.due_date__gte {
        if let Ok(date) = NaiveDate::parse_from_str(&due_gte, "%Y-%m-%d") {
            filter.due_date_gte = Some(date);
        }
    }
    if let Some(due_lte) = params.due_date__lte {
        if let Ok(date) = NaiveDate::parse_from_str(&due_lte, "%Y-%m-%d") {
            filter.due_date_lte = Some(date);
        }
    }
    filter.due_date_isnull = params.due_date__isnull;

    let sort = params.ordering.as_deref().unwrap_or("-updated_at");
    let search = params.search.as_deref();
    let page = params.page.unwrap_or(1).max(1);
    const PAGE_SIZE: i64 = 50;

    // リポジトリ呼び出し(一覧+件数を両方取得してDjangoのPageNumberPagination形式に合わせる)
    let tickets = match ticket_repo::api_find_all(&state.pool, &filter, sort, search, page).await {
        Ok(t) => t,
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
    let count = match ticket_repo::api_count_all(&state.pool, &filter, search).await {
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
        Some(format!("?page={}", page + 1))
    } else {
        None
    };
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
        Json(PaginatedTicketsOut {
            count,
            next,
            previous,
            results: tickets,
        }),
    )
        .into_response()
}

/// チケット詳細 GET /api/v1/tickets/{ticket_key}/
#[utoipa::path(
    get,
    path = "/api/v1/tickets/{ticket_key}/",
    tag = "tickets",
    params(
        ("ticket_key" = String, Path, description = "チケットキー(例: DEMO-000001)")
    ),
    responses(
        (status = 200, description = "チケット詳細を返す"),
        (status = 404, description = "チケットが見つからない")
    )
)]
pub async fn detail(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(ticket_key): Path<String>,
) -> impl IntoResponse {
    match ticket_repo::api_find_by_key(&state.pool, &ticket_key, Some(auth.user_id), &state.config.wip_ai_api_user).await {
        Ok(Some(ticket)) => {
            match membership_repo::check_ticket_access(&state.pool, ticket.base.id, auth.user_id)
                .await
            {
                Ok(true) => {
                    let mut headers = HeaderMap::new();
                    let updated_at = ticket.base.updated_at.to_rfc3339();
                    headers.insert(
                        "X-Updated-At",
                        updated_at
                            .parse()
                            .unwrap_or_else(|_| header::HeaderValue::from_static("")),
                    );
                    headers.insert(
                        "Access-Control-Expose-Headers",
                        header::HeaderValue::from_static("X-Updated-At"),
                    );
                    (StatusCode::OK, headers, Json(ticket)).into_response()
                }
                Ok(false) => (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        detail: "見つかりません".to_string(),
                    }),
                )
                    .into_response(),
                Err(e) => {
                    tracing::error!("Access check failed: {:?}", e);
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
        Ok(None) => {
            tracing::warn!("Ticket not found: {}", ticket_key);
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    detail: "見つかりません".to_string(),
                }),
            )
                .into_response()
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

/// チケット作成 POST /api/v1/tickets/
pub async fn create(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<TicketWriteIn>,
) -> impl IntoResponse {
    // G6-1: teamId は必須、project は任意。api_create で検証

    // story_points バリデーション
    if let Some(points) = body.story_points {
        if !FIBONACCI_POINTS.contains(&points) {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    detail: "ストーリーポイントはフィボナッチ数列(1, 2, 3, 5, 8, 13, 21)のいずれかを指定してください。".to_string(),
                }),
            )
                .into_response();
        }
    }

    if let Err(resp) =
        reject_unknown_team_status(&state.pool, body.project, body.team_id, &body.status).await
    {
        return resp.into_response();
    }

    // assignees のプロジェクトメンバーバリデーション（project がある場合のみ）
    if !body.assignees.is_empty() {
        if let Some(project) = body.project {
            match ticket_repo::validate_assignees_are_members(&state.pool, project, &body.assignees)
                .await
            {
                Ok(non_members) => {
                    if !non_members.is_empty() {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(ErrorResponse {
                                detail: "指定されたユーザーはプロジェクトのメンバーではありません"
                                    .to_string(),
                            }),
                        )
                            .into_response();
                    }
                }
                Err(e) => {
                    tracing::error!("Assignee validation failed: {:?}", e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            detail: "サーバーエラーが発生しました".to_string(),
                        }),
                    )
                        .into_response();
                }
            }
        }
    }

    // reviewers のプロジェクトメンバーバリデーション（project がある場合のみ）
    if !body.reviewers.is_empty() {
        if let Some(project) = body.project {
            match ticket_repo::validate_reviewers_are_members(&state.pool, project, &body.reviewers)
                .await
            {
                Ok(non_members) => {
                    if !non_members.is_empty() {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(ErrorResponse {
                                detail: "指定されたユーザーはプロジェクトのメンバーではありません"
                                    .to_string(),
                            }),
                        )
                            .into_response();
                    }
                }
                Err(e) => {
                    tracing::error!("Reviewer validation failed: {:?}", e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            detail: "サーバーエラーが発生しました".to_string(),
                        }),
                    )
                        .into_response();
                }
            }
        }
    }

    // トランザクション開始
    let mut tx = match state.pool.begin().await {
        Ok(t) => t,
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

    // チケット作成
    let ticket_id = match ticket_repo::api_create(&mut tx, &body, auth.user_id).await {
        Ok(id) => id,
        Err(e) => {
            if let Some(resp) =
                crate::presentation::handlers::team_archive_api::archived_conflict(&e)
            {
                return resp;
            }
            tracing::error!("api_create failed: {:?}", e);
            if let Err(e) = tx.rollback().await {
                tracing::error!("transaction rollback failed: {:?}", e);
            }
            let detail = e.to_string();
            if detail.contains("teamId or project is required")
                || detail.contains("teamId is required")
                || detail.contains("not a participant")
                || detail.contains("no participating teams")
                || detail.contains("multiple teams")
            {
                return (StatusCode::BAD_REQUEST, Json(ErrorResponse { detail })).into_response();
            }
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response();
        }
    };

    // コミット
    if let Err(e) = tx.commit().await {
        tracing::error!("transaction commit failed: {:?}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                detail: "サーバーエラーが発生しました".to_string(),
            }),
        )
            .into_response();
    }

    // 作成者・担当者をウォッチャー登録(以降のコメント/ステータス変更等の通知先になる)
    if let Err(e) = ticket_repo::add_watcher(&state.pool, ticket_id, auth.user_id).await {
        tracing::error!("add_watcher failed: {:?}", e);
    }
    for &assignee_id in &body.assignees {
        if assignee_id != auth.user_id {
            if let Err(e) = ticket_repo::add_watcher(&state.pool, ticket_id, assignee_id).await {
                tracing::error!("add_watcher failed: {:?}", e);
            }
        }
    }

    // 作成後の詳細を取得してLOOKUP
    let ticket_key_result: Option<String> =
        match sqlx::query_scalar("SELECT ticket_key FROM tickets_ticket WHERE id = $1")
            .bind(ticket_id)
            .fetch_optional(&state.pool)
            .await
        {
            Ok(res) => res,
            Err(e) => {
                tracing::error!("ticket lookup failed: {:?}", e);
                None
            }
        };

    if let Some(ticket_key) = ticket_key_result {
        match ticket_repo::api_find_by_key(&state.pool, &ticket_key, Some(auth.user_id), &state.config.wip_ai_api_user).await {
            Ok(Some(ticket)) => (StatusCode::CREATED, Json(ticket)).into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response(),
        }
    } else {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                detail: "サーバーエラーが発生しました".to_string(),
            }),
        )
            .into_response()
    }
}

/// ステータス変更・担当者追加の通知を送る(失敗してもチケット更新自体は成功のまま、ログのみ)。
async fn notify_ticket_change(
    state: &AppState,
    actor_id: i32,
    events: &ticket_repo::TicketChangeEvents,
) {
    if let Some((old_status, new_status)) = &events.status_change {
        if let Err(e) = notification_service::notify_status_change(
            &state.pool,
            &state.mail_sender,
            events.ticket_id,
            actor_id,
            old_status,
            new_status,
        )
        .await
        {
            tracing::error!("notify_status_change failed: {:?}", e);
        }
    }

    if !events.newly_assigned.is_empty() {
        for &uid in &events.newly_assigned {
            if let Err(e) = ticket_repo::add_watcher(&state.pool, events.ticket_id, uid).await {
                tracing::error!("add_watcher failed: {:?}", e);
            }
        }

        let names: Result<Vec<String>, _> =
            sqlx::query_scalar("SELECT username FROM accounts_user WHERE id = ANY($1) ORDER BY id")
                .bind(&events.newly_assigned)
                .fetch_all(&state.pool)
                .await;

        match names {
            Ok(names) if !names.is_empty() => {
                if let Err(e) = notification_service::notify_assigned(
                    &state.pool,
                    &state.mail_sender,
                    events.ticket_id,
                    actor_id,
                    &names.join(", "),
                )
                .await
                {
                    tracing::error!("notify_assigned failed: {:?}", e);
                }
            }
            Ok(_) => {}
            Err(e) => tracing::error!("assignee username lookup failed: {:?}", e),
        }
    }

    if !events.newly_reviewers.is_empty() {
        for &uid in &events.newly_reviewers {
            if let Err(e) = ticket_repo::add_watcher(&state.pool, events.ticket_id, uid).await {
                tracing::error!("add_watcher failed: {:?}", e);
            }
        }

        let names: Result<Vec<String>, _> =
            sqlx::query_scalar("SELECT username FROM accounts_user WHERE id = ANY($1) ORDER BY id")
                .bind(&events.newly_reviewers)
                .fetch_all(&state.pool)
                .await;

        match names {
            Ok(names) if !names.is_empty() => {
                if let Err(e) = notification_service::notify_review_requested(
                    &state.pool,
                    &state.mail_sender,
                    events.ticket_id,
                    actor_id,
                    &names.join(", "),
                )
                .await
                {
                    tracing::error!("notify_review_requested failed: {:?}", e);
                }
            }
            Ok(_) => {}
            Err(e) => tracing::error!("reviewer username lookup failed: {:?}", e),
        }
    }

    if !events.other_changed_fields.is_empty() {
        let msg = events.other_changed_fields.join(", ");
        if let Err(e) = notification_service::notify_updated(
            &state.pool,
            &state.mail_sender,
            events.ticket_id,
            actor_id,
            &msg,
        )
        .await
        {
            tracing::error!("notify_updated failed: {:?}", e);
        }
    }
}

/// チケット更新 PUT /api/v1/tickets/{ticket_key}/
pub async fn update(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(ticket_key): Path<String>,
    Json(body): Json<TicketWriteIn>,
) -> impl IntoResponse {
    // チケットを取得して権限チェック
    let ticket_id: Option<i32> =
        match sqlx::query_scalar("SELECT id::int4 FROM tickets_ticket WHERE ticket_key = $1")
            .bind(&ticket_key)
            .fetch_optional(&state.pool)
            .await
        {
            Ok(result) => result,
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

    if let Some(tid) = ticket_id {
        match membership_repo::check_ticket_access(&state.pool, tid, auth.user_id).await {
            Ok(false) => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        detail: "見つかりません".to_string(),
                    }),
                )
                    .into_response();
            }
            Err(e) => {
                tracing::error!("Access check failed: {:?}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response();
            }
            Ok(true) => {}
        }
    } else {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "見つかりません".to_string(),
            }),
        )
            .into_response();
    }

    // story_points バリデーション
    if let Some(points) = body.story_points {
        if !FIBONACCI_POINTS.contains(&points) {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    detail: "ストーリーポイントはフィボナッチ数列(1, 2, 3, 5, 8, 13, 21)のいずれかを指定してください。".to_string(),
                }),
            )
                .into_response();
        }
    }

    match ticket_scope_by_key(&state.pool, &ticket_key).await {
        Ok(Some((project_id, team_id))) => {
            if let Err(resp) =
                reject_unknown_team_status(&state.pool, project_id, team_id, &body.status).await
            {
                return resp.into_response();
            }
        }
        Ok(None) => {}
        Err(resp) => return resp.into_response(),
    }

    // assignees のプロジェクトメンバーバリデーション
    if !body.assignees.is_empty() {
        // チケットのプロジェクトIDを取得
        let project_id_opt: Option<i32> = match sqlx::query_scalar(
            "SELECT project_id::int4 FROM tickets_ticket WHERE ticket_key = $1",
        )
        .bind(&ticket_key)
        .fetch_optional(&state.pool)
        .await
        {
            Ok(result) => result,
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

        if let Some(project_id) = project_id_opt {
            match ticket_repo::validate_assignees_are_members(
                &state.pool,
                project_id,
                &body.assignees,
            )
            .await
            {
                Ok(non_members) => {
                    if !non_members.is_empty() {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(ErrorResponse {
                                detail: "指定されたユーザーはプロジェクトのメンバーではありません"
                                    .to_string(),
                            }),
                        )
                            .into_response();
                    }
                }
                Err(e) => {
                    tracing::error!("Assignee validation failed: {:?}", e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            detail: "サーバーエラーが発生しました".to_string(),
                        }),
                    )
                        .into_response();
                }
            }
        }
    }

    // reviewers のプロジェクトメンバーバリデーション
    if !body.reviewers.is_empty() {
        // チケットのプロジェクトIDを取得
        let project_id_opt: Option<i32> = match sqlx::query_scalar(
            "SELECT project_id::int4 FROM tickets_ticket WHERE ticket_key = $1",
        )
        .bind(&ticket_key)
        .fetch_optional(&state.pool)
        .await
        {
            Ok(result) => result,
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

        if let Some(project_id) = project_id_opt {
            match ticket_repo::validate_reviewers_are_members(
                &state.pool,
                project_id,
                &body.reviewers,
            )
            .await
            {
                Ok(non_members) => {
                    if !non_members.is_empty() {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(ErrorResponse {
                                detail: "指定されたユーザーはプロジェクトのメンバーではありません"
                                    .to_string(),
                            }),
                        )
                            .into_response();
                    }
                }
                Err(e) => {
                    tracing::error!("Reviewer validation failed: {:?}", e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            detail: "サーバーエラーが発生しました".to_string(),
                        }),
                    )
                        .into_response();
                }
            }
        }
    }

    // トランザクション開始
    let mut tx = match state.pool.begin().await {
        Ok(t) => t,
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

    // チケット更新
    match ticket_repo::api_update(&mut tx, &ticket_key, &body, auth.user_id).await {
        Err(e) => {
            if let Some(resp) =
                crate::presentation::handlers::team_archive_api::archived_conflict(&e)
            {
                return resp;
            }
            tracing::error!("api_update failed: {:?}", e);
            if let Err(e) = tx.rollback().await {
                tracing::error!("transaction rollback failed: {:?}", e);
            }
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response();
        }
        Ok(Some(events)) => {
            // コミット
            if let Err(e) = tx.commit().await {
                tracing::error!("transaction commit failed: {:?}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response();
            }

            notify_ticket_change(&state, auth.user_id, &events).await;

            // 更新後の詳細を取得
            match ticket_repo::api_find_by_key(&state.pool, &ticket_key, Some(auth.user_id), &state.config.wip_ai_api_user).await {
                Ok(Some(ticket)) => (StatusCode::OK, Json(ticket)).into_response(),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response(),
            }
        }
        Ok(None) => {
            tracing::warn!("Ticket not found: {}", ticket_key);
            if let Err(e) = tx.rollback().await {
                tracing::error!("transaction rollback failed: {:?}", e);
            }
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    detail: "見つかりません".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// チケット部分更新 PATCH /api/v1/tickets/{ticket_key}/
///
/// 詳細パネルからのインライン編集用。指定したフィールドのみ更新する。
/// (PUT /api/v1/tickets/{ticket_key}/ は全項目必須のフォーム編集用、こちらは部分更新用で用途が異なる)
pub async fn patch(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(ticket_key): Path<String>,
    Json(body): Json<TicketPatchIn>,
) -> impl IntoResponse {
    // チケットを取得して権限チェック
    let ticket_id: Option<i32> =
        match sqlx::query_scalar("SELECT id::int4 FROM tickets_ticket WHERE ticket_key = $1")
            .bind(&ticket_key)
            .fetch_optional(&state.pool)
            .await
        {
            Ok(result) => result,
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

    if let Some(tid) = ticket_id {
        match membership_repo::check_ticket_access(&state.pool, tid, auth.user_id).await {
            Ok(false) => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        detail: "見つかりません".to_string(),
                    }),
                )
                    .into_response();
            }
            Err(e) => {
                tracing::error!("Access check failed: {:?}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response();
            }
            Ok(true) => {}
        }
    } else {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "見つかりません".to_string(),
            }),
        )
            .into_response();
    }

    if let Some(ref status) = body.status {
        match ticket_scope_by_key(&state.pool, &ticket_key).await {
            Ok(Some((project_id, team_id))) => {
                if let Err(resp) =
                    reject_unknown_team_status(&state.pool, project_id, team_id, status).await
                {
                    return resp.into_response();
                }
            }
            Ok(None) => {}
            Err(resp) => return resp.into_response(),
        }
    }

    if let Some(Some(points)) = body.story_points {
        if !FIBONACCI_POINTS.contains(&points) {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    detail: "ストーリーポイントはフィボナッチ数列(1, 2, 3, 5, 8, 13, 21)のいずれかを指定してください。".to_string(),
                }),
            )
                .into_response();
        }
    }

    // assignees のプロジェクトメンバーバリデーション（Someの場合のみ）
    if let Some(assignees) = &body.assignees {
        if !assignees.is_empty() {
            // チケットのプロジェクトIDを取得
            let project_id_opt: Option<i32> = match sqlx::query_scalar(
                "SELECT project_id::int4 FROM tickets_ticket WHERE ticket_key = $1",
            )
            .bind(&ticket_key)
            .fetch_optional(&state.pool)
            .await
            {
                Ok(result) => result,
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

            if let Some(project_id) = project_id_opt {
                match ticket_repo::validate_assignees_are_members(
                    &state.pool,
                    project_id,
                    assignees,
                )
                .await
                {
                    Ok(non_members) => {
                        if !non_members.is_empty() {
                            return (
                                StatusCode::BAD_REQUEST,
                                Json(ErrorResponse {
                                    detail:
                                        "指定されたユーザーはプロジェクトのメンバーではありません"
                                            .to_string(),
                                }),
                            )
                                .into_response();
                        }
                    }
                    Err(e) => {
                        tracing::error!("Assignee validation failed: {:?}", e);
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                detail: "サーバーエラーが発生しました".to_string(),
                            }),
                        )
                            .into_response();
                    }
                }
            }
        }
    }

    // reviewers のプロジェクトメンバーバリデーション（Someの場合のみ）
    if let Some(reviewers) = &body.reviewers {
        if !reviewers.is_empty() {
            // チケットのプロジェクトIDを取得
            let project_id_opt: Option<i32> = match sqlx::query_scalar(
                "SELECT project_id::int4 FROM tickets_ticket WHERE ticket_key = $1",
            )
            .bind(&ticket_key)
            .fetch_optional(&state.pool)
            .await
            {
                Ok(result) => result,
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

            if let Some(project_id) = project_id_opt {
                match ticket_repo::validate_reviewers_are_members(
                    &state.pool,
                    project_id,
                    reviewers,
                )
                .await
                {
                    Ok(non_members) => {
                        if !non_members.is_empty() {
                            return (
                                StatusCode::BAD_REQUEST,
                                Json(ErrorResponse {
                                    detail:
                                        "指定されたユーザーはプロジェクトのメンバーではありません"
                                            .to_string(),
                                }),
                            )
                                .into_response();
                        }
                    }
                    Err(e) => {
                        tracing::error!("Reviewer validation failed: {:?}", e);
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                detail: "サーバーエラーが発生しました".to_string(),
                            }),
                        )
                            .into_response();
                    }
                }
            }
        }
    }

    let mut tx = match state.pool.begin().await {
        Ok(t) => t,
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

    match ticket_repo::api_patch(&mut tx, &ticket_key, &body, auth.user_id).await {
        Err(e) => {
            if let Some(resp) =
                crate::presentation::handlers::team_archive_api::archived_conflict(&e)
            {
                return resp;
            }
            tracing::error!("api_patch failed: {:?}", e);
            if let Err(e) = tx.rollback().await {
                tracing::error!("transaction rollback failed: {:?}", e);
            }
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
        Ok(Some(events)) => {
            if let Err(e) = tx.commit().await {
                tracing::error!("transaction commit failed: {:?}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response();
            }

            notify_ticket_change(&state, auth.user_id, &events).await;

            match ticket_repo::api_find_by_key(&state.pool, &ticket_key, Some(auth.user_id), &state.config.wip_ai_api_user).await {
                Ok(Some(ticket)) => (StatusCode::OK, Json(ticket)).into_response(),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response(),
            }
        }
        Ok(None) => {
            tracing::warn!("Ticket not found: {}", ticket_key);
            if let Err(e) = tx.rollback().await {
                tracing::error!("transaction rollback failed: {:?}", e);
            }
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    detail: "見つかりません".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// チケット削除 DELETE /api/v1/tickets/{ticket_key}/
pub async fn delete(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(ticket_key): Path<String>,
) -> impl IntoResponse {
    // チケットを取得して権限チェック
    let ticket_id: Option<i32> =
        match sqlx::query_scalar("SELECT id::int4 FROM tickets_ticket WHERE ticket_key = $1")
            .bind(&ticket_key)
            .fetch_optional(&state.pool)
            .await
        {
            Ok(result) => result,
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

    if let Some(tid) = ticket_id {
        match membership_repo::check_ticket_access(&state.pool, tid, auth.user_id).await {
            Ok(false) => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        detail: "見つかりません".to_string(),
                    }),
                )
                    .into_response();
            }
            Err(e) => {
                tracing::error!("Access check failed: {:?}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response();
            }
            Ok(true) => {}
        }
    } else {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "見つかりません".to_string(),
            }),
        )
            .into_response();
    }

    match ticket_repo::api_delete(&state.pool, &ticket_key).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => {
            tracing::warn!("Ticket not found: {}", ticket_key);
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    detail: "見つかりません".to_string(),
                }),
            )
                .into_response()
        }
        Err(e) => {
            if let Some(resp) =
                crate::presentation::handlers::team_archive_api::archived_conflict(&e)
            {
                return resp;
            }
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
// ヘルパー関数
// =============================================================================

/// コメント本文中で `@` の直後に候補usernameが続く箇所を検出する。
///
/// usernameは英数字のみとは限らず、このアプリでは`user@example.com`のように
/// メールアドレス形式（内部に`@`を含む）のことが多い。そのため`@([A-Za-z0-9_.-]+)`の
/// ような文字クラスベースの正規表現では、username内部の`@`で途切れて誤検出する
/// （DEMO-000116運用時に発覚）。この関数は「実在する候補usernameの一覧」を先に受け取り、
/// 本文中の`@`直後にどの候補usernameが（最長一致で）続くかを走査する方式にすることで、
/// username自体に`@`を含む場合でも正しく検出できるようにしている。
/// TipTapのメンション拡張が`editor.getText()`で出力する`@[表示名:ユーザーID]`形式を検出する正規表現。
/// 丸括弧形式`@[label](id)`はMarkdownのリンク記法と衝突しReactMarkdownに消費されてしまうため、
/// コロン区切りの単一角括弧形式にしている（フロント側のrenderMentionHighlightと合わせること）。
fn mention_bracket_pattern() -> &'static regex::Regex {
    static PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    PATTERN.get_or_init(|| regex::Regex::new(r"@\[[^:\]]*:(\d+)\]").unwrap())
}

fn find_mentioned_user_ids(comment_body: &str, candidates: &[(i32, String)]) -> Vec<i32> {
    let candidate_ids: std::collections::HashSet<i32> =
        candidates.iter().map(|(id, _)| *id).collect();
    let mut found = std::collections::HashSet::new();

    // 1. TipTapのMention拡張が出力する `@[表示名](ID)` 形式をID直接参照で検出する。
    //    IDがプロジェクトメンバー/オーナーの候補に含まれる場合のみ採用する
    //    （非メンバーのIDを本文に埋め込まれても通知しないための安全策）。
    for cap in mention_bracket_pattern().captures_iter(comment_body) {
        if let Some(id_match) = cap.get(1) {
            if let Ok(user_id) = id_match.as_str().parse::<i32>() {
                if candidate_ids.contains(&user_id) {
                    found.insert(user_id);
                }
            }
        }
    }

    // 2. 後方互換: `@[...](...)` 形式を使わずに手入力・貼り付けされた素の`@username`等も
    //    引き続き検出する（IME入力中に@が全角「＠」になることがあるため半角に正規化してから走査）。
    //    ブラケット形式で既に検出済みの範囲を空白に潰してから走査し、二重解釈を避ける。
    let without_brackets = mention_bracket_pattern()
        .replace_all(comment_body, |caps: &regex::Captures| {
            " ".repeat(caps.get(0).unwrap().as_str().chars().count())
        });
    let normalized = without_brackets.replace('\u{FF20}', "@");

    let mut sorted_candidates: Vec<&(i32, String)> =
        candidates.iter().filter(|(_, u)| !u.is_empty()).collect();
    sorted_candidates.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

    let bytes = normalized.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'@' {
            let rest = &normalized[i + 1..];
            if let Some((user_id, username)) = sorted_candidates
                .iter()
                .find(|(_, u)| rest.starts_with(u.as_str()))
            {
                found.insert(*user_id);
                i += 1 + username.len();
                continue;
            }
        }
        i += 1;
    }
    found.into_iter().collect()
}

/// メンション処理：コメント本文からメンション対象ユーザーを抽出し、通知を送信
async fn process_mentions(
    pool: &sqlx::PgPool,
    mail_sender: &Option<crate::infrastructure::mail::MailSender>,
    ticket_id: i32,
    actor_id: i32,
    comment_body: &str,
) -> anyhow::Result<()> {
    // チケット情報取得（プロジェクトID確認用）
    let ticket = match ticket_repo::find_by_id(pool, ticket_id).await? {
        Some(t) => t,
        None => return Ok(()),
    };

    let project_id = match ticket.project_id {
        Some(pid) => pid,
        None => return Ok(()),
    };

    // プロジェクト情報取得（オーナーID確認用）
    let project = match project_repo::find_by_id(pool, project_id).await? {
        Some(p) => p,
        None => return Ok(()),
    };

    // メンション候補（プロジェクトメンバー + オーナー）のID/username・表示名・エイリアス
    // 一覧を作る。username（`@user@example.com`のようなメール形式）だけでなく
    // 表示名（`@日高直樹`）やニックネーム（エイリアス）でもメンションできるよう、
    // すべて候補トークンとして登録する。最初からメンバー/オーナーだけを候補にすることで、
    // 非メンバーのusername/表示名/エイリアスの存在有無を本文から推測されることも防げる。
    let members = user_repo::find_project_members(pool, project_id).await?;
    let mut candidates: Vec<(i32, String)> = Vec::new();
    for u in members {
        candidates.push((u.id, u.username.clone()));
        if !u.display_name.is_empty() && u.display_name != u.username {
            candidates.push((u.id, u.display_name.clone()));
        }
        if let Some(alias) = u.alias {
            if !alias.is_empty() && alias != u.username && alias != u.display_name {
                candidates.push((u.id, alias));
            }
        }
    }

    if let Some(owner_id) = project.owner_id {
        if !candidates.iter().any(|(id, _)| *id == owner_id) {
            if let Some(owner) = user_repo::find_by_id(pool, owner_id).await? {
                candidates.push((owner.id, owner.username.clone()));
                if !owner.display_name.is_empty() && owner.display_name != owner.username {
                    candidates.push((owner.id, owner.display_name.clone()));
                }
                if let Some(alias) = owner.alias {
                    if !alias.is_empty() && alias != owner.username && alias != owner.display_name {
                        candidates.push((owner.id, alias));
                    }
                }
            }
        }
    }

    // 本文中に実際に登場するメンション対象を検出
    for mentioned_user_id in find_mentioned_user_ids(comment_body, &candidates) {
        // 自己メンションは通知しない
        if mentioned_user_id == actor_id {
            continue;
        }

        // メンション通知を送信
        if let Err(e) = notification_service::notify_mentioned(
            pool,
            mail_sender,
            ticket_id,
            actor_id,
            mentioned_user_id,
        )
        .await
        {
            tracing::warn!(
                "notify_mentioned failed for user {}: {:?}",
                mentioned_user_id,
                e
            );
        }
    }

    Ok(())
}

/// コメント追加 POST /api/v1/tickets/{ticket_key}/comments/
pub async fn add_comment(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(ticket_key): Path<String>,
    Json(body): Json<AddCommentIn>,
) -> impl IntoResponse {
    // ticket_key から ticket_id を解決
    let ticket_id_opt: Option<i32> =
        match sqlx::query_scalar("SELECT id::int4 FROM tickets_ticket WHERE ticket_key = $1")
            .bind(&ticket_key)
            .fetch_optional(&state.pool)
            .await
        {
            Ok(res) => res,
            Err(e) => {
                tracing::error!("ticket lookup failed: {:?}", e);
                None
            }
        };

    let ticket_id = match ticket_id_opt {
        Some(id) => id,
        None => {
            tracing::warn!("Ticket not found: {}", ticket_key);
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    detail: "見つかりません".to_string(),
                }),
            )
                .into_response();
        }
    };

    // 返信対象の実効スレッドルートを解決
    let resolved_parent_comment_id = if let Some(parent_id) = body.parent_comment_id {
        match ticket_repo::api_resolve_thread_root(&state.pool, parent_id, ticket_id).await {
            Ok(Some(root_id)) => Some(root_id),
            Ok(None) => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        detail: "返信先のコメントが見つかりません".to_string(),
                    }),
                )
                    .into_response();
            }
            Err(e) => {
                tracing::error!("resolve_thread_root failed: {:?}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response();
            }
        }
    } else {
        None
    };

    // コメント追加
    let anchor = body
        .anchor
        .as_ref()
        .map(|a| (a.start, a.end, a.quote.clone()));
    match ticket_repo::api_add_comment(
        &state.pool,
        ticket_id,
        auth.user_id,
        &body.body,
        anchor,
        resolved_parent_comment_id,
        None,
    )
    .await
    {
        Ok(comment) => {
            // 通知(返信・トップレベル問わずウォッチャー全員に通知)
            if let Err(e) = notification_service::notify_comment(
                &state.pool,
                &state.mail_sender,
                ticket_id,
                auth.user_id,
                &body.body,
            )
            .await
            {
                tracing::error!("notify_comment failed: {:?}", e);
            }

            // メンション処理
            if let Err(e) = process_mentions(
                &state.pool,
                &state.mail_sender,
                ticket_id,
                auth.user_id,
                &body.body,
            )
            .await
            {
                tracing::warn!("mention processing failed: {:?}", e);
                // メンション処理失敗はコメント投稿自体は成功しているので、エラーを返さない
            }

            (StatusCode::CREATED, Json(comment)).into_response()
        }
        Err(e) => {
            if let Some(resp) =
                crate::presentation::handlers::team_archive_api::archived_conflict(&e)
            {
                return resp;
            }
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

/// コメント一覧 GET /api/v1/tickets/{ticket_key}/comments/
pub async fn list_comments(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(ticket_key): Path<String>,
) -> impl IntoResponse {
    // ticket_key から ticket_id を解決
    let ticket_id_opt: Option<i32> =
        match sqlx::query_scalar("SELECT id::int4 FROM tickets_ticket WHERE ticket_key = $1")
            .bind(&ticket_key)
            .fetch_optional(&state.pool)
            .await
        {
            Ok(res) => res,
            Err(e) => {
                tracing::error!("ticket lookup failed: {:?}", e);
                None
            }
        };

    let ticket_id = match ticket_id_opt {
        Some(id) => id,
        None => {
            tracing::warn!("Ticket not found: {}", ticket_key);
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    detail: "見つかりません".to_string(),
                }),
            )
                .into_response();
        }
    };

    // コメント取得
    let comments_rows = match sqlx::query(
        "SELECT c.id::int4, c.body, c.created_at, c.author_id::int4, u.id::int4, u.username, u.email, u.display_name, c.updated_at, c.anchor_start, c.anchor_end, c.anchor_quote, c.parent_comment_id::int4, (c.deleted_at IS NOT NULL) AS is_deleted,
                (SELECT COUNT(*) FROM tickets_comment r WHERE r.parent_comment_id = c.id)::int4 AS reply_count,
                au.id::int4, au.username, au.email, au.display_name
         FROM tickets_comment c
         LEFT JOIN accounts_user u ON c.author_id = u.id
         LEFT JOIN accounts_user au ON c.ai_agent_acting_user_id = au.id
         WHERE c.ticket_id = $1
         ORDER BY c.created_at ASC"
    )
    .bind(ticket_id)
    .fetch_all(&state.pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!("list_comments query failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response();
        }
    };

    let comments: Vec<CommentOut> = comments_rows
        .into_iter()
        .map(|row| {
            let author_id: i32 = row.get(4);
            let author = UserSummaryOut {
                id: author_id,
                username: row.get(5),
                email: row.get(6),
                display_name: row.get(7),
            };
            let is_deleted: bool = row.get(13);
            let body = if is_deleted {
                String::new()
            } else {
                row.get(1)
            };
            let acting_user_id: Option<i32> = row.get(15);
            let acting_user = acting_user_id.map(|id| UserSummaryOut {
                id,
                username: row.get(16),
                email: row.get(17),
                display_name: row.get(18),
            });
            CommentOut {
                id: row.get(0),
                body,
                author,
                acting_user,
                created_at: row.get(2),
                updated_at: row.get(8),
                anchor_start: row.get(9),
                anchor_end: row.get(10),
                anchor_quote: row.get(11),
                parent_comment_id: row.get(12),
                is_deleted,
                reply_count: row.get(14),
                can_edit: false,
                can_delete: false,
            }
        })
        .collect();

    (StatusCode::OK, Json(comments)).into_response()
}

/// コメント編集 PATCH /api/v1/tickets/{ticket_key}/comments/{comment_id}/
///
/// 投稿者本人のみ編集可能。他人のコメントを編集しようとした場合は403を返す。
pub async fn update_comment(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((ticket_key, comment_id)): Path<(String, i32)>,
    Json(body): Json<AddCommentIn>,
) -> impl IntoResponse {
    // ticket_key から ticket_id を解決
    let ticket_id_opt: Option<i32> =
        match sqlx::query_scalar("SELECT id::int4 FROM tickets_ticket WHERE ticket_key = $1")
            .bind(&ticket_key)
            .fetch_optional(&state.pool)
            .await
        {
            Ok(res) => res,
            Err(e) => {
                tracing::error!("ticket lookup failed: {:?}", e);
                None
            }
        };

    let ticket_id = match ticket_id_opt {
        Some(id) => id,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    detail: "見つかりません".to_string(),
                }),
            )
                .into_response();
        }
    };

    // コメントの所属チケット・投稿者を確認
    let owner = match ticket_repo::api_find_comment_owner(&state.pool, comment_id).await {
        Ok(owner) => owner,
        Err(e) => {
            tracing::error!("comment lookup failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response();
        }
    };

    let (comment_ticket_id, author_id, is_deleted, acting_user_id) = match owner {
        Some(o) => o,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    detail: "コメントが見つかりません".to_string(),
                }),
            )
                .into_response();
        }
    };

    if comment_ticket_id != ticket_id {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "コメントが見つかりません".to_string(),
            }),
        )
            .into_response();
    }

    if is_deleted {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "コメントが見つかりません".to_string(),
            }),
        )
            .into_response();
    }

    // 投稿者本人、AIエージェント実行者本人、またはAiAgent投稿×担当者のみ編集可 (000100 + 000098)
    let is_own = author_id == auth.user_id;
    let is_acting_user = acting_user_id == Some(auth.user_id);
    let is_ai_agent_and_assignee = if is_own || is_acting_user {
        false
    } else {
        let author_username = match ticket_repo::api_find_user_username(&state.pool, author_id).await {
            Ok(Some(username)) => Some(username),
            _ => None,
        };
        let is_ai_agent_author = author_username.as_deref() == Some(state.config.wip_ai_api_user.as_str());
        is_ai_agent_author
            && ticket_repo::is_ticket_assignee(&state.pool, ticket_id, auth.user_id)
                .await
                .unwrap_or(false)
    };
    if !is_own && !is_acting_user && !is_ai_agent_and_assignee {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse { detail: "このコメントを編集する権限がありません".to_string() }),
        )
            .into_response();
    }

    match ticket_repo::api_update_comment(&state.pool, comment_id, &body.body).await {
        Ok(comment) => (StatusCode::OK, Json(comment)).into_response(),
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

/// コメント削除 DELETE /api/v1/tickets/{ticket_key}/comments/{comment_id}/
///
/// 投稿者本人のみ削除可能。他人のコメントを削除しようとした場合は403を返す。
pub async fn delete_comment(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((ticket_key, comment_id)): Path<(String, i32)>,
) -> impl IntoResponse {
    // ticket_key から ticket_id を解決
    let ticket_id_opt: Option<i32> =
        match sqlx::query_scalar("SELECT id::int4 FROM tickets_ticket WHERE ticket_key = $1")
            .bind(&ticket_key)
            .fetch_optional(&state.pool)
            .await
        {
            Ok(res) => res,
            Err(e) => {
                tracing::error!("ticket lookup failed: {:?}", e);
                None
            }
        };

    let ticket_id = match ticket_id_opt {
        Some(id) => id,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    detail: "見つかりません".to_string(),
                }),
            )
                .into_response();
        }
    };

    // コメントの所属チケット・投稿者を確認
    let owner = match ticket_repo::api_find_comment_owner(&state.pool, comment_id).await {
        Ok(owner) => owner,
        Err(e) => {
            tracing::error!("comment lookup failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response();
        }
    };

    let (comment_ticket_id, author_id, is_deleted, acting_user_id) = match owner {
        Some(o) => o,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    detail: "コメントが見つかりません".to_string(),
                }),
            )
                .into_response();
        }
    };

    if comment_ticket_id != ticket_id {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "コメントが見つかりません".to_string(),
            }),
        )
            .into_response();
    }

    if is_deleted {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "コメントが見つかりません".to_string(),
            }),
        )
            .into_response();
    }

    // 投稿者本人、AIエージェント実行者本人、またはAiAgent投稿×担当者のみ削除可 (000100 + 000098)
    let is_own = author_id == auth.user_id;
    let is_acting_user = acting_user_id == Some(auth.user_id);
    let is_ai_agent_and_assignee = if is_own || is_acting_user {
        false
    } else {
        let author_username = match ticket_repo::api_find_user_username(&state.pool, author_id).await {
            Ok(Some(username)) => Some(username),
            _ => None,
        };
        let is_ai_agent_author = author_username.as_deref() == Some(state.config.wip_ai_api_user.as_str());
        is_ai_agent_author
            && ticket_repo::is_ticket_assignee(&state.pool, ticket_id, auth.user_id)
                .await
                .unwrap_or(false)
    };
    if !is_own && !is_acting_user && !is_ai_agent_and_assignee {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse { detail: "このコメントを削除する権限がありません".to_string() }),
        )
            .into_response();
    }

    match ticket_repo::api_delete_comment(&state.pool, comment_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
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

#[derive(Serialize)]
pub struct WatchOut {
    #[serde(rename = "isWatching")]
    pub is_watching: bool,
}

/// チケットウォッチ登録 POST /api/v1/tickets/{ticket_key}/watch/
pub async fn watch_ticket(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(ticket_key): Path<String>,
) -> impl IntoResponse {
    let ticket_id_opt: Option<i32> =
        match sqlx::query_scalar("SELECT id::int4 FROM tickets_ticket WHERE ticket_key = $1")
            .bind(&ticket_key)
            .fetch_optional(&state.pool)
            .await
        {
            Ok(res) => res,
            Err(e) => {
                tracing::error!("ticket lookup failed: {:?}", e);
                None
            }
        };

    let ticket_id = match ticket_id_opt {
        Some(id) => id,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    detail: "見つかりません".to_string(),
                }),
            )
                .into_response();
        }
    };

    match ticket_repo::add_watcher(&state.pool, ticket_id, auth.user_id).await {
        Ok(()) => (StatusCode::OK, Json(WatchOut { is_watching: true })).into_response(),
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

/// チケットウォッチ解除 DELETE /api/v1/tickets/{ticket_key}/watch/
pub async fn unwatch_ticket(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(ticket_key): Path<String>,
) -> impl IntoResponse {
    let ticket_id_opt: Option<i32> =
        match sqlx::query_scalar("SELECT id::int4 FROM tickets_ticket WHERE ticket_key = $1")
            .bind(&ticket_key)
            .fetch_optional(&state.pool)
            .await
        {
            Ok(res) => res,
            Err(e) => {
                tracing::error!("ticket lookup failed: {:?}", e);
                None
            }
        };

    let ticket_id = match ticket_id_opt {
        Some(id) => id,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    detail: "見つかりません".to_string(),
                }),
            )
                .into_response();
        }
    };

    match ticket_repo::remove_watcher(&state.pool, ticket_id, auth.user_id).await {
        Ok(()) => (StatusCode::OK, Json(WatchOut { is_watching: false })).into_response(),
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

/// 変更ログ取得 GET /api/v1/tickets/{ticket_key}/change-logs/
pub async fn change_logs(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(ticket_key): Path<String>,
) -> impl IntoResponse {
    // ticket_key から ticket_id を解決
    let ticket_id_opt: Option<i32> =
        match sqlx::query_scalar("SELECT id::int4 FROM tickets_ticket WHERE ticket_key = $1")
            .bind(&ticket_key)
            .fetch_optional(&state.pool)
            .await
        {
            Ok(res) => res,
            Err(e) => {
                tracing::error!("ticket lookup failed: {:?}", e);
                None
            }
        };

    let ticket_id = match ticket_id_opt {
        Some(id) => id,
        None => {
            tracing::warn!("Ticket not found: {}", ticket_key);
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    detail: "見つかりません".to_string(),
                }),
            )
                .into_response();
        }
    };

    // 変更ログ取得
    match ticket_repo::api_find_change_logs(&state.pool, ticket_id).await {
        Ok(logs) => (StatusCode::OK, Json(logs)).into_response(),
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

/// ポイント履歴取得 GET /api/v1/tickets/{ticket_key}/point-history/
pub async fn point_history(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(ticket_key): Path<String>,
) -> impl IntoResponse {
    // ticket_key から ticket_id を解決
    let ticket_id_opt: Option<i32> =
        match sqlx::query_scalar("SELECT id::int4 FROM tickets_ticket WHERE ticket_key = $1")
            .bind(&ticket_key)
            .fetch_optional(&state.pool)
            .await
        {
            Ok(res) => res,
            Err(e) => {
                tracing::error!("ticket lookup failed: {:?}", e);
                None
            }
        };

    let ticket_id = match ticket_id_opt {
        Some(id) => id,
        None => {
            tracing::warn!("Ticket not found: {}", ticket_key);
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    detail: "見つかりません".to_string(),
                }),
            )
                .into_response();
        }
    };

    // ポイント履歴取得
    match ticket_repo::api_find_point_history(&state.pool, ticket_id).await {
        Ok(history) => (StatusCode::OK, Json(history)).into_response(),
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
// タスク依存関係(dependencies) — ステップ4
//
// Django側は数値ticket_idベースのURLだが、Rust側は他の/api/v1/tickets/{ticket_key}/*
// と一貫させるためticket_keyベースにする(ステップ0.5の方針、フロントエンド未使用のため
// 互換性の懸念なし)。
// =============================================================================

/// GET /api/v1/tickets/{ticket_key}/dependencies/
pub async fn list_dependencies(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(ticket_key): Path<String>,
) -> impl IntoResponse {
    let ticket_id = match ticket_repo::resolve_ticket_id(&state.pool, &ticket_key).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            tracing::warn!("Ticket not found: {}", ticket_key);
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

    match ticket_repo::find_dependencies_for_ticket(&state.pool, ticket_id).await {
        Ok(deps) => (StatusCode::OK, Json(deps)).into_response(),
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

/// POST /api/v1/tickets/{ticket_key}/dependencies/
pub async fn add_dependency(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(ticket_key): Path<String>,
    Json(body): Json<crate::domain::models::dependency_api::TaskDependencyCreateIn>,
) -> impl IntoResponse {
    use ticket_repo::CreateDependencyResult;

    let ticket_id = match ticket_repo::resolve_ticket_id(&state.pool, &ticket_key).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            tracing::warn!("Ticket not found: {}", ticket_key);
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

    let to_task = match body.to_task {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    detail: "to_task は必須です".to_string(),
                }),
            )
                .into_response();
        }
    };

    let result = ticket_repo::create_dependency(
        &state.pool,
        ticket_id,
        to_task,
        &body.dependency_type,
        auth.user_id,
    )
    .await;

    match result {
        Ok(CreateDependencyResult::Success(id)) => {
            match ticket_repo::find_dependency_by_id(&state.pool, id).await {
                Ok(Some(dep)) => (StatusCode::CREATED, Json(dep)).into_response(),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response(),
            }
        }
        Ok(CreateDependencyResult::SelfReference) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                detail: "自分自身への依存関係は作成できません。".to_string(),
            }),
        )
            .into_response(),
        Ok(CreateDependencyResult::Duplicate) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                detail: "この依存関係は既に存在します。".to_string(),
            }),
        )
            .into_response(),
        Ok(CreateDependencyResult::ToTaskNotFound) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                detail: "指定されたチケットが見つかりません。".to_string(),
            }),
        )
            .into_response(),
        Ok(CreateDependencyResult::CircularDependency) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                detail: "この依存関係を追加すると循環依存になります。".to_string(),
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

/// DELETE /api/v1/tickets/{ticket_key}/dependencies/{dep_id}/
pub async fn delete_dependency(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path((ticket_key, dep_id)): Path<(String, i32)>,
) -> impl IntoResponse {
    use ticket_repo::DeleteDependencyResult;

    let ticket_id = match ticket_repo::resolve_ticket_id(&state.pool, &ticket_key).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            tracing::warn!("Ticket not found: {}", ticket_key);
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

    match ticket_repo::delete_dependency(&state.pool, dep_id, ticket_id).await {
        Ok(DeleteDependencyResult::Deleted) => StatusCode::NO_CONTENT.into_response(),
        Ok(DeleteDependencyResult::NotFound) | Ok(DeleteDependencyResult::NotRelated) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "この依存関係は指定チケットに関連していません。".to_string(),
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

/// GET /api/v1/tickets/{ticket_key}/git-events/ — ステップ4
/// dependenciesと同じ理由でticket_keyベースのURLにする(ステップ0.5の方針)。
pub async fn git_events(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(ticket_key): Path<String>,
) -> impl IntoResponse {
    let ticket_id = match ticket_repo::resolve_ticket_id(&state.pool, &ticket_key).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            tracing::warn!("Ticket not found: {}", ticket_key);
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

    match crate::infrastructure::repositories::integration_repo::find_events_by_ticket(
        &state.pool,
        ticket_id,
    )
    .await
    {
        Ok(events) => (StatusCode::OK, Json(events)).into_response(),
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

/// GET /api/v1/tickets/export/csv/?project=<id> — ステップ4
/// Excel互換のためBOM付きUTF-8で出力する(Django側と同じ)。
#[derive(Deserialize)]
pub struct CsvExportQuery {
    pub project: Option<i32>,
}

fn csv_escape(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') || field.contains('\r') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

pub async fn export_csv(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Query(params): Query<CsvExportQuery>,
) -> impl IntoResponse {
    let project_id = match params.project {
        Some(id) => id,
        None => {
            return (StatusCode::BAD_REQUEST, "project parameter required").into_response();
        }
    };

    let rows = match ticket_repo::find_tickets_for_csv_export(&state.pool, project_id, auth.user_id)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response();
        }
    };

    let mut csv = String::from("\u{feff}");
    csv.push_str("Key,Title,Status,Priority,Type,Assignees,Category,Milestone,Labels,Start Date,Due Date,Story Points,Cycle,Created,Updated\r\n");

    for r in &rows {
        let fields = [
            r.ticket_key.as_str(),
            r.title.as_str(),
            r.status.as_str(),
            r.priority.as_str(),
            r.ticket_type.as_str(),
            r.assignees.as_str(),
            r.category.as_str(),
            r.milestone.as_str(),
            r.labels.as_str(),
            r.start_date.as_str(),
            r.due_date.as_str(),
            r.story_points.as_str(),
            r.cycle.as_str(),
            r.created_at.as_str(),
            r.updated_at.as_str(),
        ];
        csv.push_str(
            &fields
                .iter()
                .map(|f| csv_escape(f))
                .collect::<Vec<_>>()
                .join(","),
        );
        csv.push_str("\r\n");
    }

    (
        StatusCode::OK,
        [
            (
                axum::http::header::CONTENT_TYPE,
                "text/csv; charset=utf-8-sig".to_string(),
            ),
            (
                axum::http::header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"tickets_{}.csv\"", project_id),
            ),
        ],
        csv,
    )
        .into_response()
}

/// バルクインポート POST /api/v1/tickets/bulk-import/
pub async fn bulk_import(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<BulkImportIn>,
) -> impl IntoResponse {
    let mut imported = 0;
    let mut errors: Vec<BulkImportError> = Vec::new();

    // トランザクション開始
    let mut tx = match state.pool.begin().await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("DB transaction begin failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(BulkImportOut {
                    imported: 0,
                    errors: vec![BulkImportError {
                        index: 0,
                        error: "サーバーエラーが発生しました".to_string(),
                    }],
                }),
            )
                .into_response();
        }
    };

    // 各チケットをインポート
    for (idx, ticket_data) in body.tickets.iter().enumerate() {
        // BulkImportTicketInからTicketWriteInに変換
        let ticket_write_in = TicketWriteIn {
            title: ticket_data.title.clone(),
            description: ticket_data.description.clone(),
            status: ticket_data.status.clone(),
            priority: ticket_data.priority.clone(),
            ticket_type: "task".to_string(), // デフォルト値
            assignees: vec![],
            reviewers: vec![],
            category: None,
            project: Some(ticket_data.project),
            milestone: None,
            parent: None,
            start_date: None,
            due_date: None,
            labels: vec![],
            story_points: None,
            cycle: None,
            team_id: None,
            linked_rules: vec![],
        };

        // チケット作成を試みる
        match ticket_repo::api_create(&mut tx, &ticket_write_in, auth.user_id).await {
            Ok(_ticket_id) => {
                imported += 1;
            }
            Err(e) => {
                tracing::warn!("Failed to create ticket at index {}: {}", idx, e);
                errors.push(BulkImportError {
                    index: idx,
                    error: "処理に失敗しました".to_string(),
                });
            }
        }
    }

    // コミット
    if let Err(e) = tx.commit().await {
        tracing::error!("Transaction commit failed: {:?}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(BulkImportOut {
                imported: 0,
                errors: vec![BulkImportError {
                    index: 0,
                    error: "トランザクションコミット失敗".to_string(),
                }],
            }),
        )
            .into_response();
    }

    (
        StatusCode::CREATED,
        Json(BulkImportOut { imported, errors }),
    )
        .into_response()
}

// =============================================================================
// バルク削除（プロジェクトオーナーのみ）
// =============================================================================

#[derive(Deserialize)]
pub struct BulkDeleteIn {
    /// 削除対象のチケットキー（指定時は delete_all より優先）
    pub ticket_keys: Option<Vec<String>>,
    /// delete_all=true のとき、プロジェクト内の全チケットを削除
    pub project_id: Option<i32>,
    pub delete_all: Option<bool>,
}

#[derive(Serialize)]
pub struct BulkDeleteOut {
    pub deleted: i32,
}

/// チケット一括削除 POST /api/v1/tickets/bulk-delete/
/// プロジェクトオーナーのみ実行可能。物理削除（子チケットも再帰的に削除）。
pub async fn bulk_delete(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<BulkDeleteIn>,
) -> impl IntoResponse {
    let ticket_keys: Vec<String> = if body.delete_all == Some(true) {
        let project_id = match body.project_id {
            Some(id) => id,
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        detail: "delete_all には project_id が必要です".to_string(),
                    }),
                )
                    .into_response();
            }
        };
        match ticket_repo::list_root_ticket_keys_by_project(&state.pool, project_id).await {
            Ok(keys) => keys,
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
        }
    } else {
        body.ticket_keys.unwrap_or_default()
    };

    if ticket_keys.is_empty() {
        return (StatusCode::OK, Json(BulkDeleteOut { deleted: 0 })).into_response();
    }

    // 全チケットが同一プロジェクトに属することを確認し、オーナー権限を検証
    let mut project_id: Option<i32> = None;
    for key in &ticket_keys {
        match ticket_repo::get_ticket_project_id(&state.pool, key).await {
            Ok(Some(pid)) => {
                if let Some(existing) = project_id {
                    if existing != pid {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(ErrorResponse {
                                detail:
                                    "異なるプロジェクトのチケットをまとめて削除することはできません"
                                        .to_string(),
                            }),
                        )
                            .into_response();
                    }
                } else {
                    project_id = Some(pid);
                }
            }
            Ok(None) => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        detail: format!("チケット '{}' が見つかりません", key),
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
        }
    }

    let project_id = match project_id {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    detail: "削除対象のチケットがありません".to_string(),
                }),
            )
                .into_response();
        }
    };

    match resource_repo::is_project_owner(&state.pool, project_id, auth.user_id).await {
        Ok(true) => {}
        Ok(false) => {
            return (
                StatusCode::FORBIDDEN,
                Json(ErrorResponse {
                    detail: "プロジェクトオーナーのみチケットを一括削除できます".to_string(),
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

    let mut deleted = 0i32;
    for key in &ticket_keys {
        match ticket_repo::api_delete(&state.pool, key).await {
            Ok(true) => deleted += 1,
            Ok(false) => {}
            Err(e) => {
                tracing::error!("bulk delete failed for {}: {:?}", key, e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: format!("削除中にエラーが発生しました（{}件削除済み）", deleted),
                    }),
                )
                    .into_response();
            }
        }
    }

    (StatusCode::OK, Json(BulkDeleteOut { deleted })).into_response()
}
