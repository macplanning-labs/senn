/// presentation/handlers/system_admin_api.rs — システム管理 API（staff のみ）
use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio_util::io::ReaderStream;

use crate::domain::access::Viewer;
use crate::infrastructure::encrypted_settings::{decrypt_value, encrypt_value, mask_secret_tail};
use crate::infrastructure::repositories::{ai_agent_key_repo, system_settings_repo, user_repo};
use crate::presentation::state::AppState;

pub(crate) fn require_staff(
    state: &AppState,
    viewer: &Viewer,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    // customer チャネルでは system-admin API を許さない
    if state.config.app_channel.to_lowercase() == "customer" {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "detail": "Not available on customer channel" })),
        ));
    }

    if viewer.is_system_admin() {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "管理者のみアクセスできます" })),
        ))
    }
}

#[derive(Serialize)]
pub struct SystemAdminSettingsOut {
    #[serde(rename = "mailMode")]
    pub mail_mode: String,
    #[serde(rename = "mailSender")]
    pub mail_sender: String,
    #[serde(rename = "smtpHost")]
    pub smtp_host: String,
    #[serde(rename = "smtpPort")]
    pub smtp_port: u16,
    #[serde(rename = "smtpEncryption")]
    pub smtp_encryption: String,
    #[serde(rename = "smtpPasswordConfigured")]
    pub smtp_password_configured: bool,
    #[serde(rename = "gmailEnvConfigured")]
    pub gmail_env_configured: bool,
    #[serde(rename = "planType")]
    pub plan_type: String,
}

#[derive(Deserialize)]
pub struct UpdateSystemAdminSettingsIn {
    #[serde(rename = "mailMode")]
    pub mail_mode: Option<String>,
    #[serde(rename = "mailSender")]
    pub mail_sender: Option<String>,
    #[serde(rename = "smtpHost")]
    pub smtp_host: Option<String>,
    #[serde(rename = "smtpPort")]
    pub smtp_port: Option<u16>,
    #[serde(rename = "smtpEncryption")]
    pub smtp_encryption: Option<String>,
    /// 空文字は無視（キー削除防止）
    #[serde(rename = "smtpPassword")]
    pub smtp_password: Option<String>,
    #[serde(rename = "planType")]
    pub plan_type: Option<String>,
}

fn default_mail_sender(_config: &crate::config::AppConfig) -> String {
    std::env::var("EMAIL_HOST_USER").unwrap_or_else(|_| "noreply@senn.local".to_string())
}

fn gmail_env_configured() -> bool {
    std::env::var("GOOGLE_SERVICE_ACCOUNT_KEY_PATH").is_ok()
        && std::env::var("EMAIL_HOST_USER").is_ok()
}

/// GET /api/v1/system-admin/settings/
pub async fn get_settings(State(state): State<AppState>, viewer: Viewer) -> impl IntoResponse {
    if let Err(resp) = require_staff(&state, &viewer) {
        return resp.into_response();
    }

    let snapshot = match system_settings_repo::load_mail_settings(&state.pool).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("load_mail_settings failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "設定の読み込みに失敗しました" })),
            )
                .into_response();
        }
    };

    let plan_type =
        system_settings_repo::get(&state.pool, system_settings_repo::KEY_WORKSPACE_PLAN_TYPE)
            .await
            .ok()
            .flatten()
            .unwrap_or_else(|| system_settings_repo::DEFAULT_PLAN_TYPE.to_string());

    (
        StatusCode::OK,
        Json(SystemAdminSettingsOut {
            mail_mode: snapshot.mode,
            mail_sender: snapshot
                .sender
                .unwrap_or_else(|| default_mail_sender(&state.config)),
            smtp_host: snapshot.smtp_host.unwrap_or_default(),
            smtp_port: snapshot.smtp_port.unwrap_or(587),
            smtp_encryption: snapshot
                .smtp_encryption
                .unwrap_or_else(|| "starttls".to_string()),
            smtp_password_configured: snapshot.smtp_password_encrypted.is_some(),
            gmail_env_configured: gmail_env_configured(),
            plan_type,
        }),
    )
        .into_response()
}

