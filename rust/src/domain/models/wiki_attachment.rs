/// domain/models/wiki_attachment.rs — Wiki添付ファイルモデル

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WikiAttachment {
    pub id: i32,
    pub wiki_page_id: i32,
    pub filename: String,
    #[serde(rename = "filePath")]
    pub file_path: String,
    pub file_size: i32,
    pub uploader_id: i32,
    pub created_at: DateTime<Utc>,
    // 結合用
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uploader_name: Option<String>,
}
