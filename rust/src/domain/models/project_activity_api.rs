/// domain/models/project_activity_api.rs — プロジェクト Activity / 進捗報告 JSON API モデル
///
/// project_activity(監査ログ)と project_updates(進捗報告)。
/// 設計: docs/詳細設計書_プロジェクト詳細タブ.md

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// 進捗報告の健全性(project_updates.health の CHECK 制約と一致させる)
pub const HEALTH_VALUES: [&str; 3] = ["on_track", "at_risk", "off_track"];

/// 進捗報告の本文の最大文字数
pub const UPDATE_BODY_MAX_CHARS: usize = 5000;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ProjectActivityOut {
    pub id: i64,
    #[serde(rename = "eventType")]
    pub event_type: String,
    pub payload: JsonValue,
    #[serde(rename = "actorId")]
    pub actor_id: Option<i64>,
    #[serde(rename = "actorName")]
    pub actor_name: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
}

/// カーソルページング。`next` が null なら最後のページ。
/// 次のページは `?before=<next>` で取得する。
#[derive(Debug, Clone, Serialize)]
pub struct ProjectActivityPage {
    pub results: Vec<ProjectActivityOut>,
    pub next: Option<i64>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ProjectUpdateOut {
    pub id: i64,
    #[serde(rename = "projectId")]
    pub project_id: i64,
    pub health: String,
    pub body: String,
    #[serde(rename = "authorId")]
    pub author_id: Option<i64>,
    #[serde(rename = "authorName")]
    pub author_name: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "updatedAt")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectUpdatePage {
    pub results: Vec<ProjectUpdateOut>,
    pub next: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectUpdateIn {
    pub health: String,
    #[serde(default)]
    pub body: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PageQuery {
    pub limit: Option<i64>,
    pub before: Option<i64>,
    /// 進捗報告のみ: 健全性での絞り込み
    pub health: Option<String>,
}
