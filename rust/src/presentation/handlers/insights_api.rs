/// presentation/handlers/insights_api.rs — Insights 集計 API(人 vs AI のサイクルタイム・トリアージ時間・処理総量)
///
/// 設計: docs/design/実装計画_AI自動ワークフローとInsights.md §4 / §5。
/// 集計は、閲覧者が見えるチケットだけを対象にする(`insights_repo` が常に `viewer.scope()` を使う)。
/// 指定されたプロジェクト・チームが見えない場合は、既存のレポート(`reports_api`)と同じ扱いで拒否する。
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use chrono::{Duration, FixedOffset, NaiveDate, Utc};
use serde::Deserialize;
use serde_json::json;

use crate::domain::access::{Action, Viewer};
use crate::infrastructure::repositories::insights_repo::{self, Filters, Granularity};
use crate::presentation::extractors::authorize;
use crate::presentation::state::AppState;

const MAX_PERIOD_DAYS: i64 = 366;
const DEFAULT_PERIOD_DAYS: i64 = 90;

#[derive(Deserialize)]
pub struct InsightsQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    pub project_id: Option<i32>,
    pub team_id: Option<i32>,
    pub ticket_type: Option<String>,
    pub granularity: Option<String>,
}

/// 検証済みの絞り込み。
struct Validated {
    from: NaiveDate,
    to: NaiveDate,
    project_id: Option<i32>,
    team_id: Option<i32>,
    ticket_type: Option<String>,
    granularity: Granularity,
}

fn bad_request(msg: &str) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "detail": msg }))).into_response()
}

fn server_error(e: anyhow::Error) -> Response {
    tracing::error!("[Insights] 処理=集計 結果=失敗 | {:?}", e);
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"detail": "サーバーエラーが発生しました"})),
    )
        .into_response()
}

/// 日本時間の「今日」(日付の区切りは JST。サーバーの時刻設定に依存しない)。
fn today_jst() -> NaiveDate {
    let jst = FixedOffset::east_opt(9 * 3600).expect("JST");
    Utc::now().with_timezone(&jst).date_naive()
}

/// クエリを検証する(純粋関数。`today` を渡してテストできる)。
fn validate(params: &InsightsQuery, today: NaiveDate) -> Result<Validated, &'static str> {
    let parse = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d");
    let to = match params.to.as_deref() {
        Some(s) => parse(s).map_err(|_| "to は YYYY-MM-DD 形式で指定してください")?,
        None => today,
    };
    let from = match params.from.as_deref() {
        Some(s) => parse(s).map_err(|_| "from は YYYY-MM-DD 形式で指定してください")?,
        None => to - Duration::days(DEFAULT_PERIOD_DAYS),
    };
    if from > to {
        return Err("from は to 以前の日付を指定してください");
    }
    if (to - from).num_days() > MAX_PERIOD_DAYS {
        return Err("期間は 366 日以内で指定してください");
    }
    let granularity = match params.granularity.as_deref() {
        None => Granularity::Week,
        Some(g) => {
            Granularity::parse(g).ok_or("granularity は week か month を指定してください")?
        }
    };
    let ticket_type = match params.ticket_type.as_deref() {
        None | Some("") => None,
        Some(t) if t.chars().count() <= 20 => Some(t.to_string()),
        Some(_) => return Err("ticket_type が長すぎます"),
    };
    Ok(Validated {
        from,
        to,
        project_id: params.project_id,
        team_id: params.team_id,
        ticket_type,
        granularity,
    })
}

/// 指定されたプロジェクト・チームが閲覧できるかを確認する(`reports_api::workload` と同じ判定)。
async fn check_access(
    state: &AppState,
    viewer: &Viewer,
    v: &Validated,
    route: &'static str,
) -> Result<(), Response> {
    if let Some(team_id) = v.team_id {
        authorize::gate_team(&state.pool, viewer, team_id, Action::Read, Ok(()), route).await?;
    }
    if let Some(project_id) = v.project_id {
        authorize::gate_project(&state.pool, viewer, project_id, Action::Read, Ok(()), route)
            .await?;
    }
    Ok(())
}

