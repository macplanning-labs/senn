use sqlx::PgPool;

pub const KEY_OLLAMA_MODEL: &str = "ollama_model";
pub const KEY_OLLAMA_TIMEOUT: &str = "ollama_timeout";

// システム管理 (/admin) — メール・プラン
pub const KEY_MAIL_MODE: &str = "sys.mail.mode";
pub const KEY_MAIL_SENDER: &str = "sys.mail.sender";
pub const KEY_MAIL_SMTP_HOST: &str = "sys.mail.smtp_host";
pub const KEY_MAIL_SMTP_PORT: &str = "sys.mail.smtp_port";
pub const KEY_MAIL_SMTP_ENCRYPTION: &str = "sys.mail.smtp_encryption";
pub const KEY_MAIL_SMTP_PASSWORD: &str = "sys.mail.smtp_password";
pub const KEY_WORKSPACE_PLAN_TYPE: &str = "sys.workspace.plan_type";

// AI エージェント API キー（X-AI-Api-Key）
pub const KEY_AI_AGENT_KEY: &str = "sys.ai.agent_key";

pub const MAIL_MODE_GMAIL: &str = "gmail_api";
pub const MAIL_MODE_SMTP: &str = "smtp";
pub const DEFAULT_PLAN_TYPE: &str = "free";

#[derive(Debug, Clone)]
pub struct MailSettingsSnapshot {
    pub mode: String,
    pub sender: Option<String>,
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
    pub smtp_encryption: Option<String>,
    pub smtp_password_encrypted: Option<String>,
}

pub async fn load_mail_settings(pool: &PgPool) -> anyhow::Result<MailSettingsSnapshot> {
    let mode = get(pool, KEY_MAIL_MODE)
        .await?
        .unwrap_or_else(|| MAIL_MODE_GMAIL.to_string());
    let sender = get(pool, KEY_MAIL_SENDER).await?;
    let smtp_host = get(pool, KEY_MAIL_SMTP_HOST).await?;
    let smtp_port = match get(pool, KEY_MAIL_SMTP_PORT).await? {
        Some(v) => v.parse::<u16>().ok(),
        None => None,
    };
    let smtp_encryption = get(pool, KEY_MAIL_SMTP_ENCRYPTION).await?;
    let smtp_password_encrypted = get(pool, KEY_MAIL_SMTP_PASSWORD).await?;
    Ok(MailSettingsSnapshot {
        mode,
        sender,
        smtp_host,
        smtp_port,
        smtp_encryption,
        smtp_password_encrypted,
    })
}

pub async fn get(pool: &PgPool, key: &str) -> anyhow::Result<Option<String>> {
    let value: Option<String> = sqlx::query_scalar(
        "SELECT value FROM system_settings WHERE key = $1",
    )
    .bind(key)
    .fetch_optional(pool)
    .await?;
    Ok(value)
}

pub async fn set(pool: &PgPool, key: &str, value: &str) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO system_settings (key, value, updated_at)
        VALUES ($1, $2, NOW())
        ON CONFLICT (key) DO UPDATE
        SET value = EXCLUDED.value, updated_at = NOW()
        "#,
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete(pool: &PgPool, key: &str) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM system_settings WHERE key = $1")
        .bind(key)
        .execute(pool)
        .await?;
    Ok(())
}
