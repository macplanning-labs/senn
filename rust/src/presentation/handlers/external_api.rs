/// presentation/handlers/external_api.rs — 外部API(X-API-Keyヘッダー認証)
///
/// Django apps/api/views/external.py, apps/api/auth.py の移植。
/// JWT/セッション認証を使わず、X-API-Keyヘッダーの値をWIP_API_KEYと比較する。
/// 認証成功時はWIP_API_USER(デフォルト"管理者")のユーザーとして操作する。
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::domain::access::{Action, Principal, Viewer};
use crate::domain::services::ai_service;
use crate::infrastructure::access::{
    facts_repo,
    shadow::{self, Mode, Resource},
    viewer_repo,
};
use crate::infrastructure::repositories::{ai_agent_key_repo, ticket_repo, user_repo};
use crate::presentation::extractors::authorize;
use crate::presentation::state::AppState;

/// 外部 API の呼び出し元(アクセス制御の再設計 F-7。詳細設計書 §10.6・§12.3)
///
/// - 連携のキー(`access_integration` に登録したキー): 許可したチームだけで動く(`Principal::Integration`)。
///   作成者を持つ操作は、今までどおり外部 API 用のユーザー(`SENN_API_USER`)で記録する。
/// - 今の共有キー(`SENN_API_KEY`): 移行期間は今のまま使える(切り替えたら環境変数から外して無効にする)。
///   新しい判定(on)では「最初のシステム管理者」への退避をやめる。
///
/// 切り替えは `ACCESS_ENFORCE_KEY`。
#[derive(Debug, Clone)]
pub struct ExternalCaller {
    /// 作成者として記録するユーザー
    pub author_id: i32,
    /// 連携のキーなら、その閲覧者(許可したチームだけ)。今の共有キーは None
    pub viewer: Option<Viewer>,
}

impl axum::extract::FromRequestParts<AppState> for ExternalCaller {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let reject = |status: StatusCode, payload: serde_json::Value| {
            (status, Json(payload)).into_response()
        };
        let key = parts
            .headers
            .get("X-API-Key")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        // 連携のキー(登録済みのキーのハッシュと照合)
        if !key.is_empty() {
            let integration: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM access_integration WHERE key_hash = $1 AND is_active",
            )
            .bind(ai_agent_key_repo::hash_key(key))
            .fetch_optional(&state.pool)
            .await
            .map_err(|e| {
                tracing::error!("DB operation failed: {:?}", e);
                reject(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    json!({"error": "サーバーエラーが発生しました"}),
                )
            })?;
            if let Some(integration_id) = integration {
                let viewer = viewer_repo::load(
                    &state.pool,
                    Principal::Integration { integration_id },
                    viewer_repo::today_utc(),
                )
                .await
                .map_err(|e| {
                    tracing::error!("DB operation failed: {:?}", e);
                    reject(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        json!({"error": "サーバーエラーが発生しました"}),
                    )
                })?
                .ok_or_else(|| {
                    reject(
                        StatusCode::UNAUTHORIZED,
                        json!({"error": "APIキーが無効です"}),
                    )
                })?;
                let author_id = match user_repo::find_by_username(
                    &state.pool,
                    &state.config.wip_api_user,
                )
                .await
                {
                    Ok(Some(u)) => u.id,
                    Ok(None) => {
                        return Err(reject(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            json!({"error": "外部 API の記録用ユーザー(SENN_API_USER)がありません"}),
                        ))
                    }
                    Err(e) => {
                        tracing::error!("DB operation failed: {:?}", e);
                        return Err(reject(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            json!({"error": "サーバーエラーが発生しました"}),
                        ));
                    }
                };
                return Ok(ExternalCaller {
                    author_id,
                    viewer: Some(viewer),
                });
            }
        }
        // 今の共有キー
        let author_id = authenticate(state, &parts.headers)
            .await
            .map_err(|(status, payload)| reject(status, payload))?;
        Ok(ExternalCaller {
            author_id,
            viewer: None,
        })
    }
}

