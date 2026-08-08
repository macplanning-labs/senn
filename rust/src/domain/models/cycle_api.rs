/// domain/models/cycle_api.rs — JSON API 用サイクル表現
///
/// Django /api/v1/cycles/* と互換性のある構造。

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use crate::domain::models::ticket_api::UserSummaryOut;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct CycleOut {
    pub id: i32,
    pub project: i32,
    pub name: String,
    pub number: i32,
    pub status: String,
    #[serde(rename = "startDate")]
    pub start_date: NaiveDate,
    #[serde(rename = "endDate")]
    pub end_date: NaiveDate,
    #[serde(rename = "createdBy")]
    pub created_by: Option<UserSummaryOut>,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "ticketCount")]
    pub ticket_count: i64,
    #[serde(rename = "completedCount")]
    pub completed_count: i64,
    #[serde(rename = "totalPoints")]
    pub total_points: i64,
    #[serde(rename = "completedPoints")]
    pub completed_points: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CycleWriteIn {
    pub project: i32,
    pub name: String,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    #[serde(default = "default_cycle_status")]
    pub status: String,
}

fn default_cycle_status() -> String {
    "planned".to_string()
}
