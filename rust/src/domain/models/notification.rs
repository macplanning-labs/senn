/// domain/models/notification.rs — 通知モデル
///
/// In-app 通知（ベル通知）と通知送信ログ。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// NotificationCategory — 通知カテゴリ
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotificationCategory {
    Assigned,
    Commented,
    StatusChanged,
    DueSoon,
    Overdue,
    Mentioned,
    Replied,
    CycleAutoCompleted,
    Updated,
}

impl NotificationCategory {
    pub fn label(&self) -> &str {
        match self {
            Self::Assigned => "担当設定",
            Self::Commented => "コメント",
            Self::StatusChanged => "ステータス変更",
            Self::DueSoon => "期限間近",
            Self::Overdue => "期限超過",
            Self::Mentioned => "メンション",
            Self::Replied => "返信",
            Self::CycleAutoCompleted => "Cycle自動完了",
            Self::Updated => "更新",
        }
    }

    pub fn from_db(s: &str) -> Self {
        match s {
            "assigned" => Self::Assigned,
            "commented" => Self::Commented,
            "status_changed" => Self::StatusChanged,
            "due_soon" => Self::DueSoon,
            "overdue" => Self::Overdue,
            "mentioned" => Self::Mentioned,
            "replied" => Self::Replied,
            "cycle_auto_completed" => Self::CycleAutoCompleted,
            "updated" => Self::Updated,
            _ => Self::Commented,
        }
    }

    pub fn as_db_str(&self) -> &str {
        match self {
            Self::Assigned => "assigned",
            Self::Commented => "commented",
            Self::StatusChanged => "status_changed",
            Self::DueSoon => "due_soon",
            Self::Overdue => "overdue",
            Self::Mentioned => "mentioned",
            Self::Replied => "replied",
            Self::CycleAutoCompleted => "cycle_auto_completed",
            Self::Updated => "updated",
        }
    }

    pub fn icon(&self) -> &str {
        match self {
            Self::Assigned => "👤",
            Self::Commented => "💬",
            Self::StatusChanged => "🔄",
            Self::DueSoon => "⏰",
            Self::Overdue => "🔥",
            Self::Mentioned => "📢",
            Self::Replied => "↩️",
            Self::CycleAutoCompleted => "📅",
            Self::Updated => "📝",
        }
    }
}

impl std::fmt::Display for NotificationCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

// ---------------------------------------------------------------------------
// Notification — 通知エンティティ
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Notification {
    pub id: i32,
    pub user_id: i32,
    pub ticket_id: Option<i32>,
    pub category: String,
    pub title: String,
    pub message: String,
    pub is_read: bool,
    pub created_at: DateTime<Utc>,
    // 結合用
    pub ticket_key: Option<String>,
}

impl Notification {
    pub fn category_enum(&self) -> NotificationCategory {
        NotificationCategory::from_db(&self.category)
    }
}

// ---------------------------------------------------------------------------
// NotificationLog — 通知送信ログ（メール重複防止用）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct NotificationLog {
    pub id: i32,
    pub ticket_id: i32,
    pub user_id: i32,
    pub notification_type: String,
    pub sent_at: DateTime<Utc>,
}
