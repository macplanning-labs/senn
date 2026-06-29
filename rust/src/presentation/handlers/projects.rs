/// presentation/handlers/projects.rs — プロジェクト CRUD + 切替

use axum::{extract::{State, Path}, response::{Html, Redirect, IntoResponse}, Extension, Form};
use serde::Deserialize;
use tower_sessions::Session;
use crate::presentation::state::AppState;
use crate::presentation::middleware::auth::SessionUser;
use crate::infrastructure::repositories::project_repo;

pub async fn list(
    State(state): State<AppState>,
) -> Html<String> {
    let projects = project_repo::find_all(&state.pool).await.unwrap_or_default();
    Html(format!("<h1>プロジェクト一覧</h1><p>{}件</p>", projects.len()))
}

#[derive(Deserialize)]
pub struct ProjectForm {
    pub name: String,
    pub prefix: String,
    pub description: Option<String>,
}

pub async fn create(
    State(state): State<AppState>,
    Form(form): Form<ProjectForm>,
) -> Redirect {
    let _ = project_repo::create(
        &state.pool, &form.name, &form.prefix,
        form.description.as_deref().unwrap_or(""),
    ).await;
    Redirect::to("/tickets/projects")
}

pub async fn edit(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Form(form): Form<ProjectForm>,
) -> Redirect {
    let _ = project_repo::update(
        &state.pool, id, &form.name, &form.prefix,
        form.description.as_deref().unwrap_or(""),
    ).await;
    Redirect::to("/tickets/projects")
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Redirect {
    let _ = project_repo::delete(&state.pool, id).await;
    Redirect::to("/tickets/projects")
}

#[derive(Deserialize)]
pub struct SwitchForm {
    pub project_id: Option<i32>,
}

pub async fn switch(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<SwitchForm>,
) -> Redirect {
    let mut user: SessionUser = session.get("user").await.unwrap_or(None).unwrap_or_else(|| {
        SessionUser {
            user_id: 0, username: String::new(), display_name: String::new(),
            is_staff: false, must_change_password: false,
            current_project_id: None, current_project_name: None,
        }
    });

    if let Some(pid) = form.project_id {
        if let Ok(Some(p)) = project_repo::find_by_id(&state.pool, pid).await {
            user.current_project_id = Some(p.id);
            user.current_project_name = Some(p.name);
        }
    } else {
        user.current_project_id = None;
        user.current_project_name = None;
    }

    let _ = session.insert("user", user).await;
    Redirect::to("/")
}
