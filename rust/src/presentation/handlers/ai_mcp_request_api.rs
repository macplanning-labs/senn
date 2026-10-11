/// presentation/handlers/ai_mcp_request_api.rs — AI Agent を利用した MCP 操作の申請(DEMO-000192)
///
/// 利用者が申請 → システム管理者が承認 → 本人が自分専用のAIキーを発行(平文は発行時に一度だけ返す)。
/// 申請・承認・却下・取り下げ・発行・失効は access_audit_log に残す(キー・申請の中身は入れない)。
/// 自分の申請を自分で承認できるのは、有効なシステム管理者が1人だけのとき(監査記録に自己承認と残す)。
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::domain::access::Viewer;
use crate::domain::services::ai_mcp_request_service;
use crate::infrastructure::repositories::ai_mcp_request_repo::{
    self as repo, IssueError, APPROVAL_VALID_DAYS, MAX_ACTIVE_KEYS, PURPOSE_MAX_CHARS,
};
use crate::infrastructure::repositories::{
    ai_mcp_pairing_repo as pairing_repo, notification_repo, user_repo,
};
use crate::presentation::handlers::system_admin_api::require_staff;
use crate::presentation::state::AppState;

const CATEGORY_REQUEST: &str = "ai_mcp_request";
pub(crate) const CATEGORY_DECISION: &str = "ai_mcp_decision";

pub(crate) fn err(status: StatusCode, msg: impl Into<String>) -> Response {
    (status, Json(json!({ "error": msg.into() }))).into_response()
}

pub(crate) fn server_error<E: std::fmt::Debug>(e: E) -> Response {
    tracing::error!("DB operation failed: {:?}", e);
    err(
        StatusCode::INTERNAL_SERVER_ERROR,
        "サーバーエラーが発生しました",
    )
}

/// 画面(人)からの操作だけを受ける。AIキー経由では、申請・承認・発行をさせない。
#[allow(clippy::result_large_err)]
pub(crate) fn human_user_id(viewer: &Viewer) -> Result<i32, Response> {
    if viewer.principal.is_key() {
        return Err(err(
            StatusCode::FORBIDDEN,
            "この操作は、画面からログインした本人だけができます",
        ));
    }
    viewer.require_user_id()
}

/// 監査記録(best-effort。失敗しても操作自体は成功させ、ログに残す)
pub(crate) async fn audit(
    state: &AppState,
    actor: i32,
    action: &str,
    target_user: Option<i32>,
    detail: serde_json::Value,
) {
    if let Err(e) = sqlx::query(
        "INSERT INTO access_audit_log (actor_user_id, actor_kind, action, target_user_id, detail)
         VALUES ($1::int8, 'human', $2, $3::int8, $4)",
    )
    .bind(actor)
    .bind(action)
    .bind(target_user)
    .bind(detail)
    .execute(&state.pool)
    .await
    {
        tracing::error!("監査記録の書き込みに失敗: {:?}", e);
    }
}

pub(crate) async fn notify(
    state: &AppState,
    user_id: i32,
    category: &str,
    title: &str,
    message: &str,
) {
    if let Err(e) =
        notification_repo::create(&state.pool, user_id, None, category, title, message).await
    {
        tracing::warn!("通知の作成に失敗: {:?}", e);
    }
}

// ---------------------------------------------------------------------------
// 利用者側
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct MineOut {
    requests: Vec<repo::AiMcpRequestOut>,
    keys: Vec<repo::MyKeyOut>,
    #[serde(rename = "maxActiveKeys")]
    max_active_keys: i64,
    #[serde(rename = "approvalValidDays")]
    approval_valid_days: i64,
}

/// GET /api/v1/ai-mcp/requests/ — 自分の申請とキー
pub async fn list_mine(State(state): State<AppState>, viewer: Viewer) -> Response {
    let user_id = match human_user_id(&viewer) {
        Ok(id) => id,
        Err(r) => return r,
    };
    let (requests, keys) = match tokio::try_join!(
        repo::list_for_user(&state.pool, user_id),
        repo::list_keys_for_user(&state.pool, user_id)
    ) {
        Ok(v) => v,
        Err(e) => return server_error(e),
    };
    Json(MineOut {
        requests,
        keys,
        max_active_keys: MAX_ACTIVE_KEYS,
        approval_valid_days: APPROVAL_VALID_DAYS,
    })
    .into_response()
}

#[derive(Deserialize)]
pub struct CreateRequestIn {
    #[serde(default)]
    pub purpose: String,
}

