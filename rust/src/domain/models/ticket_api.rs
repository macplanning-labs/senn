/// domain/models/ticket_api.rs — JSON API (Phase 2, /api/v1/tickets/*) 専用のチケット表現
///
/// 既存の `domain::models::ticket::Ticket` と `ticket_repo.rs` の既存関数は
/// 旧プロトタイプスキーマ前提のHTMLルート(handlers/tickets.rs, gantt.rs,
/// burndown_service.rs等、11ファイルが依存)が引き続き使用するため変更しない。
/// JSON APIはDjangoの実スキーマ(tickets_ticket等)に対して別の型・別の関数
/// (ticket_repo.rsに`api_`プレフィックスで追加)を新設して対応する。
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

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
    pub project: Option<i32>,
    #[serde(rename = "teamId")]
    pub team_id: Option<i32>,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    pub description: Option<String>,
    pub category: Option<String>,
    #[serde(rename = "isAiEnabled")]
    pub is_ai_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub reviewers: Vec<UserSummaryOut>,
    pub author: UserSummaryOut,
    pub category: Option<CategoryOut>,
    pub milestone: Option<MilestoneOut>,
    pub project: Option<i32>,
    #[serde(rename = "projectPrefix")]
    pub project_prefix: Option<String>,
    #[serde(rename = "projectName")]
    pub project_name: Option<String>,
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
    pub team: Option<TeamSummaryOut>,
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
    /// AIエージェント経由(人ごとキー認証成功時)の実際の実行者。
    /// フロントはこれが Some の場合、投稿者表示を author ではなくこちらを主表示にする
    /// (WIPAPPDEV-000100)。人間の通常投稿・共有キー経由の投稿では None。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "actingUser")]
    pub acting_user: Option<UserSummaryOut>,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "updatedAt")]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "anchorStart")]
    pub anchor_start: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "anchorEnd")]
    pub anchor_end: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "anchorQuote")]
    pub anchor_quote: Option<String>,
    #[serde(rename = "parentCommentId")]
    pub parent_comment_id: Option<i32>,
    #[serde(rename = "isDeleted")]
    pub is_deleted: bool,
    #[serde(rename = "replyCount")]
    pub reply_count: i32,
    #[serde(rename = "canEdit")]
    pub can_edit: bool,
    #[serde(rename = "canDelete")]
    pub can_delete: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AttachmentOut {
    pub id: i32,
    pub filename: String,
    #[serde(rename = "fileSize")]
    pub file_size: i32,
    #[serde(rename = "sizeDisplay")]
    pub size_display: String,
    #[serde(rename = "isImage")]
    pub is_image: bool,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    pub uploader: UserSummaryOut,
    #[serde(rename = "fileUrl")]
    pub file_url: String,
    #[serde(rename = "commentId")]
    pub comment_id: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TicketLinkOut {
    pub id: i32,
    pub url: String,
    pub title: Option<String>,
    #[serde(rename = "createdBy")]
    pub created_by: UserSummaryOut,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TicketDetailOut {
    #[serde(flatten)]
    pub base: TicketListOut,
    pub description: String,
    pub comments: Vec<CommentOut>,
    pub attachments: Vec<AttachmentOut>,
    pub links: Vec<TicketLinkOut>,
    #[serde(rename = "closedAt")]
    pub closed_at: Option<DateTime<Utc>>,
    #[serde(rename = "linkedRules")]
    pub linked_rules: Vec<TeamRuleSummaryOut>,
    #[serde(rename = "linkedWikiPages")]
    pub linked_wiki_pages: Vec<LinkedWikiPageOut>,
    #[serde(rename = "isWatching")]
    pub is_watching: bool,
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
    #[serde(default)]
    pub reviewers: Vec<i32>,
    pub category: Option<i32>,
    pub project: Option<i32>,
    pub milestone: Option<i32>,
    pub parent: Option<i32>,
    pub start_date: Option<NaiveDate>,
    pub due_date: Option<NaiveDate>,
    #[serde(default)]
    pub labels: Vec<i32>,
    pub story_points: Option<i16>,
    pub cycle: Option<i32>,
    #[serde(default)]
    #[serde(alias = "teamId")]
    pub team_id: Option<i32>,
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

// ---------------------------------------------------------------------------
// 部分更新リクエスト(詳細パネルからのインライン編集。指定したフィールドのみ更新)
// ---------------------------------------------------------------------------

/// JSONキーが存在する場合のみ Some(...) に包む(存在しない場合は #[serde(default)] で None)。
/// story_points/due_date のような「nullを送って明示的にクリアする」フィールドで、
/// 「キー自体が無い(=触らない)」と「値がnull(=クリアする)」を区別するために使う。
fn deserialize_present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    serde::Deserialize::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct TicketPatchIn {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub ticket_type: Option<String>,
    /// 内側のOptionはDB上の値(NULL可)、外側のOptionは「このリクエストで指定されたか」。
    /// TicketForm.tsx(編集フォーム)は全項目を毎回PATCHで送るため、詳細パネルの
    /// 単項目インライン編集(部分送信)と両方を同じエンドポイントで正しく扱う必要がある。
    #[serde(default, deserialize_with = "deserialize_present")]
    pub category: Option<Option<i32>>,
    #[serde(default, deserialize_with = "deserialize_present")]
    pub milestone: Option<Option<i32>>,
    #[serde(default, deserialize_with = "deserialize_present")]
    pub parent: Option<Option<i32>>,
    #[serde(default, deserialize_with = "deserialize_present")]
    pub cycle: Option<Option<i32>>,
    #[serde(default, deserialize_with = "deserialize_present")]
    #[serde(alias = "teamId")]
    pub team_id: Option<Option<i32>>,
    #[serde(default, deserialize_with = "deserialize_present")]
    pub start_date: Option<Option<NaiveDate>>,
    #[serde(default, deserialize_with = "deserialize_present")]
    pub due_date: Option<Option<NaiveDate>>,
    #[serde(default, deserialize_with = "deserialize_present")]
    pub story_points: Option<Option<i16>>,
    /// 空配列を送ると全解除、キー自体が無ければ触らない
    #[serde(default)]
    pub assignees: Option<Vec<i32>>,
    #[serde(default)]
    pub reviewers: Option<Vec<i32>>,
    #[serde(default)]
    pub labels: Option<Vec<i32>>,
    #[serde(default)]
    pub linked_rules: Option<Vec<i32>>,
}

// ---------------------------------------------------------------------------
// バルクインポート用リクエスト (Markdownインポート等)
// ---------------------------------------------------------------------------

/// 個別チケット (バルクインポート用 - ticket_typeなしの簡略版)
#[derive(Debug, Clone, serde::Deserialize)]
pub struct BulkImportTicketIn {
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_backlog_status")]
    pub status: String,
    #[serde(default = "default_priority")]
    pub priority: String,
    pub project: i32,
}

fn default_backlog_status() -> String {
    "backlog".to_string()
}

/// バルクインポートリクエスト
#[derive(Debug, Clone, serde::Deserialize)]
pub struct BulkImportIn {
    pub tickets: Vec<BulkImportTicketIn>,
}

/// バルクインポート結果
#[derive(Debug, Clone, serde::Serialize)]
pub struct BulkImportOut {
    pub imported: i32,
    pub errors: Vec<BulkImportError>,
}

/// バルクインポート時のエラー情報
#[derive(Debug, Clone, serde::Serialize)]
pub struct BulkImportError {
    pub index: usize,
    pub error: String,
}
