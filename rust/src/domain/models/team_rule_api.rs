/// domain/models/team_rule_api.rs — Team Rules リソース表現
///
/// m_team_rule テーブル用の JSON API モデル。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub use crate::domain::models::ticket_api::UserSummaryOut;

#[derive(Debug, Clone, Serialize)]
pub struct TeamRuleOut {
    pub id: i32,
    pub team: Option<i32>,
    #[serde(rename = "teamName")]
    pub team_name: Option<String>,
    pub title: String,
    pub content: String,
    pub category: String,
    // 重要: sort_order と is_active は snake_case のままで返す(Django側の実装漏れ対応)
    pub sort_order: i32,
    pub is_active: bool,
    #[serde(rename = "createdBy")]
    pub created_by: Option<UserSummaryOut>,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "updatedAt")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TeamRuleWriteIn {
    pub team: Option<i32>,
    pub title: String,
    pub content: String,
    pub category: String,
    pub sort_order: i32,
    pub is_active: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TeamRuleUpdateIn {
    pub team: Option<i32>,
    pub title: Option<String>,
    pub content: Option<String>,
    pub category: Option<String>,
    pub sort_order: Option<i32>,
    pub is_active: Option<bool>,
}
