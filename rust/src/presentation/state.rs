/// presentation/state.rs — 共有ステート定義
///
/// Axum の State として全ハンドラに注入される。
/// DB プール + アプリ設定 + メール送信者 を保持する。

use std::sync::Arc;

use sqlx::PgPool;
use tokio::sync::RwLock;

use crate::config::AppConfig;
use crate::infrastructure::mail::MailSender;
use crate::infrastructure::repositories::system_settings_repo;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: AppConfig,
    pub mail_sender: Option<MailSender>,
    ollama_model_override: Arc<RwLock<Option<String>>>,
    ollama_timeout_override: Arc<RwLock<Option<u64>>>,
}

impl AppState {
    pub async fn new(
        pool: PgPool,
        config: AppConfig,
        mail_sender: Option<MailSender>,
    ) -> anyhow::Result<Self> {
        let ollama_model_override =
            system_settings_repo::get(&pool, system_settings_repo::KEY_OLLAMA_MODEL).await?;

        let ollama_timeout_override = match system_settings_repo::get(&pool, system_settings_repo::KEY_OLLAMA_TIMEOUT).await? {
            Some(timeout_str) => timeout_str.parse::<u64>().ok(),
            None => None,
        };

        Ok(Self {
            pool,
            config,
            mail_sender,
            ollama_model_override: Arc::new(RwLock::new(ollama_model_override)),
            ollama_timeout_override: Arc::new(RwLock::new(ollama_timeout_override)),
        })
    }

    pub async fn ai_config(&self) -> AppConfig {
        let mut cfg = self.config.clone();
        if let Some(model) = self.ollama_model_override.read().await.clone() {
            cfg.ollama_model = model;
        }
        if let Some(timeout) = *self.ollama_timeout_override.read().await {
            cfg.ollama_timeout_secs = timeout;
        }
        cfg
    }

    pub async fn set_ollama_model_override(&self, model: Option<String>) {
        *self.ollama_model_override.write().await = model;
    }

    pub async fn set_ollama_timeout_override(&self, timeout: Option<u64>) {
        *self.ollama_timeout_override.write().await = timeout;
    }
}
