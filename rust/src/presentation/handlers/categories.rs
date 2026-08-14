/// presentation/handlers/categories.rs — カテゴリー CRUD

use askama::Template;
use axum::{extract::{State, Path}, response::{Html, Redirect}, Extension, Form};
use serde::Deserialize;
use crate::presentation::state::AppState;
use crate::presentation::middleware::auth::SessionUser;
use crate::presentation::filters;
use crate::presentation::handlers::tickets::build_base_context;
use crate::domain::models::project::Project;
use crate::domain::models::category::Category;
use crate::infrastructure::repositories::category_repo;

#[derive(Template)]
#[template(path = "categories.html")]
struct CategoriesTemplate {
    all_projects: Vec<Project>,
    current_project_id: Option<i32>,
    nav_active: &'static str,
    overdue_count: i64,
    user_is_staff: bool,
    user_initial: String,
    user_display_name: String,
    categories: Vec<Category>,
}

pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<SessionUser>,
) -> Html<String> {
    let categories = category_repo::find_all(&state.pool).await.unwrap_or_default();

    let base = build_base_context(&state, &user).await;

    let tpl = CategoriesTemplate {
        all_projects: base.all_projects,
        current_project_id: base.current_project_id,
        nav_active: "categories",
        overdue_count: base.overdue_count,
        user_is_staff: base.user_is_staff,
        user_initial: base.user_initial,
        user_display_name: base.user_display_name,
        categories,
    };
    Html(tpl.render().unwrap_or_default())
}

#[derive(Deserialize)]
pub struct CategoryForm {
    pub name: String,
    pub slug: String,
    pub level: i16,
    pub parent_id: Option<i32>,
    pub color: Option<String>,
    pub sort_order: Option<i32>,
}

pub async fn create(
    State(state): State<AppState>,
    Form(form): Form<CategoryForm>,
) -> Redirect {
    let _ = category_repo::create(
        &state.pool, &form.name, &form.slug, form.level,
        form.parent_id, form.sort_order.unwrap_or(0),
        form.color.as_deref().unwrap_or("#6366f1"),
    ).await;
    Redirect::to("/admin/categories")
}

pub async fn edit(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Form(form): Form<CategoryForm>,
) -> Redirect {
    let _ = category_repo::update(
        &state.pool, id, &form.name,
        form.color.as_deref().unwrap_or("#6366f1"),
        form.sort_order.unwrap_or(0),
    ).await;
    Redirect::to("/admin/categories")
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Redirect {
    let _ = category_repo::delete(&state.pool, id).await;
    Redirect::to("/admin/categories")
}