/// 申請ができた(画面から、または端末の接続から)。監査記録・管理者への通知・メールを行う。
pub(crate) async fn announce_new_request(state: &AppState, user_id: i32, id: i32) {
    audit(
        state,
        user_id,
        "ai_mcp.request",
        Some(user_id),
        json!({ "request_id": id }),
    )
    .await;

    let who = match repo::find_by_id(&state.pool, id).await {
        Ok(Some(r)) => {
            if r.user.display_name.is_empty() {
                r.user.username
            } else {
                r.user.display_name
            }
        }
        _ => "利用者".to_string(),
    };
    if let Ok(admins) = repo::active_system_admin_ids(&state.pool).await {
        for admin in admins {
            notify(
                state,
                admin,
                CATEGORY_REQUEST,
                "AI Agent を利用した MCP 操作の申請",
                &format!(
                    "{} さんから申請があります。管理画面で承認または却下してください。",
                    who
                ),
            )
            .await;
        }
    }

    // メール通知を送る(best-effort)
    if let Ok(Some(requester)) = user_repo::find_by_id(&state.pool, user_id).await {
        if let Err(e) = ai_mcp_request_service::send_email_on_request(
            &state.mail_sender,
            &state.pool,
            &requester,
            &state.config.base_url,
        )
        .await
        {
            tracing::warn!("申請時のメール送信に失敗: {}", e);
        }
    }
}

