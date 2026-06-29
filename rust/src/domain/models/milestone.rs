/// domain/models/milestone.rs — マイルストーンモデル

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Milestone {
    pub id: i32,
    pub name: String,
    pub due_date: Option<NaiveDate>,
    pub description: String,
    pub project_id: Option<i32>,
    pub created_at: DateTime<Utc>,
    // 集計用
    pub total_tickets: Option<i64>,
    pub closed_tickets: Option<i64>,
}

impl Milestone {
    /// 進捗率（%）
    pub fn progress_pct(&self) -> i32 {
        let total = self.total_tickets.unwrap_or(0);
        let closed = self.closed_tickets.unwrap_or(0);
        if total == 0 {
            0
        } else {
            ((closed as f64 / total as f64) * 100.0).round() as i32
        }
    }
}
