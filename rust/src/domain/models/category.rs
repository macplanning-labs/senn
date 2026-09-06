/// domain/models/category.rs — カテゴリーモデル
///
/// 2階層構造: level=1 がフェーズ、level=2 がカテゴリー。
/// CheckConstraint: level=1 → parent_id=NULL, level=2 → parent_id=NOT NULL

use serde::{Deserialize, Serialize};

/// カテゴリー（フェーズまたはカテゴリー）
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Category {
    pub id: i32,
    pub name: String,
    pub slug: String,
    pub level: i16,
    pub parent_id: Option<i32>,
    pub sort_order: i32,
    pub color: String,
    // 結合用
    pub parent_name: Option<String>,
}

impl Category {
    /// フェーズ（第1層）かどうか
    pub fn is_phase(&self) -> bool {
        self.level == 1
    }

    /// カテゴリー（第2層）かどうか
    pub fn is_category(&self) -> bool {
        self.level == 2
    }
}
