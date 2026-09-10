/// domain/models/attachment.rs — 添付ファイルモデル

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Attachment {
    pub id: i32,
    pub ticket_id: i32,
    pub comment_id: Option<i32>,
    pub uploader_id: i32,
    pub filename: String,
    pub file_path: String,
    pub file_size: i32,
    pub created_at: DateTime<Utc>,
    // 結合用
    pub uploader_name: Option<String>,
}

impl Attachment {
    /// 画像ファイルかどうか
    pub fn is_image(&self) -> bool {
        let lower = self.filename.to_lowercase();
        lower.ends_with(".png")
            || lower.ends_with(".jpg")
            || lower.ends_with(".jpeg")
            || lower.ends_with(".gif")
            || lower.ends_with(".webp")
            || lower.ends_with(".svg")
    }

    /// ファイルサイズの人間可読表記
    pub fn human_size(&self) -> String {
        let size = self.file_size as f64;
        if size < 1024.0 {
            format!("{} B", self.file_size)
        } else if size < 1024.0 * 1024.0 {
            format!("{:.1} KB", size / 1024.0)
        } else {
            format!("{:.1} MB", size / 1024.0 / 1024.0)
        }
    }
}
