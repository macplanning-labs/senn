/// presentation/handlers/gantt.rs — ガントチャートハンドラ

use askama::Template;
use axum::{extract::State, response::Html, Extension, Json};
use serde::Deserialize;
use crate::presentation::state::AppState;
use crate::presentation::middleware::auth::SessionUser;
use crate::presentation::filters;
use crate::presentation::handlers::tickets::build_base_context;
use crate::domain::models::project::Project;
use crate::domain::services::gantt_service::{self, GanttRow};
use crate::infrastructure::repositories::ticket_repo;

#[derive(Template)]
#[template(path = "gantt.html")]
struct GanttTemplate {
    all_projects: Vec<Project>,
    current_project_id: Option<i32>,
    nav_active: &'static str,
    overdue_count: i64,
    user_is_staff: bool,
    user_initial: String,
    user_display_name: String,
    rows: Vec<GanttRow>,
}

pub async fn page(
    State(state): State<AppState>,
    Extension(user): Extension<SessionUser>,
) -> Html<String> {
    let rows = gantt_service::build_gantt_data(&state.pool, user.current_project_id)
        .await.unwrap_or_default();

    let base = build_base_context(&state, &user).await;

    let tpl = GanttTemplate {
        all_projects: base.all_projects,
        current_project_id: base.current_project_id,
        nav_active: "gantt",
        overdue_count: base.overdue_count,
        user_is_staff: base.user_is_staff,
        user_initial: base.user_initial,
        user_display_name: base.user_display_name,
        rows,
    };
    Html(tpl.render().unwrap_or_default())
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
