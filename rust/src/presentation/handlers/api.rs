/// presentation/handlers/api.rs — チケット REST API（外部システム連携用）
///
/// GET  /api/tickets → チケット一覧（JSON）
/// POST /api/tickets → チケット作成（JSON）
/// 認証: X-API-Key ヘッダー

use axum::{
    extract::{State, Query},
    response::IntoResponse,
    http::{StatusCode, HeaderMap},
    Json,
};
use serde::{Deserialize, Serialize};
use crate::presentation::state::AppState;
use crate::infrastructure::repositories::ticket_repo;
use crate::domain::services::ticket_service;

/// API キー検証
fn verify_api_key(headers: &HeaderMap) -> bool {
    let api_key = std::env::var("TICKET_API_KEY").unwrap_or_default();
    if api_key.is_empty() {
        return false;
    }
    headers
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok())
        .map(|k| k == api_key)
        .unwrap_or(false)
}

#[derive(Deserialize, Default)]
pub struct ApiListQuery {
    pub project: Option<String>,
    pub status: Option<String>,
    pub r#type: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Serialize)]
pub struct ApiTicketResponse {
    pub ticket_key: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub ticket_type: String,
    pub assignee: Option<String>,
}

pub async fn list_tickets(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ApiListQuery>,
) -> impl IntoResponse {
    if !verify_api_key(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "認証に失敗しました"})),
        ).into_response();
    }

    let _limit = query.limit.unwrap_or(50);
    let filter = ticket_repo::TicketFilter {
        status: query.status,
        ..Default::default()
    };
    let tickets = ticket_repo::find_all(
        &state.pool, &filter,
    ).await.unwrap_or_default();

    let result: Vec<ApiTicketResponse> = tickets.into_iter().map(|t| {
        ApiTicketResponse {
            ticket_key: t.ticket_key,
            title: t.title,
            status: t.status,
            priority: t.priority,
            ticket_type: t.ticket_type,
            assignee: t.assignee_name,
        }
    }).collect();

    let count = result.len();
    (
        StatusCode::OK,
        Json(serde_json::json!({"tickets": result, "count": count})),
    ).into_response()
}

#[derive(Deserialize)]
pub struct ApiCreateTicket {
    pub title: String,
    pub description: Option<String>,
    pub project: String,
    pub r#type: Option<String>,
    pub priority: Option<String>,
}

pub async fn create_ticket(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ApiCreateTicket>,
) -> impl IntoResponse {
    if !verify_api_key(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "認証に失敗しました"})),
        ).into_response();
    }

    if body.title.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "title は必須です"})),
        ).into_response();
    }

    // プロジェクト名からIDを取得
    let project = crate::infrastructure::repositories::project_repo::find_by_prefix(
        &state.pool, &body.project
    ).await;

    let project_id = match project {
        Ok(Some(p)) => p.id,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "指定されたプロジェクトが見つかりません"})),
            ).into_response();
        }
    };

    // API ユーザー（user_id = 1 をシステムユーザーとして使用）
    let api_user_id = 1;

    let req = ticket_service::CreateTicketRequest {
        title: body.title,
        description: body.description.unwrap_or_default(),
        ticket_type: body.r#type.unwrap_or_else(|| "課題".to_string()),
        priority: body.priority.unwrap_or_else(|| "中".to_string()),
        project_id: Some(project_id),
        category_id: None,
        assignee_id: None,
        milestone_id: None,
        parent_id: None,
        start_date: None,
        due_date: None,
    };

    let prefix = match crate::infrastructure::repositories::project_repo::find_by_id(
        &state.pool, project_id
    ).await {
        Ok(Some(p)) => p.prefix,
        _ => body.project.clone(),
    };

    match ticket_service::create_ticket(&state.pool, &req, api_user_id, &prefix).await {
        Ok(ticket_id) => {
            let ticket_key = if let Ok(Some(t)) = ticket_repo::find_by_id(&state.pool, ticket_id).await {
                t.ticket_key
            } else {
                format!("{}-{}", prefix, ticket_id)
            };
            (
                StatusCode::CREATED,
                Json(serde_json::json!({
                    "status": "ok",
                    "ticket_key": ticket_key,
                    "url": format!("/tickets/{}/", ticket_key)
                })),
            ).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ).into_response(),
    }
}
