/// domain/models/resource_api.rs — JSON API (Phase 3) 用のリソース表現
///
/// projects/categories/milestones/labels の4リソース。
/// Category/Milestone/Label/TeamSummaryはticket_api.rsで定義済みの型と
/// フィールド構成が完全に一致するため、そちらを再利用する(重複定義しない)。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub use crate::domain::models::ticket_api::{CategoryOut, LabelOut, MilestoneOut, TeamSummaryOut};

// ---------------------------------------------------------------------------
// プロジェクト
//
// apps/api/serializers.py の ProjectSerializer と厳密に一致させること。
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ProjectOut {
    pub id: i32,
    pub name: String,
    pub prefix: String,
    pub description: String,
    #[serde(rename = "ticketCount")]
    pub ticket_count: i64,
    #[serde(rename = "memberCount")]
    pub member_count: i64,
    pub teams: Vec<TeamSummaryOut>,
    #[serde(rename = "ownerId")]
    pub owner_id: Option<i32>,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "cycleAutoComplete")]
    pub cycle_auto_complete: bool,
    #[serde(rename = "cycleAutoCreateNext")]
    pub cycle_auto_create_next: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectWriteIn {
    pub name: String,
    pub prefix: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "teamIds")]
    pub team_ids: Vec<i32>,
}

/// JSONキー有無と null クリアを区別する（ticket_api::TicketPatchIn と同型）。
fn deserialize_present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Deserialize::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProjectPatchIn {
    #[serde(default, rename = "cycleAutoComplete")]
    pub cycle_auto_complete: Option<bool>,
    #[serde(default, rename = "cycleAutoCreateNext")]
    pub cycle_auto_create_next: Option<bool>,
}

// ---------------------------------------------------------------------------
// カテゴリー(pagination無し。配列そのままで返す)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct CategoryWriteIn {
    pub name: String,
    #[serde(default)]
    pub slug: String,
    pub level: i16,
    pub parent: Option<i32>,
    #[serde(default)]
    pub sort_order: i32,
    pub color: String,
}

// ---------------------------------------------------------------------------
// マイルストーン
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct MilestoneWriteIn {
    pub name: String,
    pub due_date: Option<chrono::NaiveDate>,
    #[serde(default)]
    pub description: String,
    pub project: Option<i32>,
}

// ---------------------------------------------------------------------------
// ラベル
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct LabelWriteIn {
    pub name: String,
    pub color: String,
    #[serde(default)]
    pub project: Option<i32>,
    #[serde(default, rename = "teamId")]
    pub team_id: Option<i32>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default = "default_is_ai_enabled", rename = "isAiEnabled")]
    pub is_ai_enabled: bool,
}

fn default_is_ai_enabled() -> bool {
    true
}
