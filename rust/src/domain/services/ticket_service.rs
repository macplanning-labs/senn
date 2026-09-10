/// domain/services/ticket_service.rs — チケットビジネスロジック
///
/// チケットの CRUD、ステータス遷移、キー生成を集約。
/// ハンドラ層はこのサービスを経由してチケット操作を行う（DB直接操作禁止）。

use chrono::NaiveDate;
use sqlx::PgPool;

use crate::domain::models::ticket::{Ticket, TicketStatus};
use crate::infrastructure::repositories::{ticket_repo, history_repo};

/// チケット作成リクエスト
pub struct CreateTicketRequest {
    pub title: String,
    pub description: String,
    pub priority: String,
    pub ticket_type: String,
    pub parent_id: Option<i32>,
    pub category_id: Option<i32>,
    pub project_id: Option<i32>,
    pub assignee_id: Option<i32>,
    pub milestone_id: Option<i32>,
    pub start_date: Option<NaiveDate>,
    pub due_date: Option<NaiveDate>,
}

/// チケット作成（キー自動生成 + 初期ステータス履歴記録）
pub async fn create_ticket(
    pool: &PgPool,
    req: &CreateTicketRequest,
    author_id: i32,
    project_prefix: &str,
) -> anyhow::Result<i32> {
    // チケットキー生成
    let ticket_key = ticket_repo::generate_next_key(pool, project_prefix).await?;

    let status = TicketStatus::Open.label();

    let ticket_id = ticket_repo::create(
        pool,
        &ticket_key, &req.title, &req.description,
        status, &req.priority, &req.ticket_type,
        req.parent_id, req.category_id, req.project_id,
        author_id, req.assignee_id, req.milestone_id,
        req.start_date, req.due_date,
    ).await?;

    // 初期ステータス履歴
    history_repo::save(pool, ticket_id, "", status, author_id).await?;

    // 作成者をウォッチャーに追加
    ticket_repo::add_watcher(pool, ticket_id, author_id).await?;

    // 担当者もウォッチャーに追加
    if let Some(aid) = req.assignee_id {
        if aid != author_id {
            ticket_repo::add_watcher(pool, ticket_id, aid).await?;
        }
    }

    Ok(ticket_id)
}

/// 子チケットの進捗から親チケットの進捗率を計算
pub fn calc_parent_progress(children: &[Ticket]) -> i32 {
    if children.is_empty() {
        return 0;
    }
    let total: i32 = children.iter()
        .map(|c| TicketStatus::from_db(&c.status).progress_pct())
        .sum();
    total / children.len() as i32
}
