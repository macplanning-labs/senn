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
    ai_agent_key_repo, cycle_repo, resource_repo, team_repo, ticket_repo, user_repo,
    wiki_attachment_repo, wiki_repo,
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
async fn authenticate_ai(
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
async fn resolve_shared_ai_user(state: &AppState) -> Result<i32, (StatusCode, serde_json::Value)> {
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
    let created = user_repo::create_user(
        &state.pool,
        username,
        "ai-agent@localhost",
        "!",
        "AI",
        "Agent",
    )
    .await
    .map_err(|e| {
        tracing::error!("AI agent account creation failed: {:?}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            err("サーバーエラーが発生しました"),
        )
    })?;

    Ok(created.id)
}

// ---------------------------------------------------------------------------
// アクセス制御の再設計(F-2・F-4)の共通部品
// ---------------------------------------------------------------------------

/// 新しい判定(`ACCESS_ENFORCE_KEY=on`)か
fn key_on() -> bool {
    shadow::mode(Resource::Key) == Mode::On
}

fn ai_db_error(e: anyhow::Error) -> axum::response::Response {
    tracing::error!("DB operation failed: {:?}", e);
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(err("サーバーエラーが発生しました")),
    )
        .into_response()
}

/// 見つからないプロジェクトの応答。新しい判定(on)では、ほかのプロジェクトの Prefix を一覧しない
/// (見えない物の名前を漏らさない。F-4・設計書 §5.3)
async fn project_not_found(state: &AppState, project_prefix: &str) -> axum::response::Response {
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
async fn ai_ticket_gate(
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
async fn ai_project_gate(
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
async fn ai_wiki_gate(
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
    /// ラベル名(存在しなければ同名でプロジェクト内に新規作成する)
    #[serde(default)]
    pub labels: Vec<String>,
    /// 担当者のユーザー名。プロジェクト未参加なら自動的にメンバーへ追加してから割り当てる。
    #[serde(default)]
    pub assignee_usernames: Vec<String>,
}
fn default_priority() -> String {
    "medium".to_string()
}
fn default_ticket_type() -> String {
    "task".to_string()
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
        _ => return (StatusCode::BAD_REQUEST, Json(err("title は必須です"))).into_response(),
    };

    // Resolve Team ID from team_id, team_slug, or project_prefix's participating teams
    let team_id: i32 = if let Some(tid) = body.team_id {
        tid
    } else if let Some(slug) = &body.team_slug {
        match sqlx::query_scalar::<_, i32>("SELECT id::int4 FROM m_team WHERE slug = $1")
            .bind(slug)
            .fetch_optional(&state.pool)
            .await
        {
            Ok(Some(tid)) => tid,
            Ok(None) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(err(format!("チーム '{}' が見つかりません", slug))),
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
        }
    } else if let Some(project_prefix) = &body.project_prefix {
        // Resolve project by prefix and get its participating teams
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

        let project = match resource_repo::find_project_by_id(&state.pool, project_id, None).await {
            Ok(Some(p)) => p,
            Ok(None) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(err("サーバーエラーが発生しました")),
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

        match project.teams.as_slice() {
            [team] => team.id,
            [] => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(err(format!(
                        "プロジェクト '{}' に参加チームがありません",
                        project_prefix
                    ))),
                )
                    .into_response();
            }
            _ => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(err(format!(
                        "プロジェクト '{}' は複数チームに参加しており、teamId/teamSlug が必須です",
                        project_prefix
                    ))),
                )
                    .into_response();
            }
        }
    } else {
        return (
            StatusCode::BAD_REQUEST,
            Json(err(
                "project_prefix, team_id, または team_slug のいずれかが必須です",
            )),
        )
            .into_response();
    };

    // Verify team exists
    match team_repo::find_team_by_id(&state.pool, team_id).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(err(format!("チーム ID {} が見つかりません", team_id))),
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
    }

    // Get project_id if project_prefix was specified
    let project_id = if let Some(project_prefix) = &body.project_prefix {
        match ticket_repo::resolve_project_id_by_prefix(&state.pool, project_prefix).await {
            Ok(Some(id)) => Some(id),
            Ok(None) => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(err(format!(
                        "プロジェクト '{}' が見つかりません",
                        project_prefix
                    ))),
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
        }
    } else {
        None
    };

    // 作成先の判定(新しい判定。持ち主がそのチームにチケットを作れること)
    let team_facts = match facts_repo::facts_for_team(&state.pool, team_id).await {
        Ok(t) => t,
        Err(e) => return ai_db_error(e),
    };
    let target = ResourceRef::Ticket {
        team: team_facts,
        project_id,
        author_id: caller.viewer.as_ref().and_then(|v| v.user_id()),
        assignee_ids: vec![],
    };
    if let Err(resp) = caller.gate(
        &state.pool,
        Some(&target),
        Action::Create,
        team_id as i64,
        "POST /api/v1/ai-agent/tickets/",
    ) {
        return resp;
    }

    // ラベルは名前指定。Project があれば Project マスタ、無ければ所属 Team マスタ。
    let mut label_ids = Vec::with_capacity(body.labels.len());
    if !body.labels.is_empty() {
        for name in &body.labels {
            match resource_repo::find_or_create_label_for_scope(
                &state.pool,
                project_id,
                Some(team_id),
                name,
            )
            .await
            {
                Ok(id) => label_ids.push(id),
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
    }

    // 担当者はユーザー名 → user_id に解決
    let mut assignee_ids = Vec::with_capacity(body.assignee_usernames.len());
    for username in &body.assignee_usernames {
        let user = match user_repo::find_by_username(&state.pool, username).await {
            Ok(Some(u)) => u,
            Ok(None) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(err(format!("ユーザー '{}' が見つかりません", username))),
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
                    crate::domain::access::can(&v, Action::Read, &target)
                        == crate::domain::access::Decision::Allow
                }
                Ok(None) => false,
                Err(e) => return ai_db_error(e),
            };
            if !visible {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(err(format!(
                        "ユーザー '{}' はこのチケットを見られないため、担当者にできません",
                        username
                    ))),
                )
                    .into_response();
            }
        // L2②: Project限定ゲストとしての追加はproject_idがある場合のみ(team_idはこのprojectの所有チーム)
        } else if let Some(pid) = project_id {
            let is_member = match team_repo::check_project_access(&state.pool, pid, user.id).await {
                Ok(b) => b,
                Err(e) => {
                    tracing::error!("DB operation failed: {:?}", e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(err("サーバーエラーが発生しました")),
                    )
                        .into_response();
                }
            };
            if !is_member {
                if let Err(e) =
                    team_repo::add_team_guest(&state.pool, team_id, user.id, pid, None).await
                {
                    tracing::error!("DB operation failed: {:?}", e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(err("サーバーエラーが発生しました")),
                    )
                        .into_response();
                }
            }
        }

        assignee_ids.push(user.id);
    }

    let ticket_in = TicketWriteIn {
        title: title.clone(),
        description: body.description.clone(),
        status: "open".to_string(),
        priority: body.priority.clone(),
        ticket_type: body.ticket_type.clone(),
        assignees: assignee_ids,
        reviewers: Vec::new(),
        category: None,
        project: project_id,
        milestone: None,
        parent: None,
        start_date: None,
        due_date: body.due_date,
        labels: label_ids,
        story_points: None,
        cycle: None,
        team_id: Some(team_id),
        linked_rules: Vec::new(),
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

    let ticket_id = match ticket_repo::api_create(&mut tx, &ticket_in, author_id).await {
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
    // AI 経由の印(画面で「AI」と表示する。作成者が持ち主本人になっても、AI 経由だと分かるように)
    if let Err(e) = ticket_repo::mark_created_via_ai(&mut tx, ticket_id, caller.legacy.key_id).await
    {
        tracing::error!("mark_created_via_ai failed: {:?}", e);
        if let Err(e) = tx.rollback().await {
            tracing::error!("transaction rollback failed: {:?}", e);
        }
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(err("サーバーエラーが発生しました")),
        )
            .into_response();
    }

    if let Err(e) = tx.commit().await {
        tracing::error!("transaction commit failed: {:?}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(err("サーバーエラーが発生しました")),
        )
            .into_response();
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

/// PATCH /api/v1/ai-agent/tickets/{ticket_key}/
/// チケットの状態(status, priority等)を更新する。
pub async fn patch_ticket(
    State(state): State<AppState>,
    caller: AiCaller,
    Path(ticket_key): Path<String>,
    Json(body): Json<TicketPatchIn>,
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

    let _ticket_id = match ticket_repo::resolve_ticket_id(&state.pool, &ticket_key).await {
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
        prefix: None,
        visibility: None,
    };

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
