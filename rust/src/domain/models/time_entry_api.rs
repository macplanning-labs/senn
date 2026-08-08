/// domain/models/time_entry_api.rs — Time Entry リソース表現
///
/// t_time_entry テーブル用の JSON API モデル。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub use crate::domain::models::ticket_api::UserSummaryOut;

#[derive(Debug, Clone, Serialize)]
pub struct TimeEntryOut {
    pub id: i32,
    pub ticket: i32,
    pub user: UserSummaryOut,
    pub description: String,
    #[serde(rename = "startTime")]
    pub start_time: Option<DateTime<Utc>>,
    #[serde(rename = "endTime")]
    pub end_time: Option<DateTime<Utc>>,
    #[serde(rename = "durationMinutes")]
    pub duration_minutes: i32,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeEntryCreateIn {
    pub ticket: i32,
    pub description: String,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub duration_minutes: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeEntryTodayOut {
    pub date: String,
    pub total_minutes: i32,
    pub entry_count: i32,
}
