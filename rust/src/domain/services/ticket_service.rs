/// domain/services/ticket_service.rs — チケットビジネスロジック
///
/// チケットの CRUD、ステータス遷移、キー生成を集約。
/// ハンドラ層はこのサービスを経由してチケット操作を行う（DB直接操作禁止）。

use chrono::NaiveDate;
use sqlx::PgPool;

use crate::domain::models::ticket::{Ticket, TicketStatus, TicketStatusHistory};
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

/// チケット更新リクエスト
pub struct UpdateTicketRequest {
    pub title: String,
    pub description: String,
    pub priority: String,
    pub ticket_type: String,
    pub category_id: Option<i32>,
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

/// チケット更新
pub async fn update_ticket(
    pool: &PgPool,
    ticket_id: i32,
    req: &UpdateTicketRequest,
) -> anyhow::Result<()> {
    ticket_repo::update(
        pool, ticket_id,
        &req.title, &req.description,
        &req.priority, &req.ticket_type,
        req.category_id, req.assignee_id, req.milestone_id,
        req.start_date, req.due_date,
    ).await?;

    Ok(())
}

/// ステータス変更（遷移ルールチェック + 履歴記録）
pub async fn change_status(
    pool: &PgPool,
    ticket_id: i32,
    new_status_str: &str,
    changed_by_id: i32,
) -> anyhow::Result<()> {
    let ticket = ticket_repo::find_by_id(pool, ticket_id).await?
        .ok_or_else(|| anyhow::anyhow!("チケットが見つかりません: {}", ticket_id))?;

    let current = TicketStatus::from_db(&ticket.status);
    let new_status = TicketStatus::from_db(new_status_str);

    // 遷移ルールチェック
    if !current.can_transition_to(&new_status) {
        return Err(anyhow::anyhow!(
            "ステータスを {} から {} に変更できません",
            current.label(), new_status.label()
        ));
    }

    // DB 更新
    ticket_repo::update_status(pool, ticket_id, new_status.label()).await?;

    // 履歴記録
    history_repo::save(
        pool, ticket_id,
        current.label(), new_status.label(),
        changed_by_id,
    ).await?;

    Ok(())
}

/// チケット削除
pub async fn delete_ticket(pool: &PgPool, ticket_id: i32) -> anyhow::Result<()> {
    ticket_repo::delete(pool, ticket_id).await
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

/// ウォッチ切り替え（トグル）
pub async fn toggle_watch(
    pool: &PgPool,
    ticket_id: i32,
    user_id: i32,
) -> anyhow::Result<bool> {
    let watching = ticket_repo::is_watching(pool, ticket_id, user_id).await?;
    if watching {
        ticket_repo::remove_watcher(pool, ticket_id, user_id).await?;
        Ok(false)
    } else {
        ticket_repo::add_watcher(pool, ticket_id, user_id).await?;
        Ok(true)
    }
}
