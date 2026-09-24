/// config.rs — アプリケーション設定
///
/// 環境変数から設定値を読み込む。
/// .env ファイルまたは Docker の環境変数で設定する。

#[derive(Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub port: u16,
    pub base_url: String,
    /// オリジンチェックで `base_url` に加えて許可するオリジン。
    /// `ADDITIONAL_ALLOWED_ORIGINS` にカンマ区切りで指定する
    /// (例: ドメイン移行中に新旧ホストを併記したい場合)。既定は空。
    pub additional_allowed_origins: Vec<String>,
    /// Cookie の Secure 属性。`COOKIE_SECURE` 環境変数で明示上書きできる。
    /// 未設定時は `base_url` が `https://` で始まるかどうかから自動判定する
    /// (ローカル開発の `http://localhost` のままSecure=trueを固定すると、
    /// ブラウザがCookieを保存できずログインが無限リダイレクトになる事故を防ぐため)。
    pub cookie_secure: bool,
    pub media_dir: String,
    pub max_upload_size: usize,
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
    pub smtp_user: Option<String>,
    pub smtp_password: Option<String>,
    pub webauthn_rp_id: String,
    pub webauthn_rp_origin: String,
    /// JWT署名鍵。`JWT_SECRET_KEY` で設定する(旧名 `DJANGO_SECRET_KEY` も当面有効)。
    /// 未設定時に既定値へフォールバックはしない。`resolve_jwt_secret` を参照。
    pub jwt_secret: String,
    /// アクセストークンの有効期限(分)。`ACCESS_TOKEN_LIFETIME_MINUTES`で上書き可能。
    pub access_token_lifetime_minutes: i64,
    /// リフレッシュトークンの有効期限(日)。`REFRESH_TOKEN_LIFETIME_DAYS`で上書き可能。
    pub refresh_token_lifetime_days: i64,
    /// MFAチャレンジトークンの有効期限(秒)。`MFA_TOKEN_LIFETIME_SECONDS`で上書き可能。
    pub mfa_token_lifetime_seconds: i64,
    /// パスワードリセットトークンの有効期限(時間)。`PASSWORD_RESET_TOKEN_TTL_HOURS`で上書き可能。
    pub password_reset_token_ttl_hours: i64,
    /// 外部API(X-API-Key認証)用の共有キー。SENN_API_KEY（移行中は WIP_API_KEY も可）。
    pub wip_api_key: Option<String>,
    /// 外部API経由の操作を実行するユーザー名。SENN_API_USER 相当(デフォルト"管理者")。
    pub wip_api_user: String,
    /// AI専用外部API(X-AI-Api-Key認証)用の共有キー。wip_api_keyとは別系統のキーとして扱い、
    /// AIエージェントによる操作を人間/他システムの操作(wip_api_key)と区別できるようにする。
    pub wip_ai_api_key: Option<String>,
    /// AI専用外部API経由の操作を実行するユーザー名。未存在ならauthenticate時に自動作成する。
    pub wip_ai_api_user: String,
    pub ollama_url: String,
    pub ollama_model: String,
    pub ollama_timeout_secs: u64,
    pub openai_api_key: Option<String>,
    pub openai_model: String,
    pub openai_timeout_secs: u64,
    /// ビルドチャネル(customer / internal)。未設定は internal（自社向け既定）
    pub app_channel: String,
}

/// `.env` のテンプレートに載せているプレースホルダ。署名鍵として受け付けない。
const PLACEHOLDER_JWT_SECRETS: &[&str] = &[
    "change_me",
    "change_me_to_a_long_random_secret",
    "change_me_to_a_random_secret_key",
    "change_me_to_another_random_secret_key",
];

/// JWT署名鍵を解決する。
///
/// `JWT_SECRET_KEY` を正とし、旧名の `DJANGO_SECRET_KEY` も当面は受け付ける。
/// 以前はここに固定値のフォールバックがあったが、その値が公開リポジトリに含まれており、
/// 未設定のまま起動したインスタンスが既知の鍵でトークンを署名してしまうため削除した。
/// 未設定・短すぎる値・テンプレートの既定値のままの場合は起動を中断する。
fn resolve_jwt_secret() -> anyhow::Result<String> {
    // 空文字は「未設定」として扱う(移行期に JWT_SECRET_KEY= と書かれていても
    // 旧名 DJANGO_SECRET_KEY へ正しくフォールバックさせるため)。
    let read = |key: &str| std::env::var(key).ok().filter(|v| !v.trim().is_empty());
    let secret = read("JWT_SECRET_KEY")
        .or_else(|| read("DJANGO_SECRET_KEY"))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "JWT_SECRET_KEY が設定されていません。`openssl rand -base64 48` などで生成した値を .env に設定してください。"
            )
        })?;
    let trimmed = secret.trim();
    if trimmed.len() < 32 {
        anyhow::bail!(
            "JWT_SECRET_KEY が短すぎます(32文字以上が必要)。`openssl rand -base64 48` などで生成した値を設定してください。"
        );
    }
    if PLACEHOLDER_JWT_SECRETS.contains(&trimmed) {
        anyhow::bail!(
            "JWT_SECRET_KEY がテンプレートの既定値のままです。`openssl rand -base64 48` などで生成した値に変更してください。"
        );
    }
    Ok(secret)
}

