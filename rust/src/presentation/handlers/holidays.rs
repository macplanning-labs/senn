/// presentation/handlers/holidays.rs — 休日管理

use askama::Template;
use axum::{extract::{State, Path}, response::{Html, Redirect}, Extension, Form};
use serde::Deserialize;
use crate::presentation::state::AppState;
use crate::presentation::middleware::auth::SessionUser;
use crate::presentation::handlers::tickets::build_base_context;
use crate::domain::models::project::Project;
use crate::domain::models::holiday::Holiday;
use crate::infrastructure::repositories::holiday_repo;

#[derive(Template)]
#[template(path = "holidays.html")]
struct HolidaysTemplate {
    all_projects: Vec<Project>,
    current_project_id: Option<i32>,
    nav_active: &'static str,
    overdue_count: i64,
    user_is_staff: bool,
    user_initial: String,
    user_display_name: String,
    holidays: Vec<Holiday>,
}

pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<SessionUser>,
) -> Html<String> {
    let holidays = holiday_repo::find_all(&state.pool).await.unwrap_or_default();

    let base = build_base_context(&state, &user).await;

    let tpl = HolidaysTemplate {
        all_projects: base.all_projects,
        current_project_id: base.current_project_id,
        nav_active: "holidays",
        overdue_count: base.overdue_count,
        user_is_staff: base.user_is_staff,
        user_initial: base.user_initial,
        user_display_name: base.user_display_name,
        holidays,
    };
    Html(tpl.render().unwrap_or_default())
}

#[derive(Deserialize)]
pub struct BulkAddForm {
    pub year: i32,
}

pub async fn bulk_add(
    State(state): State<AppState>,
    Form(form): Form<BulkAddForm>,
) -> Redirect {
    // 年の祝日データを作成（簡易版 — 将来的にAPIから取得）
    let holidays: Vec<(chrono::NaiveDate, String)> = vec![
        (chrono::NaiveDate::from_ymd_opt(form.year, 1, 1).unwrap(), "元日".to_string()),
        (chrono::NaiveDate::from_ymd_opt(form.year, 2, 11).unwrap(), "建国記念の日".to_string()),
        (chrono::NaiveDate::from_ymd_opt(form.year, 2, 23).unwrap(), "天皇誕生日".to_string()),
        (chrono::NaiveDate::from_ymd_opt(form.year, 4, 29).unwrap(), "昭和の日".to_string()),
        (chrono::NaiveDate::from_ymd_opt(form.year, 5, 3).unwrap(), "憲法記念日".to_string()),
        (chrono::NaiveDate::from_ymd_opt(form.year, 5, 4).unwrap(), "みどりの日".to_string()),
        (chrono::NaiveDate::from_ymd_opt(form.year, 5, 5).unwrap(), "こどもの日".to_string()),
        (chrono::NaiveDate::from_ymd_opt(form.year, 8, 11).unwrap(), "山の日".to_string()),
        (chrono::NaiveDate::from_ymd_opt(form.year, 11, 3).unwrap(), "文化の日".to_string()),
        (chrono::NaiveDate::from_ymd_opt(form.year, 11, 23).unwrap(), "勤労感謝の日".to_string()),
    ];
    let _ = holiday_repo::bulk_add(&state.pool, &holidays).await;
    Redirect::to("/tickets/holidays")
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Redirect {
    let _ = holiday_repo::delete(&state.pool, id).await;
    Redirect::to("/tickets/holidays")
}