/// PUT /api/v1/system-admin/settings/
pub async fn update_settings(
    State(state): State<AppState>,
    viewer: Viewer,
    Json(body): Json<UpdateSystemAdminSettingsIn>,
) -> impl IntoResponse {
    if let Err(resp) = require_staff(&state, &viewer) {
        return resp.into_response();
    }

    if let Some(mode) = &body.mail_mode {
        if mode != system_settings_repo::MAIL_MODE_GMAIL
            && mode != system_settings_repo::MAIL_MODE_SMTP
        {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "mailMode は gmail_api または smtp である必要があります" })),
            )
                .into_response();
        }
        if let Err(e) =
            system_settings_repo::set(&state.pool, system_settings_repo::KEY_MAIL_MODE, mode).await
        {
            tracing::error!("system_settings save failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "メールモードの保存に失敗しました" })),
            )
                .into_response();
        }
    }

    if let Some(sender) = &body.mail_sender {
        let trimmed = sender.trim();
        if trimmed.is_empty() {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "送信元アドレスは必須です" })),
            )
                .into_response();
        }
        if let Err(e) =
            system_settings_repo::set(&state.pool, system_settings_repo::KEY_MAIL_SENDER, trimmed)
                .await
        {
            tracing::error!("system_settings save failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "送信元アドレスの保存に失敗しました" })),
            )
                .into_response();
        }
    }

    if let Some(host) = &body.smtp_host {
        if let Err(e) = system_settings_repo::set(
            &state.pool,
            system_settings_repo::KEY_MAIL_SMTP_HOST,
            host.trim(),
        )
        .await
        {
            tracing::error!("system_settings save failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "SMTPホストの保存に失敗しました" })),
            )
                .into_response();
        }
    }

    if let Some(port) = body.smtp_port {
        if port == 0 {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "SMTPポートが不正です" })),
            )
                .into_response();
        }
        if let Err(e) = system_settings_repo::set(
            &state.pool,
            system_settings_repo::KEY_MAIL_SMTP_PORT,
            &port.to_string(),
        )
        .await
        {
            tracing::error!("system_settings save failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "SMTPポートの保存に失敗しました" })),
            )
                .into_response();
        }
    }

    if let Some(enc) = &body.smtp_encryption {
        let allowed = ["none", "starttls", "tls"];
        if !allowed.contains(&enc.as_str()) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "smtpEncryption は none / starttls / tls です" })),
            )
                .into_response();
        }
        if let Err(e) = system_settings_repo::set(
            &state.pool,
            system_settings_repo::KEY_MAIL_SMTP_ENCRYPTION,
            enc,
        )
        .await
        {
            tracing::error!("system_settings save failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "SMTP暗号化方式の保存に失敗しました" })),
            )
                .into_response();
        }
    }

    if let Some(password) = &body.smtp_password {
        if !password.is_empty() {
            match encrypt_value(password, &state.config.jwt_secret) {
                Ok(blob) => {
                    if let Err(e) = system_settings_repo::set(
                        &state.pool,
                        system_settings_repo::KEY_MAIL_SMTP_PASSWORD,
                        &blob,
                    )
                    .await
                    {
                        tracing::error!("system_settings save failed: {:?}", e);
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(json!({ "error": "SMTPパスワードの保存に失敗しました" })),
                        )
                            .into_response();
                    }
                }
                Err(e) => {
                    tracing::error!("encrypt smtp password failed: {:?}", e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": "SMTPパスワードの暗号化に失敗しました" })),
                    )
                        .into_response();
                }
            }
        }
    }

    if let Some(plan) = &body.plan_type {
        let trimmed = plan.trim();
        if trimmed.is_empty() {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "planType は空にできません" })),
            )
                .into_response();
        }
        if let Err(e) = system_settings_repo::set(
            &state.pool,
            system_settings_repo::KEY_WORKSPACE_PLAN_TYPE,
            trimmed,
        )
        .await
        {
            tracing::error!("system_settings save failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "プラン種別の保存に失敗しました" })),
            )
                .into_response();
        }
    }

    get_settings(State(state), viewer).await.into_response()
}

