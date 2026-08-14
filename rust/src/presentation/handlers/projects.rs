/// presentation/handlers/projects.rs — プロジェクト CRUD + 切替

use askama::Template;
use axum::{extract::{State, Path}, response::{Html, Redirect, IntoResponse}, Extension, Form};
use axum_extra::extract::cookie::{CookieJar, SameSite};
use serde::Deserialize;
use crate::presentation::state::AppState;
use crate::presentation::middleware::auth::{build_cookie, SessionUser, PROJECT_COOKIE};
use crate::presentation::filters;
use crate::presentation::handlers::tickets::build_base_context;
use crate::domain::models::project::Project;
use crate::infrastructure::repositories::project_repo;

#[derive(Template)]
#[template(path = "projects.html")]
struct ProjectsTemplate {
    all_projects: Vec<Project>,
    current_project_id: Option<i32>,
    nav_active: &'static str,
    overdue_count: i64,
    user_is_staff: bool,
    user_initial: String,
    user_display_name: String,
}

pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<SessionUser>,
) -> Html<String> {
    let base = build_base_context(&state, &user).await;

    let tpl = ProjectsTemplate {
        all_projects: base.all_projects,
        current_project_id: base.current_project_id,
        nav_active: "projects",
        overdue_count: base.overdue_count,
        user_is_staff: base.user_is_staff,
        user_initial: base.user_initial,
        user_display_name: base.user_display_name,
    };
    Html(tpl.render().unwrap_or_default())
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
