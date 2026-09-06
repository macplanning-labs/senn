/// domain/models/saved_view_api.rs — Saved View JSON API モデル
///
/// t_saved_view テーブル用。個人用チケット一覧フィルタ。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SavedViewOut {
    pub id: i64,
    pub project: i64,
    pub name: String,
    pub filters: JsonValue,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "updatedAt")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SavedViewCreateIn {
    pub name: String,
    pub filters: JsonValue,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SavedViewUpdateIn {
    pub name: Option<String>,
    pub filters: Option<JsonValue>,
}
