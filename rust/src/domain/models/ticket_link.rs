/// domain/models/ticket_link.rs — チケット参照リンクモデル

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TicketLink {
    pub id: i32,
    pub ticket_id: i32,
    pub url: String,
    pub title: Option<String>,
    pub created_by_id: i32,
    pub created_at: DateTime<Utc>,
    // 結合用
    pub created_by_name: Option<String>,
}
