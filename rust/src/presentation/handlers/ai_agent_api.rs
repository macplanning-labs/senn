/// presentation/handlers/ai_agent_api.rs — AI専用外部API(X-AI-Api-Keyヘッダー認証)
///
/// external_api.rs(WIP_API_KEY)と同じ「共有キー→固定ユーザーとして操作」の形だが、
/// 鍵とユーザーを完全に分離する: Claude等のAIエージェントが起こした操作を、
/// 人間/他システムからのWIP_API_KEY操作と別アカウント・別鍵として区別できるようにする。
/// 対象: プロジェクト作成、チケット作成(ラベル・担当者は名前/ユーザー名指定)。
use axum::{
    extract::{Multipart, Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::domain::access::{Action, ResourceRef};
use crate::domain::models::cycle_api::CycleWriteIn;
use crate::domain::models::resource_api::ProjectWriteIn;
use crate::domain::models::ticket_api::{TicketPatchIn, TicketWriteIn};
use crate::domain::services::ai_service;
use crate::infrastructure::access::{
    facts_repo,
    shadow::{self, Mode, Resource},
};
use crate::infrastructure::repositories::{
    ai_agent_key_repo, ai_run_repo, cycle_repo, resource_repo, team_repo, ticket_repo, user_repo,
    wiki_attachment_repo, wiki_repo, workflow_status_repo,
};
use crate::presentation::extractors::authorize;
use crate::presentation::handlers::system_admin_api;
use crate::presentation::state::AppState;

fn err(detail: impl Into<String>) -> serde_json::Value {
    json!({"error": detail.into()})
}

/// `authenticate_ai()` の戻り値。
#[derive(Debug, Clone)]
pub struct AiAuthResult {
    /// 操作主体(常に共有 ai_agent アカウントの user_id、既存動作を維持)。
    pub user_id: i32,
    /// 人ごとキーで認証できた場合のみ Some。実際にキーを保有していた人間のアカウントID。
    /// クライアント(AIエージェント)側からは指定不可で、キーのハッシュ照合結果からのみ導出される
    /// (なりすまし防止。DEMO-000100)。
    pub acting_user_id: Option<i32>,
    /// 個人キーの ID(個人キーで認証できた場合のみ)
    pub key_id: Option<i32>,
}

/// AI エージェント API の呼び出し元(アクセス制御の再設計 F-1。詳細設計書 §10.6)
///
/// 今の認証(`authenticate_ai`)の結果と、個人キーなら持ち主の閲覧者(`Principal::PersonalKey`)を持つ。
/// 共有キーは閲覧者を持たない(移行期間の扱いは `gate`。設計書 §12.2)。
/// 切り替えは `ACCESS_ENFORCE_KEY`: off / shadow は今の動作(確認なし。shadow は違いを記録)、on は新しい判定。
#[derive(Debug, Clone)]
pub struct AiCaller {
    pub legacy: AiAuthResult,
    pub viewer: Option<crate::domain::access::Viewer>,
}

impl axum::extract::FromRequestParts<AppState> for AiCaller {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let legacy = authenticate_ai(state, &parts.headers)
            .await
            .map_err(|(status, payload)| (status, Json(payload)).into_response())?;
        let viewer = match (legacy.acting_user_id, legacy.key_id) {
            (Some(owner), Some(key_id)) => crate::infrastructure::access::viewer_repo::load(
                &state.pool,
                crate::domain::access::Principal::PersonalKey {
                    user_id: owner,
                    key_id: key_id as i64,
                },
                crate::infrastructure::access::viewer_repo::today_utc(),
            )
            .await
            .map_err(|e| {
                tracing::error!("[認可] キーの持ち主の読み込みに失敗: {:?}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(err("サーバーエラーが発生しました")),
                )
                    .into_response()
            })?,
            _ => None,
        };
        // 共有キーの廃止日を過ぎた(on)は 401(F-8。設計書 §12.2)
        if legacy.acting_user_id.is_none() && shadow::mode(Resource::Key) == Mode::On {
            if let Some(sunset) = shared_key_sunset() {
                if chrono::Utc::now().date_naive() > sunset {
                    return Err((
                        StatusCode::UNAUTHORIZED,
                        Json(err(format!(
                            "共有の AI キーは {sunset} で廃止されました。SENN の設定画面で個人の AI キーを発行して使ってください"
                        ))),
                    )
                        .into_response());
                }
            }
        }
        // 持ち主が無効化されている(on)は 401
        if legacy.acting_user_id.is_some()
            && viewer.is_none()
            && shadow::mode(Resource::Key) == Mode::On
        {
            return Err(
                (StatusCode::UNAUTHORIZED, Json(err("AI用APIキーが無効です"))).into_response(),
            );
        }
        Ok(AiCaller { legacy, viewer })
    }
}

impl AiCaller {
    /// 1 件の操作の判定(今の判定には確認が無い)
    /// - 個人キー: 持ち主の権限で判定する(キー経由では、一括削除・設定の管理などは不可)
    /// - 共有キー: on では閲覧だけ(書き込みは 403。漏れの影響を読み取りに限る。設計書 §12.2)
    #[allow(clippy::result_large_err)]
    pub fn gate(
        &self,
        pool: &sqlx::PgPool,
        facts: Option<&ResourceRef>,
        action: Action,
        id: i64,
        route: &'static str,
    ) -> Result<(), axum::response::Response> {
        if self.viewer.is_none() && action != Action::Read {
            record_shared_key_write(pool, self.legacy.user_id, action, route);
        }
        match &self.viewer {
            Some(v) => authorize::gate(pool, v, facts, action, Resource::Key, id, Ok(()), route),
            None if action != Action::Read && shadow::mode(Resource::Key) == Mode::On => Err((
                StatusCode::FORBIDDEN,
                Json(err(
                    "共有の AI キーでは書き込みできません。個人の AI キーを発行して使ってください",
                )),
            )
                .into_response()),
            None => Ok(()),
        }
    }

    /// 一覧の絞り込み(個人キーは持ち主の見える範囲。共有キーは絞らない)
    pub fn filter_list<T>(
        &self,
        pool: &sqlx::PgPool,
        route: &'static str,
        items: Vec<T>,
        id_of: impl Fn(&T) -> i64,
        res_of: impl Fn(&crate::domain::access::Viewer, &T) -> ResourceRef,
    ) -> Vec<T> {
        match &self.viewer {
            Some(v) => authorize::filter_list(pool, v, Resource::Key, route, items, id_of, |t| {
                res_of(v, t)
            }),
            None => items,
        }
    }

    /// コメント・チケットの作成者(F-3)。新しい判定(on)の個人キーは、持ち主本人を作成者にする
    /// (AI 経由であることは、コメントは `ai_agent_acting_user_id`、チケットは `created_via_ai` で残る)。
    /// それ以外は、今までどおり共有の AI アカウント
    pub fn comment_author(&self) -> i32 {
        match self.legacy.acting_user_id {
            Some(owner) if self.viewer.is_some() && shadow::mode(Resource::Key) == Mode::On => {
                owner
            }
            _ => self.legacy.user_id,
        }
    }

    /// 新しい判定(on)で、個人キーのときの見える範囲(SQL で絞る一覧用)
    pub fn scope_if_on(&self) -> Option<crate::domain::access::Scope> {
        match &self.viewer {
            Some(v) if shadow::mode(Resource::Key) == Mode::On => Some(v.scope()),
            _ => None,
        }
    }
}

/// AI用APIキーを検証し、成功時は操作主体(AIエージェント用アカウント)のuser_idと、
/// (人ごとキーで認証できた場合の)実行者のuser_idを返す。
/// wip_api_key(人間/既存システム用)とは完全に別の鍵・別のユーザーを使う。
pub(crate) async fn authenticate_ai(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AiAuthResult, (StatusCode, serde_json::Value)> {
    let api_key = headers
        .get("X-AI-Api-Key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if api_key.is_empty() {
        return Err((
            StatusCode::UNAUTHORIZED,
            err("AI用APIキーが指定されていません"),
        ));
    }

    // 1. まず人ごとキー(ai_agent_api_keys)をハッシュ照合する(UNIQUE制約によるO(1) lookup)。
    //    失効済みキーがヒットした場合は共有キーへフォールバックせず、ここで確定的に401とする
    //    (失効の意図を尊重するため)。
    let key_hash = ai_agent_key_repo::hash_key(api_key);
    match ai_agent_key_repo::find_active_by_hash(&state.pool, &key_hash).await {
        Ok(Some(record)) => {
            // last_used_at 更新は best-effort。失敗しても認証自体は継続する。
            let _ = ai_agent_key_repo::touch_last_used(&state.pool, record.id).await;
            let shared_user_id = resolve_shared_ai_user(state).await?;
            return Ok(AiAuthResult {
                user_id: shared_user_id,
                acting_user_id: Some(record.user_id),
                key_id: Some(record.id),
            });
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                err("サーバーエラーが発生しました"),
            ));
        }
    }

    // 2. 人ごとキーで一致しなければ、従来の全社共有キーにフォールバックする(後方互換)。
    let expected_key = match system_admin_api::resolve_ai_agent_api_key(state).await {
        Some(k) => k,
        None => {
            return Err((
                StatusCode::UNAUTHORIZED,
                err("SENN_AI_API_KEY が設定されていません"),
            ));
        }
    };

    if !constant_time_eq(api_key.as_bytes(), expected_key.as_bytes()) {
        return Err((StatusCode::UNAUTHORIZED, err("AI用APIキーが無効です")));
    }

    let shared_user_id = resolve_shared_ai_user(state).await?;
    Ok(AiAuthResult {
        user_id: shared_user_id,
        acting_user_id: None,
        key_id: None,
    })
}

