/// infrastructure/repositories/ticket_repo.rs — チケット永続化
///
/// sqlx を使用した t_tickets テーブルの CRUD 操作。
/// JOINでユーザー名・カテゴリー名等を取得する。

use sqlx::PgPool;

use crate::domain::models::ticket::{Ticket, TicketStatusHistory};

/// チケットフィルタ条件
pub struct TicketFilter {
    pub keyword: Option<String>,
    pub status: Option<String>,
    pub priority: Option<String>,
    pub assignee_id: Option<i32>,
    pub phase_id: Option<i32>,
    pub category_id: Option<i32>,
    pub milestone_id: Option<i32>,
    pub project_id: Option<i32>,
    pub parent_id: Option<i32>,
}

impl Default for TicketFilter {
    fn default() -> Self {
        Self {
            keyword: None, status: None, priority: None,
            assignee_id: None, phase_id: None, category_id: None,
            milestone_id: None, project_id: None, parent_id: None,
        }
    }
}

/// チケット一覧取得（フィルタ付き）
pub async fn find_all(pool: &PgPool, filter: &TicketFilter) -> anyhow::Result<Vec<Ticket>> {
    let mut query = String::from(
        "SELECT t.id, t.ticket_key, t.title, t.description, t.status, t.priority,
                t.ticket_type, t.parent_id, t.category_id, t.project_id,
                t.author_id, t.assignee_id, t.milestone_id,
                t.start_date, t.due_date, t.gantt_order,
                t.created_at, t.updated_at, t.closed_at,
                au.display_name as author_name,
                asn.display_name as assignee_name,
                cat.name as category_name,
                phase.name as phase_name,
                ms.name as milestone_name,
                proj.name as project_name,
                proj.prefix as project_prefix,
                pt.ticket_key as parent_key
         FROM t_tickets t
         LEFT JOIN m_users au ON t.author_id = au.id
         LEFT JOIN m_users asn ON t.assignee_id = asn.id
         LEFT JOIN m_categories cat ON t.category_id = cat.id
         LEFT JOIN m_categories phase ON cat.parent_id = phase.id
         LEFT JOIN m_milestones ms ON t.milestone_id = ms.id
         LEFT JOIN m_projects proj ON t.project_id = proj.id
         LEFT JOIN t_tickets pt ON t.parent_id = pt.id
         WHERE 1=1"
    );

    if let Some(ref kw) = filter.keyword {
        if !kw.is_empty() {
            query.push_str(&format!(
                " AND (t.title ILIKE '%{}%' OR t.description ILIKE '%{}%' OR t.ticket_key ILIKE '%{}%')",
                kw.replace('\'', "''"), kw.replace('\'', "''"), kw.replace('\'', "''")
            ));
        }
    }
    if let Some(ref status) = filter.status {
        if !status.is_empty() {
            query.push_str(&format!(" AND t.status = '{}'", status.replace('\'', "''")));
        }
    }
    if let Some(ref priority) = filter.priority {
        if !priority.is_empty() {
            query.push_str(&format!(" AND t.priority = '{}'", priority.replace('\'', "''")));
        }
    }
    if let Some(aid) = filter.assignee_id {
        query.push_str(&format!(" AND t.assignee_id = {}", aid));
    }
    if let Some(pid) = filter.phase_id {
        query.push_str(&format!(" AND cat.parent_id = {}", pid));
    }
    if let Some(cid) = filter.category_id {
        query.push_str(&format!(" AND t.category_id = {}", cid));
    }
    if let Some(mid) = filter.milestone_id {
        query.push_str(&format!(" AND t.milestone_id = {}", mid));
    }
    if let Some(proj_id) = filter.project_id {
        query.push_str(&format!(" AND t.project_id = {}", proj_id));
    }

    query.push_str(" ORDER BY t.category_id, t.priority DESC, t.due_date NULLS LAST");

    let rows = sqlx::query_as::<_, Ticket>(&query)
        .fetch_all(pool)
        .await?;

    Ok(rows)
}

