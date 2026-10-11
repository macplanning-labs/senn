/// presentation/handlers/ai_kick_api.rs — 「AIに対応させる」(人が使う側。JWT 認証)
///
/// 設計: docs/design/実装計画_AI自動ワークフローとInsights.md §3.6。
/// - `POST /tickets/{key}/ai-kick/preview/`: 送る内容(指示)と、優先度の区分・警告・可否の見込みを返す。依頼は積まない。
/// - `POST /tickets/{key}/ai-kick/`: 依頼を積む。**起動できるのは、ログインした人だけ**(AI・キー・ランナーからは不可)。
///   プレビューで見た内容と同じ(ハッシュが一致する)ときだけ積む。警告の確認が要るときは 428 を返す。
/// - `POST /tickets/{key}/ai-kick/{run_uuid}/cancel/`: 取り消し。
/// - `GET /tickets/{key}/ai-runs/`: 最近の実行(状態・トークン)。
/// - `GET/PUT /projects/{id}/ai-kick/`: プロジェクトの許可とリポジトリ(オーナー・管理者のみ変更できる)。
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::domain::access::{Action, Viewer};
use crate::domain::services::ai_kick_policy::{KickDenied, Verdict};
use crate::domain::services::ai_kick_service::{self, KickError, KickInput};
use crate::infrastructure::repositories::ai_run_repo::AiRun;
use crate::infrastructure::repositories::{
    ai_kick_repo, ai_repo, project_team_repo, ticket_repo, user_repo,
};
use crate::presentation::extractors::authorize;
use crate::presentation::state::AppState;

fn detail(status: StatusCode, msg: &str) -> Response {
    (status, Json(json!({ "detail": msg }))).into_response()
}

fn server_error(e: impl std::fmt::Debug) -> Response {
    tracing::error!("[AI自動実行] 処理=API 結果=失敗 | {:?}", e);
    detail(
        StatusCode::INTERNAL_SERVER_ERROR,
        "サーバーエラーが発生しました",
    )
}

/// 起動を拒否したときの HTTP 応答(理由のコードと日本語のメッセージ)。
fn denied(d: KickDenied) -> Response {
    let status = match d {
        KickDenied::TicketBusy => StatusCode::CONFLICT,
        KickDenied::UserHourly
        | KickDenied::UserDaily
        | KickDenied::TicketLimit
        | KickDenied::QueueFull => StatusCode::TOO_MANY_REQUESTS,
        _ => StatusCode::FORBIDDEN,
    };
    (
        status,
        Json(json!({ "detail": d.to_string(), "code": d.code() })),
    )
        .into_response()
}

/// 人(ブラウザ・デスクトップアプリでログインした人)だけが起動できる。
/// AI・キー・ランナー・外部連携からは起動できない(AI が AI を呼ぶ連鎖を作らない)。
#[allow(clippy::result_large_err)] // 呼び出し側でそのまま応答として返すため、Response を持つ
fn require_human(state: &AppState, viewer: &Viewer) -> Result<i32, Response> {
    let _ = state;
    if viewer.principal.is_key() {
        return Err(detail(
            StatusCode::FORBIDDEN,
            "AI 自動実行を起動できるのは、ログインした人だけです",
        ));
    }
    if !viewer.is_full_member() {
        return Err(detail(
            StatusCode::FORBIDDEN,
            "ゲストは AI 自動実行を起動できません",
        ));
    }
    viewer.require_user_id()
}

struct TicketRef {
    id: i32,
    key: String,
    project_id: i32,
}

/// チケットを引いて、書き込み(または読み取り)の権限を確認する。
async fn load_ticket(
    state: &AppState,
    viewer: &Viewer,
    ticket_key: &str,
    action: Action,
    route: &'static str,
) -> Result<TicketRef, Response> {
    let row: Option<(i32, Option<i32>)> = sqlx::query_as(
        "SELECT id::int4, project_id::int4 FROM tickets_ticket WHERE ticket_key = $1",
    )
    .bind(ticket_key)
    .fetch_optional(&state.pool)
    .await
    .map_err(server_error)?;
    let Some((id, project_id)) = row else {
        return Err(detail(StatusCode::NOT_FOUND, "見つかりません"));
    };
    authorize::authorize_ticket(state, viewer, id, action, route).await?;
    let Some(project_id) = project_id else {
        return Err(detail(
            StatusCode::BAD_REQUEST,
            "プロジェクトに属さないチケットは、AI 自動実行の対象外です",
        ));
    };
    Ok(TicketRef {
        id,
        key: ticket_key.to_string(),
        project_id,
    })
}

