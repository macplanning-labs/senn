/// domain/models/saved_view_api.rs — Saved View JSON API モデル
///
/// t_saved_view テーブル用。個人用チケット一覧フィルタ。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SavedViewOut {
    pub id: i64,
    pub project: Option<i64>,
    #[serde(rename = "teamId")]
    pub team_id: Option<i64>,
    pub name: String,
    pub filters: JsonValue,
    #[serde(rename = "isShared")]
    pub is_shared: bool,
    #[serde(rename = "ownerId")]
    pub owner_id: i64,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "updatedAt")]
    pub updated_at: DateTime<Utc>,
    #[serde(rename = "viewType")]
    pub view_type: String,
}

fn default_view_type() -> String {
    "tickets".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct SavedViewCreateIn {
    pub name: String,
    pub filters: JsonValue,
    #[serde(default = "default_view_type", rename = "viewType")]
    pub view_type: String,
    #[serde(default, rename = "isShared", alias = "is_shared")]
    pub is_shared: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SavedViewUpdateIn {
    pub name: Option<String>,
    pub filters: Option<JsonValue>,
    #[serde(default, rename = "isShared", alias = "is_shared")]
    pub is_shared: Option<bool>,
}
