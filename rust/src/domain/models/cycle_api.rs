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

/// PATCH /api/v1/cycles/{id}/ 用の部分更新入力。指定したフィールドのみ上書きする。
#[derive(Debug, Clone, Deserialize)]
pub struct CyclePatchIn {
    pub project: Option<i32>,
    pub name: Option<String>,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub status: Option<String>,
}

fn default_cycle_status() -> String {
    "planned".to_string()
}

// =============================================================================
// 高度アクション(progress / complete / velocity / burndown)
//
// CycleProgressOut/VelocityEntryOutはフロントエンドがcamelCaseで参照している
// ため#[serde(rename_all = "camelCase")]を付与(Django版はDRFシリアライザを
// 使わずsnake_caseのdictをそのまま返していたが、Django削除に伴い当時の
// 「互換優先」方針は前提から外れたため、フロントエンドの実際の期待値に合わせた)。
// CompleteCycleOut/BurndownPointOutはフロントエンド側でレスポンスの個別
// フィールドを参照していないため、現状は変更不要。
// また、Django側の現行実装は各チケットの"現在の"story_pointsを都度集計するのみで
// h_task_point_history(ポイント変更履歴)は参照していない。Rust側もまずは
// Djangoと同一の挙動で移植する(動的ポイント変更を厳密に履歴ベースで再計算する
// 設計は、Django側の仕様変更が決まった時点で改めて拡張する)。
// =============================================================================

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CycleProgressOut {
    pub ticket_count: i64,
    pub completed_count: i64,
    pub in_progress_count: i64,
    pub total_points: i64,
    pub completed_points: i64,
    pub completion_rate: f64,
    pub initial_points: i64,
    pub scope_added: i64,
    pub scope_removed: i64,
    pub scope_change: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VelocityEntryOut {
    pub cycle_id: i32,
    pub cycle_number: i32,
    pub cycle_name: String,
    pub completed_count: i64,
    pub completed_points: i64,
    pub scope_change: i64,
    pub carry_over: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompleteCycleIn {
    pub carry_over_to: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompleteCycleOut {
    pub completed: bool,
    pub carried_over: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BurndownPointOut {
    pub date: String,
    pub ideal: f64,
    pub actual: i64,
    #[serde(rename = "totalScope")]
    pub total_scope: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutoCompleteLogEntry {
    pub cycle_id: i32,
    pub project_id: i32,
    pub carried_over: i64,
    pub target_cycle_id: Option<i32>,
}
