/// presentation/handlers/burndown.rs — バーンダウンチャート

use axum::{extract::{State, Query}, response::Html, Extension};
use serde::Deserialize;
use crate::presentation::state::AppState;
use crate::presentation::middleware::auth::SessionUser;
use crate::domain::services::burndown_service;

#[derive(Deserialize, Default)]
pub struct BurndownQuery {
    pub milestone_id: Option<i32>,
}

pub async fn page(
    State(state): State<AppState>,
    Extension(_user): Extension<SessionUser>,
    Query(query): Query<BurndownQuery>,
) -> Html<String> {
    let milestone_id = query.milestone_id.unwrap_or(0);
    if milestone_id == 0 {
        return Html("<h1>バーンダウンチャート</h1><p>マイルストーンを選択してください</p>".to_string());
    }

    let data = burndown_service::build_burndown_data(&state.pool, milestone_id)
        .await;

    match data {
        Ok(points) => {
            let total = points.first().map(|p| p.total).unwrap_or(0);
            let remaining = points.last().map(|p| p.remaining).unwrap_or(0);
            Html(format!(
                "<h1>バーンダウンチャート</h1><p>全チケット: {} / 残り: {}</p>",
                total, remaining
            ))
        }
        Err(e) => Html(format!("<h1>バーンダウンチャート</h1><p>エラー: {}</p>", e)),
    }
}
