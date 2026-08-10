/// domain/models/membership_api.rs — プロジェクトメンバーシップの JSON API モデル
///
/// tickets_project_membership テーブル対応

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::models::ticket_api::UserSummaryOut;

// =============================================================================
// Membership
// =============================================================================

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MembershipOut {
    pub id: i32,
    pub user: UserSummaryOut,
    pub project: i32,
    pub start_date: NaiveDate,
    pub end_date: Option<NaiveDate>,
    pub note: Option<String>,
    pub added_by: UserSummaryOut,
    pub created_at: DateTime<Utc>,
    pub is_active: bool,
    pub is_in_grace_period: bool,
    pub days_until_expiry: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MembershipCreateIn {
    #[serde(rename = "user")]
    pub user_id: i32,
    pub project: i32,
    pub start_date: NaiveDate,
    pub end_date: Option<NaiveDate>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MembershipUpdateIn {
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub note: Option<String>,
}
