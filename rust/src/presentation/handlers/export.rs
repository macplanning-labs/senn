/// presentation/handlers/export.rs — Excelエクスポート

use axum::{
    extract::{State, Query},
    response::IntoResponse,
    http::{header, StatusCode},
    Extension,
};
use serde::Deserialize;
use crate::presentation::state::AppState;
use crate::presentation::middleware::auth::SessionUser;
use crate::domain::services::{export_service, gantt_service};
use crate::infrastructure::repositories::{ticket_repo, holiday_repo, milestone_repo};

#[derive(Deserialize, Default)]
pub struct ExportQuery {
    pub status: Option<String>,
    pub priority: Option<String>,
    pub milestone_id: Option<i32>,
}

pub async fn ticket_excel(
    State(state): State<AppState>,
    Extension(user): Extension<SessionUser>,
    Query(query): Query<ExportQuery>,
) -> impl IntoResponse {
    let filter = ticket_repo::TicketFilter {
        project_id: user.current_project_id,
        status: query.status,
        priority: query.priority,
        milestone_id: query.milestone_id,
        ..Default::default()
    };

    let tickets = ticket_repo::find_all(&state.pool, &filter)
        .await.unwrap_or_default();

    match export_service::export_ticket_list(&tickets) {
        Ok(bytes) => {
            let headers = [
                (header::CONTENT_TYPE, "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
                (header::CONTENT_DISPOSITION, "attachment; filename=\"tickets.xlsx\""),
            ];
            (headers, bytes).into_response()
        }
        Err(e) => {
            tracing::error!("Excel生成エラー: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn gantt_excel(
    State(state): State<AppState>,
    Extension(user): Extension<SessionUser>,
) -> impl IntoResponse {
    let gantt_data = gantt_service::build_gantt_data(&state.pool, user.current_project_id)
        .await.unwrap_or_default();

    // 日付範囲（最小開始日〜最大終了日）
    let today = chrono::Local::now().date_naive();
    let start = gantt_data.iter()
        .filter_map(|r| r.ticket.start_date)
        .min()
        .unwrap_or(today);
    let end = gantt_data.iter()
        .filter_map(|r| r.ticket.due_date)
        .max()
        .unwrap_or(today + chrono::Duration::days(30));

    let holidays = holiday_repo::find_dates(&state.pool).await.unwrap_or_default();

    let milestones_list = milestone_repo::find_all(&state.pool, user.current_project_id)
        .await.unwrap_or_default();
    let ms_dates: Vec<(chrono::NaiveDate, String)> = milestones_list.iter()
        .filter_map(|m| m.due_date.map(|d| (d, m.name.clone())))
        .collect();

    match export_service::export_gantt(&gantt_data, (start, end), &holidays, &ms_dates) {
        Ok(bytes) => {
            let headers = [
                (header::CONTENT_TYPE, "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
                (header::CONTENT_DISPOSITION, "attachment; filename=\"gantt.xlsx\""),
            ];
            (headers, bytes).into_response()
        }
        Err(e) => {
            tracing::error!("ガントExcel生成エラー: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
