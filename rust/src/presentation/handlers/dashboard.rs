/// presentation/handlers/dashboard.rs — ダッシュボード
///
/// GET / → ダッシュボード（全画面）
/// GET /dashboard/my-tickets?tab=assigned → 自分の課題（HTMX partial）
use askama::Template;
use axum::{
    extract::{Query, State},
    response::Html,
    Extension,
};
use serde::Deserialize;

use crate::domain::models::project::Project;
use crate::domain::services::dashboard_service::{self, DashboardData};
use crate::infrastructure::repositories::project_repo;
use crate::presentation::filters;
use crate::presentation::middleware::auth::SessionUser;
use crate::presentation::state::AppState;

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate {
    all_projects: Vec<Project>,
    current_project_id: Option<i32>,
    nav_active: &'static str,
    overdue_count: i64,
    user_is_staff: bool,
    user_initial: String,
    user_display_name: String,
    data: DashboardData,
}

#[derive(Deserialize)]
pub struct DashboardQuery {
    pub my: Option<String>,
}

pub async fn index(
    State(state): State<AppState>,
    Extension(user): Extension<SessionUser>,
    Query(query): Query<DashboardQuery>,
) -> Html<String> {
    let my_filter = query.my.unwrap_or_else(|| "assigned".to_string());
    let data = dashboard_service::build_dashboard(
        &state.pool,
        user.user_id,
        user.current_project_id,
        &my_filter,
    )
    .await;
    let all_projects = project_repo::find_all(&state.pool)
        .await
        .unwrap_or_default();

    match data {
        Ok(d) => {
            let overdue_count = d.overdue_count;
            let user_initial = user
                .display_name
                .chars()
                .next()
                .map(|c| c.to_string())
                .unwrap_or_default();
            let tpl = DashboardTemplate {
                all_projects,
                current_project_id: user.current_project_id,
                nav_active: "dashboard",
                overdue_count,
                user_is_staff: user.is_staff,
                user_initial,
                user_display_name: user.display_name,
                data: d,
            };
            match tpl.render() {
                Ok(html) => Html(html),
                Err(e) => Html(format!(
                    "<h1>ダッシュボード</h1><p>テンプレートエラー: {}</p>",
                    e
                )),
            }
        }
        Err(_) => Html("<h1>ダッシュボード</h1><p>データ読み込みエラー</p>".to_string()),
    }
}

#[derive(Deserialize)]
pub struct MyTicketsQuery {
    pub tab: Option<String>,
}

pub async fn my_tickets(
    State(state): State<AppState>,
    Extension(user): Extension<SessionUser>,
    Query(query): Query<MyTicketsQuery>,
) -> Html<String> {
    let tab = query.tab.unwrap_or_else(|| "assigned".to_string());
    let data = dashboard_service::build_dashboard(
        &state.pool,
        user.user_id,
        user.current_project_id,
        &tab,
    )
    .await;

    match data {
        Ok(d) => Html(format!(
            "<div>自分の課題（{}）: {}件</div>",
            tab,
            d.my_tickets.len()
        )),
        Err(_) => Html("<div>データ読み込みエラー</div>".to_string()),
    }
}
