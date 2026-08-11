/// domain/models/dependency_api.rs — タスク依存関係 JSON API モデル
///
/// t_task_dependency テーブル用。
///
/// ステップ0.5の方針により、Django側は数値ticket_idベースのURL
/// (/api/v1/tickets/{ticket_id}/dependencies/)だが、Rust側は他の
/// /api/v1/tickets/{ticket_key}/* と一貫させるためticket_keyベースの
/// URLにする(フロントエンドは現状この機能を未使用のため互換性の懸念なし)。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub use crate::domain::models::ticket_api::UserSummaryOut;

#[derive(Debug, Clone, Serialize)]
pub struct TaskDependencyOut {
    #[serde(rename = "fromTask")]
    pub from_task: i32,
    #[serde(rename = "fromTaskKey")]
    pub from_task_key: String,
    #[serde(rename = "fromTaskTitle")]
    pub from_task_title: String,
    #[serde(rename = "toTask")]
    pub to_task: i32,
    #[serde(rename = "toTaskKey")]
    pub to_task_key: String,
    #[serde(rename = "toTaskTitle")]
    pub to_task_title: String,
    pub id: i32,
    #[serde(rename = "dependencyType")]
    pub dependency_type: String,
    #[serde(rename = "createdBy")]
    pub created_by: Option<UserSummaryOut>,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TaskDependencyCreateIn {
    pub to_task: Option<i32>,
    #[serde(default = "default_dependency_type")]
    pub dependency_type: String,
}

fn default_dependency_type() -> String {
    "blocks".to_string()
}

#[derive(Debug, Clone, Serialize)]
pub struct DependencyGraphNodeOut {
    pub id: i32,
    #[serde(rename = "ticketKey")]
    pub ticket_key: String,
    pub title: String,
    pub status: String,
    #[serde(rename = "ticketType")]
    pub ticket_type: String,
    pub assignees: Vec<UserSummaryOut>,
    #[serde(rename = "storyPoints")]
    pub story_points: Option<i16>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DependencyGraphOut {
    pub nodes: Vec<DependencyGraphNodeOut>,
    pub edges: Vec<TaskDependencyOut>,
}