/// 共有の AI アカウントでないこと(念のため。このアカウントはログインできないが、取り違えを防ぐ)。
async fn reject_ai_account(state: &AppState, user_id: i32) -> Result<(), Response> {
    match user_repo::find_by_username(&state.pool, &state.config.wip_ai_api_user).await {
        Ok(Some(u)) if u.id == user_id => Err(detail(
            StatusCode::FORBIDDEN,
            "AI のアカウントからは起動できません",
        )),
        Ok(_) => Ok(()),
        Err(e) => Err(server_error(e)),
    }
}

/// チケットの内容(要約済みの指示)。キャッシュがあればそれを使う。`generate` のときは、無ければ生成する
/// (時間がかかる場合がある)。それでも無ければ、タイトルと説明から作る。
async fn ticket_text(state: &AppState, t: &TicketRef, generate: bool) -> Result<String, Response> {
    let mut cached = ai_repo::get_ticket_ai_prompt(&state.pool, t.id)
        .await
        .map_err(server_error)?
        .filter(|s| !s.trim().is_empty());
    if cached.is_none() && generate {
        let ai_config = state.ai_config().await;
        if let Err(e) = crate::domain::services::ai_service::generate_and_cache_ai_prompt(
            t.id,
            &state.pool,
            &ai_config,
        )
        .await
        {
            tracing::warn!(
                ticket_id = t.id,
                "generate_and_cache_ai_prompt failed: {e:#}"
            );
        }
        cached = ai_repo::get_ticket_ai_prompt(&state.pool, t.id)
            .await
            .map_err(server_error)?
            .filter(|s| !s.trim().is_empty());
    }
    if let Some(c) = cached {
        return Ok(c);
    }
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT title, description FROM tickets_ticket WHERE id = $1::int4")
            .bind(t.id)
            .fetch_optional(&state.pool)
            .await
            .map_err(server_error)?;
    let (title, description) = row.unwrap_or_default();
    Ok(format!("# {}\n\n{}", title, description))
}

/// ゲストが書いた内容(チケット・コメント)を含むか。含むなら、起動前に確認を促す。
async fn has_guest_content(state: &AppState, ticket_id: i32) -> Result<bool, Response> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM tickets_ticket t JOIN accounts_user u ON u.id = t.author_id
                        WHERE t.id = $1 AND u.is_guest)
             OR EXISTS (SELECT 1 FROM tickets_comment c JOIN accounts_user u ON u.id = c.author_id
                        WHERE c.ticket_id = $1 AND u.is_guest)",
    )
    .bind(ticket_id)
    .fetch_one(&state.pool)
    .await
    .map_err(server_error)
}

fn verdict_json(v: &Verdict) -> Value {
    match v {
        Verdict::Allow { warnings } => json!({ "verdict": "allow", "warnings": warnings }),
        Verdict::NeedsConfirm { warnings } => {
            json!({ "verdict": "needs_confirm", "warnings": warnings })
        }
        Verdict::Deny(d) => json!({
            "verdict": "deny",
            "warnings": [],
            "denied": { "code": d.code(), "message": d.to_string() },
        }),
    }
}

