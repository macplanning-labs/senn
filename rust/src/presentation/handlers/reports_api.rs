/// presentation/handlers/reports_api.rs — 稼働レポート JSON API
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::json;

use crate::domain::access::{Action, Viewer};
use crate::infrastructure::access::{
    facts_repo,
    shadow::{self, Direction, Mode, Resource},
};
use crate::infrastructure::repositories::workload_report_repo;
use crate::presentation::extractors::authorize;
use crate::presentation::state::AppState;

#[derive(Deserialize)]
pub struct WorkloadQuery {
    pub period: Option<String>,
    pub project_id: Option<i32>,
    pub team_id: Option<i32>,
}

/// GET /api/v1/reports/workload/?period=&project_id=&team_id=
pub async fn workload(
    State(state): State<AppState>,
    viewer: Viewer,
    Query(params): Query<WorkloadQuery>,
) -> impl IntoResponse {
    // アクセス制御の再設計(D-5。設計書 §5.2): 閲覧者が見えるチームの範囲だけで集計する。
    // 切り替えはチームと同じ `ACCESS_ENFORCE_TEAM`。指定されたチーム・プロジェクトは、見えること
    const ROUTE: &str = "GET /api/v1/reports/workload/";
    let server_error = |e: anyhow::Error| {
        tracing::error!("DB operation failed: {:?}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"detail": "サーバーエラーが発生しました"})),
        )
            .into_response()
    };
    if let Some(team_id) = params.team_id {
        let facts = match facts_repo::facts_for_team(&state.pool, team_id).await {
            Ok(f) => f.map(crate::domain::access::ResourceRef::Team),
            Err(e) => return server_error(e),
        };
        if let Err(resp) = authorize::gate(
            &state.pool,
            &viewer,
            facts.as_ref(),
            Action::Read,
            Resource::Team,
            team_id as i64,
            Ok(()),
            ROUTE,
        ) {
            return resp;
        }
    }
    if let Some(project_id) = params.project_id {
        if let Err(resp) = authorize::gate_project(
            &state.pool,
            &viewer,
            project_id,
            Action::Read,
            Ok(()),
            ROUTE,
        )
        .await
        {
            return resp;
        }
    }
    let mode = shadow::mode(Resource::Team);
    let scope = (mode == Mode::On).then(|| viewer.scope());
    let period = params.period.unwrap_or_else(|| "week".to_string());
    let now = Utc::now();
    let days = match period.as_str() {
        "month" => 30,
        "year" => 365,
        _ => 7,
    };
    let start_date = (now - Duration::days(days)).date_naive();
    let end_date = now.date_naive();

    match workload_report_repo::get_workload_report(
        &state.pool,
        start_date,
        end_date,
        params.project_id,
        params.team_id,
        scope.as_ref(),
    )
    .await
    {
        Ok(r) => {
            // 試運転: 見える範囲だけで集計した合計が違えば記録する(resource_id は指定されたチーム。無ければ 0)
            if mode == Mode::Shadow {
                let pool = state.pool.clone();
                let scope = viewer.scope();
                let user_id = viewer.user_id();
                let (project_id, team_id, current) =
                    (params.project_id, params.team_id, r.total_minutes);
                tokio::spawn(async move {
                    match workload_report_repo::get_workload_report(
                        &pool,
                        start_date,
                        end_date,
                        project_id,
                        team_id,
                        Some(&scope),
                    )
                    .await
                    {
                        Ok(new) if new.total_minutes != current => shadow::record(
                            &pool,
                            Resource::Team,
                            user_id,
                            ROUTE,
                            vec![(Direction::NewlyHidden, team_id.unwrap_or(0) as i64)],
                        ),
                        Ok(_) => {}
                        Err(e) => tracing::warn!(
                            "[認可/試運転] 工数レポートの比較に失敗(応答は続行): {:?}",
                            e
                        ),
                    }
                });
            }
            (
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
                .into_response()
        }
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