/// 共有 ai_agent アカウントを解決する(存在しなければ自動作成する)。
/// アカウントが未作成なら、ここで一度だけ自動作成する(パスワードは使用不能ハッシュ '!' でログイン不可)。
pub(crate) async fn resolve_shared_ai_user(
    state: &AppState,
) -> Result<i32, (StatusCode, serde_json::Value)> {
    let username = &state.config.wip_ai_api_user;
    let existing = user_repo::find_by_username(&state.pool, username)
        .await
        .map_err(|e| {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                err("サーバーエラーが発生しました"),
            )
        })?;

    if let Some(u) = existing {
        return Ok(u.id);
    }

    // 初回アクセス時にAI専用アカウントを自動作成する。
    // password_hash = "!" はDjangoの「使用不能パスワード」規約(check_passwordは常にfalse)で、
    // このアカウントは対話ログインができない= API経由の操作専用であることを保証する。
    match user_repo::create_user(
        &state.pool,
        username,
        "ai-agent@localhost",
        "!",
        "AI",
        "Agent",
    )
    .await
    {
        Ok(created) => Ok(created.id),
        Err(e) => {
            // 初回に同時に 2 件来ると、片方の作成が重複で失敗する。先に作られた方を使う
            if let Ok(Some(u)) = user_repo::find_by_username(&state.pool, username).await {
                return Ok(u.id);
            }
            tracing::error!("AI agent account creation failed: {:?}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                err("サーバーエラーが発生しました"),
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// アクセス制御の再設計(F-2・F-4)の共通部品
// ---------------------------------------------------------------------------

/// 新しい判定(`ACCESS_ENFORCE_KEY=on`)か
fn key_on() -> bool {
    shadow::mode(Resource::Key) == Mode::On
}

pub(crate) fn ai_db_error(e: anyhow::Error) -> axum::response::Response {
    tracing::error!("DB operation failed: {:?}", e);
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(err("サーバーエラーが発生しました")),
    )
        .into_response()
}

/// 見つからないプロジェクトの応答。新しい判定(on)では、ほかのプロジェクトの Prefix を一覧しない
/// (見えない物の名前を漏らさない。F-4・設計書 §5.3)
pub(crate) async fn project_not_found(
    state: &AppState,
    project_prefix: &str,
) -> axum::response::Response {
    let message = format!("プロジェクト '{}' が見つかりません", project_prefix);
    if key_on() {
        return (StatusCode::NOT_FOUND, Json(err(message))).into_response();
    }
    let available = ticket_repo::list_project_prefixes(&state.pool)
        .await
        .unwrap_or_default();
    (
        StatusCode::NOT_FOUND,
        Json(json!({
            "error": message,
            "available_projects": available.into_iter().map(|(p, n)| json!({"prefix": p, "name": n})).collect::<Vec<_>>(),
        })),
    )
        .into_response()
}

/// チケット(キー指定)への操作の判定
pub(crate) async fn ai_ticket_gate(
    state: &AppState,
    caller: &AiCaller,
    ticket_key: &str,
    action: Action,
    route: &'static str,
) -> Result<(), axum::response::Response> {
    let facts = facts_repo::facts_for_ticket_key(&state.pool, ticket_key)
        .await
        .map_err(ai_db_error)?;
    let id = facts.as_ref().map_or(0, |(id, _)| *id as i64);
    caller.gate(
        &state.pool,
        facts.as_ref().map(|(_, f)| f),
        action,
        id,
        route,
    )
}

/// プロジェクト(Prefix 指定)への操作の判定
pub(crate) async fn ai_project_gate(
    state: &AppState,
    caller: &AiCaller,
    project_prefix: &str,
    action: Action,
    route: &'static str,
) -> Result<(), axum::response::Response> {
    let project_id = ticket_repo::resolve_project_id_by_prefix(&state.pool, project_prefix)
        .await
        .map_err(ai_db_error)?;
    let facts = match project_id {
        Some(pid) => facts_repo::facts_for_project(&state.pool, pid)
            .await
            .map_err(ai_db_error)?,
        None => None,
    };
    caller.gate(
        &state.pool,
        facts.as_ref(),
        action,
        project_id.unwrap_or(0) as i64,
        route,
    )
}

/// サイクルへの操作の判定
async fn ai_cycle_gate(
    state: &AppState,
    caller: &AiCaller,
    cycle_id: i32,
    action: Action,
    route: &'static str,
) -> Result<(), axum::response::Response> {
    let facts = facts_repo::facts_for_cycle(&state.pool, cycle_id)
        .await
        .map_err(ai_db_error)?;
    caller.gate(&state.pool, facts.as_ref(), action, cycle_id as i64, route)
}

/// Wiki ページへの操作の判定
pub(crate) async fn ai_wiki_gate(
    state: &AppState,
    caller: &AiCaller,
    page_id: i32,
    action: Action,
    route: &'static str,
) -> Result<(), axum::response::Response> {
    let facts = facts_repo::facts_for_wiki(&state.pool, page_id)
        .await
        .map_err(ai_db_error)?;
    caller.gate(&state.pool, facts.as_ref(), action, page_id as i64, route)
}

/// 共有の AI キーの廃止日(`SENN_AI_SHARED_KEY_SUNSET=YYYY-MM-DD`)。未設定なら無期限(F-8)
fn shared_key_sunset() -> Option<NaiveDate> {
    std::env::var("SENN_AI_SHARED_KEY_SUNSET")
        .ok()
        .and_then(|v| NaiveDate::parse_from_str(v.trim(), "%Y-%m-%d").ok())
}

/// 共有の AI キーでの書き込み(試み)を、監査記録に残す(F-8。移行期間の利用者の把握。本文は入れない)
fn record_shared_key_write(
    pool: &sqlx::PgPool,
    shared_user_id: i32,
    action: Action,
    route: &'static str,
) {
    let pool = pool.clone();
    let blocked = shadow::mode(Resource::Key) == Mode::On;
    tokio::spawn(async move {
        if let Err(e) = sqlx::query(
            "INSERT INTO access_audit_log (actor_user_id, actor_kind, action, detail)
             VALUES ($1::int8, 'system', 'shared_ai_key_write', $2)",
        )
        .bind(shared_user_id as i64)
        .bind(json!({ "route": route, "action": format!("{action:?}"), "blocked": blocked }))
        .execute(&pool)
        .await
        {
            tracing::warn!("[認可] 共有キーの記録に失敗(操作は続行): {:?}", e);
        }
    });
}

/// 共有の AI キーで呼ばれた応答に、廃止予定の警告を付ける(F-8。`Deprecation`・`Sunset`)。
/// 個人キーには付けない。切り替えが off のときは何もしない
pub async fn shared_key_notice(
    State(state): State<AppState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let shared = match req
        .headers()
        .get("X-AI-Api-Key")
        .and_then(|v| v.to_str().ok())
    {
        Some(k) if !k.is_empty() && shadow::mode(Resource::Key) != Mode::Off => {
            let hash = ai_agent_key_repo::hash_key(k);
            matches!(
                ai_agent_key_repo::find_active_by_hash(&state.pool, &hash).await,
                Ok(None)
            )
        }
        _ => false,
    };
    let mut resp = next.run(req).await;
    if shared {
        let headers = resp.headers_mut();
        headers.insert("Deprecation", HeaderValue::from_static("true"));
        if let Some(sunset) = shared_key_sunset() {
            // HTTP 日付の形(RFC 9110)。廃止日の 0:00 UTC
            let value = format!("{} 00:00:00 GMT", sunset.format("%a, %d %b %Y"));
            if let Ok(v) = HeaderValue::from_str(&value) {
                headers.insert("Sunset", v);
            }
        }
        headers.insert(
            "Warning",
            HeaderValue::from_static(
                "299 SENN \"The shared AI key is deprecated. Issue a personal AI key in SENN settings.\"",
            ),
        );
    }
    resp
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

// ---------------------------------------------------------------------------
// プロジェクト作成
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateAiProjectIn {
    pub name: Option<String>,
    pub prefix: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default, alias = "teamIds")]
    pub team_ids: Vec<i32>,
}

/// POST /api/v1/ai-agent/projects/
pub async fn create_project(
    State(state): State<AppState>,
    caller: AiCaller,
    Json(body): Json<CreateAiProjectIn>,
) -> impl IntoResponse {
    let name = match &body.name {
        Some(n) if !n.is_empty() => n,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(err("name と prefix は必須です")),
            )
                .into_response()
        }
    };
    let prefix = match &body.prefix {
        Some(p) if !p.is_empty() => p,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(err("name と prefix は必須です")),
            )
                .into_response()
        }
    };

    // Validate team_ids is not empty
    if body.team_ids.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(err("teamIds は1つ以上必須です")),
        )
            .into_response();
    }

    let teams = match facts_repo::facts_for_teams(&state.pool, &body.team_ids).await {
        Ok(t) => t,
        Err(e) => return ai_db_error(e),
    };
    if let Err(resp) = caller.gate(
        &state.pool,
        Some(&ResourceRef::Project {
            project_id: 0,
            teams,
        }),
        Action::Create,
        0,
        "POST /api/v1/ai-agent/projects/",
    ) {
        return resp;
    }

    let input = ProjectWriteIn {
        name: name.clone(),
        prefix: prefix.clone(),
        description: body.description.clone(),
        priority: "medium".to_string(),
        team_ids: body.team_ids.clone(),
    };

    match resource_repo::create_project(&state.pool, &input, None, None).await {
        Ok(project_id) => {
            match resource_repo::find_project_by_id(&state.pool, project_id, None).await {
                Ok(Some(project)) => (StatusCode::CREATED, Json(project)).into_response(),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(err("サーバーエラーが発生しました")),
                )
                    .into_response(),
            }
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            // prefixのUNIQUE制約違反である可能性が高い(既存プロジェクトと重複)
            (StatusCode::CONFLICT, Json(err(format!("プロジェクトを作成できませんでした(prefix '{}' が既に使われている可能性があります)", prefix)))).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// チケット作成(ラベルは名前指定で自動作成、担当者はユーザー名指定)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateAiTicketIn {
    pub project_prefix: Option<String>,
    pub team_id: Option<i32>,
    pub team_slug: Option<String>,
    pub title: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_priority")]
    pub priority: String,
    #[serde(default = "default_ticket_type")]
    pub ticket_type: String,
    pub due_date: Option<NaiveDate>,
    pub start_date: Option<NaiveDate>,
    /// 状態の slug(省略時 open)。そのプロジェクト・チームに登録された状態だけ受け付ける
    pub status: Option<String>,
    pub story_points: Option<i16>,
    /// Cycle の ID(そのプロジェクト・チームの Cycle だけ)
    pub cycle: Option<i32>,
    /// 親チケットの ID。parent_key と同時には指定しない
    pub parent: Option<i32>,
    /// 親チケットのキー(例: SENN-000001)
    pub parent_key: Option<String>,
    /// ラベル名(存在しなければ同名でプロジェクト内に新規作成する)
    #[serde(default)]
    pub labels: Vec<String>,
    /// 担当者のユーザー名。プロジェクト未参加なら自動的にメンバーへ追加してから割り当てる。
    #[serde(default)]
    pub assignee_usernames: Vec<String>,
}
pub(crate) fn default_priority() -> String {
    "medium".to_string()
}
pub(crate) fn default_ticket_type() -> String {
    "task".to_string()
}

pub(crate) fn bad_request(detail: impl Into<String>) -> axum::response::Response {
    (StatusCode::BAD_REQUEST, Json(err(detail))).into_response()
}

/// チケットの作成先(チーム・プロジェクト)と、作成の判定に使う対象
pub(crate) struct AiCreateTarget {
    pub team_id: i32,
    pub project_id: Option<i32>,
    pub target: ResourceRef,
}

/// 作成先を project_prefix / team_id / team_slug から決め、作成できるかを判定する
pub(crate) async fn resolve_ai_create_target(
    state: &AppState,
    caller: &AiCaller,
    project_prefix: Option<&str>,
    team_id: Option<i32>,
    team_slug: Option<&str>,
    route: &'static str,
) -> Result<AiCreateTarget, axum::response::Response> {
    // プロジェクト(指定があれば)
    let project_id = match project_prefix {
        Some(prefix) => {
            match ticket_repo::resolve_project_id_by_prefix(&state.pool, prefix).await {
                Ok(Some(id)) => Some(id),
                Ok(None) => return Err(project_not_found(state, prefix).await),
                Err(e) => return Err(ai_db_error(e)),
            }
        }
        None => None,
    };

    // チーム: team_id → team_slug → プロジェクトの参加チーム(1 つだけのとき)
    let team_id: i32 = if let Some(tid) = team_id {
        tid
    } else if let Some(slug) = team_slug {
        match sqlx::query_scalar::<_, i32>("SELECT id::int4 FROM m_team WHERE slug = $1")
            .bind(slug)
            .fetch_optional(&state.pool)
            .await
        {
            Ok(Some(tid)) => tid,
            Ok(None) => return Err(bad_request(format!("チーム '{}' が見つかりません", slug))),
            Err(e) => return Err(ai_db_error(e.into())),
        }
    } else if let (Some(pid), Some(prefix)) = (project_id, project_prefix) {
        let project = match resource_repo::find_project_by_id(&state.pool, pid, None).await {
            Ok(Some(p)) => p,
            Ok(None) => return Err(ai_db_error(anyhow::anyhow!("project {pid} vanished"))),
            Err(e) => return Err(ai_db_error(e)),
        };
        match project.teams.as_slice() {
            [team] => team.id,
            [] => {
                return Err(bad_request(format!(
                    "プロジェクト '{}' に参加チームがありません",
                    prefix
                )))
            }
            _ => {
                return Err(bad_request(format!(
                    "プロジェクト '{}' は複数チームに参加しており、teamId/teamSlug が必須です",
                    prefix
                )))
            }
        }
    } else {
        return Err(bad_request(
            "project_prefix, team_id, または team_slug のいずれかが必須です",
        ));
    };

    match team_repo::find_team_by_id(&state.pool, team_id).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            return Err(bad_request(format!(
                "チーム ID {} が見つかりません",
                team_id
            )))
        }
        Err(e) => return Err(ai_db_error(e)),
    }

    // 作成先の判定(新しい判定。持ち主がそのチームにチケットを作れること)
    let team_facts = facts_repo::facts_for_team(&state.pool, team_id)
        .await
        .map_err(ai_db_error)?;
    let target = ResourceRef::Ticket {
        team: team_facts,
        project_id,
        author_id: caller.viewer.as_ref().and_then(|v| v.user_id()),
        assignee_ids: vec![],
    };
    caller.gate(
        &state.pool,
        Some(&target),
        Action::Create,
        team_id as i64,
        route,
    )?;
    Ok(AiCreateTarget {
        team_id,
        project_id,
        target,
    })
}

