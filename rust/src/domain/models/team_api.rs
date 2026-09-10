/// domain/models/team_api.rs — チームと メンバーシップの JSON API モデル
///
/// m_team, t_team_membership テーブル対応

use chrono::{DateTime, NaiveDate, Utc};
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
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
    #[serde(alias = "slack_webhook_url")]
    pub slack_webhook_url: Option<String>,
    #[serde(default, alias = "is_active")]
    pub is_active: bool,
    #[serde(default)]
    pub prefix: Option<String>,
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
    /// camelCase `userId` が正。`user_id` も受け付ける（旧クライアント互換）
    #[serde(alias = "user_id")]
    pub user_id: i32,
    #[serde(default = "default_role")]
    pub role: String,
}

fn default_role() -> String {
    "member".to_string()
}

/// 薄い Role: admin / member のみ。旧 `leader` は admin に正規化する。
pub fn normalize_team_role(role: &str) -> Option<&'static str> {
    match role.trim().to_ascii_lowercase().as_str() {
        "admin" | "leader" => Some("admin"),
        "member" => Some("member"),
        _ => None,
    }
}

// =============================================================================
// L2: Project ゲスト(scoped_project_id 付き t_team_membership)
// =============================================================================

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamGuestOut {
    pub id: i32,
    pub team: i32,
    pub user: UserSummaryOut,
    pub project: i32,
    pub project_name: String,
    pub project_prefix: String,
    pub end_date: Option<NaiveDate>,
    pub is_active: bool,
    pub is_in_grace_period: bool,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamGuestCreateIn {
    #[serde(alias = "user_id")]
    pub user_id: i32,
    #[serde(alias = "project_id")]
    pub project_id: i32,
    #[serde(default, alias = "end_date")]
    pub end_date: Option<NaiveDate>,
}

#[cfg(test)]
mod role_tests {
    use super::normalize_team_role;

    #[test]
    fn normalizes_admin_member_and_legacy_leader() {
        assert_eq!(normalize_team_role("admin"), Some("admin"));
        assert_eq!(normalize_team_role("member"), Some("member"));
        assert_eq!(normalize_team_role("leader"), Some("admin"));
        assert_eq!(normalize_team_role("LEADER"), Some("admin"));
        assert_eq!(normalize_team_role("owner"), None);
    }
}