impl ExternalCaller {
    /// 操作の判定。連携のキーは許可したチームだけ。今の共有キーは今のまま(確認なし)
    #[allow(clippy::result_large_err)]
    fn gate(
        &self,
        pool: &sqlx::PgPool,
        facts: Option<&crate::domain::access::ResourceRef>,
        action: Action,
        id: i64,
        route: &'static str,
    ) -> Result<(), axum::response::Response> {
        match &self.viewer {
            Some(v) => authorize::gate(pool, v, facts, action, Resource::Key, id, Ok(()), route),
            None => Ok(()),
        }
    }
}

/// APIキーを検証し、成功時は操作主体のuser_idを返す。
async fn authenticate(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<i32, (StatusCode, serde_json::Value)> {
    let api_key = headers
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let expected_key = match &state.config.wip_api_key {
        Some(k) => k,
        None => {
            return Err((
                StatusCode::UNAUTHORIZED,
                json!({"error": "WIP_API_KEY が設定されていません"}),
            ));
        }
    };

    if api_key.is_empty() || !constant_time_eq(api_key.as_bytes(), expected_key.as_bytes()) {
        return Err((
            StatusCode::UNAUTHORIZED,
            json!({"error": "APIキーが無効です"}),
        ));
    }

    let user = user_repo::find_by_username(&state.pool, &state.config.wip_api_user)
        .await
        .map_err(|e| {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"error": "サーバーエラーが発生しました"}),
            )
        })?;

    let user = match user {
        Some(u) => u,
        // 新しい判定(on)では、最初のシステム管理者への退避をしない(人の権限を借りない。F-7)
        None if shadow::mode(Resource::Key) == Mode::On => {
            return Err((
                StatusCode::UNAUTHORIZED,
                json!({"error": "APIユーザーが見つかりません"}),
            ));
        }
        None => {
            let fallback = user_repo::find_first_staff_user(&state.pool)
                .await
                .map_err(|e| {
                    tracing::error!("DB operation failed: {:?}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        json!({"error": "サーバーエラーが発生しました"}),
                    )
                })?;
            match fallback {
                Some(u) => u,
                None => {
                    return Err((
                        StatusCode::UNAUTHORIZED,
                        json!({"error": "APIユーザーが見つかりません"}),
                    ));
                }
            }
        }
    };

    Ok(user.id)
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

#[derive(Deserialize)]
pub struct CreateExternalTicketIn {
    pub project_prefix: Option<String>,
    pub title: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_priority")]
    pub priority: String,
    #[serde(default = "default_ticket_type")]
    pub ticket_type: String,
    pub due_date: Option<chrono::NaiveDate>,
}
fn default_priority() -> String {
    "medium".to_string()
}
fn default_ticket_type() -> String {
    "task".to_string()
}