/// 担当者のユーザー名を user_id にする。`add_guests` が true なら、今の判定でプロジェクト外の人を Guest に足す
/// (試しに実行するときは false。副作用を残さない)
pub(crate) async fn resolve_ai_assignees(
    state: &AppState,
    tgt: &AiCreateTarget,
    usernames: &[String],
    add_guests: bool,
) -> Result<Vec<i32>, axum::response::Response> {
    let mut assignee_ids = Vec::with_capacity(usernames.len());
    for username in usernames {
        let user = match user_repo::find_by_username(&state.pool, username).await {
            Ok(Some(u)) => u,
            Ok(None) => {
                return Err(bad_request(format!(
                    "ユーザー '{}' が見つかりません",
                    username
                )))
            }
            Err(e) => return Err(ai_db_error(e)),
        };

        // F-4(新しい判定): 担当者を自動で Guest に追加しない。担当者は、そのチケットが見える人だけ
        if key_on() {
            let visible = match crate::infrastructure::access::viewer_repo::load(
                &state.pool,
                crate::domain::access::Principal::Human { user_id: user.id },
                crate::infrastructure::access::viewer_repo::today_utc(),
            )
            .await
            {
                Ok(Some(v)) => {
                    crate::domain::access::can(&v, Action::Read, &tgt.target)
                        == crate::domain::access::Decision::Allow
                }
                Ok(None) => false,
                Err(e) => return Err(ai_db_error(e)),
            };
            if !visible {
                return Err(bad_request(format!(
                    "ユーザー '{}' はこのチケットを見られないため、担当者にできません",
                    username
                )));
            }
        // L2②: Project限定ゲストとしての追加はproject_idがある場合のみ(team_idはこのprojectの所有チーム)
        } else if let (Some(pid), true) = (tgt.project_id, add_guests) {
            let is_member = team_repo::check_project_access(&state.pool, pid, user.id)
                .await
                .map_err(ai_db_error)?;
            if !is_member {
                team_repo::add_team_guest(&state.pool, tgt.team_id, user.id, pid, None)
                    .await
                    .map_err(ai_db_error)?;
            }
        }

        assignee_ids.push(user.id);
    }
    Ok(assignee_ids)
}

/// 状態の slug が、そのプロジェクト・チームに登録されているか。登録がどこにも無い古いデータは確かめない。
/// 使えないときは、その理由(Some)を返す
pub(crate) async fn check_ai_status(
    pool: &sqlx::PgPool,
    project_id: Option<i32>,
    team_id: Option<i32>,
    status: &str,
) -> anyhow::Result<Option<String>> {
    let slugs =
        workflow_status_repo::list_status_slugs_for_scope(pool, project_id, team_id).await?;
    if slugs.is_empty() || slugs.iter().any(|s| s == status) {
        return Ok(None);
    }
    Ok(Some(format!(
        "状態 '{}' はこのプロジェクト・チームにありません。使える状態: {}",
        status,
        slugs.join(", ")
    )))
}

pub(crate) async fn validate_ai_status(
    pool: &sqlx::PgPool,
    project_id: Option<i32>,
    team_id: Option<i32>,
    status: &str,
) -> Result<(), axum::response::Response> {
    match check_ai_status(pool, project_id, team_id, status).await {
        Ok(None) => Ok(()),
        Ok(Some(msg)) => Err(bad_request(msg)),
        Err(e) => Err(ai_db_error(e)),
    }
}

/// Cycle が、そのプロジェクト・チームのものか。違うときは、その理由(Some)を返す
pub(crate) async fn check_ai_cycle(
    pool: &sqlx::PgPool,
    project_id: Option<i32>,
    team_id: i32,
    cycle_id: i32,
) -> anyhow::Result<Option<String>> {
    let row: Option<(Option<i32>, Option<i32>)> =
        sqlx::query_as("SELECT project_id::int4, team_id::int4 FROM t_cycle WHERE id = $1::int8")
            .bind(cycle_id as i64)
            .fetch_optional(pool)
            .await?;
    Ok(match row {
        None => Some(format!("Cycle {} が見つかりません", cycle_id)),
        Some((c_project, c_team)) => {
            let same_project = project_id.is_some() && c_project == project_id;
            if same_project || c_team == Some(team_id) {
                None
            } else {
                Some(format!(
                    "Cycle {} は、このプロジェクト・チームのものではありません",
                    cycle_id
                ))
            }
        }
    })
}

pub(crate) async fn validate_ai_cycle(
    pool: &sqlx::PgPool,
    project_id: Option<i32>,
    team_id: i32,
    cycle_id: i32,
) -> Result<(), axum::response::Response> {
    match check_ai_cycle(pool, project_id, team_id, cycle_id).await {
        Ok(None) => Ok(()),
        Ok(Some(msg)) => Err(bad_request(msg)),
        Err(e) => Err(ai_db_error(e)),
    }
}

/// ストーリーポイントは画面と同じく、フィボナッチ数(1, 2, 3, 5, 8, 13, 21)だけ
pub(crate) fn check_ai_story_points(points: Option<i16>) -> Option<String> {
    match points {
        Some(p) if !crate::domain::models::ticket_api::FIBONACCI_POINTS.contains(&p) => {
            Some("story_points は 1, 2, 3, 5, 8, 13, 21 のどれかを指定してください".to_string())
        }
        _ => None,
    }
}

#[allow(clippy::result_large_err)]
pub(crate) fn validate_ai_story_points(
    points: Option<i16>,
) -> Result<(), axum::response::Response> {
    match check_ai_story_points(points) {
        Some(msg) => Err(bad_request(msg)),
        None => Ok(()),
    }
}

/// 親チケット(ID かキー)を ID にする
async fn resolve_ai_parent(
    pool: &sqlx::PgPool,
    parent: Option<i32>,
    parent_key: Option<&str>,
) -> Result<Option<i32>, axum::response::Response> {
    match (parent, parent_key) {
        (Some(_), Some(_)) => Err(bad_request(
            "parent と parent_key は、どちらか一方だけ指定してください",
        )),
        (Some(id), None) => {
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM tickets_ticket WHERE id = $1)")
                    .bind(id)
                    .fetch_one(pool)
                    .await
                    .map_err(|e| ai_db_error(e.into()))?;
            if exists {
                Ok(Some(id))
            } else {
                Err(bad_request(format!(
                    "親チケット ID {} が見つかりません",
                    id
                )))
            }
        }
        (None, Some(key)) => match ticket_repo::resolve_ticket_id(pool, key).await {
            Ok(Some(id)) => Ok(Some(id)),
            Ok(None) => Err(bad_request(format!(
                "親チケット '{}' が見つかりません",
                key
            ))),
            Err(e) => Err(ai_db_error(e)),
        },
        (None, None) => Ok(None),
    }
}

