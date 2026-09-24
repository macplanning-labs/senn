/// domain/models/triage_api.rs — トリアージ依頼リソース表現
///
/// t_triage_request テーブル用の JSON API モデル。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

pub use crate::domain::models::ticket_api::UserSummaryOut;

#[derive(Debug, Clone, Serialize)]
pub struct TriageRequestOut {
    pub id: i32,
    pub title: String,
    pub description: String,
    #[serde(rename = "changeType")]
    pub change_type: String,
    #[serde(rename = "changePayload")]
    pub change_payload: JsonValue,
    pub status: String,
    pub project: Option<i32>,
    pub team: Option<i32>,
    pub ticket: Option<i32>,
    #[serde(rename = "ticketKey")]
    pub ticket_key: Option<String>,
    #[serde(rename = "ticketId")]
    pub ticket_id: Option<i32>,
    #[serde(rename = "requestedBy")]
    pub requested_by: UserSummaryOut,
    #[serde(rename = "reviewedBy")]
    pub reviewed_by: Option<UserSummaryOut>,
    #[serde(rename = "reviewedAt")]
    pub reviewed_at: Option<DateTime<Utc>>,
    #[serde(rename = "reviewComment")]
    pub review_comment: String,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TriageRequestWriteIn {
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_change_type")]
    pub change_type: String,
    #[serde(default = "default_change_payload")]
    pub change_payload: JsonValue,
    pub project: Option<i32>,
    pub team: Option<i32>,
    pub ticket: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TriageRequestUpdateIn {
    pub title: Option<String>,
    pub description: Option<String>,
    pub change_type: Option<String>,
    pub change_payload: Option<JsonValue>,
    pub project: Option<i32>,
    pub team: Option<i32>,
    pub ticket: Option<i32>,
}

fn default_change_type() -> String {
    "text_request".to_string()
}
fn default_change_payload() -> JsonValue {
    serde_json::json!({})
}

#[derive(Debug, Clone, Deserialize)]
pub struct TriageReviewIn {
    #[serde(default)]
    pub comment: String,
}

/// 承認リクエストボディ。Django側はTriageReviewSerializer(comment)に加え、
/// project_idをrequest.dataから直接読む(シリアライザ外の即席フィールド)。
#[derive(Debug, Clone, Deserialize)]
pub struct TriageApproveIn {
    #[serde(default)]
    pub comment: String,
    #[serde(default)]
    pub project_id: Option<i32>,
}