impl AppConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        // WebAuthn のオリジンを未設定時に BASE_URL と一致させるため、先に解決しておく
        // (ポートを変えた構成でオリジン不一致によりパスキーが使えなくなるのを防ぐ)。
        let base_url =
            std::env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:8150".to_string());
        Ok(Self {
            database_url: crate::infrastructure::db::resolve_database_url()?,
            port: std::env::var("PORT")
                .unwrap_or_else(|_| "8150".to_string())
                .parse()?,
            base_url: base_url.clone(),
            cookie_secure: std::env::var("COOKIE_SECURE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(|| {
                    std::env::var("BASE_URL")
                        .map(|u| u.starts_with("https://"))
                        .unwrap_or(false)
                }),
            additional_allowed_origins: std::env::var("ADDITIONAL_ALLOWED_ORIGINS")
                .unwrap_or_default()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
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
            // Django互換: 環境変数ファイル は EMAIL_HOST_USER / EMAIL_HOST_PASSWORD を使う
            smtp_user: std::env::var("EMAIL_USER")
                .ok()
                .or_else(|| std::env::var("EMAIL_HOST_USER").ok()),
            smtp_password: std::env::var("EMAIL_PASSWORD")
                .ok()
                .or_else(|| std::env::var("EMAIL_HOST_PASSWORD").ok()),
            webauthn_rp_id: std::env::var("WEBAUTHN_RP_ID")
                .unwrap_or_else(|_| "localhost".to_string()),
            webauthn_rp_origin: std::env::var("WEBAUTHN_RP_ORIGIN")
                .unwrap_or_else(|_| base_url.clone()),
            jwt_secret: resolve_jwt_secret()?,
            access_token_lifetime_minutes: std::env::var("ACCESS_TOKEN_LIFETIME_MINUTES")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30),
            refresh_token_lifetime_days: std::env::var("REFRESH_TOKEN_LIFETIME_DAYS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(7),
            mfa_token_lifetime_seconds: std::env::var("MFA_TOKEN_LIFETIME_SECONDS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(300),
            password_reset_token_ttl_hours: std::env::var("PASSWORD_RESET_TOKEN_TTL_HOURS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(24),
            // SENN_* 優先、移行中のみ WIP_* フォールバック
            wip_api_key: std::env::var("SENN_API_KEY")
                .or_else(|_| std::env::var("WIP_API_KEY"))
                .ok(),
            wip_api_user: std::env::var("SENN_API_USER")
                .or_else(|_| std::env::var("WIP_API_USER"))
                .unwrap_or_else(|_| "管理者".to_string()),
            wip_ai_api_key: std::env::var("SENN_AI_API_KEY")
                .or_else(|_| std::env::var("WIP_AI_API_KEY"))
                .ok()
                .filter(|s| !s.is_empty()),
            wip_ai_api_user: std::env::var("SENN_AI_API_USER")
                .or_else(|_| std::env::var("WIP_AI_API_USER"))
                .unwrap_or_else(|_| "ai_agent".to_string()),
            ollama_url: std::env::var("OLLAMA_URL").unwrap_or_else(|_| "http://localhost:11434".to_string()),
            ollama_model: std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "qwen2.5:3b".to_string()),
            ollama_timeout_secs: std::env::var("OLLAMA_TIMEOUT").ok().and_then(|s| s.parse().ok()).unwrap_or(60),
            openai_api_key: std::env::var("OPENAI_API_KEY").ok().filter(|s| !s.is_empty()),
            openai_model: std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string()),
            openai_timeout_secs: std::env::var("OPENAI_TIMEOUT").ok().and_then(|s| s.parse().ok()).unwrap_or(30),
            app_channel: std::env::var("SENN_APP_CHANNEL")
                .or_else(|_| std::env::var("WIP_APP_CHANNEL"))
                .unwrap_or_else(|_| "internal".to_string()),
        })
    }
}
