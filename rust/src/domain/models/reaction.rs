/// domain/models/reaction.rs — チケットリアクション / カスタム絵文字モデル

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::domain::models::ticket_api::UserSummaryOut;

// リアクション出力型
#[derive(Debug, Clone, Serialize)]
pub struct ReactionOut {
    pub id: i64,
    #[serde(rename = "ticketId")]
    pub ticket_id: i64,
    #[serde(rename = "userId")]
    pub user_id: i64,
    pub user: UserSummaryOut,
    #[serde(rename = "emojiKind")]
    pub emoji_kind: String,
    #[serde(rename = "emojiValue")]
    pub emoji_value: String,
    #[serde(rename = "imageUrl")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "updatedAt")]
    pub updated_at: DateTime<Utc>,
}

// リアクション入力型（POST）
#[derive(Debug, Deserialize)]
pub struct ReactionIn {
    #[serde(rename = "emojiKind", alias = "emoji_kind")]
    pub emoji_kind: String,
    #[serde(rename = "emojiValue", alias = "emoji_value")]
    pub emoji_value: String,
}

// リアクション DB 行（SELECT）
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReactionRow {
    pub id: i64,
    pub ticket_id: i64,
    pub user_id: i64,
    pub emoji_kind: String,
    pub emoji_value: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// カスタム絵文字出力型
#[derive(Debug, Clone, Serialize)]
pub struct CustomEmojiOut {
    pub id: i64,
    #[serde(rename = "projectId")]
    pub project_id: i64,
    pub slug: String,
    pub name: String,
    #[serde(rename = "imageUrl")]
    pub image_url: String,
    #[serde(rename = "uploadedBy")]
    pub uploaded_by: UserSummaryOut,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
}

// カスタム絵文字入力型（POST multipart）
#[derive(Debug, Deserialize)]
pub struct CustomEmojiIn {
    pub slug: String,
    pub name: String,
    // file は multipart field として別途処理
}

// カスタム絵文字 DB 行
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CustomEmojiRow {
    pub id: i64,
    pub project_id: i64,
    pub slug: String,
    pub name: String,
    pub image_path: String,
    pub uploaded_by: i64,
    pub created_at: DateTime<Utc>,
}
