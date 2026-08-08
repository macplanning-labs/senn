/// domain/models/team_api.rs — チームと メンバーシップの JSON API モデル
///
/// m_team, t_team_membership テーブル対応

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::models::ticket_api::UserSummaryOut;

// =============================================================================
// Team
// =============================================================================

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamOut {
    pub id: i32,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub icon: String,
    pub color: String,
    pub slack_webhook_url: Option<String>,
    pub is_active: bool,
    pub member_count: i64,
    pub project_count: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamWriteIn {
    pub name: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub color: String,
    pub slack_webhook_url: Option<String>,
    #[serde(default)]
    pub is_active: bool,
}

// =============================================================================
// Team Membership
// =============================================================================

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamMembershipOut {
    pub id: i32,
    pub team: i32,
    pub user: UserSummaryOut,
    pub role: String,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamMembershipCreateIn {
    pub user_id: i32,
    #[serde(default = "default_role")]
    pub role: String,
}

fn default_role() -> String {
    "member".to_string()
}
