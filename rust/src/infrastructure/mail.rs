/// infrastructure/mail.rs — メール送信（Gmail API / 汎用 SMTP）
///
/// 送信時に system_settings の sys.mail.mode を参照して振り分ける。
/// Gmail API モードはサービスアカウント JSON（env）を継続利用する。

use crate::config::AppConfig;
use crate::infrastructure::encrypted_settings::decrypt_value;
use crate::infrastructure::repositories::system_settings_repo;
use base64::Engine;
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use serde_json::json;
use sqlx::PgPool;

#[derive(Clone)]
pub struct MailSender {
    pool: PgPool,
    config: AppConfig,
}

impl MailSender {
    pub fn new(pool: PgPool, config: AppConfig) -> Self {
        Self { pool, config }
    }

    fn gmail_env_configured() -> bool {
        std::env::var("GOOGLE_SERVICE_ACCOUNT_KEY_PATH").is_ok()
            && std::env::var("EMAIL_HOST_USER").is_ok()
    }

    async fn resolve_from_email(&self, db_sender: Option<&str>) -> String {
        if let Some(s) = db_sender.filter(|v| !v.is_empty()) {
            return s.to_string();
        }
        std::env::var("EMAIL_HOST_USER").unwrap_or_else(|_| "noreply@senn.local".to_string())
    }

    /// 送信可能か（モードに応じた最低限の設定があるか）
    pub async fn is_configured(&self) -> bool {
        let settings = match system_settings_repo::load_mail_settings(&self.pool).await {
            Ok(v) => v,
            Err(_) => return Self::gmail_env_configured(),
        };

        if settings.mode == system_settings_repo::MAIL_MODE_SMTP {
            settings.smtp_host.as_ref().is_some_and(|h| !h.is_empty())
                && settings.smtp_password_encrypted.is_some()
        } else {
            Self::gmail_env_configured()
        }
    }

    pub async fn send(&self, to: &str, subject: &str, body: &str) -> anyhow::Result<()> {
        let settings = system_settings_repo::load_mail_settings(&self.pool).await?;
        let from_email = self
            .resolve_from_email(settings.sender.as_deref())
            .await;

        if settings.mode == system_settings_repo::MAIL_MODE_SMTP {
            return self
                .send_via_smtp(&settings, &from_email, to, subject, body)
                .await;
        }

        if !Self::gmail_env_configured() {
            tracing::info!("📧 [DRY-RUN] Email to {}: {} | {}", to, subject, body);
            return Ok(());
        }

        self.send_via_gmail_api(&from_email, to, subject, body)
            .await
    }

    async fn send_via_smtp(
        &self,
        settings: &system_settings_repo::MailSettingsSnapshot,
        from_email: &str,
        to: &str,
        subject: &str,
        body: &str,
    ) -> anyhow::Result<()> {
        let host = settings
            .smtp_host
            .as_deref()
            .filter(|h| !h.is_empty())
            .ok_or_else(|| anyhow::anyhow!("SMTPホストが設定されていません"))?;
        let port = settings.smtp_port.unwrap_or(587);
        let encryption = settings
            .smtp_encryption
            .as_deref()
            .unwrap_or("starttls");
        let password_blob = settings
            .smtp_password_encrypted
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("SMTPパスワードが設定されていません"))?;
        let password = decrypt_value(password_blob, &self.config.jwt_secret)?;

        let email = Message::builder()
            .from(from_email.parse()?)
            .to(to.parse()?)
            .subject(subject)
            .header(ContentType::TEXT_PLAIN)
            .body(body.to_string())?;

        let creds = Credentials::new(from_email.to_string(), password);

        let mailer = match encryption {
            "tls" => AsyncSmtpTransport::<Tokio1Executor>::relay(host)?
                .port(port)
                .credentials(creds)
                .build(),
            "none" => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host)
                .port(port)
                .credentials(creds)
                .build(),
            _ => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)?
                .port(port)
                .credentials(creds)
                .build(),
        };

        mailer.send(email).await?;
        tracing::info!("📧 Email sent via SMTP to {}: {}", to, subject);
        Ok(())
    }

    async fn send_via_gmail_api(
        &self,
        from_email: &str,
        to: &str,
        subject: &str,
        body: &str,
    ) -> anyhow::Result<()> {
        let key_path = std::env::var("GOOGLE_SERVICE_ACCOUNT_KEY_PATH")?;
        let key_content = std::fs::read_to_string(&key_path)?;

        let service_account_key = yup_oauth2::parse_service_account_key(key_content)?;
        let auth = yup_oauth2::ServiceAccountAuthenticator::builder(service_account_key)
            .subject(from_email.to_string())
            .build()
            .await?;

        let token = auth
            .token(&["https://www.googleapis.com/auth/gmail.send"])
            .await?;
        let access_token = token
            .token()
            .ok_or_else(|| anyhow::anyhow!("Gmail API: アクセストークンが取得できませんでした"))?;

        let encoded_subject = format!(
            "=?UTF-8?B?{}?=",
            base64::engine::general_purpose::STANDARD.encode(subject.as_bytes())
        );
        let email_message = format!(
            "From: {}\r\nTo: {}\r\nSubject: {}\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=UTF-8\r\n\r\n{}",
            from_email, to, encoded_subject, body
        );

        let encoded = base64::engine::general_purpose::STANDARD.encode(email_message.as_bytes());

        let client = reqwest::Client::new();
        let request_body = json!({
            "raw": encoded
        });

        let response = client
            .post("https://www.googleapis.com/gmail/v1/users/me/messages/send")
            .bearer_auth(access_token)
            .json(&request_body)
            .send()
            .await?;

        if response.status().is_success() {
            tracing::info!("📧 Email sent via Gmail API to {}: {}", to, subject);
            Ok(())
        } else {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Gmail API error: {}", error_text)
        }
    }
}