/// GET /api/v1/insights/cycle-time/
pub async fn cycle_time(
    State(state): State<AppState>,
    viewer: Viewer,
    Query(params): Query<InsightsQuery>,
) -> Response {
    const ROUTE: &str = "GET /api/v1/insights/cycle-time/";
    let v = match validate(&params, today_jst()) {
        Ok(v) => v,
        Err(msg) => return bad_request(msg),
    };
    if let Err(resp) = check_access(&state, &viewer, &v, ROUTE).await {
        return resp;
    }
    let scope = viewer.scope();
    let f = filters(&v);
    match insights_repo::cycle_time_stats(&state.pool, &f, &scope).await {
        Ok(r) => (StatusCode::OK, Json(r)).into_response(),
        Err(e) => server_error(e),
    }
}

/// GET /api/v1/insights/ai-usage/
pub async fn ai_usage(
    State(state): State<AppState>,
    viewer: Viewer,
    Query(params): Query<InsightsQuery>,
) -> Response {
    const ROUTE: &str = "GET /api/v1/insights/ai-usage/";
    let v = match validate(&params, today_jst()) {
        Ok(v) => v,
        Err(msg) => return bad_request(msg),
    };
    if let Err(resp) = check_access(&state, &viewer, &v, ROUTE).await {
        return resp;
    }
    let scope = viewer.scope();
    let f = filters(&v);
    match insights_repo::ai_usage_stats(&state.pool, &f, v.granularity, &scope).await {
        Ok(r) => (StatusCode::OK, Json(r)).into_response(),
        Err(e) => server_error(e),
    }
}

/// GET /api/v1/insights/triage-time/
pub async fn triage_time(
    State(state): State<AppState>,
    viewer: Viewer,
    Query(params): Query<InsightsQuery>,
) -> Response {
    const ROUTE: &str = "GET /api/v1/insights/triage-time/";
    let v = match validate(&params, today_jst()) {
        Ok(v) => v,
        Err(msg) => return bad_request(msg),
    };
    if let Err(resp) = check_access(&state, &viewer, &v, ROUTE).await {
        return resp;
    }
    let scope = viewer.scope();
    let f = filters(&v);
    match insights_repo::triage_time_stats(&state.pool, &f, v.granularity, &scope).await {
        Ok(r) => (StatusCode::OK, Json(r)).into_response(),
        Err(e) => server_error(e),
    }
}

/// GET /api/v1/insights/throughput/
pub async fn throughput(
    State(state): State<AppState>,
    viewer: Viewer,
    Query(params): Query<InsightsQuery>,
) -> Response {
    const ROUTE: &str = "GET /api/v1/insights/throughput/";
    let v = match validate(&params, today_jst()) {
        Ok(v) => v,
        Err(msg) => return bad_request(msg),
    };
    if let Err(resp) = check_access(&state, &viewer, &v, ROUTE).await {
        return resp;
    }
    let scope = viewer.scope();
    let f = filters(&v);
    match insights_repo::throughput_stats(&state.pool, &f, v.granularity, &scope).await {
        Ok(r) => (StatusCode::OK, Json(r)).into_response(),
        Err(e) => server_error(e),
    }
}

fn filters(v: &Validated) -> Filters<'_> {
    Filters {
        from: v.from,
        to: v.to,
        project_id: v.project_id,
        team_id: v.team_id,
        ticket_type: v.ticket_type.as_deref(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(from: Option<&str>, to: Option<&str>, g: Option<&str>) -> InsightsQuery {
        InsightsQuery {
            from: from.map(String::from),
            to: to.map(String::from),
            project_id: None,
            team_id: None,
            ticket_type: None,
            granularity: g.map(String::from),
        }
    }
    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn defaults_to_last_90_days_and_week() {
        let v = validate(&q(None, None, None), d("2026-10-07")).unwrap();
        assert_eq!(v.to, d("2026-10-07"));
        assert_eq!(v.from, d("2026-07-09"));
        assert_eq!(v.granularity, Granularity::Week);
    }

    #[test]
    fn rejects_bad_dates_order_range_and_granularity() {
        let today = d("2026-10-07");
        assert!(validate(&q(Some("2026/10/01"), None, None), today).is_err());
        assert!(validate(&q(None, Some("not-a-date"), None), today).is_err());
        assert!(validate(&q(Some("2026-10-05"), Some("2026-10-01"), None), today).is_err());
        assert!(validate(&q(Some("2025-10-05"), Some("2026-10-07"), None), today).is_err());
        assert!(validate(&q(None, None, Some("day")), today).is_err());
    }

    #[test]
    fn accepts_exactly_366_days() {
        assert!(validate(
            &q(Some("2025-10-06"), Some("2026-10-07"), None),
            d("2026-10-07")
        )
        .is_ok());
    }
}
