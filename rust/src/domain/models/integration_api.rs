/// domain/models/integration_api.rs — Git連携 JSON API モデル
///
/// t_git_integration / t_git_event テーブル用。
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub use crate::domain::models::ticket_api::UserSummaryOut;

#[derive(Debug, Clone, Serialize)]
pub struct GitIntegrationOut {
    pub id: i32,
    pub project: Option<i32>,
    pub team: Option<i32>,
    pub provider: String,
    #[serde(rename = "repositoryUrl")]
    pub repository_url: String,
    #[serde(rename = "webhookSecret")]
    pub webhook_secret: String,
    #[serde(rename = "isActive")]
    pub is_active: bool,
    #[serde(rename = "autoStatusTransition")]
    pub auto_status_transition: bool,
    #[serde(rename = "createdBy")]
    pub created_by: Option<UserSummaryOut>,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "eventCount")]
    pub event_count: i64,
    #[serde(rename = "installationId")]
    pub installation_id: Option<i64>,
    #[serde(rename = "lastDeliveryAt")]
    pub last_delivery_at: Option<DateTime<Utc>>,
    #[serde(rename = "lastDeliveryEvent")]
    pub last_delivery_event: Option<String>,
    #[serde(rename = "lastDeliveryResult")]
    pub last_delivery_result: Option<String>,
    #[serde(rename = "lastDeliveryNote")]
    pub last_delivery_note: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitIntegrationWriteIn {
    pub project: Option<i32>,
    pub team: Option<i32>,
    #[serde(default = "default_provider")]
    pub provider: String,
    pub repository_url: String,
    pub webhook_secret: String,
    #[serde(default = "default_true")]
    pub is_active: bool,
    #[serde(
        default = "default_true",
        rename = "autoStatusTransition",
        alias = "auto_status_transition"
    )]
    pub auto_status_transition: bool,
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
    pub team: Option<i32>,
    pub provider: Option<String>,
    pub repository_url: Option<String>,
    pub webhook_secret: Option<String>,
    pub is_active: Option<bool>,
    #[serde(
        default,
        rename = "autoStatusTransition",
        alias = "auto_status_transition"
    )]
    pub auto_status_transition: Option<bool>,
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
    // ── Phase 2: PR の詳細(追加のみ) ──
    #[serde(rename = "isDraft")]
    pub is_draft: bool,
    /// none | approved | changes_requested
    #[serde(rename = "reviewState")]
    pub review_state: String,
    /// none | pending | success | failure
    #[serde(rename = "ciState")]
    pub ci_state: String,
    pub reviewers: Vec<String>,
    pub additions: Option<i32>,
    pub deletions: Option<i32>,
    #[serde(rename = "changedFiles")]
    pub changed_files: Option<i32>,
    #[serde(rename = "baseBranch")]
    pub base_branch: String,
}
