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
    /// JWT署名鍵。DjangoのSECRET_KEY(DJANGO_SECRET_KEY)と同一の値を使うことで、
    /// Rustが発行したトークンをDjangoのJWTAuthenticationが検証でき、その逆も可能になる。
    pub jwt_secret: String,
    /// 外部API(X-API-Key認証)用の共有キー。DjangoのWIP_API_KEYと同じ値を使う。
    pub wip_api_key: Option<String>,
    /// 外部API経由の操作を実行するユーザー名。DjangoのWIP_API_USER相当(デフォルト"管理者")。
    pub wip_api_user: String,
}

impl AppConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            database_url: crate::infrastructure::db::resolve_database_url()?,
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
            // config/settings.py の SECRET_KEY と同じフォールバック値
            // (DJANGO_SECRET_KEY未設定時はDjango側もこの値を使う)
            jwt_secret: std::env::var("DJANGO_SECRET_KEY").unwrap_or_else(|_| {
                "django-insecure-t)!aocxrf)m)b4sh!jcuthbr6_*em#x%chw@a7976ehs!ct=qb".to_string()
            }),
            wip_api_key: std::env::var("WIP_API_KEY").ok(),
            wip_api_user: std::env::var("WIP_API_USER").unwrap_or_else(|_| "管理者".to_string()),
        })
    }
}
