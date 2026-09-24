/// presentation/handlers/reports_api.rs — 稼働レポート JSON API

use axum::{
    extract::{State, Query},
    response::IntoResponse,
    http::StatusCode,
    Json,
    Extension,
};
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::json;

use crate::presentation::state::AppState;
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::infrastructure::repositories::workload_report_repo;

#[derive(Deserialize)]
pub struct WorkloadQuery {
    pub period: Option<String>,
    pub project_id: Option<i32>,
    pub team_id: Option<i32>,
}

/// GET /api/v1/reports/workload/?period=&project_id=&team_id=
pub async fn workload(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Query(params): Query<WorkloadQuery>,
) -> impl IntoResponse {
    let period = params.period.unwrap_or_else(|| "week".to_string());
    let now = Utc::now();
    let days = match period.as_str() {
        "month" => 30,
        "year" => 365,
        _ => 7,
    };
    let start_date = (now - Duration::days(days)).date_naive();
    let end_date = now.date_naive();

    match workload_report_repo::get_workload_report(&state.pool, start_date, end_date, params.project_id, params.team_id).await {
        Ok(r) => (
            StatusCode::OK,
            Json(json!({
                "period": period,
                "startDate": r.start_date.format("%Y-%m-%d").to_string(),
                "endDate": r.end_date.format("%Y-%m-%d").to_string(),
                "summary": {
                    "totalMinutes": r.total_minutes,
                    "totalHours": (r.total_minutes as f64 / 60.0 * 10.0).round() / 10.0,
                    "daysWithWork": r.days_with_work,
                    "avgMinutesPerDay": r.avg_minutes_per_day,
                },
                "daily": r.daily,
                "byUser": r.by_user,
                "byProject": r.by_project,
            })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"detail": "サーバーエラーが発生しました"})),
            )
                .into_response()
        }
    }
}