/// ラベル名を ID にする(無ければ作る)。呼び出し側のトランザクションの中で行う
pub(crate) async fn resolve_ai_labels(
    conn: &mut sqlx::PgConnection,
    project_id: Option<i32>,
    team_id: i32,
    names: &[String],
) -> anyhow::Result<Vec<i32>> {
    let mut ids = Vec::with_capacity(names.len());
    for name in names {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        let id = resource_repo::find_or_create_label_for_scope_conn(
            conn,
            project_id,
            Some(team_id),
            name,
        )
        .await?;
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    Ok(ids)
}

/// チケットを 1 件作り、AI 経由の印を付ける(呼び出し側のトランザクションの中で)
pub(crate) async fn insert_ai_ticket(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    input: &TicketWriteIn,
    author_id: i32,
    key_id: Option<i32>,
) -> anyhow::Result<i32> {
    let ticket_id = ticket_repo::api_create(tx, input, author_id).await?;
    // AI 経由の印(画面で「AI」と表示する。作成者が持ち主本人になっても、AI 経由だと分かるように)
    ticket_repo::mark_created_via_ai(tx, ticket_id, key_id).await?;
    Ok(ticket_id)
}

/// POST /api/v1/ai-agent/tickets/
pub async fn create_ticket(
    State(state): State<AppState>,
    caller: AiCaller,
    Json(body): Json<CreateAiTicketIn>,
) -> impl IntoResponse {
    // 作成者: 新しい判定(on)の個人キーは持ち主本人。それ以外は共有の AI アカウント(F-3)
    let author_id = caller.comment_author();

    let title = match &body.title {
        Some(t) if !t.is_empty() => t,
        _ => return bad_request("title は必須です"),
    };

    let tgt = match resolve_ai_create_target(
        &state,
        &caller,
        body.project_prefix.as_deref(),
        body.team_id,
        body.team_slug.as_deref(),
        "POST /api/v1/ai-agent/tickets/",
    )
    .await
    {
        Ok(t) => t,
        Err(resp) => return resp,
    };

    // 追加の項目(状態・ストーリーポイント・Cycle・親)の確認
    if let Some(status) = &body.status {
        if let Err(resp) =
            validate_ai_status(&state.pool, tgt.project_id, Some(tgt.team_id), status).await
        {
            return resp;
        }
    }
    if let Err(resp) = validate_ai_story_points(body.story_points) {
        return resp;
    }
    if let Some(cycle_id) = body.cycle {
        if let Err(resp) =
            validate_ai_cycle(&state.pool, tgt.project_id, tgt.team_id, cycle_id).await
        {
            return resp;
        }
    }
    let parent = match resolve_ai_parent(&state.pool, body.parent, body.parent_key.as_deref()).await
    {
        Ok(p) => p,
        Err(resp) => return resp,
    };

    let assignee_ids =
        match resolve_ai_assignees(&state, &tgt, &body.assignee_usernames, true).await {
            Ok(ids) => ids,
            Err(resp) => return resp,
        };

    let mut tx = match state.pool.begin().await {
        Ok(t) => t,
        Err(e) => return ai_db_error(e.into()),
    };

    // ラベルは名前指定。Project があれば Project マスタ、無ければ所属 Team マスタ。
    let label_ids =
        match resolve_ai_labels(&mut tx, tgt.project_id, tgt.team_id, &body.labels).await {
            Ok(ids) => ids,
            Err(e) => return ai_db_error(e),
        };

    let ticket_in = TicketWriteIn {
        title: title.clone(),
        description: body.description.clone(),
        status: body.status.clone().unwrap_or_else(|| "open".to_string()),
        priority: body.priority.clone(),
        ticket_type: body.ticket_type.clone(),
        assignees: assignee_ids,
        reviewers: Vec::new(),
        category: None,
        project: tgt.project_id,
        milestone: None,
        parent,
        start_date: body.start_date,
        due_date: body.due_date,
        labels: label_ids,
        story_points: body.story_points,
        cycle: body.cycle,
        team_id: Some(tgt.team_id),
        linked_rules: Vec::new(),
    };

    let ticket_id =
        match insert_ai_ticket(&mut tx, &ticket_in, author_id, caller.legacy.key_id).await {
            Ok(id) => id,
            Err(e) => {
                tracing::error!("api_create failed: {:?}", e);
                if let Err(e) = tx.rollback().await {
                    tracing::error!("transaction rollback failed: {:?}", e);
                }
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(err("サーバーエラーが発生しました")),
                )
                    .into_response();
            }
        };

    if let Err(e) = tx.commit().await {
        tracing::error!("transaction commit failed: {:?}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(err("サーバーエラーが発生しました")),
        )
            .into_response();
    }

    // 「着手」の状態で作ったときは、開始日が空なら入れる(更新のときと同じ)
    if body.status.is_some() && body.start_date.is_none() {
        if let Err(e) =
            ai_run_repo::set_start_date_if_started(&state.pool, ticket_id, caller.legacy.user_id)
                .await
        {
            tracing::warn!(
                "[AI API] 処理=開始日の設定 結果=失敗 影響=開始日が空のまま | ticket_id={} {:?}",
                ticket_id,
                e
            );
        }
    }

    let ticket_key: Option<String> =
        sqlx::query_scalar("SELECT ticket_key FROM tickets_ticket WHERE id = $1")
            .bind(ticket_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    (
        StatusCode::CREATED,
        Json(json!({
            "id": ticket_id,
            "ticket_key": ticket_key,
            "title": title,
            "url": ticket_key.as_ref().map(|k| format!("/tickets/{}/", k)),
        })),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// プロジェクト一覧(読み取り)
// ---------------------------------------------------------------------------

/// GET /api/v1/ai-agent/projects/
pub async fn list_projects(State(state): State<AppState>, caller: AiCaller) -> impl IntoResponse {
    const PAGE: i64 = 1;
    let filter = crate::domain::models::resource_api::ProjectListFilter::default();
    match resource_repo::find_all_projects(&state.pool, PAGE, None, &filter, None).await {
        Ok(projects) => {
            let projects = caller.filter_list(
                &state.pool,
                "GET /api/v1/ai-agent/projects/",
                projects,
                |p| p.id as i64,
                |v, p| ResourceRef::Project {
                    project_id: p.id,
                    teams: p
                        .teams
                        .iter()
                        .map(|t| v.team_facts_for_read(t.id))
                        .collect(),
                },
            );
            (StatusCode::OK, Json(projects)).into_response()
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Wiki ページ一覧(読み取り)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct WikiPagesQuery {
    pub project_prefix: Option<String>,
}

/// GET /api/v1/ai-agent/wiki-pages/?project_prefix=XXX
pub async fn list_wiki_pages(
    State(state): State<AppState>,
    caller: AiCaller,
    Query(params): Query<WikiPagesQuery>,
) -> impl IntoResponse {
    let project_prefix = match &params.project_prefix {
        Some(p) if !p.is_empty() => p,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(err("project_prefix は必須です")),
            )
                .into_response()
        }
    };

    if let Err(resp) = ai_project_gate(
        &state,
        &caller,
        project_prefix,
        Action::Read,
        "GET /api/v1/ai-agent/wiki-pages/",
    )
    .await
    {
        return resp;
    }
    let project_id =
        match ticket_repo::resolve_project_id_by_prefix(&state.pool, project_prefix).await {
            Ok(Some(id)) => id,
            Ok(None) => {
                return project_not_found(&state, project_prefix).await;
            }
            Err(e) => {
                tracing::error!("DB operation failed: {:?}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(err("サーバーエラーが発生しました")),
                )
                    .into_response();
            }
        };

    match wiki_repo::find_by_project(&state.pool, Some(project_id)).await {
        Ok(pages) => (StatusCode::OK, Json(pages)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// チケット一覧(読み取り)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct TicketsQuery {
    pub project_prefix: Option<String>,
    pub page: Option<i64>,
    /// 状態(カンマ区切りで複数。例: open,in_progress)
    pub status: Option<String>,
    /// 除く状態(カンマ区切り。例: resolved,closed,canceled で「未完了だけ」)
    pub status_not: Option<String>,
    /// ラベル名
    pub label: Option<String>,
    /// Cycle の ID
    pub cycle: Option<i32>,
    /// 担当者のユーザー名
    pub assignee: Option<String>,
    /// この日時以降に更新されたもの(RFC 3339。例: 2026-10-09T00:00:00+09:00。日付だけなら、その日の 0 時 UTC)
    pub updated_after: Option<String>,
}

/// カンマ区切りの一覧(空の要素は除く)
fn split_csv(v: &Option<String>) -> Option<Vec<String>> {
    v.as_ref().map(|s| {
        s.split(',')
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
            .collect()
    })
}

/// 日時(RFC 3339)か、日付(YYYY-MM-DD。0 時 UTC)を読む
fn parse_updated_after(v: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(v.trim()) {
        return Some(dt.with_timezone(&chrono::Utc));
    }
    NaiveDate::parse_from_str(v.trim(), "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|dt| dt.and_utc())
}

/// GET /api/v1/ai-agent/tickets/?project_prefix=XXX
pub async fn list_tickets(
    State(state): State<AppState>,
    caller: AiCaller,
    Query(params): Query<TicketsQuery>,
) -> impl IntoResponse {
    let project_prefix = match &params.project_prefix {
        Some(p) if !p.is_empty() => p,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(err("project_prefix は必須です")),
            )
                .into_response()
        }
    };

    if let Err(resp) = ai_project_gate(
        &state,
        &caller,
        project_prefix,
        Action::Read,
        "GET /api/v1/ai-agent/tickets/",
    )
    .await
    {
        return resp;
    }
    let project_id =
        match ticket_repo::resolve_project_id_by_prefix(&state.pool, project_prefix).await {
            Ok(Some(id)) => id,
            Ok(None) => {
                return project_not_found(&state, project_prefix).await;
            }
            Err(e) => {
                tracing::error!("DB operation failed: {:?}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(err("サーバーエラーが発生しました")),
                )
                    .into_response();
            }
        };

    let mut filter = ticket_repo::ApiTicketFilter::default();
    filter.project = Some(project_id);
    // 新しい判定(on)では、個人キーの持ち主の見える範囲で絞る(共有キーは絞らない。設計書 §12.2)
    filter.scope = caller.scope_if_on();
    // 絞り込み(DEMO-000222)
    filter.status = split_csv(&params.status);
    filter.status_not = split_csv(&params.status_not);
    filter.label_name = params
        .label
        .as_ref()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty());
    filter.cycle = params.cycle;
    if let Some(v) = &params.updated_after {
        match parse_updated_after(v) {
            Some(dt) => filter.updated_after = Some(dt),
            None => {
                return bad_request(
                    "updated_after は日時(例: 2026-10-09T00:00:00+09:00)か日付(例: 2026-10-09)で指定してください",
                )
            }
        }
    }
    if let Some(username) = params.assignee.as_deref().filter(|u| !u.is_empty()) {
        match user_repo::find_by_username(&state.pool, username).await {
            Ok(Some(u)) => filter.assignees = Some(u.id),
            Ok(None) => return bad_request(format!("ユーザー '{}' が見つかりません", username)),
            Err(e) => return ai_db_error(e),
        }
    }

    let page = params.page.unwrap_or(1).max(1);

    let tickets = match ticket_repo::api_find_all(&state.pool, &filter, "-updated_at", None, page)
        .await
    {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("[AIエージェントAPI/チケット一覧] 処理=チケット取得 結果=失敗 影響=一覧を返せない | {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
    };
    // 試運転: 持ち主に見えない行を記録する(on では SQL で絞り済み)
    let tickets = caller.filter_list(
        &state.pool,
        "GET /api/v1/ai-agent/tickets/",
        tickets,
        |t| t.id as i64,
        |v, t| ResourceRef::Ticket {
            team: t.team.as_ref().map(|tm| v.team_facts_for_read(tm.id)),
            project_id: t.project,
            author_id: Some(t.author.id),
            assignee_ids: t.assignees.iter().map(|a| a.id).collect(),
        },
    );

    let count = match ticket_repo::api_count_all(&state.pool, &filter, None).await {
        Ok(c) => c,
        Err(e) => {
            // 件数取得の失敗は一覧そのものの失敗ではないので、ヘッダー無しで返す（フェイルソフト）
            tracing::error!("[AIエージェントAPI/チケット一覧] 処理=件数取得 結果=失敗 影響=X-Total-Countヘッダー省略 | {}", e);
            let mut resp = (StatusCode::OK, Json(tickets)).into_response();
            return resp;
        }
    };

    let mut resp = (StatusCode::OK, Json(tickets)).into_response();
    resp.headers_mut().insert(
        "x-total-count",
        HeaderValue::from_str(&count.to_string()).unwrap_or(HeaderValue::from_static("0")),
    );
    resp.into_response()
}

// ---------------------------------------------------------------------------
// チケット詳細取得
// ---------------------------------------------------------------------------

/// GET /api/v1/ai-agent/tickets/{ticket_key}/
pub async fn get_ticket(
    State(state): State<AppState>,
    caller: AiCaller,
    Path(ticket_key): Path<String>,
) -> impl IntoResponse {
    if let Err(resp) = ai_ticket_gate(
        &state,
        &caller,
        &ticket_key,
        Action::Read,
        "GET /api/v1/ai-agent/tickets/{ticket_key}/",
    )
    .await
    {
        return resp;
    }
    match ticket_repo::api_find_by_key(
        &state.pool,
        &ticket_key,
        None,
        &state.config.wip_ai_api_user,
    )
    .await
    {
        Ok(Some(ticket)) => (StatusCode::OK, Json(ticket)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(err(format!("チケット '{}' が見つかりません", ticket_key))),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// チケット更新(ステータス・優先度等)
// ---------------------------------------------------------------------------

/// ラベルの名前での指定(AI 専用。AI はラベルの番号を知らないため)
/// - `label_names`: この名前のラベルに置き換える
/// - `add_labels`: 足す(無ければ作る)
/// - `remove_labels`: 外す(付いていなければ何もしない)
#[derive(Default)]
struct AiLabelOps {
    replace: Option<Vec<String>>,
    add: Vec<String>,
    remove: Vec<String>,
}

impl AiLabelOps {
    fn is_empty(&self) -> bool {
        self.replace.is_none() && self.add.is_empty() && self.remove.is_empty()
    }

    /// 入力から名前の指定を取り出す。残りを返す
    #[allow(clippy::result_large_err)]
    fn take_from(
        raw: serde_json::Value,
    ) -> Result<(Self, serde_json::Value), axum::response::Response> {
        let serde_json::Value::Object(mut map) = raw else {
            return Err(bad_request("入力は JSON のオブジェクトで指定してください"));
        };
        let mut take = |key: &str| -> Result<Option<Vec<String>>, axum::response::Response> {
            match map.remove(key) {
                None | Some(serde_json::Value::Null) => Ok(None),
                Some(v) => serde_json::from_value::<Vec<String>>(v)
                    .map(Some)
                    .map_err(|_| bad_request(format!("{} は文字列の配列で指定してください", key))),
            }
        };
        let replace = take("label_names")?;
        let add = take("add_labels")?.unwrap_or_default();
        let remove = take("remove_labels")?.unwrap_or_default();
        if replace.is_some() && (!add.is_empty() || !remove.is_empty()) {
            return Err(bad_request(
                "label_names(置き換え)と add_labels / remove_labels は、同時に指定できません",
            ));
        }
        Ok((
            AiLabelOps {
                replace,
                add,
                remove,
            },
            serde_json::Value::Object(map),
        ))
    }

    /// 更新後のラベルの ID の一覧を決める
    async fn apply(
        &self,
        conn: &mut sqlx::PgConnection,
        ticket_id: i32,
        project_id: Option<i32>,
        team_id: i32,
    ) -> anyhow::Result<Vec<i32>> {
        if let Some(names) = &self.replace {
            return resolve_ai_labels(conn, project_id, team_id, names).await;
        }
        let current: Vec<(i32, String)> = sqlx::query_as(
            "SELECT l.id::int4, l.name FROM tickets_ticket_labels tl
             JOIN m_label l ON l.id = tl.labelmodel_id
             WHERE tl.ticketmodel_id = $1 ORDER BY tl.id",
        )
        .bind(ticket_id)
        .fetch_all(&mut *conn)
        .await?;
        let mut ids: Vec<i32> = current
            .iter()
            .filter(|(_, name)| !self.remove.iter().any(|r| r.trim() == name))
            .map(|(id, _)| *id)
            .collect();
        for id in resolve_ai_labels(conn, project_id, team_id, &self.add).await? {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        Ok(ids)
    }
}

/// PATCH /api/v1/ai-agent/tickets/{ticket_key}/
/// チケットの状態(status, priority等)を更新する。
pub async fn patch_ticket(
    State(state): State<AppState>,
    caller: AiCaller,
    Path(ticket_key): Path<String>,
    Json(raw): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(resp) = ai_ticket_gate(
        &state,
        &caller,
        &ticket_key,
        Action::Write,
        "PATCH /api/v1/ai-agent/tickets/{ticket_key}/",
    )
    .await
    {
        return resp;
    }
    let ai_user_id = caller.legacy.user_id;

    // ラベルの名前での指定(AI 専用)を取り出してから、残りを通常の更新項目として読む
    let label_ops = match AiLabelOps::take_from(raw) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let (label_ops, raw) = label_ops;
    let mut body: TicketPatchIn = match serde_json::from_value(raw) {
        Ok(b) => b,
        Err(e) => return bad_request(format!("入力の形が正しくありません: {}", e)),
    };
    if body.labels.is_some() && !label_ops.is_empty() {
        return bad_request(
            "labels(番号)と label_names / add_labels / remove_labels(名前)は、同時に指定できません",
        );
    }

    let (ticket_id, project_id, team_id) = match sqlx::query_as::<_, (i32, Option<i32>, Option<i32>)>(
        "SELECT id::int4, project_id::int4, team_id::int4 FROM tickets_ticket WHERE ticket_key = $1",
    )
    .bind(&ticket_key)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(Some(row)) => row,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(err(format!("チケット '{}' が見つかりません", ticket_key))),
            )
                .into_response();
        }
        Err(e) => return ai_db_error(e.into()),
    };

    // 状態は、そのプロジェクト・チームに登録されたものだけ(存在しない状態を入れると、画面で Backlog に見える)
    if let Some(status) = &body.status {
        if let Err(resp) = validate_ai_status(&state.pool, project_id, team_id, status).await {
            return resp;
        }
    }
    if let Some(points) = body.story_points {
        if let Err(resp) = validate_ai_story_points(points) {
            return resp;
        }
    }

    let mut tx = match state.pool.begin().await {
        Ok(t) => t,
        Err(e) => return ai_db_error(e.into()),
    };

    if !label_ops.is_empty() {
        let Some(scope_team) = team_id else {
            if let Err(e) = tx.rollback().await {
                tracing::error!("transaction rollback failed: {:?}", e);
            }
            return bad_request("チームの無いチケットには、名前でラベルを付けられません");
        };
        match label_ops
            .apply(&mut tx, ticket_id, project_id, scope_team)
            .await
        {
            Ok(ids) => body.labels = Some(ids),
            Err(e) => {
                if let Err(e) = tx.rollback().await {
                    tracing::error!("transaction rollback failed: {:?}", e);
                }
                return ai_db_error(e);
            }
        }
    }

    match ticket_repo::api_patch(&mut tx, &ticket_key, &body, ai_user_id).await {
        Err(e) => {
            tracing::error!("api_patch failed: {:?}", e);
            if let Err(e) = tx.rollback().await {
                tracing::error!("transaction rollback failed: {:?}", e);
            }
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
        Ok(Some(_)) => {
            if let Err(e) = tx.commit().await {
                tracing::error!("transaction commit failed: {:?}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(err("サーバーエラーが発生しました")),
                )
                    .into_response();
            }

            // AI が状態を「着手」に進めたとき、開始日が空なら入れる(人が入れた開始日は変えない)。
            // 失敗しても、状態の更新は成功させる
            if body.status.is_some() {
                if let Err(e) =
                    ai_run_repo::set_start_date_if_started(&state.pool, ticket_id, ai_user_id).await
                {
                    tracing::warn!(
                        "[AI API] 処理=開始日の設定 結果=失敗 影響=開始日が空のまま | ticket={} {:?}",
                        ticket_key,
                        e
                    );
                }
            }

            match ticket_repo::api_find_by_key(
                &state.pool,
                &ticket_key,
                None,
                &state.config.wip_ai_api_user,
            )
            .await
            {
                Ok(Some(ticket)) => (StatusCode::OK, Json(ticket)).into_response(),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(err("サーバーエラーが発生しました")),
                )
                    .into_response(),
            }
        }
        Ok(None) => {
            if let Err(e) = tx.rollback().await {
                tracing::error!("transaction rollback failed: {:?}", e);
            }
            (StatusCode::NOT_FOUND, Json(err("見つかりません"))).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// チケット論理削除
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct DeleteTicketQuery {
    pub reason: Option<String>,
}

/// DELETE /api/v1/ai-agent/tickets/{ticket_key}/?reason=...
/// 物理削除ではなく論理削除(status を canceled に変更)。削除理由を監査コメントとして残す。
pub async fn delete_ticket(
    State(state): State<AppState>,
    caller: AiCaller,
    Path(ticket_key): Path<String>,
    Query(params): Query<DeleteTicketQuery>,
) -> impl IntoResponse {
    if let Err(resp) = ai_ticket_gate(
        &state,
        &caller,
        &ticket_key,
        Action::Delete,
        "DELETE /api/v1/ai-agent/tickets/{ticket_key}/",
    )
    .await
    {
        return resp;
    }
    let ai_auth = caller.legacy.clone();
    let ai_user_id = ai_auth.user_id;

    let reason = match &params.reason {
        Some(r) if !r.trim().is_empty() => r.clone(),
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(err("reason(削除理由)は必須です")),
            )
                .into_response()
        }
    };

    let ticket_id = match ticket_repo::resolve_ticket_id(&state.pool, &ticket_key).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(err(format!("チケット '{}' が見つかりません", ticket_key))),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
    };

    // 監査コメントを先に投稿(ステータス変更が失敗しても、少なくとも削除が試みられた記録は残る)
    let comment_body = format!("[AI Agent] このチケットはAIエージェント経由で削除(ステータス変更: canceled)されました。理由: {}", reason);
    if let Err(e) = ticket_repo::api_add_comment(
        &state.pool,
        ticket_id,
        caller.comment_author(),
        &comment_body,
        None,
        None,
        ai_auth.acting_user_id,
    )
    .await
    {
        tracing::error!("Failed to post audit comment: {:?}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(err("監査コメントの投稿に失敗しました")),
        )
            .into_response();
    }

    let patch_in = crate::domain::models::ticket_api::TicketPatchIn {
        status: Some("canceled".to_string()),
        ..Default::default()
    };

    let mut tx = match state.pool.begin().await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
    };

    match ticket_repo::api_patch(&mut tx, &ticket_key, &patch_in, ai_user_id).await {
        Ok(Some(_)) => {
            if let Err(e) = tx.commit().await {
                tracing::error!("transaction commit failed: {:?}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(err("サーバーエラーが発生しました")),
                )
                    .into_response();
            }
            match ticket_repo::api_find_by_key(
                &state.pool,
                &ticket_key,
                None,
                &state.config.wip_ai_api_user,
            )
            .await
            {
                Ok(Some(ticket)) => (StatusCode::OK, Json(ticket)).into_response(),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(err("サーバーエラーが発生しました")),
                )
                    .into_response(),
            }
        }
        Ok(None) => {
            if let Err(e) = tx.rollback().await {
                tracing::error!("transaction rollback failed: {:?}", e);
            }
            (StatusCode::NOT_FOUND, Json(err("見つかりません"))).into_response()
        }
        Err(e) => {
            tracing::error!("api_patch failed: {:?}", e);
            if let Err(e) = tx.rollback().await {
                tracing::error!("transaction rollback failed: {:?}", e);
            }
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// コメント追加
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct AddAiCommentIn {
    pub body: String,
}

/// POST /api/v1/ai-agent/tickets/{ticket_key}/comments/
/// チケットにコメント(解決ノート等)を追加する。
pub async fn add_comment(
    State(state): State<AppState>,
    caller: AiCaller,
    Path(ticket_key): Path<String>,
    Json(body): Json<AddAiCommentIn>,
) -> impl IntoResponse {
    if let Err(resp) = ai_ticket_gate(
        &state,
        &caller,
        &ticket_key,
        Action::Write,
        "POST /api/v1/ai-agent/tickets/{ticket_key}/comments/",
    )
    .await
    {
        return resp;
    }
    let ai_auth = caller.legacy.clone();

    let ticket_id = match ticket_repo::resolve_ticket_id(&state.pool, &ticket_key).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(err(format!("チケット '{}' が見つかりません", ticket_key))),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
    };

    match ticket_repo::api_add_comment(
        &state.pool,
        ticket_id,
        caller.comment_author(),
        &body.body,
        None,
        None,
        ai_auth.acting_user_id,
    )
    .await
    {
        Ok(comment) => {
            // メンション通知(画面で書いたコメントと同じ。AI 経由でも通知する)
            if let Err(e) = crate::presentation::handlers::tickets_api::process_mentions(
                &state.pool,
                &state.mail_sender,
                ticket_id,
                caller.comment_author(),
                &body.body,
                None,
            )
            .await
            {
                tracing::warn!("mention processing failed: {:?}", e);
            }
            let pool = state.pool.clone();
            let ai_config = state.ai_config().await;
            tokio::spawn(async move {
                if let Err(e) =
                    ai_service::generate_and_cache_ai_prompt(ticket_id, &pool, &ai_config).await
                {
                    tracing::warn!(ticket_id, "generate_and_cache_ai_prompt failed: {e:#}");
                }
            });
            (StatusCode::CREATED, Json(comment)).into_response()
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
    }
}

/// PATCH /api/v1/ai-agent/tickets/{ticket_key}/comments/{comment_id}/
/// 既存コメントの本文を直す。AI が直せるのは次のコメントだけ。
/// - 自分(人ごとのキーの持ち主)が AI 経由で書いたコメント
/// - 共有の AI アカウントが書き、実行者の記録が無いコメント(共有キー時代のもの)
/// 人が画面で書いたコメントと、ほかの人が AI 経由で書いたコメントは直せない。
pub async fn update_comment(
    State(state): State<AppState>,
    caller: AiCaller,
    Path((ticket_key, comment_id)): Path<(String, i32)>,
    Json(body): Json<AddAiCommentIn>,
) -> impl IntoResponse {
    const ROUTE: &str = "PATCH /api/v1/ai-agent/tickets/{ticket_key}/comments/{comment_id}/";
    if let Err(resp) = ai_ticket_gate(&state, &caller, &ticket_key, Action::Write, ROUTE).await {
        return resp;
    }
    if body.body.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(err("body は必須です"))).into_response();
    }
    let ticket_id = match ticket_repo::resolve_ticket_id(&state.pool, &ticket_key).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(err(format!("チケット '{}' が見つかりません", ticket_key))),
            )
                .into_response();
        }
        Err(e) => return ai_db_error(e),
    };
    let (comment_ticket_id, author_id, is_deleted, acting_user_id) =
        match ticket_repo::api_find_comment_owner(&state.pool, comment_id).await {
            Ok(Some(o)) => o,
            Ok(None) => {
                return (StatusCode::NOT_FOUND, Json(err("コメントが見つかりません")))
                    .into_response();
            }
            Err(e) => return ai_db_error(e),
        };
    if comment_ticket_id != ticket_id || is_deleted {
        return (StatusCode::NOT_FOUND, Json(err("コメントが見つかりません"))).into_response();
    }
    let editable = match acting_user_id {
        Some(acting) => caller.legacy.acting_user_id == Some(acting),
        None => author_id == caller.legacy.user_id,
    };
    if !editable {
        return (
            StatusCode::FORBIDDEN,
            Json(err("このコメントは AI からは編集できません(自分が AI 経由で書いたコメントだけ編集できます)")),
        )
            .into_response();
    }
    let previous_body: Option<String> =
        sqlx::query_scalar("SELECT body FROM tickets_comment WHERE id = $1")
            .bind(comment_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);
    match ticket_repo::api_update_comment(&state.pool, comment_id, &body.body).await {
        Ok(comment) => {
            // 編集で新しく増えたメンションだけ通知する
            if let Err(e) = crate::presentation::handlers::tickets_api::process_mentions(
                &state.pool,
                &state.mail_sender,
                ticket_id,
                caller.comment_author(),
                &body.body,
                previous_body.as_deref(),
            )
            .await
            {
                tracing::warn!("mention processing failed: {:?}", e);
            }
            let pool = state.pool.clone();
            let ai_config = state.ai_config().await;
            tokio::spawn(async move {
                if let Err(e) =
                    ai_service::generate_and_cache_ai_prompt(ticket_id, &pool, &ai_config).await
                {
                    tracing::warn!(ticket_id, "generate_and_cache_ai_prompt failed: {e:#}");
                }
            });
            (StatusCode::OK, Json(comment)).into_response()
        }
        Err(e) => ai_db_error(e),
    }
}

// ---------------------------------------------------------------------------
// Cycle作成
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateAiCycleIn {
    pub project_prefix: Option<String>,
    pub name: Option<String>,
    #[serde(default)]
    pub description: String,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    #[serde(default = "default_cycle_status")]
    pub status: String,
}
fn default_cycle_status() -> String {
    "planned".to_string()
}

/// POST /api/v1/ai-agent/cycles/
pub async fn create_cycle(
    State(state): State<AppState>,
    caller: AiCaller,
    Json(body): Json<CreateAiCycleIn>,
) -> impl IntoResponse {
    if let Some(prefix) = body.project_prefix.as_deref().filter(|p| !p.is_empty()) {
        if let Err(resp) = ai_project_gate(
            &state,
            &caller,
            prefix,
            Action::Write,
            "POST /api/v1/ai-agent/cycles/",
        )
        .await
        {
            return resp;
        }
    }
    let ai_user_id = caller.legacy.user_id;

    let project_prefix = match &body.project_prefix {
        Some(p) if !p.is_empty() => p,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(err("project_prefix, name, start_date, end_date は必須です")),
            )
                .into_response()
        }
    };
    let name = match &body.name {
        Some(n) if !n.is_empty() => n,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(err("project_prefix, name, start_date, end_date は必須です")),
            )
                .into_response()
        }
    };
    let (start_date, end_date) = match (body.start_date, body.end_date) {
        (Some(s), Some(e)) => (s, e),
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(err("project_prefix, name, start_date, end_date は必須です")),
            )
                .into_response()
        }
    };
    if start_date >= end_date {
        return (
            StatusCode::BAD_REQUEST,
            Json(err("start_date は end_date より前である必要があります")),
        )
            .into_response();
    }

    let project_id =
        match ticket_repo::resolve_project_id_by_prefix(&state.pool, project_prefix).await {
            Ok(Some(id)) => id,
            Ok(None) => {
                return project_not_found(&state, project_prefix).await;
            }
            Err(e) => {
                tracing::error!("DB operation failed: {:?}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(err("サーバーエラーが発生しました")),
                )
                    .into_response();
            }
        };

    // project の参加チームから team_id を決定
    let participating_teams: Vec<i32> = match sqlx::query_scalar(
        "SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1::int4 ORDER BY team_id"
    )
    .bind(project_id)
    .fetch_all(&state.pool)
    .await
    {
        Ok(teams) => teams,
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    let team_id = match participating_teams.as_slice() {
        [single_team] => *single_team,
        [] => {
            return (
                StatusCode::BAD_REQUEST,
                Json(err("project has no participating teams")),
            )
                .into_response();
        }
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(err(
                    "project has multiple teams; specify team_id or team_slug",
                )),
            )
                .into_response();
        }
    };

    let cycle_in = CycleWriteIn {
        project: Some(project_id),
        name: name.clone(),
        description: body.description.clone(),
        start_date,
        end_date,
        status: body.status.clone(),
        team_id: Some(team_id),
    };

    match cycle_repo::create_cycle(&state.pool, &cycle_in, ai_user_id).await {
        Ok(cycle_id) => match cycle_repo::find_cycle_by_id(&state.pool, cycle_id).await {
            Ok(Some(cycle)) => (StatusCode::CREATED, Json(cycle)).into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response(),
        },
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// 依存関係参照
// ---------------------------------------------------------------------------

/// GET /api/v1/ai-agent/tickets/{ticket_key}/dependencies/
pub async fn list_ticket_dependencies(
    State(state): State<AppState>,
    caller: AiCaller,
    Path(ticket_key): Path<String>,
) -> impl IntoResponse {
    if let Err(resp) = ai_ticket_gate(
        &state,
        &caller,
        &ticket_key,
        Action::Read,
        "GET /api/v1/ai-agent/tickets/{ticket_key}/dependencies/",
    )
    .await
    {
        return resp;
    }
    let _ai_user_id = caller.legacy.user_id;

    let ticket_id = match ticket_repo::resolve_ticket_id(&state.pool, &ticket_key).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(err(format!("チケット '{}' が見つかりません", ticket_key))),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
    };

    match ticket_repo::find_dependencies_for_ticket(&state.pool, ticket_id).await {
        Ok(deps) => (StatusCode::OK, Json(deps)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
    }
}

/// GET /api/v1/ai-agent/projects/{project_prefix}/dependencies/
pub async fn get_project_dependency_graph(
    State(state): State<AppState>,
    caller: AiCaller,
    Path(project_prefix): Path<String>,
) -> impl IntoResponse {
    if let Err(resp) = ai_project_gate(
        &state,
        &caller,
        &project_prefix,
        Action::Read,
        "GET /api/v1/ai-agent/projects/{project_prefix}/dependencies/",
    )
    .await
    {
        return resp;
    }
    let _ai_user_id = caller.legacy.user_id;

    let project_id =
        match ticket_repo::resolve_project_id_by_prefix(&state.pool, &project_prefix).await {
            Ok(Some(id)) => id,
            Ok(None) => {
                return project_not_found(&state, &project_prefix).await;
            }
            Err(e) => {
                tracing::error!("DB operation failed: {:?}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(err("サーバーエラーが発生しました")),
                )
                    .into_response();
            }
        };

    match ticket_repo::find_dependency_graph_for_project(&state.pool, project_id).await {
        Ok(graph) => (StatusCode::OK, Json(graph)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Wiki添付ファイル
// ---------------------------------------------------------------------------

/// POST /api/v1/ai-agent/wiki-pages/{slug}/attachments/?project_prefix=XXX
pub async fn upload_wiki_attachment(
    State(state): State<AppState>,
    caller: AiCaller,
    Path(slug): Path<String>,
    Query(params): Query<WikiPagesQuery>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let ai_user_id = caller.legacy.user_id;

    let project_prefix = match &params.project_prefix {
        Some(p) if !p.is_empty() => p,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(err("project_prefix は必須です")),
            )
                .into_response()
        }
    };

    let project_id =
        match ticket_repo::resolve_project_id_by_prefix(&state.pool, project_prefix).await {
            Ok(Some(id)) => id,
            Ok(None) => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(err(format!(
                        "プロジェクト '{}' が見つかりません",
                        project_prefix
                    ))),
                )
                    .into_response()
            }
            Err(e) => {
                tracing::error!("DB operation failed: {:?}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(err("サーバーエラーが発生しました")),
                )
                    .into_response();
            }
        };

    let wiki_page = match wiki_repo::find_by_slug(&state.pool, Some(project_id), &slug).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(err(format!("Wikiページ '{}' が見つかりません", slug))),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
    };
    if let Err(resp) = ai_wiki_gate(
        &state,
        &caller,
        wiki_page.id,
        Action::Write,
        "POST /api/v1/ai-agent/wiki-pages/{slug}/attachments/",
    )
    .await
    {
        return resp;
    }

    let mut original_filename = String::new();
    let mut file_bytes = Vec::new();
    let mut has_file = false;

    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some("file") {
            original_filename = field.file_name().unwrap_or("unnamed").to_string();
            match field.bytes().await {
                Ok(bytes) => {
                    file_bytes = bytes.to_vec();
                    has_file = true;
                }
                Err(e) => {
                    tracing::error!("Failed to read file bytes: {}", e);
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(err("ファイルの読み込みに失敗しました")),
                    )
                        .into_response();
                }
            }
            break;
        }
    }

    if !has_file || file_bytes.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(err("file フィールドが必要です")),
        )
            .into_response();
    }

    let file_size = file_bytes.len() as i32;
    let media_dir = std::path::PathBuf::from(&state.config.media_dir);
    let attachments_dir = media_dir
        .join("wiki_attachments")
        .join(wiki_page.id.to_string());

    if let Err(e) = tokio::fs::create_dir_all(&attachments_dir).await {
        tracing::error!("Failed to create wiki attachment directory: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(err("サーバーエラーが発生しました")),
        )
            .into_response();
    }

    let uuid = Uuid::new_v4().to_string();
    let stored_filename = format!("{}_{}", uuid, original_filename);
    let file_path = attachments_dir.join(&stored_filename);

    if let Err(e) = tokio::fs::write(&file_path, &file_bytes).await {
        tracing::error!("Failed to write file: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(err("サーバーエラーが発生しました")),
        )
            .into_response();
    }

    let relative_path = format!("wiki_attachments/{}/{}", wiki_page.id, stored_filename);

    match wiki_attachment_repo::create(
        &state.pool,
        wiki_page.id,
        ai_user_id,
        &original_filename,
        &relative_path,
        file_size,
    )
    .await
    {
        Ok(id) => (
            StatusCode::CREATED,
            Json(json!({
                "id": id,
                "filename": original_filename,
                "fileSize": file_size,
                "fileUrl": format!("/media/{}", relative_path),
            })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            let _ = tokio::fs::remove_file(&file_path).await;
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
    }
}

/// GET /api/v1/ai-agent/wiki-pages/{slug}/attachments/?project_prefix=XXX
pub async fn list_wiki_attachments(
    State(state): State<AppState>,
    caller: AiCaller,
    Path(slug): Path<String>,
    Query(params): Query<WikiPagesQuery>,
) -> impl IntoResponse {
    let _ai_user_id = caller.legacy.user_id;

    let project_prefix = match &params.project_prefix {
        Some(p) if !p.is_empty() => p,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(err("project_prefix は必須です")),
            )
                .into_response()
        }
    };

    let project_id =
        match ticket_repo::resolve_project_id_by_prefix(&state.pool, project_prefix).await {
            Ok(Some(id)) => id,
            Ok(None) => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(err(format!(
                        "プロジェクト '{}' が見つかりません",
                        project_prefix
                    ))),
                )
                    .into_response()
            }
            Err(e) => {
                tracing::error!("DB operation failed: {:?}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(err("サーバーエラーが発生しました")),
                )
                    .into_response();
            }
        };

    let wiki_page = match wiki_repo::find_by_slug(&state.pool, Some(project_id), &slug).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(err(format!("Wikiページ '{}' が見つかりません", slug))),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
    };
    if let Err(resp) = ai_wiki_gate(
        &state,
        &caller,
        wiki_page.id,
        Action::Read,
        "GET /api/v1/ai-agent/wiki-pages/{slug}/attachments/",
    )
    .await
    {
        return resp;
    }

    match wiki_attachment_repo::find_by_wiki_page(&state.pool, wiki_page.id).await {
        Ok(list) => (StatusCode::OK, Json(list)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Cycle CRUD (一覧/詳細/更新/削除)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CyclesQuery {
    pub project_prefix: Option<String>,
    pub status: Option<String>,
}

/// GET /api/v1/ai-agent/cycles/?project_prefix=XXX&status=YYY
pub async fn list_cycles(
    State(state): State<AppState>,
    caller: AiCaller,
    Query(params): Query<CyclesQuery>,
) -> impl IntoResponse {
    let project_id = match &params.project_prefix {
        Some(p) if !p.is_empty() => {
            match ticket_repo::resolve_project_id_by_prefix(&state.pool, p).await {
                Ok(Some(id)) => Some(id),
                Ok(None) => {
                    return (
                        StatusCode::NOT_FOUND,
                        Json(err(format!("プロジェクト '{}' が見つかりません", p))),
                    )
                        .into_response()
                }
                Err(e) => {
                    tracing::error!("DB operation failed: {:?}", e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(err("サーバーエラーが発生しました")),
                    )
                        .into_response();
                }
            }
        }
        _ => None,
    };

    match cycle_repo::find_all_cycles(&state.pool, project_id, None, params.status.as_deref()).await
    {
        Ok(cycles) => {
            let cycles = caller.filter_list(
                &state.pool,
                "GET /api/v1/ai-agent/cycles/",
                cycles,
                |c| c.id as i64,
                |v, c| ResourceRef::Cycle {
                    team: v.team_facts_for_read(c.team.as_ref().map_or(0, |t| t.id)),
                },
            );
            (StatusCode::OK, Json(cycles)).into_response()
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
    }
}

/// GET /api/v1/ai-agent/cycles/{id}/
pub async fn get_cycle(
    State(state): State<AppState>,
    caller: AiCaller,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    if let Err(resp) = ai_cycle_gate(
        &state,
        &caller,
        id,
        Action::Read,
        "GET /api/v1/ai-agent/cycles/{id}/",
    )
    .await
    {
        return resp;
    }
    match cycle_repo::find_cycle_by_id(&state.pool, id).await {
        Ok(Some(cycle)) => (StatusCode::OK, Json(cycle)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, Json(err("サイクルが見つかりません"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct CyclePatchIn {
    pub name: Option<String>,
    pub description: Option<String>,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub status: Option<String>,
    #[serde(alias = "teamId")]
    pub team_id: Option<Option<i32>>,
}

/// PATCH /api/v1/ai-agent/cycles/{id}/
pub async fn patch_cycle(
    State(state): State<AppState>,
    caller: AiCaller,
    Path(id): Path<i32>,
    Json(body): Json<CyclePatchIn>,
) -> impl IntoResponse {
    if let Err(resp) = ai_cycle_gate(
        &state,
        &caller,
        id,
        Action::Write,
        "PATCH /api/v1/ai-agent/cycles/{id}/",
    )
    .await
    {
        return resp;
    }
    let existing = match cycle_repo::find_cycle_by_id(&state.pool, id).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, Json(err("サイクルが見つかりません"))).into_response()
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
    };

    let team_id_val = body
        .team_id
        .flatten()
        .or_else(|| existing.team.as_ref().map(|t| t.id));

    let merged = CycleWriteIn {
        project: existing.project,
        name: body.name.clone().unwrap_or(existing.name.clone()),
        description: body
            .description
            .clone()
            .unwrap_or(existing.description.clone()),
        start_date: body.start_date.unwrap_or(existing.start_date),
        end_date: body.end_date.unwrap_or(existing.end_date),
        status: body.status.clone().unwrap_or(existing.status.clone()),
        team_id: team_id_val,
    };

    if merged.start_date >= merged.end_date {
        return (
            StatusCode::BAD_REQUEST,
            Json(err("start_date は end_date より前である必要があります")),
        )
            .into_response();
    }

    match cycle_repo::update_cycle(&state.pool, id, &merged).await {
        Ok(true) => match cycle_repo::find_cycle_by_id(&state.pool, id).await {
            Ok(Some(cycle)) => (StatusCode::OK, Json(cycle)).into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response(),
        },
        Ok(false) => (StatusCode::NOT_FOUND, Json(err("サイクルが見つかりません"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
    }
}

/// DELETE /api/v1/ai-agent/cycles/{id}/
pub async fn delete_cycle(
    State(state): State<AppState>,
    caller: AiCaller,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    if let Err(resp) = ai_cycle_gate(
        &state,
        &caller,
        id,
        Action::Delete,
        "DELETE /api/v1/ai-agent/cycles/{id}/",
    )
    .await
    {
        return resp;
    }
    match cycle_repo::delete_cycle(&state.pool, id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, Json(err("サイクルが見つかりません"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// 依存関係 追加/削除
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct AddAiDependencyIn {
    pub to_task_key: Option<String>,
    #[serde(default = "default_dependency_type")]
    pub dependency_type: String,
}
fn default_dependency_type() -> String {
    "blocks".to_string()
}

/// 依存の種類。画面が扱うのは blocks(先に終わらせる)と relates_to(関連)だけ
pub(crate) const AI_DEPENDENCY_TYPES: [&str; 2] = ["blocks", "relates_to"];

#[allow(clippy::result_large_err)]
pub(crate) fn validate_ai_dependency_type(t: &str) -> Result<(), axum::response::Response> {
    if AI_DEPENDENCY_TYPES.contains(&t) {
        Ok(())
    } else {
        Err(bad_request(format!(
            "dependency_type は blocks か relates_to を指定してください(指定: '{}')",
            t
        )))
    }
}

/// POST /api/v1/ai-agent/tickets/{ticket_key}/dependencies/
pub async fn add_dependency(
    State(state): State<AppState>,
    caller: AiCaller,
    Path(ticket_key): Path<String>,
    Json(body): Json<AddAiDependencyIn>,
) -> impl IntoResponse {
    if let Err(resp) = ai_ticket_gate(
        &state,
        &caller,
        &ticket_key,
        Action::Write,
        "POST /api/v1/ai-agent/tickets/{ticket_key}/dependencies/",
    )
    .await
    {
        return resp;
    }
    use ticket_repo::CreateDependencyResult;

    let ai_user_id = caller.legacy.user_id;

    let ticket_id = match ticket_repo::resolve_ticket_id(&state.pool, &ticket_key).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(err(format!("チケット '{}' が見つかりません", ticket_key))),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
    };

    let to_task_key = match &body.to_task_key {
        Some(k) if !k.is_empty() => k,
        _ => return (StatusCode::BAD_REQUEST, Json(err("to_task_key は必須です"))).into_response(),
    };
    if let Err(resp) = validate_ai_dependency_type(&body.dependency_type) {
        return resp;
    }
    let to_task = match ticket_repo::resolve_ticket_id(&state.pool, to_task_key).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(err(format!("チケット '{}' が見つかりません", to_task_key))),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
    };

    let result = ticket_repo::create_dependency(
        &state.pool,
        ticket_id,
        to_task,
        &body.dependency_type,
        ai_user_id,
    )
    .await;

    match result {
        Ok(CreateDependencyResult::Success(id)) => {
            match ticket_repo::find_dependency_by_id(&state.pool, id).await {
                Ok(Some(dep)) => (StatusCode::CREATED, Json(dep)).into_response(),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(err("サーバーエラーが発生しました")),
                )
                    .into_response(),
            }
        }
        Ok(CreateDependencyResult::SelfReference) => (
            StatusCode::BAD_REQUEST,
            Json(err("自分自身への依存関係は作成できません。")),
        )
            .into_response(),
        Ok(CreateDependencyResult::Duplicate) => (
            StatusCode::BAD_REQUEST,
            Json(err("この依存関係は既に存在します。")),
        )
            .into_response(),
        Ok(CreateDependencyResult::ToTaskNotFound) => (
            StatusCode::BAD_REQUEST,
            Json(err("指定されたチケットが見つかりません。")),
        )
            .into_response(),
        Ok(CreateDependencyResult::CircularDependency) => (
            StatusCode::BAD_REQUEST,
            Json(err("この依存関係を追加すると循環依存になります。")),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
    }
}

/// DELETE /api/v1/ai-agent/tickets/{ticket_key}/dependencies/{dep_id}/
pub async fn delete_dependency(
    State(state): State<AppState>,
    caller: AiCaller,
    Path((ticket_key, dep_id)): Path<(String, i32)>,
) -> impl IntoResponse {
    if let Err(resp) = ai_ticket_gate(
        &state,
        &caller,
        &ticket_key,
        Action::Write,
        "DELETE /api/v1/ai-agent/tickets/{ticket_key}/dependencies/{dep_id}/",
    )
    .await
    {
        return resp;
    }
    use ticket_repo::DeleteDependencyResult;

    let ticket_id = match ticket_repo::resolve_ticket_id(&state.pool, &ticket_key).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(err(format!("チケット '{}' が見つかりません", ticket_key))),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
    };

    match ticket_repo::delete_dependency(&state.pool, dep_id, ticket_id).await {
        Ok(DeleteDependencyResult::Deleted) => StatusCode::NO_CONTENT.into_response(),
        Ok(DeleteDependencyResult::NotFound) | Ok(DeleteDependencyResult::NotRelated) => (
            StatusCode::NOT_FOUND,
            Json(err("この依存関係は指定チケットに関連していません。")),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Wiki添付 差し替え/削除
// ---------------------------------------------------------------------------

/// PUT /api/v1/ai-agent/wiki-pages/{slug}/attachments/{attachment_id}/
pub async fn replace_wiki_attachment(
    State(state): State<AppState>,
    caller: AiCaller,
    Path((_slug, attachment_id)): Path<(String, i32)>,
    mut multipart: axum::extract::Multipart,
) -> impl IntoResponse {
    let existing = match crate::infrastructure::repositories::wiki_attachment_repo::find_by_id(
        &state.pool,
        attachment_id,
    )
    .await
    {
        Ok(Some(a)) => a,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(err("添付ファイルが見つかりません")),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
    };
    if let Err(resp) = ai_wiki_gate(
        &state,
        &caller,
        existing.wiki_page_id,
        Action::Write,
        "PUT /api/v1/ai-agent/wiki-pages/{slug}/attachments/{attachment_id}/",
    )
    .await
    {
        return resp;
    }

    let mut original_filename = String::new();
    let mut file_bytes = Vec::new();
    let mut has_file = false;

    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some("file") {
            original_filename = field.file_name().unwrap_or("unnamed").to_string();
            match field.bytes().await {
                Ok(bytes) => {
                    file_bytes = bytes.to_vec();
                    has_file = true;
                }
                Err(e) => {
                    tracing::error!("Failed to read file bytes: {}", e);
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(err("ファイルの読み込みに失敗しました")),
                    )
                        .into_response();
                }
            }
            break;
        }
    }

    if !has_file || file_bytes.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(err("file フィールドが必要です")),
        )
            .into_response();
    }

    let file_size = file_bytes.len() as i32;
    let media_dir = std::path::PathBuf::from(&state.config.media_dir);
    let attachments_dir = media_dir
        .join("wiki_attachments")
        .join(existing.wiki_page_id.to_string());

    if let Err(e) = tokio::fs::create_dir_all(&attachments_dir).await {
        tracing::error!("Failed to create wiki attachment directory: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(err("サーバーエラーが発生しました")),
        )
            .into_response();
    }

    let uuid = Uuid::new_v4().to_string();
    let stored_filename = format!("{}_{}", uuid, original_filename);
    let new_file_path = attachments_dir.join(&stored_filename);

    if let Err(e) = tokio::fs::write(&new_file_path, &file_bytes).await {
        tracing::error!("Failed to write file: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(err("サーバーエラーが発生しました")),
        )
            .into_response();
    }

    let relative_path = format!(
        "wiki_attachments/{}/{}",
        existing.wiki_page_id, stored_filename
    );
    let old_file_path = media_dir.join(&existing.file_path);

    match crate::infrastructure::repositories::wiki_attachment_repo::update(
        &state.pool,
        attachment_id,
        &original_filename,
        &relative_path,
        file_size,
    )
    .await
    {
        Ok(true) => {
            let _ = tokio::fs::remove_file(&old_file_path).await;
            (
                StatusCode::OK,
                Json(json!({
                    "id": attachment_id,
                    "filename": original_filename,
                    "fileSize": file_size,
                    "fileUrl": format!("/media/{}", relative_path),
                })),
            )
                .into_response()
        }
        Ok(false) => {
            let _ = tokio::fs::remove_file(&new_file_path).await;
            (
                StatusCode::NOT_FOUND,
                Json(err("添付ファイルが見つかりません")),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            let _ = tokio::fs::remove_file(&new_file_path).await;
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
    }
}

/// DELETE /api/v1/ai-agent/wiki-pages/{slug}/attachments/{attachment_id}/
pub async fn delete_wiki_attachment(
    State(state): State<AppState>,
    caller: AiCaller,
    Path((_slug, attachment_id)): Path<(String, i32)>,
) -> impl IntoResponse {
    let existing = match crate::infrastructure::repositories::wiki_attachment_repo::find_by_id(
        &state.pool,
        attachment_id,
    )
    .await
    {
        Ok(Some(a)) => a,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(err("添付ファイルが見つかりません")),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
    };
    if let Err(resp) = ai_wiki_gate(
        &state,
        &caller,
        existing.wiki_page_id,
        Action::Write,
        "DELETE /api/v1/ai-agent/wiki-pages/{slug}/attachments/{attachment_id}/",
    )
    .await
    {
        return resp;
    }

    match crate::infrastructure::repositories::wiki_attachment_repo::delete(
        &state.pool,
        attachment_id,
    )
    .await
    {
        Ok(true) => {
            let media_dir = std::path::PathBuf::from(&state.config.media_dir);
            let _ = tokio::fs::remove_file(media_dir.join(&existing.file_path)).await;
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(err("添付ファイルが見つかりません")),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// チケット参照リンク(リンク機能)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct AddAiTicketLinkIn {
    pub url: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
}

/// GET /api/v1/ai-agent/tickets/{ticket_key}/links/
pub async fn list_ticket_links(
    State(state): State<AppState>,
    caller: AiCaller,
    Path(ticket_key): Path<String>,
) -> impl IntoResponse {
    if let Err(resp) = ai_ticket_gate(
        &state,
        &caller,
        &ticket_key,
        Action::Read,
        "GET /api/v1/ai-agent/tickets/{ticket_key}/links/",
    )
    .await
    {
        return resp;
    }
    let ticket_id = match ticket_repo::resolve_ticket_id(&state.pool, &ticket_key).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(err(format!("チケット '{}' が見つかりません", ticket_key))),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
    };

    match crate::infrastructure::repositories::ticket_link_repo::find_by_ticket(
        &state.pool,
        ticket_id,
    )
    .await
    {
        Ok(links) => {
            let out: Vec<_> = links
                .into_iter()
                .map(|l| {
                    json!({
                        "id": l.id,
                        "url": l.url,
                        "title": l.title,
                        "createdBy": l.created_by_name,
                        "createdAt": l.created_at,
                    })
                })
                .collect();
            (StatusCode::OK, Json(out)).into_response()
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
    }
}

/// POST /api/v1/ai-agent/tickets/{ticket_key}/links/
pub async fn add_ticket_link(
    State(state): State<AppState>,
    caller: AiCaller,
    Path(ticket_key): Path<String>,
    Json(body): Json<AddAiTicketLinkIn>,
) -> impl IntoResponse {
    if let Err(resp) = ai_ticket_gate(
        &state,
        &caller,
        &ticket_key,
        Action::Write,
        "POST /api/v1/ai-agent/tickets/{ticket_key}/links/",
    )
    .await
    {
        return resp;
    }
    let ai_user_id = caller.legacy.user_id;

    let url = match &body.url {
        Some(u) if !u.trim().is_empty() => u,
        _ => return (StatusCode::BAD_REQUEST, Json(err("url は必須です"))).into_response(),
    };

    let ticket_id = match ticket_repo::resolve_ticket_id(&state.pool, &ticket_key).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(err(format!("チケット '{}' が見つかりません", ticket_key))),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
    };

    match crate::infrastructure::repositories::ticket_link_repo::create(
        &state.pool,
        ticket_id,
        url,
        body.title.as_deref(),
        ai_user_id,
    )
    .await
    {
        Ok(id) => {
            match crate::infrastructure::repositories::ticket_link_repo::find_by_id(&state.pool, id)
                .await
            {
                Ok(Some(l)) => (
                    StatusCode::CREATED,
                    Json(json!({
                        "id": l.id, "url": l.url, "title": l.title, "createdAt": l.created_at,
                    })),
                )
                    .into_response(),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(err("サーバーエラーが発生しました")),
                )
                    .into_response(),
            }
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
    }
}

/// DELETE /api/v1/ai-agent/tickets/{ticket_key}/links/{link_id}/
pub async fn delete_ticket_link(
    State(state): State<AppState>,
    caller: AiCaller,
    Path((ticket_key, link_id)): Path<(String, i32)>,
) -> impl IntoResponse {
    if let Err(resp) = ai_ticket_gate(
        &state,
        &caller,
        &ticket_key,
        Action::Write,
        "DELETE /api/v1/ai-agent/tickets/{ticket_key}/links/{link_id}/",
    )
    .await
    {
        return resp;
    }
    let ticket_id = match ticket_repo::resolve_ticket_id(&state.pool, &ticket_key).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(err(format!("チケット '{}' が見つかりません", ticket_key))),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
    };

    match crate::infrastructure::repositories::ticket_link_repo::delete(
        &state.pool,
        link_id,
        ticket_id,
    )
    .await
    {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, Json(err("見つかりません"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// チーム（一覧・作成）
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateAiTeamIn {
    pub name: Option<String>,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub color: String,
    pub slack_webhook_url: Option<String>,
    /// チケット番号の頭(英数字1〜20文字。例: AUT → AUT-000001)。省略時は slug から作る
    pub prefix: Option<String>,
    /// 省略時 true（AI経由は有効なチームを作る）
    #[serde(default = "default_team_is_active")]
    pub is_active: bool,
}
fn default_team_is_active() -> bool {
    true
}

/// GET /api/v1/ai-agent/teams/
pub async fn list_teams(State(state): State<AppState>, caller: AiCaller) -> impl IntoResponse {
    match crate::infrastructure::repositories::team_repo::find_all_teams(&state.pool, 1).await {
        Ok(teams) => {
            let teams = caller.filter_list(
                &state.pool,
                "GET /api/v1/ai-agent/teams/",
                teams,
                |t| t.id as i64,
                |v, t| ResourceRef::Team(v.team_facts_for_read(t.id)),
            );
            (StatusCode::OK, Json(teams)).into_response()
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
    }
}

/// POST /api/v1/ai-agent/teams/
pub async fn create_team(
    State(state): State<AppState>,
    caller: AiCaller,
    Json(body): Json<CreateAiTeamIn>,
) -> impl IntoResponse {
    // キー経由ではチームを作れない(新しい判定。チームの作成は人が画面で行う)
    if key_on() {
        return (
            StatusCode::FORBIDDEN,
            Json(err("AI キーではチームを作成できません")),
        )
            .into_response();
    }
    let ai_auth = caller.legacy.clone();

    let name = match &body.name {
        Some(n) if !n.trim().is_empty() => n.trim().to_string(),
        _ => return (StatusCode::BAD_REQUEST, Json(err("name は必須です"))).into_response(),
    };

    let input = crate::domain::models::team_api::TeamWriteIn {
        name,
        slug: body.slug,
        description: body.description,
        icon: if body.icon.is_empty() {
            "👥".to_string()
        } else {
            body.icon
        },
        color: if body.color.is_empty() {
            "#6366f1".to_string()
        } else {
            body.color
        },
        slack_webhook_url: body.slack_webhook_url,
        is_active: body.is_active,
        prefix: body.prefix.clone(),
        visibility: None,
    };
    if let Err(resp) = check_ai_team_prefix(&state.pool, body.prefix.as_deref(), None).await {
        return resp;
    }

    match crate::infrastructure::repositories::team_repo::create_team(&state.pool, &input).await {
        Ok(team_id) => {
            // 人ごとキーで作った場合は、キーを持つ本人を最初の管理者にする(画面からの作成と同じ)。
            // これを怠ると、管理者のいないチームができ、誰もメンバーを追加できなくなる。
            // 共有キー(誰か特定できない)の場合は追加しない。
            if let Some(user_id) = ai_auth.acting_user_id {
                if let Err(e) = crate::infrastructure::repositories::team_repo::add_team_member(
                    &state.pool,
                    team_id,
                    user_id,
                    "admin",
                )
                .await
                {
                    tracing::error!("Failed to add team creator as admin: {:?}", e);
                }
            }
            match crate::infrastructure::repositories::team_repo::find_team_by_id(
                &state.pool,
                team_id,
            )
            .await
            {
                Ok(Some(team)) => (StatusCode::CREATED, Json(team)).into_response(),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(err("サーバーエラーが発生しました")),
                )
                    .into_response(),
            }
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("チームを作成できませんでした")),
            )
                .into_response()
        }
    }
}

/// Prefix の形と重複を確かめる。重複なら、どのチームと重なったかを返す(DEMO-000217)
async fn check_ai_team_prefix(
    pool: &sqlx::PgPool,
    prefix: Option<&str>,
    exclude_team_id: Option<i32>,
) -> Result<(), axum::response::Response> {
    let normalized = match team_repo::normalize_and_validate_prefix(prefix) {
        Ok(Some(p)) => p,
        Ok(None) => return Ok(()),
        Err(e) => return Err(bad_request(format!("prefix: {}", e))),
    };
    let other: Option<(String, String)> = sqlx::query_as(
        "SELECT name, slug FROM m_team
         WHERE UPPER(prefix) = $1 AND ($2::int4 IS NULL OR id <> $2::int4)
         LIMIT 1",
    )
    .bind(&normalized)
    .bind(exclude_team_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| ai_db_error(e.into()))?;
    match other {
        Some((name, slug)) => Err((
            StatusCode::CONFLICT,
            Json(err(format!(
                "Prefix '{}' は、チーム「{}」(slug: {})で使われています。別の Prefix を指定してください",
                normalized, name, slug
            ))),
        )
            .into_response()),
        None => Ok(()),
    }
}

/// チームの変更(指定した項目だけ)
#[derive(Deserialize)]
pub struct PatchAiTeamIn {
    pub name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    /// チケット番号の頭。変えても、作成済みのチケットの番号は変わらない(以後の新しいチケットから)
    pub prefix: Option<String>,
    pub is_active: Option<bool>,
}

/// PATCH /api/v1/ai-agent/teams/{team}/ (team は ID か slug)
///
/// チームの設定の変更は、チームの管理者の個人キーだけ(共有キー・管理者でない人のキーは 403)。
/// 新しい判定(on)では、キー経由の設定の管理はできない(画面と同じ。キー経由は ManageSettings 不可)
pub async fn patch_team(
    State(state): State<AppState>,
    caller: AiCaller,
    Path(team): Path<String>,
    Json(body): Json<PatchAiTeamIn>,
) -> impl IntoResponse {
    if key_on() {
        return (
            StatusCode::FORBIDDEN,
            Json(err(
                "AI キーではチームの設定を変更できません。画面から変更してください",
            )),
        )
            .into_response();
    }
    let team_id: i32 = match team.parse::<i32>() {
        Ok(id) => id,
        Err(_) => match sqlx::query_scalar::<_, i32>("SELECT id::int4 FROM m_team WHERE slug = $1")
            .bind(&team)
            .fetch_optional(&state.pool)
            .await
        {
            Ok(Some(id)) => id,
            Ok(None) => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(err(format!("チーム '{}' が見つかりません", team))),
                )
                    .into_response()
            }
            Err(e) => return ai_db_error(e.into()),
        },
    };
    let Some(owner) = caller.legacy.acting_user_id else {
        return (
            StatusCode::FORBIDDEN,
            Json(err(
                "共有の AI キーではチームを変更できません。個人の AI キーを使ってください",
            )),
        )
            .into_response();
    };
    match crate::presentation::handlers::access_guard::can_manage_team(&state.pool, owner, team_id)
        .await
    {
        Ok(true) => {}
        Ok(false) => {
            return (
                StatusCode::FORBIDDEN,
                Json(err(
                    "チームの設定を変えられるのは、そのチームの管理者だけです",
                )),
            )
                .into_response()
        }
        Err(e) => return ai_db_error(e),
    }
    let current = match team_repo::find_team_by_id(&state.pool, team_id).await {
        Ok(Some(t)) => t,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(err(format!("チーム '{}' が見つかりません", team))),
            )
                .into_response()
        }
        Err(e) => return ai_db_error(e),
    };
    if current.archived_at.is_some() {
        return (
            StatusCode::CONFLICT,
            Json(err(
                "アーカイブ済みのチームは変更できません(先に画面で復元してください)",
            )),
        )
            .into_response();
    }
    if let Some(name) = &body.name {
        if name.trim().is_empty() {
            return bad_request("name は空にできません");
        }
    }
    if let Err(resp) =
        check_ai_team_prefix(&state.pool, body.prefix.as_deref(), Some(team_id)).await
    {
        return resp;
    }

    // update_team は全項目を書き換えるので、指定の無い項目は今の値を入れる
    let input = crate::domain::models::team_api::TeamWriteIn {
        name: body
            .name
            .as_ref()
            .map(|n| n.trim().to_string())
            .unwrap_or(current.name),
        slug: String::new(),
        description: body.description.unwrap_or_default(),
        icon: body.icon.unwrap_or_default(),
        color: body.color.unwrap_or_default(),
        slack_webhook_url: current.slack_webhook_url,
        is_active: body.is_active.unwrap_or(current.is_active),
        prefix: body.prefix,
        visibility: None,
    };
    match team_repo::update_team(&state.pool, team_id, &input).await {
        Ok(true) => match team_repo::find_team_by_id(&state.pool, team_id).await {
            Ok(Some(t)) => (StatusCode::OK, Json(t)).into_response(),
            Ok(None) => (StatusCode::NOT_FOUND, Json(err("見つかりません"))).into_response(),
            Err(e) => ai_db_error(e),
        },
        Ok(false) => (StatusCode::NOT_FOUND, Json(err("見つかりません"))).into_response(),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("Prefix") || msg.contains("別のチームで使われています") {
                bad_request(msg)
            } else {
                ai_db_error(e)
            }
        }
    }
}
