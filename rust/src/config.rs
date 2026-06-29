/// config.rs — アプリケーション設定
///
/// 環境変数から設定値を読み込む。
/// .env ファイルまたは Docker の環境変数で設定する。

#[derive(Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub port: u16,
    pub base_url: String,
    pub session_secret: String,
    pub cookie_name: String,
    pub media_dir: String,
    pub max_upload_size: usize,
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
    pub smtp_user: Option<String>,
    pub smtp_password: Option<String>,
    pub webauthn_rp_id: String,
    pub webauthn_rp_origin: String,
}

impl AppConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            database_url: std::env::var("DATABASE_URL")
                .map_err(|_| anyhow::anyhow!("DATABASE_URL must be set"))?,
            port: std::env::var("PORT")
                .unwrap_or_else(|_| "8150".to_string())
                .parse()?,
            base_url: std::env::var("BASE_URL")
                .unwrap_or_else(|_| "http://localhost:8150".to_string()),
            session_secret: std::env::var("SESSION_SECRET")
                .unwrap_or_else(|_| {
                    // 開発時のみデフォルト値を使用（本番では必須）
                    "dev-secret-key-change-me-in-production-at-least-64-chars-long!!".to_string()
                }),
            cookie_name: std::env::var("COOKIE_NAME")
                .unwrap_or_else(|_| "wip_session".to_string()),
            media_dir: std::env::var("MEDIA_DIR")
                .unwrap_or_else(|_| "media".to_string()),
            max_upload_size: std::env::var("MAX_UPLOAD_SIZE")
                .unwrap_or_else(|_| "10485760".to_string())
                .parse()
                .unwrap_or(10_485_760),
            smtp_host: std::env::var("EMAIL_HOST").ok(),
            smtp_port: std::env::var("EMAIL_PORT")
                .ok()
                .and_then(|s| s.parse().ok()),
            smtp_user: std::env::var("EMAIL_USER").ok(),
            smtp_password: std::env::var("EMAIL_PASSWORD").ok(),
            webauthn_rp_id: std::env::var("WEBAUTHN_RP_ID")
                .unwrap_or_else(|_| "localhost".to_string()),
            webauthn_rp_origin: std::env::var("WEBAUTHN_RP_ORIGIN")
                .unwrap_or_else(|_| "http://localhost:8150".to_string()),
        })
    }
}