/// POST /api/v1/external/tickets/
pub async fn create_ticket(
    State(state): State<AppState>,
    caller: ExternalCaller,
    Json(body): Json<CreateExternalTicketIn>,
) -> impl IntoResponse {
    let author_id = caller.author_id;

    let project_prefix = match &body.project_prefix {
        Some(p) if !p.is_empty() => p,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "project_prefix と title は必須です"})),
            )
                .into_response();
        }
    };
    let title = match &body.title {
        Some(t) if !t.is_empty() => t,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "project_prefix と title は必須です"})),
            )
                .into_response();
        }
    };

    // 連携のキー: 作成先のプロジェクトが、許可したチームのものであること
    if caller.viewer.is_some() {
        let project_id =
            match ticket_repo::resolve_project_id_by_prefix(&state.pool, project_prefix).await {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!("DB operation failed: {:?}", e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({"error": "サーバーエラーが発生しました"})),
                    )
                        .into_response();
                }
            };
        let facts = match project_id {
            Some(pid) => match facts_repo::facts_for_project(&state.pool, pid).await {
                Ok(f) => f,
                Err(e) => {
                    tracing::error!("DB operation failed: {:?}", e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({"error": "サーバーエラーが発生しました"})),
                    )
                        .into_response();
                }
            },
            None => None,
        };
        if let Err(resp) = caller.gate(
            &state.pool,
            facts.as_ref(),
            Action::Read,
            project_id.unwrap_or(0) as i64,
            "POST /api/v1/external/tickets/",
        ) {
            return resp;
        }
    }

    match ticket_repo::api_create_external(
        &state.pool,
        project_prefix,
        title,
        &body.description,
        &body.priority,
        &body.ticket_type,
        body.due_date,
        author_id,
    )
    .await
    {
        Ok(Some(r)) => (
            StatusCode::CREATED,
            Json(json!({
                "id": r.id,
                "ticket_key": r.ticket_key,
                "title": title,
                "project": r.project_name,
                "status": r.status,
                "url": format!("/tickets/{}/", r.ticket_key),
            })),
        )
            .into_response(),
        // 新しい判定(on)・連携のキーでは、ほかのプロジェクトの Prefix を一覧しない(F-4)
        Ok(None) if caller.viewer.is_some() || shadow::mode(Resource::Key) == Mode::On => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": format!("プロジェクト '{}' が見つかりません", project_prefix)})),
        )
            .into_response(),
        Ok(None) => {
            let available = ticket_repo::list_project_prefixes(&state.pool)
                .await
                .unwrap_or_default();
            (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": format!("プロジェクト '{}' が見つかりません", project_prefix),
                    "available_projects": available.into_iter().map(|(p, n)| json!({"prefix": p, "name": n})).collect::<Vec<_>>(),
                })),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "サーバーエラーが発生しました"})),
            )
                .into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct CreateExternalCommentIn {
    pub comment: Option<String>,
}

/// POST /api/v1/external/tickets/{ticket_key}/comments/
pub async fn create_comment(
    State(state): State<AppState>,
    caller: ExternalCaller,
    Path(ticket_key): Path<String>,
    Json(body): Json<CreateExternalCommentIn>,
) -> impl IntoResponse {
    let author_id = caller.author_id;
    // 連携のキー: コメントするチケットが、許可したチームのものであること
    if caller.viewer.is_some() {
        let facts = match facts_repo::facts_for_ticket_key(&state.pool, &ticket_key).await {
            Ok(f) => f,
            Err(e) => {
                tracing::error!("DB operation failed: {:?}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "サーバーエラーが発生しました"})),
                )
                    .into_response();
            }
        };
        if let Err(resp) = caller.gate(
            &state.pool,
            facts.as_ref().map(|(_, f)| f),
            Action::Write,
            facts.as_ref().map_or(0, |(id, _)| *id as i64),
            "POST /api/v1/external/tickets/{ticket_key}/comments/",
        ) {
            return resp;
        }
    }

    let comment_body = match &body.comment {
        Some(c) if !c.is_empty() => c,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "comment は必須です"})),
            )
                .into_response();
        }
    };

    let ticket_id = match ticket_repo::resolve_ticket_id(&state.pool, &ticket_key).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": format!("チケット '{}' が見つかりません", ticket_key)})),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "サーバーエラーが発生しました"})),
            )
                .into_response();
        }
    };

    match ticket_repo::api_add_comment(
        &state.pool,
        ticket_id,
        author_id,
        comment_body,
        None,
        None,
        None,
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
            (
                StatusCode::CREATED,
                Json(json!({
                    "id": comment.id,
                    "ticket_key": ticket_key,
                    "created_at": comment.created_at.to_rfc3339(),
                    "url": format!("/tickets/{}/", ticket_key),
                })),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "サーバーエラーが発生しました"})),
            )
                .into_response()
        }
    }
}