/// チケット1件取得（ID指定）
pub async fn find_by_id(pool: &PgPool, id: i32) -> anyhow::Result<Option<Ticket>> {
    let ticket = sqlx::query_as::<_, Ticket>(
        "SELECT t.id, t.ticket_key, t.title, t.description, t.status, t.priority,
                t.ticket_type, t.parent_id, t.category_id, t.project_id,
                t.author_id, t.assignee_id, t.milestone_id,
                t.start_date, t.due_date, t.gantt_order,
                t.created_at, t.updated_at, t.closed_at,
                au.display_name as author_name,
                asn.display_name as assignee_name,
                cat.name as category_name,
                phase.name as phase_name,
                ms.name as milestone_name,
                proj.name as project_name,
                proj.prefix as project_prefix,
                pt.ticket_key as parent_key
         FROM t_tickets t
         LEFT JOIN m_users au ON t.author_id = au.id
         LEFT JOIN m_users asn ON t.assignee_id = asn.id
         LEFT JOIN m_categories cat ON t.category_id = cat.id
         LEFT JOIN m_categories phase ON cat.parent_id = phase.id
         LEFT JOIN m_milestones ms ON t.milestone_id = ms.id
         LEFT JOIN m_projects proj ON t.project_id = proj.id
         LEFT JOIN t_tickets pt ON t.parent_id = pt.id
         WHERE t.id = $1"
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(ticket)
}

/// チケット1件取得（ticket_key 指定）
pub async fn find_by_key(pool: &PgPool, key: &str) -> anyhow::Result<Option<Ticket>> {
    let ticket = sqlx::query_as::<_, Ticket>(
        "SELECT t.id, t.ticket_key, t.title, t.description, t.status, t.priority,
                t.ticket_type, t.parent_id, t.category_id, t.project_id,
                t.author_id, t.assignee_id, t.milestone_id,
                t.start_date, t.due_date, t.gantt_order,
                t.created_at, t.updated_at, t.closed_at,
                au.display_name as author_name,
                asn.display_name as assignee_name,
                cat.name as category_name,
                phase.name as phase_name,
                ms.name as milestone_name,
                proj.name as project_name,
                proj.prefix as project_prefix,
                pt.ticket_key as parent_key
         FROM t_tickets t
         LEFT JOIN m_users au ON t.author_id = au.id
         LEFT JOIN m_users asn ON t.assignee_id = asn.id
         LEFT JOIN m_categories cat ON t.category_id = cat.id
         LEFT JOIN m_categories phase ON cat.parent_id = phase.id
         LEFT JOIN m_milestones ms ON t.milestone_id = ms.id
         LEFT JOIN m_projects proj ON t.project_id = proj.id
         LEFT JOIN t_tickets pt ON t.parent_id = pt.id
         WHERE t.ticket_key = $1"
    )
    .bind(key)
    .fetch_optional(pool)
    .await?;

    Ok(ticket)
}

/// チケット作成
pub async fn create(
    pool: &PgPool,
    ticket_key: &str, title: &str, description: &str,
    status: &str, priority: &str, ticket_type: &str,
    parent_id: Option<i32>, category_id: Option<i32>, project_id: Option<i32>,
    author_id: i32, assignee_id: Option<i32>, milestone_id: Option<i32>,
    start_date: Option<chrono::NaiveDate>, due_date: Option<chrono::NaiveDate>,
) -> anyhow::Result<i32> {
    let row = sqlx::query_scalar::<_, i32>(
        "INSERT INTO t_tickets (ticket_key, title, description, status, priority, ticket_type,
                                parent_id, category_id, project_id, author_id, assignee_id,
                                milestone_id, start_date, due_date)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
         RETURNING id"
    )
    .bind(ticket_key).bind(title).bind(description)
    .bind(status).bind(priority).bind(ticket_type)
    .bind(parent_id).bind(category_id).bind(project_id)
    .bind(author_id).bind(assignee_id).bind(milestone_id)
    .bind(start_date).bind(due_date)
    .fetch_one(pool)
    .await?;

    Ok(row)
}

/// チケット更新
pub async fn update(
    pool: &PgPool, id: i32,
    title: &str, description: &str,
    priority: &str, ticket_type: &str,
    category_id: Option<i32>, assignee_id: Option<i32>,
    milestone_id: Option<i32>,
    start_date: Option<chrono::NaiveDate>, due_date: Option<chrono::NaiveDate>,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE t_tickets SET title=$2, description=$3, priority=$4, ticket_type=$5,
                category_id=$6, assignee_id=$7, milestone_id=$8,
                start_date=$9, due_date=$10, updated_at=NOW()
         WHERE id=$1"
    )
    .bind(id).bind(title).bind(description)
    .bind(priority).bind(ticket_type)
    .bind(category_id).bind(assignee_id).bind(milestone_id)
    .bind(start_date).bind(due_date)
    .execute(pool)
    .await?;

    Ok(())
}

/// ステータス更新
pub async fn update_status(pool: &PgPool, id: i32, new_status: &str) -> anyhow::Result<()> {
    let closed_at = if new_status == "完了" {
        Some(chrono::Utc::now())
    } else {
        None
    };

    sqlx::query(
        "UPDATE t_tickets SET status=$2, closed_at=$3, updated_at=NOW() WHERE id=$1"
    )
    .bind(id).bind(new_status).bind(closed_at)
    .execute(pool)
    .await?;

    Ok(())
}