#[derive(Serialize)]
pub struct AiAgentKeyOut {
    pub configured: bool,
    #[serde(rename = "maskedTail")]
    pub masked_tail: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateAiAgentKeyIn {
    #[serde(rename = "agentKey")]
    pub agent_key: Option<String>,
}

/// DB → env フォールバックで AI エージェント API キーを解決（認証と共有）
pub async fn resolve_ai_agent_api_key(state: &AppState) -> Option<String> {
    if let Ok(Some(blob)) =
        system_settings_repo::get(&state.pool, system_settings_repo::KEY_AI_AGENT_KEY).await
    {
        if !blob.is_empty() {
            if let Ok(plain) = decrypt_value(&blob, &state.config.jwt_secret) {
                if !plain.is_empty() {
                    return Some(plain);
                }
            } else {
                tracing::error!("sys.ai.agent_key の復号に失敗しました");
            }
        }
    }
    state.config.wip_ai_api_key.clone()
}

/// GET /api/v1/system-admin/ai-agent-key/
pub async fn get_ai_agent_key(State(state): State<AppState>, viewer: Viewer) -> impl IntoResponse {
    if let Err(resp) = require_staff(&state, &viewer) {
        return resp.into_response();
    }

    let from_db = system_settings_repo::get(&state.pool, system_settings_repo::KEY_AI_AGENT_KEY)
        .await
        .ok()
        .flatten()
        .filter(|v| !v.is_empty());

    let from_env = state
        .config
        .wip_ai_api_key
        .as_ref()
        .filter(|v| !v.is_empty());

    let configured = from_db.is_some() || from_env.is_some();
    let masked_tail = if let Some(blob) = from_db {
        decrypt_value(&blob, &state.config.jwt_secret)
            .ok()
            .map(|v| mask_secret_tail(&v))
    } else if let Some(key) = from_env {
        Some(mask_secret_tail(key))
    } else {
        None
    };

    (
        StatusCode::OK,
        Json(AiAgentKeyOut {
            configured,
            masked_tail,
        }),
    )
        .into_response()
}

/// PUT /api/v1/system-admin/ai-agent-key/
pub async fn update_ai_agent_key(
    State(state): State<AppState>,
    viewer: Viewer,
    Json(body): Json<UpdateAiAgentKeyIn>,
) -> impl IntoResponse {
    if let Err(resp) = require_staff(&state, &viewer) {
        return resp.into_response();
    }

    let Some(key) = body.agent_key else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "agentKey が必要です" })),
        )
            .into_response();
    };

    if key.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "空のキーは保存できません（既存キーを維持する場合は送信しないでください）" })),
        )
            .into_response();
    }

    let blob = match encrypt_value(key.trim(), &state.config.jwt_secret) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("encrypt agent key failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "AIエージェントキーの暗号化に失敗しました" })),
            )
                .into_response();
        }
    };

    if let Err(e) =
        system_settings_repo::set(&state.pool, system_settings_repo::KEY_AI_AGENT_KEY, &blob).await
    {
        tracing::error!("system_settings save failed: {:?}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "AIエージェントキーの保存に失敗しました" })),
        )
            .into_response();
    }

    get_ai_agent_key(State(state), viewer).await.into_response()
}

// ---------------------------------------------------------------------------
// 人ごとのAIエージェント用APIキー (DEMO-000100)
//
// 全社共有の1本のキーとは別に、staffが特定ユーザー向けにAPIキーを発行できる。
// 発行時に返す平文キーはこの応答限りで、以後DBにはSHA-256ハッシュのみが残る。
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateAiAgentPersonalKeyIn {
    #[serde(rename = "userId")]
    pub user_id: i32,
    pub label: Option<String>,
}