/// POST /api/v1/ai-mcp/requests/ — 申請する
pub async fn create_request(
    State(state): State<AppState>,
    viewer: Viewer,
    Json(body): Json<CreateRequestIn>,
) -> Response {
    let user_id = match human_user_id(&viewer) {
        Ok(id) => id,
        Err(r) => return r,
    };
    let purpose = body.purpose.trim();
    if purpose.chars().count() > PURPOSE_MAX_CHARS {
        return err(
            StatusCode::BAD_REQUEST,
            format!("用途は{}文字以内で書いてください", PURPOSE_MAX_CHARS),
        );
    }
    let id =
        match repo::create(&state.pool, user_id, purpose).await {
            Ok(Some(id)) => id,
            Ok(None) => return err(
                StatusCode::CONFLICT,
                "申請中、または承認済み(キー未発行)の申請があります。先にそちらを進めてください",
            ),
            Err(e) => return server_error(e),
        };
    announce_new_request(&state, user_id, id).await;
    match repo::find_by_id(&state.pool, id).await {
        Ok(Some(r)) => (StatusCode::CREATED, Json(r)).into_response(),
        Ok(None) => err(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => server_error(e),
    }
}

/// POST /api/v1/ai-mcp/requests/{id}/cancel/ — 取り下げ(申請中・承認済み)
pub async fn cancel_request(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
) -> Response {
    let user_id = match human_user_id(&viewer) {
        Ok(id) => id,
        Err(r) => return r,
    };
    match repo::cancel(&state.pool, id, user_id).await {
        Ok(true) => {
            audit(
                &state,
                user_id,
                "ai_mcp.cancel",
                Some(user_id),
                json!({ "request_id": id }),
            )
            .await;
            if let Err(e) =
                pairing_repo::on_request_closed(&state.pool, id, "取り下げられました").await
            {
                tracing::warn!("接続の状態の更新に失敗: {:?}", e);
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => err(StatusCode::NOT_FOUND, "取り下げできる申請が見つかりません"),
        Err(e) => server_error(e),
    }
}

#[derive(Deserialize, Default)]
pub struct IssueIn {
    pub label: Option<String>,
}

#[derive(Serialize)]
struct IssuedOut {
    id: i32,
    /// 発行直後のみ含まれる平文キー。サーバーには保存されず、二度と取得できない。
    #[serde(rename = "plainKey")]
    plain_key: String,
    #[serde(rename = "keyPrefix")]
    key_prefix: String,
}

/// POST /api/v1/ai-mcp/requests/{id}/issue/ — 承認済みの申請から、自分のキーを発行する
pub async fn issue_key(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
    body: Option<Json<IssueIn>>,
) -> Response {
    let user_id = match human_user_id(&viewer) {
        Ok(id) => id,
        Err(r) => return r,
    };
    let label = body
        .and_then(|Json(b)| b.label)
        .map(|l| l.trim().chars().take(100).collect::<String>())
        .filter(|l| !l.is_empty());
    match repo::issue(&state.pool, id, user_id, label.as_deref()).await {
        Ok(Ok(k)) => {
            audit(
                &state,
                user_id,
                "ai_mcp.issue",
                Some(user_id),
                json!({ "request_id": id, "key_id": k.key_id, "key_prefix": k.key_prefix }),
            )
            .await;
            (
                StatusCode::CREATED,
                Json(IssuedOut {
                    id: k.key_id,
                    plain_key: k.plain_key,
                    key_prefix: k.key_prefix,
                }),
            )
                .into_response()
        }
        Ok(Err(IssueError::NotFound)) => err(StatusCode::NOT_FOUND, "申請が見つかりません"),
        Ok(Err(IssueError::NotApproved)) => err(
            StatusCode::CONFLICT,
            "この申請は、キーを発行できる状態ではありません",
        ),
        Ok(Err(IssueError::Expired)) => err(
            StatusCode::CONFLICT,
            "承認の有効期限が切れています。もう一度申請してください",
        ),
        Ok(Err(IssueError::TooManyKeys)) => err(
            StatusCode::CONFLICT,
            format!(
                "有効なキーが上限({}本)に達しています。使わないキーを失効してください",
                MAX_ACTIVE_KEYS
            ),
        ),
        Err(e) => server_error(e),
    }
}

/// DELETE /api/v1/ai-mcp/keys/{id}/ — 自分のキーを失効する
pub async fn revoke_my_key(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
) -> Response {
    let user_id = match human_user_id(&viewer) {
        Ok(id) => id,
        Err(r) => return r,
    };
    match repo::revoke_own_key(&state.pool, id, user_id).await {
        Ok(true) => {
            audit(
                &state,
                user_id,
                "ai_mcp.revoke",
                Some(user_id),
                json!({ "key_id": id }),
            )
            .await;
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => err(StatusCode::NOT_FOUND, "失効できるキーが見つかりません"),
        Err(e) => server_error(e),
    }
}

// ---------------------------------------------------------------------------
// 管理者側
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct AdminListQuery {
    pub status: Option<String>,
}

#[derive(Serialize)]
struct AdminListOut {
    requests: Vec<repo::AiMcpRequestOut>,
    #[serde(rename = "pendingCount")]
    pending_count: i64,
}

/// GET /api/v1/system-admin/ai-mcp-requests/?status=pending — 申請一覧(承認待ちの件数つき)
pub async fn admin_list(
    State(state): State<AppState>,
    viewer: Viewer,
    Query(q): Query<AdminListQuery>,
) -> Response {
    if let Err(r) = require_staff(&state, &viewer) {
        return r.into_response();
    }
    let status = q.status.as_deref().filter(|s| !s.is_empty());
    match tokio::try_join!(
        repo::list_for_admin(&state.pool, status),
        repo::count_pending(&state.pool)
    ) {
        Ok((requests, pending_count)) => Json(AdminListOut {
            requests,
            pending_count,
        })
        .into_response(),
        Err(e) => server_error(e),
    }
}

#[derive(Deserialize, Default)]
pub struct DecisionIn {
    #[serde(default)]
    pub note: String,
}

/// 承認・却下の共通の前処理。管理者のID・対象の申請・自己承認の可否を確かめる。
async fn decide_precheck(
    state: &AppState,
    viewer: &Viewer,
    id: i32,
) -> Result<(i32, repo::AiMcpRequestOut, bool), Response> {
    if let Err(r) = require_staff(state, viewer) {
        return Err(r.into_response());
    }
    let admin_id = human_user_id(viewer)?;
    let req = match repo::find_by_id(&state.pool, id).await {
        Ok(Some(r)) => r,
        Ok(None) => return Err(err(StatusCode::NOT_FOUND, "申請が見つかりません")),
        Err(e) => return Err(server_error(e)),
    };
    let mut self_decision = false;
    if req.user.id == admin_id {
        let admins = repo::count_active_system_admins(&state.pool)
            .await
            .map_err(server_error)?;
        if admins > 1 {
            return Err(err(
                StatusCode::FORBIDDEN,
                "自分の申請は自分では承認・却下できません。ほかの管理者に依頼してください",
            ));
        }
        self_decision = true;
    }
    Ok((admin_id, req, self_decision))
}

/// POST /api/v1/system-admin/ai-mcp-requests/{id}/approve/
pub async fn admin_approve(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
    body: Option<Json<DecisionIn>>,
) -> Response {
    let (admin_id, req, self_decision) = match decide_precheck(&state, &viewer, id).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let note = body.map(|Json(b)| b.note).unwrap_or_default();
    match repo::approve(&state.pool, id, admin_id, note.trim()).await {
        Ok(true) => {}
        Ok(false) => return err(StatusCode::CONFLICT, "この申請は、すでに処理されています"),
        Err(e) => return server_error(e),
    }
    audit(
        &state,
        admin_id,
        "ai_mcp.approve",
        Some(req.user.id),
        json!({ "request_id": id, "self_approval": self_decision }),
    )
    .await;
    // 端末の接続が結び付いていれば、ここでキーを自動登録する(利用者の操作は要らない)
    let paired = match pairing_repo::on_request_approved(&state.pool, id).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("接続の自動登録に失敗: {:?}", e);
            None
        }
    };
    if paired == Some(true) {
        audit(
            &state,
            admin_id,
            "ai_mcp.pair_register",
            Some(req.user.id),
            json!({ "request_id": id }),
        )
        .await;
    }
    let (title, message) = match paired {
        Some(true) => (
            "AI Agent を利用した MCP 操作: 使えるようになりました",
            "承認されました。Claude から、そのまま使えます(キーの操作は要りません)。".to_string(),
        ),
        Some(false) => (
            "AI Agent を利用した MCP 操作: 接続できませんでした",
            "承認されましたが、接続できませんでした。設定の「詳細」で、使わないキーを失効してから、もう一度接続してください。".to_string(),
        ),
        None => (
            "AI Agent を利用した MCP 操作の申請: 承認されました",
            format!(
                "設定画面からキーを発行できます({}日以内)。",
                APPROVAL_VALID_DAYS
            ),
        ),
    };
    notify(&state, req.user.id, CATEGORY_DECISION, title, &message).await;

    // メール通知を送る(best-effort)
    if let Ok(Some(requester)) = user_repo::find_by_id(&state.pool, req.user.id).await {
        if let Err(e) = ai_mcp_request_service::send_email_on_approval(
            &state.mail_sender,
            &requester,
            &state.config.base_url,
            paired == Some(true),
        )
        .await
        {
            tracing::warn!("承認時のメール送信に失敗: {}", e);
        }
    }
    match repo::find_by_id(&state.pool, id).await {
        Ok(Some(r)) => Json(r).into_response(),
        Ok(None) => err(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => server_error(e),
    }
}

/// POST /api/v1/system-admin/ai-mcp-requests/{id}/reject/
pub async fn admin_reject(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
    body: Option<Json<DecisionIn>>,
) -> Response {
    let (admin_id, req, self_decision) = match decide_precheck(&state, &viewer, id).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let note = body.map(|Json(b)| b.note).unwrap_or_default();
    match repo::reject(&state.pool, id, admin_id, note.trim()).await {
        Ok(true) => {}
        Ok(false) => return err(StatusCode::CONFLICT, "この申請は、すでに処理されています"),
        Err(e) => return server_error(e),
    }
    audit(
        &state,
        admin_id,
        "ai_mcp.reject",
        Some(req.user.id),
        json!({ "request_id": id, "self_decision": self_decision }),
    )
    .await;
    if let Err(e) = pairing_repo::on_request_closed(
        &state.pool,
        id,
        &if note.trim().is_empty() {
            "却下されました".to_string()
        } else {
            format!("却下されました。理由: {}", note.trim())
        },
    )
    .await
    {
        tracing::warn!("接続の状態の更新に失敗: {:?}", e);
    }
    let message = if note.trim().is_empty() {
        "申請は却下されました。".to_string()
    } else {
        format!("申請は却下されました。理由: {}", note.trim())
    };
    notify(
        &state,
        req.user.id,
        CATEGORY_DECISION,
        "AI Agent を利用した MCP 操作の申請: 却下されました",
        &message,
    )
    .await;

    // メール通知を送る(best-effort)
    if let Ok(Some(requester)) = user_repo::find_by_id(&state.pool, req.user.id).await {
        if let Err(e) = ai_mcp_request_service::send_email_on_rejection(
            &state.mail_sender,
            &requester,
            note.trim(),
        )
        .await
        {
            tracing::warn!("却下時のメール送信に失敗: {}", e);
        }
    }
    match repo::find_by_id(&state.pool, id).await {
        Ok(Some(r)) => Json(r).into_response(),
        Ok(None) => err(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => server_error(e),
    }
}
