/// domain/models/ticket.rs — チケットドメインモデル
///
/// チケットエンティティおよび値オブジェクト（TicketStatus, Priority, TicketType）を定義。
/// ステータス遷移ルールはここに集約（KI #064/#065 準拠: 遷移ルール1箇所定義）。

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// TicketStatus — チケットステータス
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TicketStatus {
    Open,
    InProgress,
    Resolved,
    Closed,
}

impl TicketStatus {
    /// 日本語ラベル（DB 格納値としても使用）
    pub fn label(&self) -> &str {
        match self {
            Self::Open => "未対応",
            Self::InProgress => "処理中",
            Self::Resolved => "処理済み",
            Self::Closed => "完了",
        }
    }

    /// DB 文字列 → enum
    pub fn from_db(s: &str) -> Self {
        match s {
            "未対応" => Self::Open,
            "処理中" => Self::InProgress,
            "処理済み" => Self::Resolved,
            "完了" => Self::Closed,
            _ => Self::Open,
        }
    }

    /// 終了状態かどうか
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Closed)
    }

    /// 現在のステータスから遷移可能な一覧
    pub fn transitions(&self) -> Vec<TicketStatus> {
        match self {
            Self::Open => vec![Self::InProgress, Self::Closed],
            Self::InProgress => vec![Self::Resolved, Self::Open, Self::Closed],
            Self::Resolved => vec![Self::Closed, Self::InProgress],
            Self::Closed => vec![Self::InProgress, Self::Open],
        }
    }

    /// 遷移可能かチェック
    pub fn can_transition_to(&self, target: &TicketStatus) -> bool {
        self.transitions().contains(target)
    }

    /// 全ステータスのリスト
    pub fn all() -> Vec<TicketStatus> {
        vec![Self::Open, Self::InProgress, Self::Resolved, Self::Closed]
    }

    /// 進捗率（バーンダウン用）
    pub fn progress_pct(&self) -> i32 {
        match self {
            Self::Open => 0,
            Self::InProgress => 50,
            Self::Resolved => 80,
            Self::Closed => 100,
        }
    }
}

impl std::fmt::Display for TicketStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

// ---------------------------------------------------------------------------
// Priority — 優先度
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Priority {
    High,
    Medium,
    Low,
}

impl Priority {
    pub fn label(&self) -> &str {
        match self {
            Self::High => "高",
            Self::Medium => "中",
            Self::Low => "低",
        }
    }

    pub fn from_db(s: &str) -> Self {
        match s {
            "高" => Self::High,
            "中" => Self::Medium,
            "低" => Self::Low,
            _ => Self::Medium,
        }
    }

    pub fn sort_order(&self) -> i32 {
        match self {
            Self::High => 1,
            Self::Medium => 2,
            Self::Low => 3,
        }
    }

    pub fn all() -> Vec<Priority> {
        vec![Self::High, Self::Medium, Self::Low]
    }
}

impl std::fmt::Display for Priority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

// ---------------------------------------------------------------------------
// TicketType — チケット種別
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TicketType {
    Bug,
    Issue,
    Task,
    Qa,
}

impl TicketType {
    pub fn label(&self) -> &str {
        match self {
            Self::Bug => "不具合",
            Self::Issue => "課題",
            Self::Task => "作業依頼",
            Self::Qa => "QA",
        }
    }

    pub fn from_db(s: &str) -> Self {
        match s {
            "不具合" => Self::Bug,
            "課題" => Self::Issue,
            "作業依頼" => Self::Task,
            "QA" => Self::Qa,
            _ => Self::Issue,
        }
    }

    pub fn icon(&self) -> &str {
        match self {
            Self::Bug => "🐛",
            Self::Issue => "📋",
            Self::Task => "📝",
            Self::Qa => "🔍",
        }
    }

    pub fn all() -> Vec<TicketType> {
        vec![Self::Bug, Self::Issue, Self::Task, Self::Qa]
    }
}

impl std::fmt::Display for TicketType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

// ---------------------------------------------------------------------------
// Ticket — チケットエンティティ
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Ticket {
    pub id: i32,
    pub ticket_key: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub priority: String,
    pub ticket_type: String,
    pub parent_id: Option<i32>,
    pub category_id: Option<i32>,
    pub project_id: Option<i32>,
    pub author_id: i32,
    pub assignee_id: Option<i32>,
    pub milestone_id: Option<i32>,
    pub start_date: Option<NaiveDate>,
    pub due_date: Option<NaiveDate>,
    pub gantt_order: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    // 結合用フィールド（select_related 相当）
    pub author_name: Option<String>,
    pub assignee_name: Option<String>,
    pub category_name: Option<String>,
    pub phase_name: Option<String>,
    pub milestone_name: Option<String>,
    pub project_name: Option<String>,
    pub project_prefix: Option<String>,
    pub parent_key: Option<String>,
}

impl Ticket {
    /// ステータス enum を取得
    pub fn status_enum(&self) -> TicketStatus {
        TicketStatus::from_db(&self.status)
    }

    /// 優先度 enum を取得
    pub fn priority_enum(&self) -> Priority {
        Priority::from_db(&self.priority)
    }

    /// 種別 enum を取得
    pub fn ticket_type_enum(&self) -> TicketType {
        TicketType::from_db(&self.ticket_type)
    }

    /// 期限超過かどうか
    pub fn is_overdue(&self) -> bool {
        if self.status_enum().is_terminal() {
            return false;
        }
        match self.due_date {
            Some(due) => due < chrono::Local::now().date_naive(),
            None => false,
        }
    }
}

// ---------------------------------------------------------------------------
// TicketStatusHistory — ステータス変更履歴
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TicketStatusHistory {
    pub id: i32,
    pub ticket_id: i32,
    pub old_status: String,
    pub new_status: String,
    pub changed_by_id: Option<i32>,
    pub changed_at: DateTime<Utc>,
    // 結合用
    pub changed_by_name: Option<String>,
    pub ticket_key: Option<String>,
    pub ticket_title: Option<String>,
}