#[derive(Serialize)]
pub struct CreateAiAgentPersonalKeyOut {
    pub id: i32,
    /// 発行直後のみ含まれる平文キー。この値はサーバー側に保存されず、二度と取得できない。
    #[serde(rename = "plainKey")]
    pub plain_key: String,
    #[serde(rename = "keyPrefix")]
    pub key_prefix: String,
}

/// GET /api/v1/system-admin/ai-agent-personal-keys/ — 発行済みキー一覧(staff限定、平文は含まない)
pub async fn list_ai_agent_personal_keys(
    State(state): State<AppState>,
    viewer: Viewer,
) -> impl IntoResponse {
    if let Err(resp) = require_staff(&state, &viewer) {
        return resp.into_response();
    }

    match ai_agent_key_repo::list_all(&state.pool).await {
        Ok(keys) => (StatusCode::OK, Json(keys)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "サーバーエラーが発生しました" })),
            )
                .into_response()
        }
    }
}

/// POST /api/v1/system-admin/ai-agent-personal-keys/ — 新規発行(staff限定)。
/// レスポンスに平文キーを含むのはこの一度きり。以後は取得不可(GitHub PAT等と同じUX)。
pub async fn create_ai_agent_personal_key(
    State(state): State<AppState>,
    viewer: Viewer,
    Json(body): Json<CreateAiAgentPersonalKeyIn>,
) -> impl IntoResponse {
    let staff_id = match viewer.require_user_id() {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    if let Err(resp) = require_staff(&state, &viewer) {
        return resp.into_response();
    }

    match user_repo::find_by_id(&state.pool, body.user_id).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "指定されたユーザーが見つかりません" })),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "サーバーエラーが発生しました" })),
            )
                .into_response();
        }
    }

    let plain_key = ai_agent_key_repo::generate_plain_key();
    let key_hash = ai_agent_key_repo::hash_key(&plain_key);
    let key_prefix = ai_agent_key_repo::display_prefix(&plain_key);
    let label = body.label.as_deref().filter(|l| !l.trim().is_empty());

    match ai_agent_key_repo::create(
        &state.pool,
        body.user_id,
        &key_hash,
        &key_prefix,
        label,
        staff_id,
    )
    .await
    {
        Ok(id) => (
            StatusCode::CREATED,
            Json(CreateAiAgentPersonalKeyOut {
                id,
                plain_key,
                key_prefix,
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "キーの発行に失敗しました" })),
            )
                .into_response()
        }
    }
}

/// DELETE /api/v1/system-admin/ai-agent-personal-keys/{id}/ — 失効(staff限定、ソフト削除・冪等)
pub async fn revoke_ai_agent_personal_key(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    if let Err(resp) = require_staff(&state, &viewer) {
        return resp.into_response();
    }

    match ai_agent_key_repo::revoke(&state.pool, id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "キーが見つからないか、既に失効しています" })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "サーバーエラーが発生しました" })),
            )
                .into_response()
        }
    }
}

struct PgConnParams {
    host: String,
    port: u16,
    user: String,
    password: String,
    database: String,
}

fn parse_database_url(database_url: &str) -> Result<PgConnParams, String> {
    let parsed = url::Url::parse(database_url).map_err(|e| e.to_string())?;
    if parsed.scheme() != "postgres" && parsed.scheme() != "postgresql" {
        return Err("DATABASE_URL は postgres スキームである必要があります".to_string());
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| "DATABASE_URL にホストがありません".to_string())?
        .to_string();
    let port = parsed.port().unwrap_or(5432);
    let user = parsed.username().to_string();
    let password = parsed.password().unwrap_or("").to_string();
    let database = parsed.path().trim_start_matches('/').to_string();
    if database.is_empty() {
        return Err("DATABASE_URL にデータベース名がありません".to_string());
    }
    Ok(PgConnParams {
        host,
        port,
        user,
        password,
        database,
    })
}

