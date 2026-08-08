/// presentation/handlers/tickets_api.rs — JSON チケット API
///
/// Djangoの /api/v1/tickets/* と挙動を一致させるハンドラー。
/// Phase 2: チケットの作成・読み取り・更新・削除。

use axum::{
    extract::{State, Path, Query},
    response::{IntoResponse, Response},
    http::{StatusCode, HeaderMap, header},
    Json,
    Extension,
};
use serde::{Deserialize, Serialize};
use chrono::NaiveDate;
use sqlx::Row;

use crate::presentation::state::AppState;
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::infrastructure::repositories::ticket_repo;
use crate::domain::models::ticket_api::*;

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
pub struct AddCommentIn {
    pub body: String,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub detail: String,
}

// =============================================================================
// ハンドラー実装
// =============================================================================

/// チケット一覧 GET /api/v1/tickets/
pub async fn list(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
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
    filter.milestone = params.milestone;
    filter.category = params.category;
    filter.labels = params.labels;
    filter.parent = params.parent;
    filter.parent_isnull = params.parent__isnull;

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
pub async fn detail(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(ticket_key): Path<String>,
) -> impl IntoResponse {
    match ticket_repo::api_find_by_key(&state.pool, &ticket_key).await {
        Ok(Some(ticket)) => {
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

/// チケット作成 POST /api/v1/tickets/
pub async fn create(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<TicketWriteIn>,
) -> impl IntoResponse {
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
            tracing::error!("api_create failed: {:?}", e);
            if let Err(e) = tx.rollback().await { tracing::error!("transaction rollback failed: {:?}", e); }
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

    // 作成後の詳細を取得してLOOKUP
    let ticket_key_result: Option<String> = match sqlx::query_scalar(
        "SELECT ticket_key FROM tickets_ticket WHERE id = $1"
    )
    .bind(ticket_id)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(res) => res,
        Err(e) => { tracing::error!("ticket lookup failed: {:?}", e); None }
    };

    if let Some(ticket_key) = ticket_key_result {
        match ticket_repo::api_find_by_key(&state.pool, &ticket_key).await {
            Ok(Some(ticket)) => {
                (StatusCode::CREATED, Json(ticket)).into_response()
            }
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

/// チケット更新 PUT /api/v1/tickets/{ticket_key}/
pub async fn update(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(ticket_key): Path<String>,
    Json(body): Json<TicketWriteIn>,
) -> impl IntoResponse {
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
            tracing::error!("api_update failed: {:?}", e);
            if let Err(e) = tx.rollback().await { tracing::error!("transaction rollback failed: {:?}", e); }
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response();
        }
        Ok(Some(_)) => {
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

            // 更新後の詳細を取得
            match ticket_repo::api_find_by_key(&state.pool, &ticket_key).await {
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
            if let Err(e) = tx.rollback().await { tracing::error!("transaction rollback failed: {:?}", e); }
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
    Extension(_auth): Extension<AuthUser>,
    Path(ticket_key): Path<String>,
) -> impl IntoResponse {
    match ticket_repo::api_delete(&state.pool, &ticket_key).await {
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

/// コメント追加 POST /api/v1/tickets/{ticket_key}/comments/
pub async fn add_comment(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(ticket_key): Path<String>,
    Json(body): Json<AddCommentIn>,
) -> impl IntoResponse {
    // ticket_key から ticket_id を解決
    let ticket_id_opt: Option<i32> = match sqlx::query_scalar(
        "SELECT id::int4 FROM tickets_ticket WHERE ticket_key = $1"
    )
    .bind(&ticket_key)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(res) => res,
        Err(e) => { tracing::error!("ticket lookup failed: {:?}", e); None }
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

    // コメント追加
    match ticket_repo::api_add_comment(&state.pool, ticket_id, auth.user_id, &body.body).await
    {
        Ok(comment) => (StatusCode::CREATED, Json(comment)).into_response(),
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

/// コメント一覧 GET /api/v1/tickets/{ticket_key}/comments/
pub async fn list_comments(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(ticket_key): Path<String>,
) -> impl IntoResponse {
    // ticket_key から ticket_id を解決
    let ticket_id_opt: Option<i32> = match sqlx::query_scalar(
        "SELECT id::int4 FROM tickets_ticket WHERE ticket_key = $1"
    )
    .bind(&ticket_key)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(res) => res,
        Err(e) => { tracing::error!("ticket lookup failed: {:?}", e); None }
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

    // コメント取得
    let comments_rows = match sqlx::query(
        "SELECT c.id::int4, c.body, c.created_at, c.author_id::int4, u.id::int4, u.username, u.email, u.display_name
         FROM tickets_comment c
         LEFT JOIN accounts_user u ON c.author_id = u.id
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
            CommentOut {
                id: row.get(0),
                body: row.get(1),
                author,
                created_at: row.get(2),
            }
        })
        .collect();

    (StatusCode::OK, Json(comments)).into_response()
}

/// 変更ログ取得 GET /api/v1/tickets/{ticket_key}/change-logs/
pub async fn change_logs(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(ticket_key): Path<String>,
) -> impl IntoResponse {
    // ticket_key から ticket_id を解決
    let ticket_id_opt: Option<i32> = match sqlx::query_scalar(
        "SELECT id::int4 FROM tickets_ticket WHERE ticket_key = $1"
    )
    .bind(&ticket_key)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(res) => res,
        Err(e) => { tracing::error!("ticket lookup failed: {:?}", e); None }
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
    let ticket_id_opt: Option<i32> = match sqlx::query_scalar(
        "SELECT id::int4 FROM tickets_ticket WHERE ticket_key = $1"
    )
    .bind(&ticket_key)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(res) => res,
        Err(e) => { tracing::error!("ticket lookup failed: {:?}", e); None }
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
