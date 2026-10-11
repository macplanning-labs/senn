/// presentation/handlers/ai_runner_api.rs — ランナー(AI を実行する社内マシン)用の入口
///
/// 設計: docs/design/実装計画_AI自動ワークフローとInsights.md §3.6。
/// - 認証は、**ランナー専用のトークン**(`X-Runner-Token`。ハッシュだけを保存)。人ごとの AI キーとは別。
/// - ランナーができるのは、「依頼を 1 件取得する」「自分が取得した実行の着手・ハートビート・完了・失敗を報告する」ことだけ。
///   **他のランナーの実行や、他のチケットは読めない・触れない**(取得した 1 件の指示だけを受け取る)。
/// - パスは `/api/v1/ai-agent/runner/...`(既存の ai-agent の入口と同じ範囲。Cloudflare Access の設定・nginx の変更が不要)。
/// - 着手・完了・失敗の報告と、トークン量の記録は**ランナー(AI の外側)**が行う。AI は SENN の鍵を持たない。
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::domain::services::ai_kick_service::{self, Usage};
use crate::domain::services::ai_run_service::AiRunError;
use crate::infrastructure::repositories::ai_kick_repo::{self, Runner};
use crate::infrastructure::repositories::ai_run_repo::AiRun;
use crate::presentation::state::AppState;

fn detail(status: StatusCode, msg: &str) -> Response {
    (status, Json(json!({ "detail": msg }))).into_response()
}

fn server_error(e: impl std::fmt::Debug) -> Response {
    tracing::error!("[AIランナー] 処理=API 結果=失敗 | {:?}", e);
    detail(
        StatusCode::INTERNAL_SERVER_ERROR,
        "サーバーエラーが発生しました",
    )
}

/// ランナーの呼び出し元。トークンのハッシュで、失効していないランナーを探す。
#[derive(Debug, Clone)]
pub struct RunnerCaller(pub Runner);

pub const RUNNER_TOKEN_HEADER: &str = "X-Runner-Token";

impl axum::extract::FromRequestParts<AppState> for RunnerCaller {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        runner_from_headers(state, &parts.headers)
            .await
            .map(RunnerCaller)
    }
}

