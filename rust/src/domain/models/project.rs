/// domain/models/project.rs — プロジェクトモデル

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Project {
    pub id: i32,
    pub name: String,
    pub prefix: String,
    pub description: String,
    pub created_at: DateTime<Utc>,
}
