/// domain/models/ticket_api.rs — JSON API (Phase 2, /api/v1/tickets/*) 専用のチケット表現
///
/// 既存の `domain::models::ticket::Ticket` と `ticket_repo.rs` の既存関数は
/// 旧プロトタイプスキーマ前提のHTMLルート(handlers/tickets.rs, gantt.rs,
/// burndown_service.rs等、11ファイルが依存)が引き続き使用するため変更しない。
/// JSON APIはDjangoの実スキーマ(tickets_ticket等)に対して別の型・別の関数
/// (ticket_repo.rsに`api_`プレフィックスで追加)を新設して対応する。

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;

// ---------------------------------------------------------------------------
// FK参照サマリ(一覧・詳細で共通)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct UserSummaryOut {
    pub id: i32,
    pub username: String,
    pub email: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CategoryOut {
    pub id: i32,
    pub name: String,
    pub slug: String,
    pub level: i16,
    pub parent: Option<i32>,
    pub sort_order: i32,
    pub color: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneOut {
    pub id: i32,
    pub name: String,
    #[serde(rename = "dueDate")]
    pub due_date: Option<NaiveDate>,
    pub description: String,
    pub project: Option<i32>,
    #[serde(rename = "openTicketCount")]
    pub open_ticket_count: i64,
    #[serde(rename = "closedTicketCount")]
    pub closed_ticket_count: i64,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LabelOut {
    pub id: i32,
    pub name: String,
    pub color: String,
    pub project: i32,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TeamSummaryOut {
    pub id: i32,
    pub name: String,
    pub slug: String,
    pub icon: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TeamRuleSummaryOut {
    pub id: i32,
    pub title: String,
    pub category: String,
    #[serde(rename = "teamName")]
    pub team_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LinkedWikiPageOut {
    pub id: i32,
    pub title: String,
    pub slug: String,
    pub category: String,
}

// ---------------------------------------------------------------------------
// チケット一覧・詳細レスポンス
//
// apps/api/serializers.py の TicketListSerializer / TicketDetailSerializer と
// 厳密にフィールド名を一致させること(camelCase / gantt_order等の例外あり)。
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct TicketListOut {
    pub id: i32,
    #[serde(rename = "ticketKey")]
    pub ticket_key: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    #[serde(rename = "ticketType")]
    pub ticket_type: String,
    pub assignees: Vec<UserSummaryOut>,
    pub author: UserSummaryOut,
    pub category: Option<CategoryOut>,
    pub milestone: Option<MilestoneOut>,
    pub project: i32,
    pub parent: Option<i32>,
    pub labels: Vec<LabelOut>,
    #[serde(rename = "startDate")]
    pub start_date: Option<NaiveDate>,
    #[serde(rename = "dueDate")]
    pub due_date: Option<NaiveDate>,
    #[serde(rename = "storyPoints")]
    pub story_points: Option<i16>,
    pub cycle: Option<i32>,
    #[serde(rename = "cycleName")]
    pub cycle_name: Option<String>,
    #[serde(rename = "assignedTeam")]
    pub assigned_team: Option<TeamSummaryOut>,
    #[serde(rename = "commentCount")]
    pub comment_count: i64,
    #[serde(rename = "childCount")]
    pub child_count: i64,
    #[serde(rename = "totalTimeSpent")]
    pub total_time_spent: i64,
    // gantt_orderはDjango側もsnake_caseのまま(serializers.pyで明示的にrenameされていない)
    pub gantt_order: i32,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "updatedAt")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommentOut {
    pub id: i32,
    pub body: String,
    pub author: UserSummaryOut,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TicketDetailOut {
    #[serde(flatten)]
    pub base: TicketListOut,
    pub description: String,
    pub comments: Vec<CommentOut>,
    #[serde(rename = "closedAt")]
    pub closed_at: Option<DateTime<Utc>>,
    #[serde(rename = "linkedRules")]
    pub linked_rules: Vec<TeamRuleSummaryOut>,
    #[serde(rename = "linkedWikiPages")]
    pub linked_wiki_pages: Vec<LinkedWikiPageOut>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChangeLogOut {
    pub id: i32,
    #[serde(rename = "fieldName")]
    pub field_name: String,
    #[serde(rename = "oldValue")]
    pub old_value: String,
    #[serde(rename = "newValue")]
    pub new_value: String,
    #[serde(rename = "changedBy")]
    pub changed_by: Option<UserSummaryOut>,
    #[serde(rename = "changedAt")]
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PointHistoryOut {
    pub id: i32,
    // old_points/new_pointsはDjango側もsnake_caseのまま(serializers.pyで明示的に
    // renameされていない。TaskPointHistorySerializer参照)
    pub old_points: Option<i32>,
    pub new_points: Option<i32>,
    #[serde(rename = "changedBy")]
    pub changed_by: Option<UserSummaryOut>,
    pub reason: String,
    #[serde(rename = "changedAt")]
    pub changed_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// 作成・更新リクエスト(TicketCreateSerializer相当。snake_case受け付け)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Deserialize)]
pub struct TicketWriteIn {
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_status")]
    pub status: String,
    #[serde(default = "default_priority")]
    pub priority: String,
    pub ticket_type: String,
    #[serde(default)]
    pub assignees: Vec<i32>,
    pub category: Option<i32>,
    pub project: i32,
    pub milestone: Option<i32>,
    pub parent: Option<i32>,
    pub start_date: Option<NaiveDate>,
    pub due_date: Option<NaiveDate>,
    #[serde(default)]
    pub labels: Vec<i32>,
    pub story_points: Option<i16>,
    pub cycle: Option<i32>,
    pub assigned_team: Option<i32>,
    #[serde(default)]
    pub linked_rules: Vec<i32>,
}

fn default_status() -> String {
    "open".to_string()
}
fn default_priority() -> String {
    "medium".to_string()
}

/// story_pointsに許容されるフィボナッチ数列(apps/tickets/domain/value_objects.py FIBONACCI_POINTS)
pub const FIBONACCI_POINTS: [i16; 7] = [1, 2, 3, 5, 8, 13, 21];
