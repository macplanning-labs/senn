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

    // assignees のプロジェクトメンバーバリデーション
    if !body.assignees.is_empty() {
        match ticket_repo::validate_assignees_are_members(&state.pool, body.project, &body.assignees).await {
            Ok(non_members) => {
                if !non_members.is_empty() {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse {
                            detail: "指定されたユーザーはプロジェクトのメンバーではありません".to_string(),
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

    // assignees のプロジェクトメンバーバリデーション
    if !body.assignees.is_empty() {
        // チケットのプロジェクトIDを取得
        let project_id_opt: Option<i32> = match sqlx::query_scalar(
            "SELECT project_id::int4 FROM tickets_ticket WHERE ticket_key = $1"
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
            match ticket_repo::validate_assignees_are_members(&state.pool, project_id, &body.assignees).await {
                Ok(non_members) => {
                    if !non_members.is_empty() {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(ErrorResponse {
                                detail: "指定されたユーザーはプロジェクトのメンバーではありません".to_string(),
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
                "SELECT project_id::int4 FROM tickets_ticket WHERE ticket_key = $1"
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
                match ticket_repo::validate_assignees_are_members(&state.pool, project_id, assignees).await {
                    Ok(non_members) => {
                        if !non_members.is_empty() {
                            return (
                                StatusCode::BAD_REQUEST,
                                Json(ErrorResponse {
                                    detail: "指定されたユーザーはプロジェクトのメンバーではありません".to_string(),
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
            tracing::error!("api_patch failed: {:?}", e);
            if let Err(e) = tx.rollback().await { tracing::error!("transaction rollback failed: {:?}", e); }
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
        Ok(Some(_)) => {
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

    match ticket_repo::find_dependencies_for_ticket(&state.pool, ticket_id).await {
        Ok(deps) => (StatusCode::OK, Json(deps)).into_response(),
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

    let to_task = match body.to_task {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse { detail: "to_task は必須です".to_string() }),
            )
                .into_response();
        }
    };

    let result = ticket_repo::create_dependency(&state.pool, ticket_id, to_task, &body.dependency_type, auth.user_id).await;

    match result {
        Ok(CreateDependencyResult::Success(id)) => match ticket_repo::find_dependency_by_id(&state.pool, id).await {
            Ok(Some(dep)) => (StatusCode::CREATED, Json(dep)).into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() }),
            )
                .into_response(),
        },
        Ok(CreateDependencyResult::SelfReference) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { detail: "自分自身への依存関係は作成できません。".to_string() }),
        )
            .into_response(),
        Ok(CreateDependencyResult::Duplicate) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { detail: "この依存関係は既に存在します。".to_string() }),
        )
            .into_response(),
        Ok(CreateDependencyResult::ToTaskNotFound) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { detail: "指定されたチケットが見つかりません。".to_string() }),
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

    match ticket_repo::delete_dependency(&state.pool, dep_id, ticket_id).await {
        Ok(DeleteDependencyResult::Deleted) => StatusCode::NO_CONTENT.into_response(),
        Ok(DeleteDependencyResult::NotFound) | Ok(DeleteDependencyResult::NotRelated) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse { detail: "この依存関係は指定チケットに関連していません。".to_string() }),
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

    match crate::infrastructure::repositories::integration_repo::find_events_by_ticket(&state.pool, ticket_id).await {
        Ok(events) => (StatusCode::OK, Json(events)).into_response(),
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
    Extension(_auth): Extension<AuthUser>,
    Query(params): Query<CsvExportQuery>,
) -> impl IntoResponse {
    let project_id = match params.project {
        Some(id) => id,
        None => {
            return (StatusCode::BAD_REQUEST, "project parameter required").into_response();
        }
    };

    let rows = match ticket_repo::find_tickets_for_csv_export(&state.pool, project_id).await {
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
            r.ticket_key.as_str(), r.title.as_str(), r.status.as_str(), r.priority.as_str(),
            r.ticket_type.as_str(), r.assignees.as_str(), r.category.as_str(), r.milestone.as_str(),
            r.labels.as_str(), r.start_date.as_str(), r.due_date.as_str(), r.story_points.as_str(),
            r.cycle.as_str(), r.created_at.as_str(), r.updated_at.as_str(),
        ];
        csv.push_str(&fields.iter().map(|f| csv_escape(f)).collect::<Vec<_>>().join(","));
        csv.push_str("\r\n");
    }

    (
        StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, "text/csv; charset=utf-8-sig".to_string()),
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
            category: None,
            project: ticket_data.project,
            milestone: None,
            parent: None,
            start_date: None,
            due_date: None,
            labels: vec![],
            story_points: None,
            cycle: None,
            assigned_team: None,
            linked_rules: vec![],
        };

        // チケット作成を試みる
        match ticket_repo::api_create(&mut tx, &ticket_write_in, auth.user_id).await {
            Ok(_ticket_id) => {
                imported += 1;
            }
            Err(e) => {
                tracing::warn!("Failed to create ticket at index {}: {:?}", idx, e);
                errors.push(BulkImportError {
                    index: idx,
                    error: format!("{}", e),
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