/// POST /api/v1/tickets/{ticket_key}/ai-kick/preview/
pub async fn preview(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(ticket_key): Path<String>,
) -> Response {
    const ROUTE: &str = "POST /api/v1/tickets/{ticket_key}/ai-kick/preview/";
    let user_id = match require_human(&state, &viewer) {
        Ok(u) => u,
        Err(r) => return r,
    };
    let t = match load_ticket(&state, &viewer, &ticket_key, Action::Write, ROUTE).await {
        Ok(t) => t,
        Err(r) => return r,
    };
    if let Err(r) = reject_ai_account(&state, user_id).await {
        return r;
    }
    let text = match ticket_text(&state, &t, true).await {
        Ok(x) => x,
        Err(r) => return r,
    };
    let prompt = ai_kick_service::build_kick_prompt(&t.key, &text);
    let hash = ai_kick_service::prompt_hash(&prompt);
    let pv = match ai_kick_service::preview_kick(
        &state.pool,
        t.id,
        t.project_id,
        user_id,
        viewer.is_system_admin(),
    )
    .await
    {
        Ok(p) => p,
        Err(KickError::NotFound) => return detail(StatusCode::NOT_FOUND, "見つかりません"),
        Err(KickError::Internal(e)) => return server_error(e),
        Err(KickError::Denied(d)) => return denied(d),
        Err(KickError::NeedsConfirm(_)) => unreachable!("preview は確認を要求しない"),
    };
    let project = match ai_kick_repo::get_project_kick(&state.pool, t.project_id).await {
        Ok(p) => p,
        Err(e) => return server_error(e),
    };
    let external = match has_guest_content(&state, t.id).await {
        Ok(b) => b,
        Err(r) => return r,
    };
    let mut body = verdict_json(&pv.verdict);
    // 外から入りうる内容を含むときは、警告を足す(確認を促す)
    if external {
        if let Some(arr) = body.get_mut("warnings").and_then(|w| w.as_array_mut()) {
            arr.push(json!({
                "code": "external_content",
                "message": "ゲストが書いた内容を含みます。AI に渡る内容を、よく確認してください",
            }));
        }
    }
    let obj = body.as_object_mut().expect("object");
    obj.insert("ticketKey".into(), json!(t.key));
    obj.insert("prompt".into(), json!(prompt));
    obj.insert("promptHash".into(), json!(hash));
    obj.insert("priority".into(), json!(pv.classified.priority));
    obj.insert("priorityClass".into(), json!(pv.classified.class.as_str()));
    obj.insert(
        "priorityLabel".into(),
        json!(pv.classified.class.label_ja()),
    );
    obj.insert("blocksOpen".into(), json!(pv.classified.blocks_open));
    obj.insert("caps".into(), json!(pv.caps));
    obj.insert(
        "repo".into(),
        json!(project.as_ref().map(|p| p.ai_kick_repo.clone())),
    );
    obj.insert(
        "baseBranch".into(),
        json!(project.as_ref().map(|p| p.ai_kick_base_branch.clone())),
    );
    obj.insert("externalContent".into(), json!(external));
    (StatusCode::OK, Json(body)).into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KickBody {
    /// プレビューで見た内容のハッシュ(必須)。違えば、内容が変わったとして拒否する
    pub prompt_hash: String,
    /// 警告を見たうえで、「承知して実行する」を選んだ
    #[serde(default)]
    pub confirmed: bool,
}

/// POST /api/v1/tickets/{ticket_key}/ai-kick/
pub async fn kick(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(ticket_key): Path<String>,
    Json(body): Json<KickBody>,
) -> Response {
    const ROUTE: &str = "POST /api/v1/tickets/{ticket_key}/ai-kick/";
    let user_id = match require_human(&state, &viewer) {
        Ok(u) => u,
        Err(r) => return r,
    };
    let t = match load_ticket(&state, &viewer, &ticket_key, Action::Write, ROUTE).await {
        Ok(t) => t,
        Err(r) => return r,
    };
    if let Err(r) = reject_ai_account(&state, user_id).await {
        return r;
    }
    // 確認した内容と、いま送る内容が同じか(確認のあとにチケットが変わっていないか)
    let text = match ticket_text(&state, &t, false).await {
        Ok(x) => x,
        Err(r) => return r,
    };
    let prompt = ai_kick_service::build_kick_prompt(&t.key, &text);
    if ai_kick_service::prompt_hash(&prompt) != body.prompt_hash {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "detail": "確認したあとに、チケットの内容が変わりました。もう一度、内容を確認してください",
                "code": "prompt_changed",
            })),
        )
            .into_response();
    }
    match ai_kick_service::request_kick(
        &state.pool,
        KickInput {
            ticket_id: t.id,
            project_id: t.project_id,
            user_id,
            is_admin: viewer.is_system_admin(),
            confirmed: body.confirmed,
            prompt: &prompt,
        },
    )
    .await
    {
        Ok(run) => (
            StatusCode::ACCEPTED,
            Json(json!({
                "runUuid": run.run_uuid,
                "status": "queued",
                "priorityClass": run.priority_class,
            })),
        )
            .into_response(),
        Err(KickError::Denied(d)) => denied(d),
        Err(KickError::NeedsConfirm(warnings)) => (
            StatusCode::PRECONDITION_REQUIRED,
            Json(json!({
                "detail": "警告を確認して、「承知して実行する」を選んでください",
                "code": "needs_confirm",
                "warnings": warnings,
            })),
        )
            .into_response(),
        Err(KickError::NotFound) => detail(StatusCode::NOT_FOUND, "見つかりません"),
        Err(KickError::Internal(e)) => server_error(e),
    }
}

