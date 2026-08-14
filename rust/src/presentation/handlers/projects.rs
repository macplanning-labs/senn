/// presentation/handlers/projects.rs — プロジェクト CRUD + 切替

use axum::{extract::{State, Path}, response::{Html, Redirect, IntoResponse}, Extension, Form};
use axum_extra::extract::cookie::{CookieJar, SameSite};
use serde::Deserialize;
use crate::presentation::state::AppState;
use crate::presentation::middleware::auth::{build_cookie, PROJECT_COOKIE};
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
    Form(form): Form<SwitchForm>,
) -> impl IntoResponse {
    let jar = CookieJar::new();
    let jar = match form.project_id {
        Some(pid) => jar.add(build_cookie(PROJECT_COOKIE, pid.to_string(), "/", 30 * 24 * 3600, SameSite::Lax, state.config.cookie_secure)),
        None => jar.add(build_cookie(PROJECT_COOKIE, String::new(), "/", 0, SameSite::Lax, state.config.cookie_secure)),
    };
    (jar, Redirect::to("/")).into_response()
}