/// POST /api/v1/system-admin/backup/export/
pub async fn backup_export(State(state): State<AppState>, viewer: Viewer) -> impl IntoResponse {
    let user_id = match viewer.require_user_id() {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    if let Err(resp) = require_staff(&state, &viewer) {
        return resp.into_response();
    }

    // 監査ログには、誰が出力したかをユーザー名で残す(ID だけでは追いにくい)
    let username = match user_repo::find_by_id(&state.pool, user_id).await {
        Ok(Some(u)) => u.username,
        _ => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "ユーザー情報の取得に失敗しました" })),
            )
                .into_response();
        }
    };
    tracing::info!(
        event = "system_admin_backup_export",
        user_id = user_id,
        username = %username,
        "DB backup export requested"
    );

    let conn = match parse_database_url(&state.config.database_url) {
        Ok(v) => v,
        Err(msg) => {
            tracing::error!("DATABASE_URL parse failed: {}", msg);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "データベース接続情報の解析に失敗しました" })),
            )
                .into_response();
        }
    };

    // set -o pipefail: pg_dump 失敗時に gzip だけ成功して壊れたファイルを返さない
    let shell_cmd = format!(
        "set -o pipefail; PGPASSWORD=\"$PGPASS\" pg_dump -h \"{}\" -p {} -U \"{}\" --no-owner --no-privileges \"{}\" | gzip -cn",
        conn.host.replace('"', "\\\""),
        conn.port,
        conn.user.replace('"', "\\\""),
        conn.database.replace('"', "\\\""),
    );

    let mut child = match Command::new("sh")
        .arg("-c")
        .arg(&shell_cmd)
        .env("PGPASS", &conn.password)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("pg_dump spawn failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "バックアップ処理の起動に失敗しました" })),
            )
                .into_response();
        }
    };

    let stdout = match child.stdout.take() {
        Some(s) => s,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "バックアップ出力の取得に失敗しました" })),
            )
                .into_response();
        }
    };

    let mut stderr = child.stderr.take();

    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let filename = format!("senn_backup_{timestamp}.sql.gz");

    let stream = ReaderStream::new(stdout)
        .map(|chunk| chunk.map_err(|e| std::io::Error::other(e.to_string())));

    let mut response = Response::new(Body::from_stream(stream));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/gzip"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
            .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
    );

    let audit_user_id = user_id;
    tokio::spawn(async move {
        let status = child.wait().await;
        let mut err_buf = String::new();
        if let Some(mut stderr) = stderr {
            let _ = stderr.read_to_string(&mut err_buf).await;
        }
        match status {
            Ok(exit) if exit.success() => {
                tracing::info!(
                    event = "system_admin_backup_export_complete",
                    user_id = audit_user_id,
                    "DB backup export completed"
                );
            }
            Ok(exit) => {
                tracing::error!(
                    event = "system_admin_backup_export_failed",
                    user_id = audit_user_id,
                    exit_code = ?exit.code(),
                    stderr = %err_buf,
                    "DB backup export failed"
                );
            }
            Err(e) => {
                tracing::error!(
                    event = "system_admin_backup_export_failed",
                    user_id = audit_user_id,
                    error = ?e,
                    stderr = %err_buf,
                    "DB backup export wait failed"
                );
            }
        }
    });

    response
}

/// パスキーのリセットで開ける、新規登録の窓の長さ(時間)
pub(crate) const PASSKEY_ENROLL_WINDOW_HOURS: i64 = 24;