/// POST /api/v1/tickets/{ticket_key}/ai-kick/{run_uuid}/cancel/
pub async fn cancel(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((ticket_key, run_uuid)): Path<(String, Uuid)>,
) -> Response {
    const ROUTE: &str = "POST /api/v1/tickets/{ticket_key}/ai-kick/{run_uuid}/cancel/";
    let user_id = match require_human(&state, &viewer) {
        Ok(u) => u,
        Err(r) => return r,
    };
    let t = match load_ticket(&state, &viewer, &ticket_key, Action::Write, ROUTE).await {
        Ok(t) => t,
        Err(r) => return r,
    };
    let run: Option<AiRun> = match sqlx::query_as::<_, AiRun>(
        "SELECT * FROM t_ai_run WHERE run_uuid = $1 AND ticket_id = $2 AND queued_at IS NOT NULL",
    )
    .bind(run_uuid)
    .bind(i64::from(t.id))
    .fetch_optional(&state.pool)
    .await
    {
        Ok(r) => r,
        Err(e) => return server_error(e),
    };
    let Some(run) = run else {
        return detail(StatusCode::NOT_FOUND, "実行が見つかりません");
    };
    // 起動した人か、管理者だけが取り消せる
    if run.triggered_by_id != Some(i64::from(user_id)) && !viewer.is_system_admin() {
        return detail(
            StatusCode::FORBIDDEN,
            "取り消せるのは、起動した人と管理者だけです",
        );
    }
    if run.result.is_some() {
        return (StatusCode::OK, Json(json!({ "status": "finished" }))).into_response();
    }
    match ai_kick_service::cancel(&state.pool, &run, user_id).await {
        Ok(()) => (StatusCode::OK, Json(json!({ "status": "cancelling" }))).into_response(),
        Err(e) => server_error(e),
    }
}

/// 画面に出す状態: queued(順番待ち)/ running(実行中)/ stopping(停止中)/ finished(終了)。
fn status_of(r: &AiRun) -> &'static str {
    if r.result.is_some() {
        "finished"
    } else if r.stop_requested_at.is_some() {
        "stopping"
    } else if r.claimed_at.is_some() {
        "running"
    } else if r.queued_at.is_some() {
        "queued"
    } else {
        "prepared" // 手動コピー用に準備されただけ
    }
}

