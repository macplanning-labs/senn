/// domain/models/chat_integration_api.rs — チャット通知連携 JSON API モデル
///
/// t_chat_integration テーブル用。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub use crate::domain::models::ticket_api::UserSummaryOut;

pub const VALID_PROVIDERS: [&str; 4] = ["slack", "google_chat", "teams", "chatwork"];

#[derive(Debug, Clone, Serialize)]
pub struct ChatIntegrationOut {
    pub id: i32,
    pub project: Option<i32>,
    pub team: Option<i32>,
    pub provider: String,
    #[serde(rename = "webhookUrl")]
    pub webhook_url: Option<String>,
    #[serde(rename = "apiToken")]
    pub api_token: Option<String>,
    #[serde(rename = "roomId")]
    pub room_id: Option<String>,
    #[serde(rename = "enabledCategories")]
    pub enabled_categories: Vec<String>,
    #[serde(rename = "isActive")]
    pub is_active: bool,
    #[serde(rename = "createdBy")]
    pub created_by: Option<UserSummaryOut>,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatIntegrationWriteIn {
    pub project: Option<i32>,
    pub team: Option<i32>,
    pub provider: String,
    #[serde(default, rename = "webhookUrl", alias = "webhook_url")]
    pub webhook_url: Option<String>,
    #[serde(default, rename = "apiToken", alias = "api_token")]
    pub api_token: Option<String>,
    #[serde(default, rename = "roomId", alias = "room_id")]
    pub room_id: Option<String>,
    #[serde(default = "default_categories", rename = "enabledCategories", alias = "enabled_categories")]
    pub enabled_categories: Vec<String>,
    #[serde(default = "default_true")]
    pub is_active: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatIntegrationUpdateIn {
    pub project: Option<i32>,
    pub team: Option<i32>,
    pub provider: Option<String>,
    #[serde(default, rename = "webhookUrl", alias = "webhook_url")]
    pub webhook_url: Option<String>,
    #[serde(default, rename = "apiToken", alias = "api_token")]
    pub api_token: Option<String>,
    #[serde(default, rename = "roomId", alias = "room_id")]
    pub room_id: Option<String>,
    #[serde(default, rename = "enabledCategories", alias = "enabled_categories")]
    pub enabled_categories: Option<Vec<String>>,
    pub is_active: Option<bool>,
}

fn default_categories() -> Vec<String> {
    vec!["assigned".to_string()]
}
fn default_true() -> bool {
    true
}
