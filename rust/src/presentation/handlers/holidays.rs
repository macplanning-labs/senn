/// presentation/handlers/holidays.rs — 休日管理

use axum::{extract::{State, Path}, response::{Html, Redirect}, Form};
use serde::Deserialize;
use crate::presentation::state::AppState;
use crate::infrastructure::repositories::holiday_repo;

pub async fn list(
    State(state): State<AppState>,
) -> Html<String> {
    let holidays = holiday_repo::find_all(&state.pool).await.unwrap_or_default();
    Html(format!("<h1>休日管理</h1><p>{}件</p>", holidays.len()))
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
