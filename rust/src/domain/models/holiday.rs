/// domain/models/holiday.rs — 休日モデル（ガントカレンダー用）

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Holiday {
    pub id: i32,
    pub date: NaiveDate,
    pub name: String,
    pub recurring: bool,
}