/// GET /api/v1/tickets/{ticket_key}/ai-runs/
pub async fn list_runs(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(ticket_key): Path<String>,
) -> Response {
    const ROUTE: &str = "GET /api/v1/tickets/{ticket_key}/ai-runs/";
    let ticket_id: Option<i32> =
        match sqlx::query_scalar("SELECT id::int4 FROM tickets_ticket WHERE ticket_key = $1")
            .bind(&ticket_key)
            .fetch_optional(&state.pool)
            .await
        {
            Ok(v) => v,
            Err(e) => return server_error(e),
        };
    let Some(ticket_id) = ticket_id else {
        return detail(StatusCode::NOT_FOUND, "見つかりません");
    };
    if let Err(r) =
        authorize::authorize_ticket(&state, &viewer, ticket_id, Action::Read, ROUTE).await
    {
        return r;
    }
    let runs =
        match ai_kick_repo::recent_runs_of_ticket(&state.pool, i64::from(ticket_id), 10).await {
            Ok(r) => r,
            Err(e) => return server_error(e),
        };
    let items: Vec<Value> = runs
        .iter()
        // 手動コピー用に準備されただけで、使われていないものは出さない
        .filter(|r| r.queued_at.is_some() || r.started_at.is_some() || r.result.is_some())
        .map(|r| {
            json!({
                "runUuid": r.run_uuid,
                "status": status_of(r),
                "triggerType": r.trigger_type,
                "priorityClass": r.priority_class,
                "queuedAt": r.queued_at,
                "startedAt": r.started_at,
                "endedAt": r.ended_at,
                "result": r.result,
                "summary": r.summary,
                "weightedTokens": r.weighted_tokens,
                "stopReason": r.stop_reason,
                "errorDetail": r.error_detail,
            })
        })
        .collect();
    (StatusCode::OK, Json(json!({ "runs": items }))).into_response()
}

// ---- プロジェクトの設定 ------------------------------------------------------