/// POST /api/v1/system-admin/users/{id}/passkeys/reset/
///
/// 指定したユーザーのパスキーを、すべて削除する(パスキーを失くした管理者の復旧。DEMO-000153 A-4)。
/// - システム管理者だけが実行できる。**自分自身には使えない**(パスワードだけを知る攻撃者が、
///   自分のパスキーを消して入れ替えられないようにするため。別の管理者が行う)
/// - 実行の記録(ログ)を残し、対象の本人にメールで知らせる(メールを送れない環境では、記録のみ)
/// - リセットしたあと、本人はパスワードで入り、パスキーを登録し直す
pub async fn reset_user_passkeys(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(target_id): Path<i32>,
) -> impl IntoResponse {
    if let Err(resp) = require_staff(&state, &viewer) {
        return resp.into_response();
    }
    let actor_id = match viewer.require_user_id() {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    if actor_id == target_id {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "detail": "自分自身のパスキーはリセットできません。別のシステム管理者に依頼してください" })),
        )
            .into_response();
    }

    let target = match user_repo::find_by_id(&state.pool, target_id).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "detail": "ユーザーが見つかりません" })),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!(error = ?e, "ユーザーの取得に失敗しました");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "detail": "サーバーエラーが発生しました" })),
            )
                .into_response();
        }
    };

    let deleted = match user_repo::delete_all_passkeys(&state.pool, target_id).await {
        Ok(n) => n,
        Err(e) => {
            tracing::error!(error = ?e, "パスキーの削除に失敗しました");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "detail": "サーバーエラーが発生しました" })),
            )
                .into_response();
        }
    };

    // リセットした本人が、パスワードでログインして、パスキーを登録し直せるように、窓を開ける(24 時間。A-4b)
    if let Err(e) =
        user_repo::open_passkey_enroll_window(&state.pool, target_id, PASSKEY_ENROLL_WINDOW_HOURS)
            .await
    {
        tracing::error!(error = ?e, "パスキーの登録の窓を開けられませんでした");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "detail": "サーバーエラーが発生しました" })),
        )
            .into_response();
    }

    // 実行の記録(秘密の値は含まない)
    tracing::warn!(
        event = "admin_passkey_reset",
        actor_user_id = actor_id,
        target_user_id = target_id,
        deleted_count = deleted,
        "システム管理者がユーザーのパスキーをリセットしました"
    );

    // 本人への通知(失敗しても、リセットは取り消さない。失敗は記録する)
    let mut notified = false;
    if !target.email.is_empty() {
        if let Some(mailer) = state.mail_sender.as_ref() {
            if mailer.is_configured().await {
                let actor_name = match user_repo::find_by_id(&state.pool, actor_id).await {
                    Ok(Some(a)) => a.display().to_string(),
                    _ => String::from("システム管理者"),
                };
                let body = format!(
                    "{} さんのパスキーが、システム管理者({})によってリセットされました。\n\n\
                     パスワードでログインしたあと、パスキーを登録し直してください。\n\
                     心当たりがない場合は、すぐにシステム管理者に連絡してください。",
                    target.display(),
                    actor_name
                );
                match mailer
                    .send(&target.email, "【SENN】パスキーがリセットされました", &body)
                    .await
                {
                    Ok(()) => notified = true,
                    Err(e) => {
                        tracing::warn!(error = ?e, "パスキーのリセットの通知メールを送れませんでした")
                    }
                }
            }
        }
    }

    (
        StatusCode::OK,
        Json(json!({ "deleted": deleted, "notified": notified, "enrollWindowHours": PASSKEY_ENROLL_WINDOW_HOURS })),
    )
        .into_response()
}

/// GET /api/v1/system-admin/realtime/stats/
///
/// リアルタイム同期の計測（接続数・部屋数・リングのメモリの目安・配信数・遅い接続の切断・再同期・死活確認の往復など）。
/// 管理者のみ。数えた値を返すだけで、配信には影響しない。
pub async fn get_realtime_stats(
    State(state): State<AppState>,
    viewer: Viewer,
) -> impl IntoResponse {
    if let Err(resp) = require_staff(&state, &viewer) {
        return resp.into_response();
    }
    let mut body = serde_json::to_value(state.realtime.snapshot().await).unwrap_or_default();
    if let Some(obj) = body.as_object_mut() {
        obj.insert("enabled".to_string(), json!(state.realtime_enabled));
    }
    (StatusCode::OK, Json(body)).into_response()
}

#[cfg(test)]
mod passkey_reset_tests {
    use super::*;
    use crate::infrastructure::repositories::user_repo;
    use crate::test_support::{
        add_test_team_member, create_test_team, create_test_user, test_pool, test_state,
        test_viewer, unique_suffix,
    };
    use axum::body::to_bytes;

