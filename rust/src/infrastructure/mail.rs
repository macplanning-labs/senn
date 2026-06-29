/// infrastructure/mail.rs — SMTP メール送信
///
/// lettre を使用した非同期メール送信。
/// SMTP 設定が未設定の場合はログ出力のみ。

use crate::config::AppConfig;
use std::sync::Arc;

#[derive(Clone)]
pub struct MailSender {
    transport: Option<Arc<lettre::AsyncSmtpTransport<lettre::Tokio1Executor>>>,
    from: String,
}

impl MailSender {
    /// 設定から MailSender を構築
    pub fn new(config: &AppConfig) -> Self {
        let transport = match (&config.smtp_host, &config.smtp_user, &config.smtp_password) {
            (Some(host), Some(user), Some(pass)) => {
                let port = config.smtp_port.unwrap_or(587);
                match lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::starttls_relay(host) {
                    Ok(builder) => {
                        let t = builder
                            .port(port)
                            .credentials(lettre::transport::smtp::authentication::Credentials::new(
                                user.clone(),
                                pass.clone(),
                            ))
                            .build();
                        tracing::info!("✅ SMTP configured: {}:{}", host, port);
                        Some(Arc::new(t))
                    }
                    Err(e) => {
                        tracing::warn!("⚠️ SMTP setup failed: {}", e);
                        None
                    }
                }
            }
            _ => {
                tracing::info!("📧 SMTP not configured, emails will be logged only");
                None
            }
        };

        let from = config.smtp_user.clone().unwrap_or_else(|| "noreply@wip.local".to_string());

        Self { transport, from }
    }

    /// メール送信
    pub async fn send(&self, to: &str, subject: &str, body: &str) -> anyhow::Result<()> {
        use lettre::{AsyncTransport, Message};

        let email = Message::builder()
            .from(self.from.parse()?)
            .to(to.parse()?)
            .subject(subject)
            .body(body.to_string())?;

        match &self.transport {
            Some(transport) => {
                transport.send(email).await?;
                tracing::info!("📧 Email sent to {}: {}", to, subject);
            }
            None => {
                tracing::info!("📧 [DRY-RUN] Email to {}: {} | {}", to, subject, body);
            }
        }

        Ok(())
    }
}
