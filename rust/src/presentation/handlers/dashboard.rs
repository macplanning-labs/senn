/// presentation/handlers/dashboard.rs — ダッシュボード
///
/// GET / → ダッシュボード（全画面）
/// GET /dashboard/my-tickets?tab=assigned → 自分の課題（HTMX partial）

use axum::{extract::{State, Query}, response::Html, Extension};
use serde::Deserialize;
use crate::presentation::state::AppState;
use crate::presentation::middleware::auth::SessionUser;
use crate::domain::services::dashboard_service;

pub async fn index(
    State(state): State<AppState>,
    Extension(user): Extension<SessionUser>,
) -> Html<String> {
    let data = dashboard_service::build_dashboard(
        &state.pool, user.user_id, user.current_project_id, "assigned"
    ).await;

    match data {
        Ok(d) => {
            // TODO: Askamaテンプレートに差し替え
            Html(format!(
                "<h1>ダッシュボード</h1><p>ようこそ {}さん</p><p>チケット合計: {}</p>",
                user.display_name, d.total_count
            ))
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
        &state.pool, user.user_id, user.current_project_id, &tab
    ).await;

    match data {
        Ok(d) => Html(format!("<div>自分の課題（{}）: {}件</div>", tab, d.my_tickets.len())),
        Err(_) => Html("<div>データ読み込みエラー</div>".to_string()),
    }
}
