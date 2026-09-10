/// domain/models/notification_api.rs — JSON API 用通知表現
///
/// Django /api/v1/notifications/* と互換性のある構造。
/// 既存の domain::models::notification::Notification (プロトタイプ) との
/// 区別のため、このファイルで API 専用の型を定義。

use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct NotificationOut {
    pub id: i32,
    pub category: String,
    pub title: String,
    pub message: String,
    #[serde(rename = "ticketKey")]
    pub ticket_key: Option<String>,
    #[serde(rename = "projectKey")]
    pub project_key: Option<String>,
    #[serde(rename = "teamSlug")]
    pub team_slug: Option<String>,
    #[serde(rename = "wikiTitle")]
    pub wiki_title: Option<String>,
    #[serde(rename = "isRead")]
    pub is_read: bool,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    pub ticket: Option<i32>,
    pub wiki_page: Option<i32>,
}