pub(crate) fn valid_repo(s: &str) -> bool {
    let mut parts = s.split('/');
    let (Some(owner), Some(name), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    let ok = |p: &str| {
        !p.is_empty()
            && p.len() <= 100
            && !p.starts_with('.')
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    };
    ok(owner) && ok(name)
}

fn valid_branch(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 100
        && !s.starts_with('-')
        && !s.starts_with('/')
        && !s.ends_with('/')
        && !s.contains("..")
        && !s.contains("//")
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'))
}

async fn can_manage_project(
    state: &AppState,
    viewer: &Viewer,
    project_id: i32,
) -> Result<bool, Response> {
    let user_id = viewer.require_user_id()?;
    project_team_repo::can_manage(&state.pool, project_id, user_id, viewer.is_system_admin())
        .await
        .map_err(server_error)
}

/// GET /api/v1/projects/{id}/ai-kick/
pub async fn get_project_settings(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(project_id): Path<i32>,
) -> Response {
    const ROUTE: &str = "GET /api/v1/projects/{id}/ai-kick/";
    if let Err(r) = authorize::gate_project(
        &state.pool,
        &viewer,
        project_id,
        Action::Read,
        Ok(()),
        ROUTE,
    )
    .await
    {
        return r;
    }
    let p = match ai_kick_repo::get_project_kick(&state.pool, project_id).await {
        Ok(Some(p)) => p,
        Ok(None) => return detail(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => return server_error(e),
    };
    let can_edit = match can_manage_project(&state, &viewer, project_id).await {
        Ok(b) => b,
        Err(r) => return r,
    };
    let sys = match ai_kick_repo::get_settings(&state.pool).await {
        Ok(s) => s,
        Err(e) => return server_error(e),
    };
    (
        StatusCode::OK,
        Json(json!({
            "enabled": p.ai_kick_enabled,
            "repo": p.ai_kick_repo,
            "baseBranch": p.ai_kick_base_branch,
            "canEdit": can_edit,
            // 管理者がシステムで有効にしているか(無効なら、ここで許可しても起動できない)
            "systemEnabled": sys.enabled && sys.limits.is_some() && !sys.killed,
            "systemKilled": sys.killed,
            // システム管理者が AI の作業を許可したリポジトリ(この中から選ぶ)
            "allowedRepos": sys.allowed_repos,
        })),
    )
        .into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectKickBody {
    pub enabled: bool,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub base_branch: String,
    /// 「チケットの内容とリポジトリのコードが Anthropic に送られる」ことを確認した。有効にするときは必須
    #[serde(default)]
    pub data_sharing_acknowledged: bool,
}

/// PUT /api/v1/projects/{id}/ai-kick/ — プロジェクトのオーナーと管理者だけ。
pub async fn put_project_settings(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(project_id): Path<i32>,
    Json(body): Json<ProjectKickBody>,
) -> Response {
    const ROUTE: &str = "PUT /api/v1/projects/{id}/ai-kick/";
    let user_id = match viewer.require_user_id() {
        Ok(u) => u,
        Err(r) => return r,
    };
    if viewer.principal.is_key() || !viewer.is_full_member() {
        return detail(
            StatusCode::FORBIDDEN,
            "この設定を変更できるのは、ログインしたメンバーだけです",
        );
    }
    if let Err(r) = authorize::gate_project(
        &state.pool,
        &viewer,
        project_id,
        Action::Write,
        Ok(()),
        ROUTE,
    )
    .await
    {
        return r;
    }
    match can_manage_project(&state, &viewer, project_id).await {
        Ok(true) => {}
        Ok(false) => {
            return detail(
                StatusCode::FORBIDDEN,
                "AI 自動実行の許可を変更できるのは、プロジェクトのオーナーと管理者だけです",
            )
        }
        Err(r) => return r,
    }
    let repo = body.repo.trim().to_string();
    let branch = if body.base_branch.trim().is_empty() {
        "develop".to_string()
    } else {
        body.base_branch.trim().to_string()
    };
    if !repo.is_empty() && !valid_repo(&repo) {
        return detail(
            StatusCode::BAD_REQUEST,
            "リポジトリは「owner/name」の形式で指定してください",
        );
    }
    if !valid_branch(&branch) {
        return detail(StatusCode::BAD_REQUEST, "ブランチ名が正しくありません");
    }
    if body.enabled && repo.is_empty() {
        return detail(
            StatusCode::BAD_REQUEST,
            "AI 自動実行を許可するには、AI が作業するリポジトリを指定してください",
        );
    }
    // リポジトリは、システム管理者が許可した一覧からだけ選べる(ほかのチームのリポジトリを指定させない)
    if !repo.is_empty() {
        match ai_kick_repo::get_settings(&state.pool).await {
            Ok(sys) if sys.repo_allowed(&repo) => {}
            Ok(_) => {
                return detail(
                    StatusCode::BAD_REQUEST,
                    "このリポジトリは、システム管理者が AI の作業を許可していません。許可されたリポジトリから選んでください",
                )
            }
            Err(e) => return server_error(e),
        }
    }
    // 有効にするときは、外部の AI(Anthropic)にデータが送られることの確認が必要
    if body.enabled && !body.data_sharing_acknowledged {
        return detail(
            StatusCode::BAD_REQUEST,
            "有効にするには、チケットの内容とリポジトリのコードが Anthropic 社に送られることを確認してください",
        );
    }
    match ai_kick_repo::set_project_kick(&state.pool, project_id, body.enabled, &repo, &branch)
        .await
    {
        Ok(true) => {}
        Ok(false) => return detail(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => return server_error(e),
    }
    ai_kick_repo::audit(
        &state.pool,
        Some(i64::from(user_id)),
        "human",
        "ai_kick.project_settings",
        json!({ "projectId": project_id, "enabled": body.enabled, "repo": repo, "baseBranch": branch, "dataSharingAcknowledged": body.data_sharing_acknowledged }),
    )
    .await;
    (
        StatusCode::OK,
        Json(json!({ "enabled": body.enabled, "repo": repo, "baseBranch": branch })),
    )
        .into_response()
}

// ticket_repo は、将来の拡張(チケットの状態表示)で使う
#[allow(unused_imports)]
use ticket_repo as _;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_names_are_validated() {
        assert!(valid_repo("example-org/senn"));
        assert!(valid_repo("my-org/my_repo.v2"));
        for bad in [
            "", "senn", "a/b/c", "/b", "a/", "../x", "a/..", "a/.git", "a b/c", "a/b;rm", "a/b\\n",
        ] {
            assert!(!valid_repo(bad), "拒否される: {bad:?}");
        }
    }

    #[test]
    fn branch_names_are_validated() {
        assert!(valid_branch("develop"));
        assert!(valid_branch("release/2026-10"));
        for bad in [
            "", "-x", "/x", "x/", "a..b", "a//b", "a b", "a;b", "$(x)", "a\\nb",
        ] {
            assert!(!valid_branch(bad), "拒否される: {bad:?}");
        }
    }
}