async fn runner_from_headers(state: &AppState, headers: &HeaderMap) -> Result<Runner, Response> {
    let unauthorized = || detail(StatusCode::UNAUTHORIZED, "ランナーの認証に失敗しました");
    let token = headers
        .get(RUNNER_TOKEN_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or_else(unauthorized)?;
    // トークンの形式を先に確認する(明らかに違うものは、DB を引かずに拒否する)
    if !token.starts_with("srun_") || token.len() < 40 || token.len() > 200 {
        return Err(unauthorized());
    }
    let hash = ai_kick_repo::hash_token(token);
    let runner = ai_kick_repo::find_active_runner(&state.pool, &hash)
        .await
        .map_err(server_error)?
        .ok_or_else(unauthorized)?;
    if let Err(e) = ai_kick_repo::touch_runner(&state.pool, runner.id).await {
        tracing::warn!("[AIランナー] 処理=最終接続の更新 結果=失敗 | {:?}", e);
    }
    Ok(runner)
}

/// 自分が取得した実行だけを取る。他のランナーの実行・存在しない実行は、同じ 404(存在を教えない)。
async fn own_run(state: &AppState, runner: &Runner, run_uuid: &Uuid) -> Result<AiRun, Response> {
    ai_kick_repo::find_run_of_runner(&state.pool, runner.id, run_uuid)
        .await
        .map_err(server_error)?
        .ok_or_else(|| detail(StatusCode::NOT_FOUND, "実行が見つかりません"))
}

async fn ai_user_id(state: &AppState) -> Result<i32, Response> {
    crate::presentation::handlers::ai_agent_api::resolve_shared_ai_user(state)
        .await
        .map_err(|(status, body)| (status, Json(body)).into_response())
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClaimBody {
    /// false なら、依頼は取らず、接続だけ知らせる(実行してよい時間帯の外で待機しているとき)
    #[serde(default = "yes")]
    pub accepting: bool,
}
fn yes() -> bool {
    true
}

/// POST /api/v1/ai-agent/runner/claim/
pub async fn claim(
    State(state): State<AppState>,
    RunnerCaller(runner): RunnerCaller,
    body: Option<Json<ClaimBody>>,
) -> Response {
    let accepting = body.map(|Json(b)| b.accepting).unwrap_or(true);
    if !accepting {
        return (
            StatusCode::OK,
            Json(json!({ "job": null, "accepting": false })),
        )
            .into_response();
    }
    match ai_kick_service::claim(&state.pool, &runner, &state.config.jwt_secret).await {
        Ok(job) => (StatusCode::OK, Json(json!({ "job": job }))).into_response(),
        Err(e) => server_error(e),
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StartBody {
    pub model: Option<String>,
}

/// POST /api/v1/ai-agent/runner/runs/{run_uuid}/start/
pub async fn start(
    State(state): State<AppState>,
    RunnerCaller(runner): RunnerCaller,
    Path(run_uuid): Path<Uuid>,
    body: Option<Json<StartBody>>,
) -> Response {
    let run = match own_run(&state, &runner, &run_uuid).await {
        Ok(r) => r,
        Err(r) => return r,
    };
    // 停止を求められた実行は、始めさせない
    if run.stop_requested_at.is_some() || run.result.is_some() {
        return (
            StatusCode::OK,
            Json(json!({ "stop": true, "reason": run.stop_reason })),
        )
            .into_response();
    }
    let ai = match ai_user_id(&state).await {
        Ok(i) => i,
        Err(r) => return r,
    };
    let model = body.and_then(|Json(b)| b.model);
    match ai_kick_service::start(&state.pool, ai, &run, model).await {
        Ok(()) => (StatusCode::OK, Json(json!({ "stop": false }))).into_response(),
        Err(AiRunError::NotFound) => detail(StatusCode::NOT_FOUND, "実行が見つかりません"),
        Err(AiRunError::Conflict(m)) | Err(AiRunError::BadRequest(m)) => {
            detail(StatusCode::BAD_REQUEST, &m)
        }
        Err(AiRunError::Internal(e)) => server_error(e),
    }
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct UsageBody {
    #[serde(default)]
    pub input_tokens: i64,
    #[serde(default)]
    pub output_tokens: i64,
    #[serde(default)]
    pub cache_creation_tokens: i64,
    #[serde(default)]
    pub cache_read_tokens: i64,
}

impl UsageBody {
    /// 極端な値(負・非現実的に大きい)は、安全側に丸める。
    fn usage(self) -> Usage {
        const MAX: i64 = 10_000_000_000;
        let c = |v: i64| v.clamp(0, MAX);
        Usage {
            input_tokens: c(self.input_tokens),
            output_tokens: c(self.output_tokens),
            cache_creation_tokens: c(self.cache_creation_tokens),
            cache_read_tokens: c(self.cache_read_tokens),
        }
    }
}

/// POST /api/v1/ai-agent/runner/runs/{run_uuid}/heartbeat/ — 使用量を送り、続けてよいかを受け取る。
pub async fn heartbeat(
    State(state): State<AppState>,
    RunnerCaller(runner): RunnerCaller,
    Path(run_uuid): Path<Uuid>,
    Json(body): Json<UsageBody>,
) -> Response {
    let run = match own_run(&state, &runner, &run_uuid).await {
        Ok(r) => r,
        Err(r) => return r,
    };
    if run.result.is_some() {
        // すでに終わった実行には、止める指示を返す(ランナーはプロセスを終了する)
        return (
            StatusCode::OK,
            Json(json!({ "stop": true, "reason": run.stop_reason.clone().unwrap_or_else(|| "finished".into()) })),
        )
            .into_response();
    }
    match ai_kick_service::heartbeat(&state.pool, &run, &body.usage()).await {
        Ok(out) => (
            StatusCode::OK,
            Json(json!({ "stop": out.stop_reason.is_some(), "reason": out.stop_reason })),
        )
            .into_response(),
        Err(e) => server_error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinishBody {
    /// resolved | blocked | aborted
    pub result: String,
    #[serde(default)]
    pub summary: String,
    #[serde(flatten)]
    pub usage: UsageBody,
    pub usd_equivalent: Option<f64>,
}

/// POST /api/v1/ai-agent/runner/runs/{run_uuid}/finish/
pub async fn finish(
    State(state): State<AppState>,
    RunnerCaller(runner): RunnerCaller,
    Path(run_uuid): Path<Uuid>,
    Json(body): Json<FinishBody>,
) -> Response {
    if body.summary.chars().count() > 2000 {
        return detail(
            StatusCode::BAD_REQUEST,
            "summary は 2000 字以内で指定してください",
        );
    }
    let run = match own_run(&state, &runner, &run_uuid).await {
        Ok(r) => r,
        Err(r) => return r,
    };
    let ai = match ai_user_id(&state).await {
        Ok(i) => i,
        Err(r) => return r,
    };
    match ai_kick_service::finish(
        &state.pool,
        ai,
        &run,
        &body.result,
        &body.summary,
        &body.usage.usage(),
        body.usd_equivalent.filter(|v| v.is_finite() && *v >= 0.0),
    )
    .await
    {
        Ok(()) => (StatusCode::OK, Json(json!({ "recorded": true }))).into_response(),
        Err(AiRunError::NotFound) => detail(StatusCode::NOT_FOUND, "実行が見つかりません"),
        Err(AiRunError::BadRequest(m)) | Err(AiRunError::Conflict(m)) => {
            detail(StatusCode::BAD_REQUEST, &m)
        }
        Err(AiRunError::Internal(e)) => server_error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorBody {
    pub detail: String,
    #[serde(flatten)]
    pub usage: UsageBody,
}

/// POST /api/v1/ai-agent/runner/runs/{run_uuid}/error/
pub async fn error(
    State(state): State<AppState>,
    RunnerCaller(runner): RunnerCaller,
    Path(run_uuid): Path<Uuid>,
    Json(body): Json<ErrorBody>,
) -> Response {
    let n = body.detail.chars().count();
    if n == 0 || n > 2000 {
        return detail(
            StatusCode::BAD_REQUEST,
            "detail は 1〜2000 字で指定してください",
        );
    }
    let run = match own_run(&state, &runner, &run_uuid).await {
        Ok(r) => r,
        Err(r) => return r,
    };
    let ai = match ai_user_id(&state).await {
        Ok(i) => i,
        Err(r) => return r,
    };
    match ai_kick_service::report_failure(&state.pool, ai, &run, &body.detail, &body.usage.usage())
        .await
    {
        Ok(()) => (StatusCode::OK, Json(json!({ "recorded": true }))).into_response(),
        Err(AiRunError::NotFound) => detail(StatusCode::NOT_FOUND, "実行が見つかりません"),
        Err(AiRunError::BadRequest(m)) | Err(AiRunError::Conflict(m)) => {
            detail(StatusCode::BAD_REQUEST, &m)
        }
        Err(AiRunError::Internal(e)) => server_error(e),
    }
}