/// チケット削除
pub async fn delete(pool: &PgPool, id: i32) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM t_tickets WHERE id=$1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// 次のチケットキーを生成（例: WIP-000042）
pub async fn generate_next_key(pool: &PgPool, prefix: &str) -> anyhow::Result<String> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM t_tickets WHERE ticket_key LIKE $1"
    )
    .bind(format!("{}-%", prefix))
    .fetch_one(pool)
    .await?;

    Ok(format!("{}-{:06}", prefix, count + 1))
}

/// ステータス別件数
pub async fn count_by_status(pool: &PgPool, project_id: Option<i32>) -> anyhow::Result<Vec<(String, i64)>> {
    let rows: Vec<(String, i64)> = if let Some(pid) = project_id {
        sqlx::query_as(
            "SELECT status, COUNT(*) FROM t_tickets WHERE project_id=$1 GROUP BY status"
        )
        .bind(pid)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as(
            "SELECT status, COUNT(*) FROM t_tickets GROUP BY status"
        )
        .fetch_all(pool)
        .await?
    };

    Ok(rows)
}

/// ガント用: gantt_order 更新
pub async fn update_gantt_order(pool: &PgPool, id: i32, order: i32) -> anyhow::Result<()> {
    sqlx::query("UPDATE t_tickets SET gantt_order=$2 WHERE id=$1")
        .bind(id).bind(order)
        .execute(pool)
        .await?;
    Ok(())
}

/// ガント用: 日程更新
pub async fn update_dates(
    pool: &PgPool, id: i32,
    start_date: Option<chrono::NaiveDate>, due_date: Option<chrono::NaiveDate>,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE t_tickets SET start_date=$2, due_date=$3, updated_at=NOW() WHERE id=$1"
    )
    .bind(id).bind(start_date).bind(due_date)
    .execute(pool)
    .await?;
    Ok(())
}

/// 子チケット取得
pub async fn find_children(pool: &PgPool, parent_id: i32) -> anyhow::Result<Vec<Ticket>> {
    let tickets = sqlx::query_as::<_, Ticket>(
        "SELECT t.id, t.ticket_key, t.title, t.description, t.status, t.priority,
                t.ticket_type, t.parent_id, t.category_id, t.project_id,
                t.author_id, t.assignee_id, t.milestone_id,
                t.start_date, t.due_date, t.gantt_order,
                t.created_at, t.updated_at, t.closed_at,
                au.display_name as author_name,
                asn.display_name as assignee_name,
                cat.name as category_name,
                phase.name as phase_name,
                ms.name as milestone_name,
                proj.name as project_name,
                proj.prefix as project_prefix,
                pt.ticket_key as parent_key
         FROM t_tickets t
         LEFT JOIN m_users au ON t.author_id = au.id
         LEFT JOIN m_users asn ON t.assignee_id = asn.id
         LEFT JOIN m_categories cat ON t.category_id = cat.id
         LEFT JOIN m_categories phase ON cat.parent_id = phase.id
         LEFT JOIN m_milestones ms ON t.milestone_id = ms.id
         LEFT JOIN m_projects proj ON t.project_id = proj.id
         LEFT JOIN t_tickets pt ON t.parent_id = pt.id
         WHERE t.parent_id = $1
         ORDER BY t.gantt_order, t.id"
    )
    .bind(parent_id)
    .fetch_all(pool)
    .await?;

    Ok(tickets)
}

/// ウォッチ追加
pub async fn add_watcher(pool: &PgPool, ticket_id: i32, user_id: i32) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO t_ticket_watchers (ticket_id, user_id) VALUES ($1, $2)
         ON CONFLICT DO NOTHING"
    )
    .bind(ticket_id).bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// ウォッチ解除
pub async fn remove_watcher(pool: &PgPool, ticket_id: i32, user_id: i32) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM t_ticket_watchers WHERE ticket_id=$1 AND user_id=$2")
        .bind(ticket_id).bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// ウォッチしているか
pub async fn is_watching(pool: &PgPool, ticket_id: i32, user_id: i32) -> anyhow::Result<bool> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM t_ticket_watchers WHERE ticket_id=$1 AND user_id=$2)"
    )
    .bind(ticket_id).bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

/// ウォッチャー一覧（user_id のリスト）
pub async fn find_watchers(pool: &PgPool, ticket_id: i32) -> anyhow::Result<Vec<i32>> {
    let ids: Vec<(i32,)> = sqlx::query_as(
        "SELECT user_id FROM t_ticket_watchers WHERE ticket_id=$1"
    )
    .bind(ticket_id)
    .fetch_all(pool)
    .await?;
    Ok(ids.into_iter().map(|(id,)| id).collect())
}
