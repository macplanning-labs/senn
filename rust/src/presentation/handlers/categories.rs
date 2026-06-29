/// presentation/handlers/categories.rs — カテゴリー CRUD

use axum::{extract::{State, Path}, response::{Html, Redirect}, Form};
use serde::Deserialize;
use crate::presentation::state::AppState;
use crate::infrastructure::repositories::category_repo;

pub async fn list(
    State(state): State<AppState>,
) -> Html<String> {
    let categories = category_repo::find_all(&state.pool).await.unwrap_or_default();
    Html(format!("<h1>カテゴリー設定</h1><p>{}件</p>", categories.len()))
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