    async fn add_passkey(pool: &sqlx::PgPool, user_id: i32) {
        sqlx::query(
            "INSERT INTO mfa_webauthn_credential (credential_id, public_key, passkey_json, sign_count, name, created_at, user_id)
             VALUES ($1, $2, '{}', 0, 'test', now(), $3)",
        )
        .bind(format!("cred-{}", unique_suffix()).into_bytes())
        .bind(vec![1u8, 2, 3])
        .bind(user_id as i64)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn make_system_admin(pool: &sqlx::PgPool, user_id: i32) {
        sqlx::query(
            "UPDATE accounts_user SET is_staff = true, is_system_admin = true WHERE id = $1",
        )
        .bind(user_id as i64)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn call(state: &AppState, actor: i32, target: i32) -> (StatusCode, serde_json::Value) {
        let pool = &state.pool;
        let viewer = test_viewer(pool, actor).await;
        let resp = reset_user_passkeys(State(state.clone()), viewer, Path(target))
            .await
            .into_response();
        let status = resp.status();
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or_default())
    }

    #[tokio::test]
    async fn only_another_system_admin_can_reset_and_only_the_target_is_affected() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let state = test_state(&pool).await;
        let admin = create_test_user(&pool, "pk-admin").await;
        make_system_admin(&pool, admin).await;
        let target = create_test_user(&pool, "pk-target").await;
        let other = create_test_user(&pool, "pk-other").await;
        let plain = create_test_user(&pool, "pk-plain").await;
        for u in [target, other] {
            add_passkey(&pool, u).await;
            add_passkey(&pool, u).await;
        }

        // 管理者でない人は断られる(削除されない)
        let (st, _) = call(&state, plain, target).await;
        assert_eq!(st, StatusCode::FORBIDDEN);
        assert_eq!(user_repo::count_passkeys(&pool, target).await.unwrap(), 2);

        // 自分自身には使えない(管理者本人のパスキーも消えない)
        add_passkey(&pool, admin).await;
        let (st, _) = call(&state, admin, admin).await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
        assert_eq!(user_repo::count_passkeys(&pool, admin).await.unwrap(), 1);

        // 存在しないユーザー
        let (st, _) = call(&state, admin, 2_000_000_000).await;
        assert_eq!(st, StatusCode::NOT_FOUND);

        // 別の管理者は、対象のパスキーだけを削除できる
        let (st, body) = call(&state, admin, target).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(body["deleted"], 2);
        assert_eq!(user_repo::count_passkeys(&pool, target).await.unwrap(), 0);
        assert_eq!(user_repo::count_passkeys(&pool, other).await.unwrap(), 2);
        // リセットした本人だけ、パスキーの新規登録の窓が開く(他の人は閉じたまま)。A-4b
        assert!(user_repo::passkey_enroll_window_open(&pool, target)
            .await
            .unwrap());
        assert!(!user_repo::passkey_enroll_window_open(&pool, other)
            .await
            .unwrap());
        assert_eq!(body["enrollWindowHours"], 24);
    }

