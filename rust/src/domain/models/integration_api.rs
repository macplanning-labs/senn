/// domain/models/integration_api.rs — Git連携 JSON API モデル
///
/// t_git_integration / t_git_event テーブル用。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub use crate::domain::models::ticket_api::UserSummaryOut;

#[derive(Debug, Clone, Serialize)]
pub struct GitIntegrationOut {
    pub id: i32,
    pub project: i32,
    pub provider: String,
    #[serde(rename = "repositoryUrl")]
    pub repository_url: String,
    #[serde(rename = "webhookSecret")]
    pub webhook_secret: String,
    #[serde(rename = "isActive")]
    pub is_active: bool,
    #[serde(rename = "createdBy")]
    pub created_by: Option<UserSummaryOut>,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "eventCount")]
    pub event_count: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitIntegrationWriteIn {
    pub project: i32,
    #[serde(default = "default_provider")]
    pub provider: String,
    pub repository_url: String,
    pub webhook_secret: String,
    #[serde(default = "default_true")]
    pub is_active: bool,
}

fn default_provider() -> String {
    "github".to_string()
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitIntegrationUpdateIn {
    pub project: Option<i32>,
    pub provider: Option<String>,
    pub repository_url: Option<String>,
    pub webhook_secret: Option<String>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitEventOut {
    pub id: i32,
    #[serde(rename = "eventType")]
    pub event_type: String,
    pub title: String,
    pub url: String,
    pub sha: String,
    #[serde(rename = "shaShort")]
    pub sha_short: String,
    pub branch: String,
    #[serde(rename = "authorName")]
    pub author_name: String,
    #[serde(rename = "authorAvatarUrl")]
    pub author_avatar_url: String,
    #[serde(rename = "prNumber")]
    pub pr_number: Option<i32>,
    #[serde(rename = "prState")]
    pub pr_state: String,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
}
