/// presentation/handlers/gantt.rs — ガントチャートハンドラ

use axum::{extract::State, response::Html, Extension, Json};
use serde::Deserialize;
use crate::presentation::state::AppState;
use crate::presentation::middleware::auth::SessionUser;
use crate::domain::services::gantt_service;
use crate::infrastructure::repositories::ticket_repo;

pub async fn page(
    State(state): State<AppState>,
    Extension(user): Extension<SessionUser>,
) -> Html<String> {
    let data = gantt_service::build_gantt_data(&state.pool, user.current_project_id)
        .await.unwrap_or_default();
    // TODO: Askamaテンプレートに差し替え
    Html(format!("<h1>ガントチャート</h1><p>{}件のチケット</p>", data.len()))
}

#[derive(Deserialize)]
pub struct ReorderItem {
    pub ticket_id: i32,
    pub gantt_order: i32,
}

pub async fn reorder(
    State(state): State<AppState>,
    Json(items): Json<Vec<ReorderItem>>,
) -> Json<serde_json::Value> {
    for item in &items {
        let _ = ticket_repo::update_gantt_order(&state.pool, item.ticket_id, item.gantt_order).await;
    }
    Json(serde_json::json!({"status": "ok"}))
}

#[derive(Deserialize)]
pub struct UpdateDates {
    pub ticket_id: i32,
    pub start_date: Option<String>,
    pub due_date: Option<String>,
}

pub async fn update_dates(
    State(state): State<AppState>,
    Json(data): Json<UpdateDates>,
) -> Json<serde_json::Value> {
    let start = data.start_date.as_deref()
        .and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
    let end = data.due_date.as_deref()
        .and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
    let _ = ticket_repo::update_dates(&state.pool, data.ticket_id, start, end).await;
    Json(serde_json::json!({"status": "ok"}))
}