    #[tokio::test]
    async fn legacy_passkey_rows_without_json_do_not_count_as_registered() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = create_test_user(&pool, "pk-legacy").await;
        // 昔の行: passkey_json が空。ログインの確認では使えないので、登録済みとは数えない(数えると、管理者が締め出される)
        sqlx::query(
            "INSERT INTO mfa_webauthn_credential (credential_id, public_key, sign_count, name, created_at, user_id)
             VALUES ($1, $2, 0, 'legacy', now(), $3)",
        )
        .bind(format!("legacy-{}", unique_suffix()).into_bytes())
        .bind(vec![1u8])
        .bind(user as i64)
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(user_repo::count_passkeys(&pool, user).await.unwrap(), 0);
        add_passkey(&pool, user).await;
        assert_eq!(user_repo::count_passkeys(&pool, user).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn admin_account_detection_covers_system_admin_and_team_owner_only() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let sys = create_test_user(&pool, "adm-sys").await;
        make_system_admin(&pool, sys).await;
        let owner = create_test_user(&pool, "adm-owner").await;
        let member = create_test_user(&pool, "adm-member").await;
        let plain = create_test_user(&pool, "adm-plain").await;
        let team = create_test_team(&pool, "adm-team").await;
        add_test_team_member(&pool, team, owner, "admin").await;
        add_test_team_member(&pool, team, member, "member").await;

        assert!(user_repo::is_admin_account(&pool, sys).await.unwrap());
        assert!(user_repo::is_admin_account(&pool, owner).await.unwrap());
        assert!(!user_repo::is_admin_account(&pool, member).await.unwrap());
        assert!(!user_repo::is_admin_account(&pool, plain).await.unwrap());

        // プロジェクト限定のゲストの行は、Owner として数えない
        let guest = create_test_user(&pool, "adm-guest").await;
        let project = crate::test_support::create_test_project(&pool, "ADMG", sys).await;
        sqlx::query(
            "INSERT INTO t_team_membership (role, joined_at, user_id, team_id, scoped_project_id)
             VALUES ('admin', now(), $1, $2, $3)",
        )
        .bind(guest as i64)
        .bind(team as i64)
        .bind(project as i64)
        .execute(&pool)
        .await
        .unwrap();
        assert!(!user_repo::is_admin_account(&pool, guest).await.unwrap());

        // 無効化したアカウントは、管理者として数えない
        sqlx::query("UPDATE accounts_user SET is_active = false WHERE id = $1")
            .bind(owner as i64)
            .execute(&pool)
            .await
            .unwrap();
        assert!(!user_repo::is_admin_account(&pool, owner).await.unwrap());
    }

    #[tokio::test]
    async fn status_reports_registration_and_enforcement_window() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let _guard = crate::test_support::ADMIN_PASSKEY_SETTING_LOCK.lock().await;
        use crate::domain::services::auth_service::{
            admin_passkey_status, ADMIN_PASSKEY_ENFORCE_AFTER_KEY,
        };
        use crate::infrastructure::repositories::system_settings_repo;
        let admin = create_test_user(&pool, "st-admin").await;
        make_system_admin(&pool, admin).await;

        // 開始日時が未設定: 管理者で未登録なら登録が必要だが、強制はしない
        system_settings_repo::delete(&pool, ADMIN_PASSKEY_ENFORCE_AFTER_KEY)
            .await
            .unwrap();
        let st = admin_passkey_status(&pool, admin).await.unwrap();
        assert!(st.is_admin && !st.has_passkey && st.must_register());
        assert!(!st.is_enforced(chrono::Utc::now()));

        // 開始日時を過去にする: 強制の期間。パスキーを登録すると、登録は不要になる
        let past = (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339();
        system_settings_repo::set(&pool, ADMIN_PASSKEY_ENFORCE_AFTER_KEY, &past)
            .await
            .unwrap();
        let st = admin_passkey_status(&pool, admin).await.unwrap();
        assert!(st.is_enforced(chrono::Utc::now()));
        add_passkey(&pool, admin).await;
        let st = admin_passkey_status(&pool, admin).await.unwrap();
        assert!(st.has_passkey && !st.must_register());

        // 管理者でない人は、パスキーを調べず、登録も不要
        let plain = create_test_user(&pool, "st-plain").await;
        let st = admin_passkey_status(&pool, plain).await.unwrap();
        assert!(!st.is_admin && !st.must_register());

        // 開始日時が読めない値なら、強制しない(安全側ではなく、締め出さない側に倒す)
        system_settings_repo::set(&pool, ADMIN_PASSKEY_ENFORCE_AFTER_KEY, "不正な値")
            .await
            .unwrap();
        let st = admin_passkey_status(&pool, admin).await.unwrap();
        assert!(st.enforce_after.is_none() && !st.is_enforced(chrono::Utc::now()));
        system_settings_repo::delete(&pool, ADMIN_PASSKEY_ENFORCE_AFTER_KEY)
            .await
            .unwrap();
    }
}
