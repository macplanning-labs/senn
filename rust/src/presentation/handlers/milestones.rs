/// presentation/handlers/milestones.rs — マイルストーン CRUD

use axum::{extract::{State, Path}, response::{Html, Redirect}, Extension, Form};
use serde::Deserialize;
use crate::presentation::state::AppState;
use crate::presentation::middleware::auth::SessionUser;
use crate::infrastructure::repositories::milestone_repo;

fn parse_date(s: Option<&str>) -> Option<chrono::NaiveDate> {
    s.and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
}

pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<SessionUser>,
) -> Html<String> {
    let milestones = milestone_repo::find_all(&state.pool, user.current_project_id)
        .await.unwrap_or_default();
    Html(format!("<h1>マイルストーン</h1><p>{}件</p>", milestones.len()))
}

#[derive(Deserialize)]
pub struct MilestoneForm {
    pub name: String,
    pub due_date: Option<String>,
    pub description: Option<String>,
    pub project_id: Option<i32>,
}

pub async fn create(
    State(state): State<AppState>,
    Form(form): Form<MilestoneForm>,
) -> Redirect {
    let _ = milestone_repo::create(
        &state.pool, &form.name,
        parse_date(form.due_date.as_deref()),
        form.description.as_deref().unwrap_or(""),
        form.project_id,
    ).await;
    Redirect::to("/milestones")
}

pub async fn edit(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Form(form): Form<MilestoneForm>,
) -> Redirect {
    let _ = milestone_repo::update(
        &state.pool, id, &form.name,
        parse_date(form.due_date.as_deref()),
        form.description.as_deref().unwrap_or(""),
    ).await;
    Redirect::to("/milestones")
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Redirect {
    let _ = milestone_repo::delete(&state.pool, id).await;
    Redirect::to("/milestones")
}
