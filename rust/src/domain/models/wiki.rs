/// domain/models/wiki.rs — Wiki モデル
///
/// WikiPage と WikiRevision（編集履歴）。
/// [[Wikiリンク]] 記法のレンダリングはサービス層で実装。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Wiki カテゴリ
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WikiCategory {
    Manual,
    Minutes,
    Spec,
    Knowhow,
    Glossary,
    Other,
}

impl WikiCategory {
    pub fn label(&self) -> &str {
        match self {
            Self::Manual => "📘 マニュアル",
            Self::Minutes => "📋 議事録",
            Self::Spec => "📐 仕様書",
            Self::Knowhow => "💡 ノウハウ",
            Self::Glossary => "📚 用語集",
            Self::Other => "📄 その他",
        }
    }

    pub fn from_db(s: &str) -> Self {
        match s {
            "manual" => Self::Manual,
            "minutes" => Self::Minutes,
            "spec" => Self::Spec,
            "knowhow" => Self::Knowhow,
            "glossary" => Self::Glossary,
            "other" | _ => Self::Other,
        }
    }

    pub fn as_db_str(&self) -> &str {
        match self {
            Self::Manual => "manual",
            Self::Minutes => "minutes",
            Self::Spec => "spec",
            Self::Knowhow => "knowhow",
            Self::Glossary => "glossary",
            Self::Other => "other",
        }
    }

    pub fn all() -> Vec<WikiCategory> {
        vec![
            Self::Manual, Self::Minutes, Self::Spec,
            Self::Knowhow, Self::Glossary, Self::Other,
        ]
    }
}

impl std::fmt::Display for WikiCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

// ---------------------------------------------------------------------------
// WikiPage — Wiki ページ
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WikiPage {
    pub id: i32,
    pub project_id: Option<i32>,
    pub title: String,
    pub slug: String,
    pub category: String,
    pub content: String,
    pub author_id: i32,
    pub last_editor_id: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    // 結合用
    pub author_name: Option<String>,
    pub last_editor_name: Option<String>,
    pub project_name: Option<String>,
}

impl WikiPage {
    pub fn category_enum(&self) -> WikiCategory {
        WikiCategory::from_db(&self.category)
    }
}

// ---------------------------------------------------------------------------
// WikiRevision — 編集履歴
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WikiRevision {
    pub id: i32,
    pub page_id: i32,
    pub content: String,
    pub editor_id: Option<i32>,
    pub comment: String,
    pub created_at: DateTime<Utc>,
    // 結合用
    pub editor_name: Option<String>,
}
