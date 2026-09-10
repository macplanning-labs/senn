/// domain/models/workflow_status_api.rs — Workflow Status リソース表現
///
/// t_workflow_status テーブル用の JSON API モデル。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowStatusOut {
    pub id: i32,
    pub project: Option<i32>,
    #[serde(rename = "teamId")]
    pub team_id: Option<i32>,
    pub name: String,
    pub slug: String,
    pub category: String,
    pub color: String,
    pub position: i32,
    #[serde(rename = "isDefault")]
    pub is_default: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStatusWriteIn {
    #[serde(default)]
    pub project: Option<i32>,
    #[serde(default)]
    pub team_id: Option<i32>,
    pub name: String,
    pub slug: String,
    pub category: String,
    pub color: String,
    pub position: i32,
    pub is_default: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStatusUpdateIn {
    pub project: Option<i32>,
    #[serde(default)]
    pub team_id: Option<i32>,
    pub name: Option<String>,
    pub slug: Option<String>,
    pub category: Option<String>,
    pub color: Option<String>,
    pub position: Option<i32>,
    pub is_default: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReorderIn {
    pub order: Vec<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReorderOut {
    pub detail: String,
}
