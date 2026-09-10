/// domain/models/comment.rs — コメントモデル

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Comment {
    pub id: i32,
    pub ticket_id: i32,
    pub author_id: i32,
    pub body: String,
    pub created_at: DateTime<Utc>,
    // 結合用
    pub author_name: Option<String>,
    // インラインコメント用アンカー
    #[sqlx(default)]
    pub anchor_start: Option<i32>,
    #[sqlx(default)]
    pub anchor_end: Option<i32>,
    #[sqlx(default)]
    pub anchor_quote: Option<String>,
}
