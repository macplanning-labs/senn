/// domain/models/wiki_api.rs — Wiki JSON API モデル
///
/// wiki_page / wiki_revision テーブル用。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub use crate::domain::models::ticket_api::UserSummaryOut;

#[derive(Debug, Clone, Serialize)]
pub struct WikiPageListOut {
    pub id: i32,
    pub title: String,
    pub slug: String,
    pub category: String,
    pub project: Option<i32>,
    pub team: Option<i32>,
    pub author: UserSummaryOut,
    #[serde(rename = "lastEditor")]
    pub last_editor: Option<UserSummaryOut>,
    #[serde(rename = "revisionCount")]
    pub revision_count: i64,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "updatedAt")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LinkedTicketSummaryOut {
    pub id: i32,
    #[serde(rename = "ticketKey")]
    pub ticket_key: String,
    pub title: String,
    pub status: String,
    pub priority: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WikiPageDetailOut {
    pub id: i32,
    pub title: String,
    pub slug: String,
    pub category: String,
    pub project: Option<i32>,
    pub team: Option<i32>,
    pub content: String,
    pub author: UserSummaryOut,
    #[serde(rename = "lastEditor")]
    pub last_editor: Option<UserSummaryOut>,
    #[serde(rename = "renderedContent")]
    pub rendered_content: String,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "updatedAt")]
    pub updated_at: DateTime<Utc>,
    #[serde(rename = "linkedTickets")]
    pub linked_tickets: Vec<LinkedTicketSummaryOut>,
}

/// 作成用。slugはサーバー側で自動生成(Django側もslugはread_only)。
#[derive(Debug, Clone, Deserialize)]
pub struct WikiPageCreateIn {
    pub title: String,
    #[serde(default = "default_category")]
    pub category: String,
    pub project: Option<i32>,
    pub team: Option<i32>,
    #[serde(default)]
    pub content: String,
}

/// 更新用。slugは変更しない(Django側もタイトル変更でslugは追従しない)。
/// revision_commentはDjango側もシリアライザ外の即席フィールド(request.dataから直接読む)。
#[derive(Debug, Clone, Deserialize)]
pub struct WikiPageUpdateIn {
    pub title: Option<String>,
    pub category: Option<String>,
    pub project: Option<i32>,
    pub team: Option<i32>,
    pub content: Option<String>,
    #[serde(default)]
    pub revision_comment: String,
}

fn default_category() -> String {
    "other".to_string()
}

#[derive(Debug, Clone, Serialize)]
pub struct WikiRevisionOut {
    pub id: i32,
    pub content: String,
    pub editor: Option<UserSummaryOut>,
    pub comment: String,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LinkTicketIn {
    pub ticket_id: i32,
}
