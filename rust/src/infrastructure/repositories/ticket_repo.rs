/// infrastructure/repositories/ticket_repo.rs — チケット永続化
///
/// sqlx を使用した t_tickets テーブルの CRUD 操作。
/// JOINでユーザー名・カテゴリー名等を取得する。

use sqlx::{PgPool, Row};
use chrono::{NaiveDate, Utc};

use crate::domain::models::ticket::{Ticket, TicketStatusHistory};
use crate::domain::models::ticket_api::*;

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

// =============================================================================
// JSON API 専用関数(Phase 2, /api/v1/tickets/*)
// API側の新規スキーマ(tickets_ticket等)を使用。既存関数とは別。
// =============================================================================

/// チケットフィルタ条件(JSON API用)
#[derive(Debug, Default)]
pub struct ApiTicketFilter {
    pub status: Option<Vec<String>>,
    pub priority: Option<Vec<String>>,
    pub assignees: Option<i32>,
    pub project: Option<i32>,
    pub project_prefix: Option<String>,
    pub milestone: Option<i32>,
    pub category: Option<i32>,
    pub labels: Option<i32>,
    pub parent: Option<i32>,
    pub parent_isnull: Option<bool>,
    pub due_date_gte: Option<NaiveDate>,
    pub due_date_lte: Option<NaiveDate>,
    pub due_date_isnull: Option<bool>,
}

/// チケット一覧取得(JSON API用)
pub async fn api_find_all(
    pool: &PgPool,
    filter: &ApiTicketFilter,
    sort: &str,
    search: Option<&str>,
    page: i64,
) -> anyhow::Result<Vec<TicketListOut>> {
    // DjangoのDEFAULT_PAGINATION_CLASS(PageNumberPagination, PAGE_SIZE=50)と一致させる
    const PAGE_SIZE: i64 = 50;
    let page = page.max(1);
    let offset = (page - 1) * PAGE_SIZE;
    // 標準化されたソート値
    let order_clause = match sort {
        "created_at" => "t.created_at ASC",
        "-created_at" => "t.created_at DESC",
        "updated_at" => "t.updated_at ASC",
        "-updated_at" => "t.updated_at DESC",
        "due_date" => "t.due_date ASC",
        "-due_date" => "t.due_date DESC",
        "priority" => "t.priority ASC",
        "-priority" => "t.priority DESC",
        "gantt_order" => "t.gantt_order ASC",
        "-gantt_order" => "t.gantt_order DESC",
        _ => "t.updated_at DESC", // デフォルト
    };

    // 基本クエリ：チケット+スカラー値
    let mut query = String::from(
        "SELECT
            t.id::int4, t.ticket_key, t.title, t.status, t.priority, t.ticket_type,
            t.author_id::int4, t.category_id::int4, t.project_id::int4, t.milestone_id::int4, t.parent_id::int4,
            t.start_date, t.due_date, t.story_points, t.cycle_id::int4, t.assigned_team_id::int4,
            t.gantt_order, t.created_at, t.updated_at,
            au.id::int4 as author_id_2, au.username as author_username, au.email as author_email, au.display_name as author_display_name,
            cat.id::int4 as cat_id, cat.name as cat_name, cat.slug as cat_slug, cat.level as cat_level, cat.parent_id::int4 as cat_parent, cat.sort_order as cat_sort, cat.color as cat_color,
            ms.id::int4 as ms_id, ms.name as ms_name, ms.due_date as ms_due_date, ms.description as ms_desc, ms.project_id::int4 as ms_project, ms.created_at as ms_created,
            proj.id::int4 as proj_id,
            tc.name as cycle_name,
            tm.id::int4 as team_id, tm.name as team_name, tm.slug as team_slug, tm.icon as team_icon, tm.color as team_color,
            (SELECT COUNT(*) FROM tickets_comment WHERE ticket_id = t.id) as comment_count,
            (SELECT COUNT(*) FROM tickets_ticket WHERE parent_id = t.id) as child_count,
            COALESCE((SELECT COUNT(*) FROM t_time_entry WHERE ticket_id = t.id), 0) as time_spent
         FROM tickets_ticket t
         LEFT JOIN accounts_user au ON t.author_id = au.id
         LEFT JOIN tickets_category cat ON t.category_id = cat.id
         LEFT JOIN milestones_milestone ms ON t.milestone_id = ms.id
         LEFT JOIN tickets_project proj ON t.project_id = proj.id
         LEFT JOIN t_cycle tc ON t.cycle_id = tc.id
         LEFT JOIN m_team tm ON t.assigned_team_id = tm.id
         WHERE 1=1"
    );

    // フィルタ条件を動的に追加
    let mut param_count = 1;

    // status フィルタ
    if let Some(ref statuses) = filter.status {
        if !statuses.is_empty() {
            if statuses.len() == 1 {
                query.push_str(&format!(" AND t.status = ${}", param_count));
                param_count += 1;
            } else {
                let placeholders = (0..statuses.len())
                    .map(|i| format!("${}", param_count + i))
                    .collect::<Vec<_>>()
                    .join(",");
                query.push_str(&format!(" AND t.status = ANY(ARRAY[{}]::text[])", placeholders));
                param_count += statuses.len();
            }
        }
    }

    // priority フィルタ
    if let Some(ref priorities) = filter.priority {
        if !priorities.is_empty() {
            if priorities.len() == 1 {
                query.push_str(&format!(" AND t.priority = ${}", param_count));
                param_count += 1;
            } else {
                let placeholders = (0..priorities.len())
                    .map(|i| format!("${}", param_count + i))
                    .collect::<Vec<_>>()
                    .join(",");
                query.push_str(&format!(" AND t.priority = ANY(ARRAY[{}]::text[])", placeholders));
                param_count += priorities.len();
            }
        }
    }

    // assignees フィルタ (EXISTS)
    if let Some(assignee_id) = filter.assignees {
        query.push_str(&format!(
            " AND EXISTS(SELECT 1 FROM tickets_ticket_assignees WHERE ticketmodel_id = t.id AND user_id = ${})",
            param_count
        ));
        param_count += 1;
    }

    // project フィルタ
    if let Some(proj_id) = filter.project {
        query.push_str(&format!(" AND t.project_id = ${}", param_count));
        param_count += 1;
    }

    // project__prefix フィルタ
    if let Some(ref prefix) = filter.project_prefix {
        query.push_str(&format!(
            " AND EXISTS(SELECT 1 FROM tickets_project WHERE id = t.project_id AND prefix = ${})",
            param_count
        ));
        param_count += 1;
    }

    // milestone フィルタ
    if let Some(ms_id) = filter.milestone {
        query.push_str(&format!(" AND t.milestone_id = ${}", param_count));
        param_count += 1;
    }

    // category フィルタ
    if let Some(cat_id) = filter.category {
        query.push_str(&format!(" AND t.category_id = ${}", param_count));
        param_count += 1;
    }

    // labels フィルタ (EXISTS)
    if let Some(label_id) = filter.labels {
        query.push_str(&format!(
            " AND EXISTS(SELECT 1 FROM tickets_ticket_labels WHERE ticketmodel_id = t.id AND labelmodel_id = ${})",
            param_count
        ));
        param_count += 1;
    }

    // parent フィルタ
    if let Some(parent_id) = filter.parent {
        query.push_str(&format!(" AND t.parent_id = ${}", param_count));
        param_count += 1;
    }

    // parent_isnull フィルタ
    if let Some(is_null) = filter.parent_isnull {
        if is_null {
            query.push_str(" AND t.parent_id IS NULL");
        } else {
            query.push_str(" AND t.parent_id IS NOT NULL");
        }
    }

    // due_date gte
    if let Some(due_gte) = filter.due_date_gte {
        query.push_str(&format!(" AND t.due_date >= ${}", param_count));
        param_count += 1;
    }

    // due_date lte
    if let Some(due_lte) = filter.due_date_lte {
        query.push_str(&format!(" AND t.due_date <= ${}", param_count));
        param_count += 1;
    }

    // due_date_isnull
    if let Some(is_null) = filter.due_date_isnull {
        if is_null {
            query.push_str(" AND t.due_date IS NULL");
        } else {
            query.push_str(" AND t.due_date IS NOT NULL");
        }
    }

    // 全文検索
    if let Some(search_term) = search {
        if !search_term.is_empty() {
            let search_pattern = format!("%{}%", search_term);
            query.push_str(&format!(
                " AND (t.title ILIKE ${} OR t.description ILIKE ${} OR t.ticket_key ILIKE ${})",
                param_count,
                param_count + 1,
                param_count + 2
            ));
            param_count += 3;
        }
    }

    // ソート追加 + ページネーション(Django PageNumberPagination相当)
    query.push_str(&format!(
        " ORDER BY {} LIMIT ${} OFFSET ${}",
        order_clause,
        param_count,
        param_count + 1
    ));

    // パラメータをバインド
    let mut sql_query = sqlx::query(&query);

    if let Some(ref statuses) = filter.status {
        for status in statuses {
            sql_query = sql_query.bind(status.as_str());
        }
    }
    if let Some(ref priorities) = filter.priority {
        for priority in priorities {
            sql_query = sql_query.bind(priority.as_str());
        }
    }
    if let Some(assignee_id) = filter.assignees {
        sql_query = sql_query.bind(assignee_id);
    }
    if let Some(proj_id) = filter.project {
        sql_query = sql_query.bind(proj_id);
    }
    if let Some(ref prefix) = filter.project_prefix {
        sql_query = sql_query.bind(prefix.as_str());
    }
    if let Some(ms_id) = filter.milestone {
        sql_query = sql_query.bind(ms_id);
    }
    if let Some(cat_id) = filter.category {
        sql_query = sql_query.bind(cat_id);
    }
    if let Some(label_id) = filter.labels {
        sql_query = sql_query.bind(label_id);
    }
    if let Some(parent_id) = filter.parent {
        sql_query = sql_query.bind(parent_id);
    }
    if let Some(due_gte) = filter.due_date_gte {
        sql_query = sql_query.bind(due_gte);
    }
    if let Some(due_lte) = filter.due_date_lte {
        sql_query = sql_query.bind(due_lte);
    }
    if let Some(search_term) = search {
        if !search_term.is_empty() {
            let pattern = format!("%{}%", search_term);
            sql_query = sql_query.bind(pattern.clone());
            sql_query = sql_query.bind(pattern.clone());
            sql_query = sql_query.bind(pattern);
        }
    }
    sql_query = sql_query.bind(PAGE_SIZE).bind(offset);

    let rows = sql_query.fetch_all(pool).await?;

    // ticket_id のリストを集める
    let ticket_ids: Vec<i32> = rows.iter().map(|row| row.get::<i32, _>(0)).collect();

    // assignees をまとめて取得
    let assignees_map: std::collections::HashMap<i32, Vec<UserSummaryOut>> =
        if !ticket_ids.is_empty() {
            let assignees_rows = sqlx::query(
                "SELECT ta.ticketmodel_id::int4, u.id::int4, u.username, u.email, u.display_name
                 FROM tickets_ticket_assignees ta
                 JOIN accounts_user u ON ta.user_id = u.id
                 WHERE ta.ticketmodel_id = ANY($1)
                 ORDER BY u.id"
            )
            .bind(&ticket_ids)
            .fetch_all(pool)
            .await?;

            let mut map: std::collections::HashMap<i32, Vec<UserSummaryOut>> =
                std::collections::HashMap::new();
            for row in assignees_rows {
                let ticket_id: i32 = row.get(0);
                let user = UserSummaryOut {
                    id: row.get(1),
                    username: row.get(2),
                    email: row.get(3),
                    display_name: row.get(4),
                };
                map.entry(ticket_id).or_insert_with(Vec::new).push(user);
            }
            map
        } else {
            std::collections::HashMap::new()
        };

    // labels をまとめて取得
    let labels_map: std::collections::HashMap<i32, Vec<LabelOut>> =
        if !ticket_ids.is_empty() {
            let labels_rows = sqlx::query(
                "SELECT tl.ticketmodel_id::int4, l.id::int4, l.name, l.color, l.project_id::int4, l.created_at, l.description, l.category, l.is_ai_enabled
                 FROM tickets_ticket_labels tl
                 JOIN m_label l ON tl.labelmodel_id = l.id
                 WHERE tl.ticketmodel_id = ANY($1)
                 ORDER BY l.id"
            )
            .bind(&ticket_ids)
            .fetch_all(pool)
            .await?;

            let mut map: std::collections::HashMap<i32, Vec<LabelOut>> =
                std::collections::HashMap::new();
            for row in labels_rows {
                let ticket_id: i32 = row.get(0);
                let label = LabelOut {
                    id: row.get(1),
                    name: row.get(2),
                    color: row.get(3),
                    project: row.get(4),
                    created_at: row.get(5),
                    description: row.get(6),
                    category: row.get(7),
                    is_ai_enabled: row.get(8),
                };
                map.entry(ticket_id).or_insert_with(Vec::new).push(label);
            }
            map
        } else {
            std::collections::HashMap::new()
        };

    // milestones のカウント取得
    let milestone_counts_map: std::collections::HashMap<
        i32,
        (i64, i64),
    > = if !ticket_ids.is_empty() {
        let milestone_ids: Vec<i32> = rows
            .iter()
            .filter_map(|row| row.get::<Option<i32>, _>(14))
            .collect();

        if !milestone_ids.is_empty() {
            let counts_rows = sqlx::query(
                "SELECT milestone_id::int4,
                        COUNT(CASE WHEN status != 'closed' THEN 1 END) as open_count,
                        COUNT(CASE WHEN status = 'closed' THEN 1 END) as closed_count
                 FROM tickets_ticket
                 WHERE milestone_id = ANY($1)
                 GROUP BY milestone_id"
            )
            .bind(&milestone_ids)
            .fetch_all(pool)
            .await?;

            let mut map: std::collections::HashMap<i32, (i64, i64)> =
                std::collections::HashMap::new();
            for row in counts_rows {
                let ms_id: i32 = row.get(0);
                let open: i64 = row.get(1);
                let closed: i64 = row.get(2);
                map.insert(ms_id, (open, closed));
            }
            map
        } else {
            std::collections::HashMap::new()
        }
    } else {
        std::collections::HashMap::new()
    };

    // レスポンス構築
    let result = rows
        .into_iter()
        .map(|row| {
            let ticket_id: i32 = row.get(0);
            let ticket_key: String = row.get(1);
            let title: String = row.get(2);
            let status: String = row.get(3);
            let priority: String = row.get(4);
            let ticket_type: String = row.get(5);

            let author_id: i32 = row.get(6);
            let author_username: String = row.get(20);
            let author_email: String = row.get(21);
            let author_display_name: String = row.get(22);

            let category_id_opt: Option<i32> = row.get(7);
            let milestone_id_opt: Option<i32> = row.get(9);
            let project_id: i32 = row.get(8);
            let parent_id: Option<i32> = row.get(10);

            let start_date: Option<NaiveDate> = row.get(11);
            let due_date: Option<NaiveDate> = row.get(12);
            let story_points: Option<i16> = row.get(13);
            let cycle_id: Option<i32> = row.get(14);
            let assigned_team_id_opt: Option<i32> = row.get(15);

            let gantt_order: i32 = row.get(16);
            let created_at: chrono::DateTime<chrono::Utc> = row.get(17);
            let updated_at: chrono::DateTime<chrono::Utc> = row.get(18);

            let comment_count: i64 = row.get(43);
            let child_count: i64 = row.get(44);
            let total_time_spent: i64 = row.get(45);

            // Author
            let author = UserSummaryOut {
                id: author_id,
                username: author_username,
                email: author_email,
                display_name: author_display_name,
            };

            // Category
            let category = if let Some(cat_id) = category_id_opt {
                row.get::<Option<i32>, _>(23).and_then(|_| {
                    Some(CategoryOut {
                        id: cat_id,
                        name: row.get(24),
                        slug: row.get(25),
                        level: row.get(26),
                        parent: row.get(27),
                        sort_order: row.get(28),
                        color: row.get(29),
                    })
                })
            } else {
                None
            };

            // Milestone
            let milestone = if let Some(ms_id) = milestone_id_opt {
                row.get::<Option<i32>, _>(30).and_then(|_| {
                    let (open_count, closed_count) = milestone_counts_map
                        .get(&ms_id)
                        .copied()
                        .unwrap_or((0, 0));

                    Some(MilestoneOut {
                        id: ms_id,
                        name: row.get(31),
                        due_date: row.get(32),
                        description: row.get(33),
                        project: row.get(34),
                        open_ticket_count: open_count,
                        closed_ticket_count: closed_count,
                        created_at: row.get(35),
                    })
                })
            } else {
                None
            };

            // Cycle name
            let cycle_name: Option<String> = row.get(37);

            // Assigned Team
            let assigned_team = if let Some(team_id) = assigned_team_id_opt {
                row.get::<Option<i32>, _>(38).and_then(|_| {
                    Some(TeamSummaryOut {
                        id: team_id,
                        name: row.get(39),
                        slug: row.get(40),
                        icon: row.get(41),
                        color: row.get(42),
                    })
                })
            } else {
                None
            };

            // Assignees
            let assignees = assignees_map
                .get(&ticket_id)
                .cloned()
                .unwrap_or_default();

            // Labels
            let labels = labels_map.get(&ticket_id).cloned().unwrap_or_default();

            TicketListOut {
                id: ticket_id,
                ticket_key,
                title,
                status,
                priority,
                ticket_type,
                assignees,
                author,
                category,
                milestone,
                project: project_id,
                parent: parent_id,
                labels,
                start_date,
                due_date,
                story_points,
                cycle: cycle_id,
                cycle_name,
                assigned_team,
                comment_count,
                child_count,
                total_time_spent,
                gantt_order,
                created_at,
                updated_at,
            }
        })
        .collect();

    Ok(result)
}

/// チケット件数取得(JSON API一覧用。api_find_allと同じフィルタ条件をLIMIT/OFFSET無しで
/// 数えるだけ。DjangoのPageNumberPaginationのcountフィールドと一致させるために必要)
pub async fn api_count_all(
    pool: &PgPool,
    filter: &ApiTicketFilter,
    search: Option<&str>,
) -> anyhow::Result<i64> {
    let mut query = String::from("SELECT COUNT(*) FROM tickets_ticket t WHERE 1=1");
    let mut param_count = 1;

    if let Some(ref statuses) = filter.status {
        if !statuses.is_empty() {
            if statuses.len() == 1 {
                query.push_str(&format!(" AND t.status = ${}", param_count));
                param_count += 1;
            } else {
                let placeholders = (0..statuses.len())
                    .map(|i| format!("${}", param_count + i))
                    .collect::<Vec<_>>()
                    .join(",");
                query.push_str(&format!(" AND t.status = ANY(ARRAY[{}]::text[])", placeholders));
                param_count += statuses.len();
            }
        }
    }
    if let Some(ref priorities) = filter.priority {
        if !priorities.is_empty() {
            if priorities.len() == 1 {
                query.push_str(&format!(" AND t.priority = ${}", param_count));
                param_count += 1;
            } else {
                let placeholders = (0..priorities.len())
                    .map(|i| format!("${}", param_count + i))
                    .collect::<Vec<_>>()
                    .join(",");
                query.push_str(&format!(" AND t.priority = ANY(ARRAY[{}]::text[])", placeholders));
                param_count += priorities.len();
            }
        }
    }
    if filter.assignees.is_some() {
        query.push_str(&format!(
            " AND EXISTS(SELECT 1 FROM tickets_ticket_assignees WHERE ticketmodel_id = t.id AND user_id = ${})",
            param_count
        ));
        param_count += 1;
    }
    if filter.project.is_some() {
        query.push_str(&format!(" AND t.project_id = ${}", param_count));
        param_count += 1;
    }
    if filter.project_prefix.is_some() {
        query.push_str(&format!(
            " AND EXISTS(SELECT 1 FROM tickets_project WHERE id = t.project_id AND prefix = ${})",
            param_count
        ));
        param_count += 1;
    }
    if filter.milestone.is_some() {
        query.push_str(&format!(" AND t.milestone_id = ${}", param_count));
        param_count += 1;
    }
    if filter.category.is_some() {
        query.push_str(&format!(" AND t.category_id = ${}", param_count));
        param_count += 1;
    }
    if filter.labels.is_some() {
        query.push_str(&format!(
            " AND EXISTS(SELECT 1 FROM tickets_ticket_labels WHERE ticketmodel_id = t.id AND labelmodel_id = ${})",
            param_count
        ));
        param_count += 1;
    }
    if filter.parent.is_some() {
        query.push_str(&format!(" AND t.parent_id = ${}", param_count));
        param_count += 1;
    }
    if let Some(is_null) = filter.parent_isnull {
        query.push_str(if is_null { " AND t.parent_id IS NULL" } else { " AND t.parent_id IS NOT NULL" });
    }
    if filter.due_date_gte.is_some() {
        query.push_str(&format!(" AND t.due_date >= ${}", param_count));
        param_count += 1;
    }
    if filter.due_date_lte.is_some() {
        query.push_str(&format!(" AND t.due_date <= ${}", param_count));
        param_count += 1;
    }
    if let Some(is_null) = filter.due_date_isnull {
        query.push_str(if is_null { " AND t.due_date IS NULL" } else { " AND t.due_date IS NOT NULL" });
    }
    if let Some(search_term) = search {
        if !search_term.is_empty() {
            query.push_str(&format!(
                " AND (t.title ILIKE ${} OR t.description ILIKE ${} OR t.ticket_key ILIKE ${})",
                param_count, param_count + 1, param_count + 2
            ));
        }
    }

    let mut sql_query = sqlx::query_scalar::<_, i64>(&query);
    if let Some(ref statuses) = filter.status {
        for status in statuses {
            sql_query = sql_query.bind(status.as_str());
        }
    }
    if let Some(ref priorities) = filter.priority {
        for priority in priorities {
            sql_query = sql_query.bind(priority.as_str());
        }
    }
    if let Some(assignee_id) = filter.assignees {
        sql_query = sql_query.bind(assignee_id);
    }
    if let Some(proj_id) = filter.project {
        sql_query = sql_query.bind(proj_id);
    }
    if let Some(ref prefix) = filter.project_prefix {
        sql_query = sql_query.bind(prefix.as_str());
    }
    if let Some(ms_id) = filter.milestone {
        sql_query = sql_query.bind(ms_id);
    }
    if let Some(cat_id) = filter.category {
        sql_query = sql_query.bind(cat_id);
    }
    if let Some(label_id) = filter.labels {
        sql_query = sql_query.bind(label_id);
    }
    if let Some(parent_id) = filter.parent {
        sql_query = sql_query.bind(parent_id);
    }
    if let Some(due_gte) = filter.due_date_gte {
        sql_query = sql_query.bind(due_gte);
    }
    if let Some(due_lte) = filter.due_date_lte {
        sql_query = sql_query.bind(due_lte);
    }
    if let Some(search_term) = search {
        if !search_term.is_empty() {
            let pattern = format!("%{}%", search_term);
            sql_query = sql_query.bind(pattern.clone());
            sql_query = sql_query.bind(pattern.clone());
            sql_query = sql_query.bind(pattern);
        }
    }

    let count = sql_query.fetch_one(pool).await?;
    Ok(count)
}

/// チケット詳細取得(JSON API用)
pub async fn api_find_by_key(pool: &PgPool, ticket_key: &str) -> anyhow::Result<Option<TicketDetailOut>> {
    // 基本的なチケット情報を取得
    let base_query_result = sqlx::query(
        "SELECT
            t.id::int4, t.ticket_key, t.title, t.description, t.status, t.priority, t.ticket_type,
            t.author_id::int4, t.category_id::int4, t.project_id::int4, t.milestone_id::int4, t.parent_id::int4,
            t.start_date, t.due_date, t.story_points, t.cycle_id::int4, t.assigned_team_id::int4,
            t.gantt_order, t.created_at, t.updated_at, t.closed_at,
            au.id::int4 as author_id_2, au.username as author_username, au.email as author_email, au.display_name as author_display_name,
            cat.id::int4 as cat_id, cat.name as cat_name, cat.slug as cat_slug, cat.level as cat_level, cat.parent_id::int4 as cat_parent, cat.sort_order as cat_sort, cat.color as cat_color,
            ms.id::int4 as ms_id, ms.name as ms_name, ms.due_date as ms_due_date, ms.description as ms_desc, ms.project_id::int4 as ms_project, ms.created_at as ms_created,
            proj.id::int4 as proj_id,
            tc.name as cycle_name,
            tm.id::int4 as team_id, tm.name as team_name, tm.slug as team_slug, tm.icon as team_icon, tm.color as team_color,
            (SELECT COUNT(*) FROM tickets_comment WHERE ticket_id = t.id) as comment_count,
            (SELECT COUNT(*) FROM tickets_ticket WHERE parent_id = t.id) as child_count,
            COALESCE((SELECT COUNT(*) FROM t_time_entry WHERE ticket_id = t.id), 0) as time_spent
         FROM tickets_ticket t
         LEFT JOIN accounts_user au ON t.author_id = au.id
         LEFT JOIN tickets_category cat ON t.category_id = cat.id
         LEFT JOIN milestones_milestone ms ON t.milestone_id = ms.id
         LEFT JOIN tickets_project proj ON t.project_id = proj.id
         LEFT JOIN t_cycle tc ON t.cycle_id = tc.id
         LEFT JOIN m_team tm ON t.assigned_team_id = tm.id
         WHERE t.ticket_key = $1"
    )
    .bind(ticket_key)
    .fetch_optional(pool)
    .await?;

    let row = match base_query_result {
        Some(r) => r,
        None => return Ok(None),
    };

    let ticket_id: i32 = row.get(0);

    // Assignees
    let assignees_rows = sqlx::query(
        "SELECT ta.user_id::int4, u.id::int4, u.username, u.email, u.display_name
         FROM tickets_ticket_assignees ta
         JOIN accounts_user u ON ta.user_id = u.id
         WHERE ta.ticketmodel_id = $1
         ORDER BY u.id"
    )
    .bind(ticket_id)
    .fetch_all(pool)
    .await?;

    let assignees: Vec<UserSummaryOut> = assignees_rows
        .into_iter()
        .map(|r| UserSummaryOut {
            id: r.get(1),
            username: r.get(2),
            email: r.get(3),
            display_name: r.get(4),
        })
        .collect();

    // Labels
    let labels_rows = sqlx::query(
        "SELECT tl.labelmodel_id::int4, l.id::int4, l.name, l.color, l.project_id::int4, l.created_at, l.description, l.category, l.is_ai_enabled
         FROM tickets_ticket_labels tl
         JOIN m_label l ON tl.labelmodel_id = l.id
         WHERE tl.ticketmodel_id = $1
         ORDER BY l.id"
    )
    .bind(ticket_id)
    .fetch_all(pool)
    .await?;

    let labels: Vec<LabelOut> = labels_rows
        .into_iter()
        .map(|r| LabelOut {
            id: r.get(1),
            name: r.get(2),
            color: r.get(3),
            project: r.get(4),
            created_at: r.get(5),
            description: r.get(6),
            category: r.get(7),
            is_ai_enabled: r.get(8),
        })
        .collect();

    // Comments
    let comments_rows = sqlx::query(
        "SELECT c.id::int4, c.body, c.created_at, c.author_id::int4, u.id::int4, u.username, u.email, u.display_name
         FROM tickets_comment c
         LEFT JOIN accounts_user u ON c.author_id = u.id
         WHERE c.ticket_id = $1
         ORDER BY c.created_at ASC"
    )
    .bind(ticket_id)
    .fetch_all(pool)
    .await?;

    let comments: Vec<CommentOut> = comments_rows
        .into_iter()
        .map(|r| {
            let author_id: i32 = r.get(4);
            let author = UserSummaryOut {
                id: author_id,
                username: r.get(5),
                email: r.get(6),
                display_name: r.get(7),
            };
            CommentOut {
                id: r.get(0),
                body: r.get(1),
                author,
                created_at: r.get(2),
            }
        })
        .collect();

    // Linked Rules
    let linked_rules_rows = sqlx::query(
        "SELECT tr.id::int4, tr.title, tr.category, tm.name
         FROM tickets_ticket_linked_rules ttlr
         LEFT JOIN m_team_rule tr ON ttlr.teamrulemodel_id = tr.id
         LEFT JOIN m_team tm ON tr.team_id = tm.id
         WHERE ttlr.ticketmodel_id = $1"
    )
    .bind(ticket_id)
    .fetch_all(pool)
    .await?;

    let linked_rules: Vec<TeamRuleSummaryOut> = linked_rules_rows
        .into_iter()
        .map(|r| TeamRuleSummaryOut {
            id: r.get(0),
            title: r.get(1),
            category: r.get(2),
            team_name: r.get(3),
        })
        .collect();

    // Linked Wiki Pages
    let linked_wiki_rows = sqlx::query(
        "SELECT w.id::int4, w.title, w.slug, w.category
         FROM wiki_page_linked_tickets wplt
         LEFT JOIN wiki_page w ON wplt.wikipage_id = w.id
         WHERE wplt.ticketmodel_id = $1"
    )
    .bind(ticket_id)
    .fetch_all(pool)
    .await?;

    let linked_wiki_pages: Vec<LinkedWikiPageOut> = linked_wiki_rows
        .into_iter()
        .map(|r| LinkedWikiPageOut {
            id: r.get(0),
            title: r.get(1),
            slug: r.get(2),
            category: r.get(3),
        })
        .collect();

    // Milestone counts
    let milestone_id_opt: Option<i32> = row.get(10);
    let (open_count, closed_count) = if let Some(ms_id) = milestone_id_opt {
        let counts: (i64, i64) = sqlx::query_as(
            "SELECT
                COUNT(CASE WHEN status != 'closed' THEN 1 END),
                COUNT(CASE WHEN status = 'closed' THEN 1 END)
             FROM tickets_ticket
             WHERE milestone_id = $1"
        )
        .bind(ms_id)
        .fetch_one(pool)
        .await
        .unwrap_or((0, 0));
        counts
    } else {
        (0, 0)
    };

    // Build base TicketListOut
    let author_id: i32 = row.get(7);
    let author_username: String = row.get(22);
    let author_email: String = row.get(23);
    let author_display_name: String = row.get(24);

    let category_id_opt: Option<i32> = row.get(8);
    let category = if let Some(cat_id) = category_id_opt {
        row.get::<Option<i32>, _>(25).and_then(|_| {
            Some(CategoryOut {
                id: cat_id,
                name: row.get(26),
                slug: row.get(27),
                level: row.get(28),
                parent: row.get(29),
                sort_order: row.get(30),
                color: row.get(31),
            })
        })
    } else {
        None
    };

    let milestone = if let Some(ms_id) = milestone_id_opt {
        row.get::<Option<i32>, _>(32).and_then(|_| {
            Some(MilestoneOut {
                id: ms_id,
                name: row.get(33),
                due_date: row.get(34),
                description: row.get(35),
                project: row.get(36),
                open_ticket_count: open_count,
                closed_ticket_count: closed_count,
                created_at: row.get(37),
            })
        })
    } else {
        None
    };

    let project_id: i32 = row.get(9);
    let parent_id: Option<i32> = row.get(11);
    let assigned_team_id_opt: Option<i32> = row.get(16);
    let assigned_team = if let Some(team_id) = assigned_team_id_opt {
        row.get::<Option<i32>, _>(40).and_then(|_| {
            Some(TeamSummaryOut {
                id: team_id,
                name: row.get(41),
                slug: row.get(42),
                icon: row.get(43),
                color: row.get(44),
            })
        })
    } else {
        None
    };

    let base = TicketListOut {
        id: ticket_id,
        ticket_key: row.get(1),
        title: row.get(2),
        status: row.get(4),
        priority: row.get(5),
        ticket_type: row.get(6),
        assignees,
        author: UserSummaryOut {
            id: author_id,
            username: author_username,
            email: author_email,
            display_name: author_display_name,
        },
        category,
        milestone,
        project: project_id,
        parent: parent_id,
        labels,
        start_date: row.get(12),
        due_date: row.get(13),
        story_points: row.get(14),
        cycle: row.get(15),
        cycle_name: row.get(39),
        assigned_team,
        comment_count: row.get(45),
        child_count: row.get(46),
        total_time_spent: row.get(47),
        gantt_order: row.get(17),
        created_at: row.get(18),
        updated_at: row.get(19),
    };

    Ok(Some(TicketDetailOut {
        base,
        description: row.get(3),
        comments,
        closed_at: row.get(20),
        linked_rules,
        linked_wiki_pages,
    }))
}

/// ticket_key から ticket_id を解決する共通ヘルパー(Step 0.5: 識別子体系の統一)。
///
/// 対外APIの識別子は原則 ticket_key(文字列)に統一し、dependencies/git-events等
/// Django側が数値ticket_idベースで実装されているリソースをRustへ移植する際は、
/// この関数でticket_idへ解決してから内部クエリを行う方針とする。
pub async fn resolve_ticket_id(pool: &PgPool, ticket_key: &str) -> anyhow::Result<Option<i32>> {
    let id: Option<i32> = sqlx::query_scalar(
        "SELECT id::int4 FROM tickets_ticket WHERE ticket_key = $1"
    )
    .bind(ticket_key)
    .fetch_optional(pool)
    .await?;

    Ok(id)
}

/// project_prefix から project_id を解決する共通ヘルパー(api_create_externalと同じ検索をAI用外部APIでも再利用)。
pub async fn resolve_project_id_by_prefix(pool: &PgPool, project_prefix: &str) -> anyhow::Result<Option<i32>> {
    let id: Option<i32> = sqlx::query_scalar(
        "SELECT id::int4 FROM tickets_project WHERE prefix = $1"
    )
    .bind(project_prefix)
    .fetch_optional(pool)
    .await?;

    Ok(id)
}

/// チケットキー採番(JSON API用、トランザクション必須)
pub async fn api_generate_ticket_key(
    conn: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    project_id: i32,
) -> anyhow::Result<String> {
    // プロジェクトから prefix を取得
    let prefix: String = sqlx::query_scalar(
        "SELECT COALESCE(prefix, 'TICKET') FROM tickets_project WHERE id = $1"
    )
    .bind(project_id)
    .fetch_optional(conn.as_mut())
    .await?
    .unwrap_or_else(|| "TICKET".to_string());

    // アドバイザリロック (hashtext は PostgreSQL 内置関数)
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(&prefix)
        .execute(conn.as_mut())
        .await?;

    // 既存の最大キーを取得
    let pattern = format!("{}-%", prefix);
    let max_key: Option<String> = sqlx::query_scalar(
        "SELECT ticket_key FROM tickets_ticket WHERE ticket_key LIKE $1 ORDER BY id DESC LIMIT 1"
    )
    .bind(&pattern)
    .fetch_optional(conn.as_mut())
    .await?;

    let num = if let Some(key) = max_key {
        // ticket_key の最後の `-` 以降をパース
        if let Some(last_dash_idx) = key.rfind('-') {
            let num_part = &key[last_dash_idx + 1..];
            num_part.parse::<i32>().unwrap_or(0) + 1
        } else {
            1
        }
    } else {
        1
    };

    Ok(format!("{}-{:06}", prefix, num))
}

/// コメント追加(JSON API用)
pub async fn api_add_comment(
    pool: &PgPool,
    ticket_id: i32,
    author_id: i32,
    body: &str,
) -> anyhow::Result<CommentOut> {
    let comment_row = sqlx::query(
        "INSERT INTO tickets_comment (body, author_id, ticket_id, created_at)
         VALUES ($1, $2, $3, NOW())
         RETURNING id::int4, body, created_at, author_id::int4"
    )
    .bind(body)
    .bind(author_id)
    .bind(ticket_id)
    .fetch_one(pool)
    .await?;

    let comment_id: i32 = comment_row.get(0);
    let comment_body: String = comment_row.get(1);
    let created_at: chrono::DateTime<Utc> = comment_row.get(2);

    // 作成者情報を取得
    let author = sqlx::query_as::<_, UserSummaryOut>(
        "SELECT id::int4, username, email, display_name FROM accounts_user WHERE id = $1"
    )
    .bind(author_id)
    .fetch_one(pool)
    .await?;

    Ok(CommentOut {
        id: comment_id,
        body: comment_body,
        author,
        created_at,
    })
}

/// 変更ログ取得(JSON API用)
pub async fn api_find_change_logs(
    pool: &PgPool,
    ticket_id: i32,
) -> anyhow::Result<Vec<ChangeLogOut>> {
    let rows = sqlx::query(
        "SELECT cl.id::int4, cl.field_name, cl.old_value, cl.new_value, cl.changed_at,
                cl.changed_by_id::int4, u.id::int4, u.username, u.email, u.display_name
         FROM tickets_change_log cl
         LEFT JOIN accounts_user u ON cl.changed_by_id = u.id
         WHERE cl.ticket_id = $1
         ORDER BY cl.changed_at DESC
         LIMIT 50"
    )
    .bind(ticket_id)
    .fetch_all(pool)
    .await?;

    let result = rows
        .into_iter()
        .map(|row| {
            let changed_by = row
                .get::<Option<i32>, _>(6)
                .and_then(|_| {
                    Some(UserSummaryOut {
                        id: row.get(6),
                        username: row.get(7),
                        email: row.get(8),
                        display_name: row.get(9),
                    })
                });

            ChangeLogOut {
                id: row.get(0),
                field_name: row.get(1),
                old_value: row.get(2),
                new_value: row.get(3),
                changed_by,
                changed_at: row.get(4),
            }
        })
        .collect();

    Ok(result)
}

/// ポイント履歴取得(JSON API用)
pub async fn api_find_point_history(
    pool: &PgPool,
    ticket_id: i32,
) -> anyhow::Result<Vec<PointHistoryOut>> {
    let rows = sqlx::query(
        "SELECT ph.id::int4, ph.old_points, ph.new_points, ph.reason, ph.changed_at,
                ph.changed_by_id::int4, u.id::int4, u.username, u.email, u.display_name
         FROM h_task_point_history ph
         LEFT JOIN accounts_user u ON ph.changed_by_id = u.id
         WHERE ph.ticket_id = $1
         ORDER BY ph.changed_at DESC
         LIMIT 50"
    )
    .bind(ticket_id)
    .fetch_all(pool)
    .await?;

    let result = rows
        .into_iter()
        .map(|row| {
            let changed_by = row
                .get::<Option<i32>, _>(6)
                .and_then(|_| {
                    Some(UserSummaryOut {
                        id: row.get(6),
                        username: row.get(7),
                        email: row.get(8),
                        display_name: row.get(9),
                    })
                });

            PointHistoryOut {
                id: row.get(0),
                old_points: row.get(1),
                new_points: row.get(2),
                reason: row.get(3),
                changed_by,
                changed_at: row.get(4),
            }
        })
        .collect();

    Ok(result)
}

/// チケット削除(JSON API用)
/// チケット削除(JSON API用)。
///
/// DjangoのFK制約はDB上NO ACTIONで、on_delete=CASCADE/SET_NULLはDjango ORMの
/// アプリケーション層collectorが担っている(DB自体には自動cascadeが無い)。
/// tickets_ticket.parent は自己参照CASCADEのため、子孫チケットも再帰的に
/// 収集してまとめて削除する(Djangoの削除挙動と一致させる)。
pub async fn api_delete(pool: &PgPool, ticket_key: &str) -> anyhow::Result<bool> {
    let mut tx = pool.begin().await?;

    let root_id: Option<i32> = sqlx::query_scalar(
        "SELECT id::int4 FROM tickets_ticket WHERE ticket_key = $1"
    )
    .bind(ticket_key)
    .fetch_optional(&mut *tx)
    .await?;

    let root_id = match root_id {
        Some(id) => id,
        None => return Ok(false),
    };

    // 自身+子孫チケット(再帰、parent_idはon_delete=CASCADE)のIDを収集
    let ids: Vec<i32> = sqlx::query_scalar(
        "WITH RECURSIVE descendants AS (
            SELECT id FROM tickets_ticket WHERE id = $1::int4
            UNION ALL
            SELECT t.id FROM tickets_ticket t
            JOIN descendants d ON t.parent_id = d.id
         )
         SELECT id::int4 FROM descendants"
    )
    .bind(root_id)
    .fetch_all(&mut *tx)
    .await?;

    // 直接の子(on_delete=CASCADE、M2M中間テーブルはDjangoが自動除去)
    sqlx::query("DELETE FROM tickets_comment WHERE ticket_id = ANY($1)").bind(&ids).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM tickets_attachment WHERE ticket_id = ANY($1)").bind(&ids).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM tickets_status_history WHERE ticket_id = ANY($1)").bind(&ids).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM tickets_ticket_watchers WHERE ticketmodel_id = ANY($1)").bind(&ids).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM tickets_ticket_labels WHERE ticketmodel_id = ANY($1)").bind(&ids).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM tickets_change_log WHERE ticket_id = ANY($1)").bind(&ids).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM tickets_ticket_assignees WHERE ticketmodel_id = ANY($1)").bind(&ids).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM t_task_dependency WHERE from_task_id = ANY($1) OR to_task_id = ANY($1)").bind(&ids).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM t_time_entry WHERE ticket_id = ANY($1)").bind(&ids).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM t_git_event WHERE ticket_id = ANY($1)").bind(&ids).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM notifications_log WHERE ticket_id = ANY($1)").bind(&ids).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM notifications_notification WHERE ticket_id = ANY($1)").bind(&ids).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM notifications_user_read_state WHERE ticket_id = ANY($1)").bind(&ids).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM h_task_point_history WHERE ticket_id = ANY($1)").bind(&ids).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM tickets_ticket_linked_rules WHERE ticketmodel_id = ANY($1)").bind(&ids).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM wiki_page_linked_tickets WHERE ticketmodel_id = ANY($1)").bind(&ids).execute(&mut *tx).await?;

    // on_delete=SET_NULL
    sqlx::query("UPDATE t_triage_request SET ticket_id = NULL WHERE ticket_id = ANY($1)").bind(&ids).execute(&mut *tx).await?;

    let result = sqlx::query("DELETE FROM tickets_ticket WHERE id = ANY($1)")
        .bind(&ids)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(result.rows_affected() > 0)
}

/// チケット作成(JSON API用、トランザクション必須)
pub async fn api_create(
    conn: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    input: &TicketWriteIn,
    author_id: i32,
) -> anyhow::Result<i32> {
    // ticket_key を生成
    let ticket_key = api_generate_ticket_key(conn, input.project).await?;

    // gantt_order を決定
    let gantt_order: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(gantt_order), 0) + 1 FROM tickets_ticket WHERE project_id = $1"
    )
    .bind(input.project)
    .fetch_one(conn.as_mut())
    .await?;

    // チケットをINSERT
    let ticket_id: i32 = sqlx::query_scalar(
        "INSERT INTO tickets_ticket (
            ticket_key, title, description, status, priority, ticket_type,
            author_id, category_id, project_id, milestone_id, parent_id,
            start_date, due_date, story_points, cycle_id, assigned_team_id,
            gantt_order, created_at, updated_at
         ) VALUES (
            $1, $2, $3, $4, $5, $6,
            $7, $8, $9, $10, $11,
            $12, $13, $14, $15, $16,
            $17, NOW(), NOW()
         )
         RETURNING id::int4"
    )
    .bind(&ticket_key)
    .bind(&input.title)
    .bind(&input.description)
    .bind(&input.status)
    .bind(&input.priority)
    .bind(&input.ticket_type)
    .bind(author_id)
    .bind(input.category)
    .bind(input.project)
    .bind(input.milestone)
    .bind(input.parent)
    .bind(input.start_date)
    .bind(input.due_date)
    .bind(input.story_points)
    .bind(input.cycle)
    .bind(input.assigned_team)
    .bind(gantt_order)
    .fetch_one(conn.as_mut())
    .await?;

    // Assignees を追加
    for &assignee_id in &input.assignees {
        sqlx::query(
            "INSERT INTO tickets_ticket_assignees (ticketmodel_id, user_id) VALUES ($1, $2)
             ON CONFLICT DO NOTHING"
        )
        .bind(ticket_id)
        .bind(assignee_id)
        .execute(conn.as_mut())
        .await?;
    }

    // Labels を追加
    for &label_id in &input.labels {
        sqlx::query(
            "INSERT INTO tickets_ticket_labels (ticketmodel_id, labelmodel_id) VALUES ($1, $2)
             ON CONFLICT DO NOTHING"
        )
        .bind(ticket_id)
        .bind(label_id)
        .execute(conn.as_mut())
        .await?;
    }

    // Linked Rules を追加
    for &rule_id in &input.linked_rules {
        sqlx::query(
            "INSERT INTO tickets_ticket_linked_rules (ticketmodel_id, teamrulemodel_id) VALUES ($1, $2)
             ON CONFLICT DO NOTHING"
        )
        .bind(ticket_id)
        .bind(rule_id)
        .execute(conn.as_mut())
        .await?;
    }

    Ok(ticket_id)
}

/// チケット更新(JSON API用、トランザクション必須)
pub async fn api_update(
    conn: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ticket_key: &str,
    input: &TicketWriteIn,
    user_id: i32,
) -> anyhow::Result<Option<()>> {
    // チケットID取得
    let ticket_id_opt: Option<i32> = sqlx::query_scalar(
        "SELECT id::int4 FROM tickets_ticket WHERE ticket_key = $1"
    )
    .bind(ticket_key)
    .fetch_optional(conn.as_mut())
    .await?;

    let ticket_id = match ticket_id_opt {
        Some(id) => id,
        None => return Ok(None),
    };

    // 更新前のスナップショット(10項目)
    let old_row = sqlx::query(
        "SELECT title, status, priority, ticket_type, category_id::int4, milestone_id::int4,
                description, start_date, due_date, story_points, cycle_id::int4
         FROM tickets_ticket WHERE id = $1"
    )
    .bind(ticket_id)
    .fetch_one(conn.as_mut())
    .await?;

    let old_title: String = old_row.get(0);
    let old_status: String = old_row.get(1);
    let old_priority: String = old_row.get(2);
    let old_ticket_type: String = old_row.get(3);
    let old_category_id: Option<i32> = old_row.get(4);
    let old_milestone_id: Option<i32> = old_row.get(5);
    let old_description: String = old_row.get(6);
    let old_start_date: Option<NaiveDate> = old_row.get(7);
    let old_due_date: Option<NaiveDate> = old_row.get(8);
    let old_story_points: Option<i16> = old_row.get(9);
    let old_cycle_id: Option<i32> = old_row.get(10);

    // 更新前の assignees
    let old_assignees: Vec<i32> = sqlx::query_scalar(
        "SELECT user_id::int4 FROM tickets_ticket_assignees WHERE ticketmodel_id = $1 ORDER BY user_id"
    )
    .bind(ticket_id)
    .fetch_all(conn.as_mut())
    .await?;

    // UPDATE チケット本体
    sqlx::query(
        "UPDATE tickets_ticket SET
            title = $1, status = $2, priority = $3, ticket_type = $4,
            category_id = $5, milestone_id = $6, cycle_id = $7,
            description = $8, start_date = $9, due_date = $10,
            story_points = $11, assigned_team_id = $12, updated_at = NOW()
         WHERE id = $13"
    )
    .bind(&input.title)
    .bind(&input.status)
    .bind(&input.priority)
    .bind(&input.ticket_type)
    .bind(input.category)
    .bind(input.milestone)
    .bind(input.cycle)
    .bind(&input.description)
    .bind(input.start_date)
    .bind(input.due_date)
    .bind(input.story_points)
    .bind(input.assigned_team)
    .bind(ticket_id)
    .execute(conn.as_mut())
    .await?;

    // 中間テーブル全削除→再INSERT
    sqlx::query("DELETE FROM tickets_ticket_assignees WHERE ticketmodel_id = $1")
        .bind(ticket_id)
        .execute(conn.as_mut())
        .await?;
    for &assignee_id in &input.assignees {
        sqlx::query(
            "INSERT INTO tickets_ticket_assignees (ticketmodel_id, user_id) VALUES ($1, $2)"
        )
        .bind(ticket_id)
        .bind(assignee_id)
        .execute(conn.as_mut())
        .await?;
    }

    sqlx::query("DELETE FROM tickets_ticket_labels WHERE ticketmodel_id = $1")
        .bind(ticket_id)
        .execute(conn.as_mut())
        .await?;
    for &label_id in &input.labels {
        sqlx::query(
            "INSERT INTO tickets_ticket_labels (ticketmodel_id, labelmodel_id) VALUES ($1, $2)"
        )
        .bind(ticket_id)
        .bind(label_id)
        .execute(conn.as_mut())
        .await?;
    }

    sqlx::query("DELETE FROM tickets_ticket_linked_rules WHERE ticketmodel_id = $1")
        .bind(ticket_id)
        .execute(conn.as_mut())
        .await?;
    for &rule_id in &input.linked_rules {
        sqlx::query(
            "INSERT INTO tickets_ticket_linked_rules (ticketmodel_id, teamrulemodel_id) VALUES ($1, $2)"
        )
        .bind(ticket_id)
        .bind(rule_id)
        .execute(conn.as_mut())
        .await?;
    }

    // ステータス変更ログ
    if old_status != input.status {
        sqlx::query(
            "INSERT INTO tickets_status_history (old_status, new_status, changed_by_id, changed_at, ticket_id)
             VALUES ($1, $2, $3, NOW(), $4)"
        )
        .bind(&old_status)
        .bind(&input.status)
        .bind(user_id)
        .bind(ticket_id)
        .execute(conn.as_mut())
        .await?;
    }

    // 変更ログ(10項目)
    let changes = [
        ("title", old_title.clone(), input.title.clone()),
        ("status", old_status.clone(), input.status.clone()),
        ("priority", old_priority.clone(), input.priority.clone()),
        ("ticket_type", old_ticket_type.clone(), input.ticket_type.clone()),
        (
            "category_id",
            old_category_id.map_or(String::new(), |id| id.to_string()),
            input.category.map_or(String::new(), |id| id.to_string()),
        ),
        (
            "milestone_id",
            old_milestone_id.map_or(String::new(), |id| id.to_string()),
            input.milestone.map_or(String::new(), |id| id.to_string()),
        ),
        (
            "cycle_id",
            old_cycle_id.map_or(String::new(), |id| id.to_string()),
            input.cycle.map_or(String::new(), |id| id.to_string()),
        ),
        ("description", old_description.clone(), input.description.clone()),
        (
            "start_date",
            old_start_date.map_or(String::new(), |d| d.to_string()),
            input.start_date.map_or(String::new(), |d| d.to_string()),
        ),
        (
            "due_date",
            old_due_date.map_or(String::new(), |d| d.to_string()),
            input.due_date.map_or(String::new(), |d| d.to_string()),
        ),
    ];

    for (field, old_val, new_val) in &changes {
        if old_val != new_val {
            // field_name をタイトルケースに変換
            let field_name = format_field_name(field);

            sqlx::query(
                "INSERT INTO tickets_change_log (field_name, old_value, new_value, changed_by_id, changed_at, ticket_id)
                 VALUES ($1, $2, $3, $4, NOW(), $5)"
            )
            .bind(&field_name)
            .bind(old_val.as_str())
            .bind(new_val.as_str())
            .bind(user_id)
            .bind(ticket_id)
            .execute(conn.as_mut())
            .await?;
        }
    }

    // Assignees変更ログ
    // Django側は集合(set)として比較しているため、順序の違いだけでは変更とみなさない。
    // old_assigneesはORDER BY user_idで取得済みなのでinput側もソートして比較する。
    let mut sorted_new_assignees = input.assignees.clone();
    sorted_new_assignees.sort_unstable();
    sorted_new_assignees.dedup();
    if old_assignees != sorted_new_assignees {
        let old_usernames = if !old_assignees.is_empty() {
            let usernames: Vec<String> = sqlx::query_scalar(
                "SELECT username FROM accounts_user WHERE id = ANY($1) ORDER BY id"
            )
            .bind(&old_assignees)
            .fetch_all(conn.as_mut())
            .await?;
            if usernames.is_empty() {
                "(なし)".to_string()
            } else {
                usernames.join(", ")
            }
        } else {
            "(なし)".to_string()
        };

        let new_usernames = if !input.assignees.is_empty() {
            let usernames: Vec<String> = sqlx::query_scalar(
                "SELECT username FROM accounts_user WHERE id = ANY($1) ORDER BY id"
            )
            .bind(&input.assignees)
            .fetch_all(conn.as_mut())
            .await?;
            if usernames.is_empty() {
                "(なし)".to_string()
            } else {
                usernames.join(", ")
            }
        } else {
            "(なし)".to_string()
        };

        sqlx::query(
            "INSERT INTO tickets_change_log (field_name, old_value, new_value, changed_by_id, changed_at, ticket_id)
             VALUES ($1, $2, $3, $4, NOW(), $5)"
        )
        .bind("Assignees")
        .bind(&old_usernames)
        .bind(&new_usernames)
        .bind(user_id)
        .bind(ticket_id)
        .execute(conn.as_mut())
        .await?;
    }

    // story_points変更ログ
    if old_story_points != input.story_points {
        let old_points_str = old_story_points.map_or(String::new(), |p| p.to_string());
        let new_points_str = input.story_points.map_or(String::new(), |p| p.to_string());

        sqlx::query(
            "INSERT INTO h_task_point_history (old_points, new_points, reason, changed_by_id, changed_at, ticket_id)
             VALUES ($1, $2, $3, $4, NOW(), $5)"
        )
        .bind(old_story_points)
        .bind(input.story_points)
        .bind("")
        .bind(user_id)
        .bind(ticket_id)
        .execute(conn.as_mut())
        .await?;
    }

    Ok(Some(()))
}

/// チケット部分更新(詳細パネルからのインライン編集用)。
/// TicketPatchInで指定されたフィールドのみを更新する(未指定フィールドは触らない)。
pub async fn api_patch(
    conn: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ticket_key: &str,
    input: &TicketPatchIn,
    user_id: i32,
) -> anyhow::Result<Option<()>> {
    let ticket_id_opt: Option<i32> = sqlx::query_scalar(
        "SELECT id::int4 FROM tickets_ticket WHERE ticket_key = $1"
    )
    .bind(ticket_key)
    .fetch_optional(conn.as_mut())
    .await?;

    let ticket_id = match ticket_id_opt {
        Some(id) => id,
        None => return Ok(None),
    };

    let old_row = sqlx::query(
        "SELECT title, status, priority, ticket_type, category_id::int4, milestone_id::int4,
                description, start_date, due_date, story_points, cycle_id::int4
         FROM tickets_ticket WHERE id = $1"
    )
    .bind(ticket_id)
    .fetch_one(conn.as_mut())
    .await?;

    let old_title: String = old_row.get(0);
    let old_status: String = old_row.get(1);
    let old_priority: String = old_row.get(2);
    let old_ticket_type: String = old_row.get(3);
    let old_category_id: Option<i32> = old_row.get(4);
    let old_milestone_id: Option<i32> = old_row.get(5);
    let old_description: String = old_row.get(6);
    let old_start_date: Option<NaiveDate> = old_row.get(7);
    let old_due_date: Option<NaiveDate> = old_row.get(8);
    let old_story_points: Option<i16> = old_row.get(9);
    let old_cycle_id: Option<i32> = old_row.get(10);

    let old_assignees: Vec<i32> = sqlx::query_scalar(
        "SELECT user_id::int4 FROM tickets_ticket_assignees WHERE ticketmodel_id = $1 ORDER BY user_id"
    )
    .bind(ticket_id)
    .fetch_all(conn.as_mut())
    .await?;

    // 動的UPDATE(指定されたフィールドのみSETに含める)
    let mut builder = sqlx::QueryBuilder::<sqlx::Postgres>::new("UPDATE tickets_ticket SET updated_at = NOW()");
    let mut has_column_update = false;

    if let Some(title) = &input.title {
        builder.push(", title = ").push_bind(title.clone());
        has_column_update = true;
    }
    if let Some(description) = &input.description {
        builder.push(", description = ").push_bind(description.clone());
        has_column_update = true;
    }
    if let Some(status) = &input.status {
        builder.push(", status = ").push_bind(status.clone());
        has_column_update = true;
    }
    if let Some(priority) = &input.priority {
        builder.push(", priority = ").push_bind(priority.clone());
        has_column_update = true;
    }
    if let Some(ticket_type) = &input.ticket_type {
        builder.push(", ticket_type = ").push_bind(ticket_type.clone());
        has_column_update = true;
    }
    if let Some(category) = input.category {
        builder.push(", category_id = ").push_bind(category);
        has_column_update = true;
    }
    if let Some(milestone) = input.milestone {
        builder.push(", milestone_id = ").push_bind(milestone);
        has_column_update = true;
    }
    if let Some(cycle) = input.cycle {
        builder.push(", cycle_id = ").push_bind(cycle);
        has_column_update = true;
    }
    if let Some(assigned_team) = input.assigned_team {
        builder.push(", assigned_team_id = ").push_bind(assigned_team);
        has_column_update = true;
    }
    if let Some(start_date) = input.start_date {
        builder.push(", start_date = ").push_bind(start_date);
        has_column_update = true;
    }
    if let Some(story_points) = input.story_points {
        builder.push(", story_points = ").push_bind(story_points);
        has_column_update = true;
    }
    if let Some(due_date) = input.due_date {
        builder.push(", due_date = ").push_bind(due_date);
        has_column_update = true;
    }

    if has_column_update {
        builder.push(" WHERE id = ").push_bind(ticket_id);
        builder.build().execute(conn.as_mut()).await?;
    }

    // 中間テーブル全置換(キーが指定された場合のみ)
    if let Some(assignees) = &input.assignees {
        sqlx::query("DELETE FROM tickets_ticket_assignees WHERE ticketmodel_id = $1")
            .bind(ticket_id)
            .execute(conn.as_mut())
            .await?;
        for &assignee_id in assignees {
            sqlx::query(
                "INSERT INTO tickets_ticket_assignees (ticketmodel_id, user_id) VALUES ($1, $2)"
            )
            .bind(ticket_id)
            .bind(assignee_id)
            .execute(conn.as_mut())
            .await?;
        }
    }

    if let Some(labels) = &input.labels {
        sqlx::query("DELETE FROM tickets_ticket_labels WHERE ticketmodel_id = $1")
            .bind(ticket_id)
            .execute(conn.as_mut())
            .await?;
        for &label_id in labels {
            sqlx::query(
                "INSERT INTO tickets_ticket_labels (ticketmodel_id, labelmodel_id) VALUES ($1, $2)"
            )
            .bind(ticket_id)
            .bind(label_id)
            .execute(conn.as_mut())
            .await?;
        }
    }

    if let Some(linked_rules) = &input.linked_rules {
        sqlx::query("DELETE FROM tickets_ticket_linked_rules WHERE ticketmodel_id = $1")
            .bind(ticket_id)
            .execute(conn.as_mut())
            .await?;
        for &rule_id in linked_rules {
            sqlx::query(
                "INSERT INTO tickets_ticket_linked_rules (ticketmodel_id, teamrulemodel_id) VALUES ($1, $2)"
            )
            .bind(ticket_id)
            .bind(rule_id)
            .execute(conn.as_mut())
            .await?;
        }
    }

    // ステータス変更ログ
    if let Some(new_status) = &input.status {
        if &old_status != new_status {
            sqlx::query(
                "INSERT INTO tickets_status_history (old_status, new_status, changed_by_id, changed_at, ticket_id)
                 VALUES ($1, $2, $3, NOW(), $4)"
            )
            .bind(&old_status)
            .bind(new_status)
            .bind(user_id)
            .bind(ticket_id)
            .execute(conn.as_mut())
            .await?;
        }
    }

    // 変更ログ(指定されたフィールドのみ、実際に値が変わった場合のみ)
    let mut changes: Vec<(&str, String, String)> = Vec::new();
    if let Some(title) = &input.title {
        if &old_title != title {
            changes.push(("title", old_title.clone(), title.clone()));
        }
    }
    if let Some(description) = &input.description {
        if &old_description != description {
            changes.push(("description", old_description.clone(), description.clone()));
        }
    }
    if let Some(status) = &input.status {
        if &old_status != status {
            changes.push(("status", old_status.clone(), status.clone()));
        }
    }
    if let Some(priority) = &input.priority {
        if &old_priority != priority {
            changes.push(("priority", old_priority.clone(), priority.clone()));
        }
    }
    if let Some(ticket_type) = &input.ticket_type {
        if &old_ticket_type != ticket_type {
            changes.push(("ticket_type", old_ticket_type.clone(), ticket_type.clone()));
        }
    }
    if let Some(category) = input.category {
        if old_category_id != category {
            changes.push((
                "category_id",
                old_category_id.map_or(String::new(), |id| id.to_string()),
                category.map_or(String::new(), |id| id.to_string()),
            ));
        }
    }
    if let Some(milestone) = input.milestone {
        if old_milestone_id != milestone {
            changes.push((
                "milestone_id",
                old_milestone_id.map_or(String::new(), |id| id.to_string()),
                milestone.map_or(String::new(), |id| id.to_string()),
            ));
        }
    }
    if let Some(cycle) = input.cycle {
        if old_cycle_id != cycle {
            changes.push((
                "cycle_id",
                old_cycle_id.map_or(String::new(), |id| id.to_string()),
                cycle.map_or(String::new(), |id| id.to_string()),
            ));
        }
    }
    if let Some(start_date) = input.start_date {
        if old_start_date != start_date {
            changes.push((
                "start_date",
                old_start_date.map_or(String::new(), |d| d.to_string()),
                start_date.map_or(String::new(), |d| d.to_string()),
            ));
        }
    }
    if let Some(due_date) = input.due_date {
        if old_due_date != due_date {
            changes.push((
                "due_date",
                old_due_date.map_or(String::new(), |d| d.to_string()),
                due_date.map_or(String::new(), |d| d.to_string()),
            ));
        }
    }

    for (field, old_val, new_val) in &changes {
        let field_name = format_field_name(field);
        sqlx::query(
            "INSERT INTO tickets_change_log (field_name, old_value, new_value, changed_by_id, changed_at, ticket_id)
             VALUES ($1, $2, $3, $4, NOW(), $5)"
        )
        .bind(&field_name)
        .bind(old_val.as_str())
        .bind(new_val.as_str())
        .bind(user_id)
        .bind(ticket_id)
        .execute(conn.as_mut())
        .await?;
    }

    // Assignees変更ログ(集合として比較、順序違いは無視)
    if let Some(assignees) = &input.assignees {
        let mut sorted_new_assignees = assignees.clone();
        sorted_new_assignees.sort_unstable();
        sorted_new_assignees.dedup();
        if old_assignees != sorted_new_assignees {
            let old_usernames = if !old_assignees.is_empty() {
                let usernames: Vec<String> = sqlx::query_scalar(
                    "SELECT username FROM accounts_user WHERE id = ANY($1) ORDER BY id"
                )
                .bind(&old_assignees)
                .fetch_all(conn.as_mut())
                .await?;
                if usernames.is_empty() { "(なし)".to_string() } else { usernames.join(", ") }
            } else {
                "(なし)".to_string()
            };

            let new_usernames = if !assignees.is_empty() {
                let usernames: Vec<String> = sqlx::query_scalar(
                    "SELECT username FROM accounts_user WHERE id = ANY($1) ORDER BY id"
                )
                .bind(assignees)
                .fetch_all(conn.as_mut())
                .await?;
                if usernames.is_empty() { "(なし)".to_string() } else { usernames.join(", ") }
            } else {
                "(なし)".to_string()
            };

            sqlx::query(
                "INSERT INTO tickets_change_log (field_name, old_value, new_value, changed_by_id, changed_at, ticket_id)
                 VALUES ($1, $2, $3, $4, NOW(), $5)"
            )
            .bind("Assignees")
            .bind(&old_usernames)
            .bind(&new_usernames)
            .bind(user_id)
            .bind(ticket_id)
            .execute(conn.as_mut())
            .await?;
        }
    }

    // story_points変更ログ(サイクルのvelocity/burndown集計が参照する)
    if let Some(story_points) = input.story_points {
        if old_story_points != story_points {
            sqlx::query(
                "INSERT INTO h_task_point_history (old_points, new_points, reason, changed_by_id, changed_at, ticket_id)
                 VALUES ($1, $2, $3, $4, NOW(), $5)"
            )
            .bind(old_story_points)
            .bind(story_points)
            .bind("")
            .bind(user_id)
            .bind(ticket_id)
            .execute(conn.as_mut())
            .await?;
        }
    }

    Ok(Some(()))
}

/// field_name をタイトルケースに変換(Pythonの.title()相当)
fn format_field_name(field: &str) -> String {
    field
        .replace("_id", "")
        .replace("_", " ")
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    let rest: String = chars.collect();
                    first.to_uppercase().collect::<String>() + &rest
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// =============================================================================
// 外部API(X-API-Key認証)用
// =============================================================================

pub struct ExternalTicketResult {
    pub id: i32,
    pub ticket_key: String,
    pub project_name: String,
    pub status: String,
}

/// project_prefixで指定したプロジェクトが見つからない場合に返す、利用可能な
/// プロジェクト一覧(prefix, name)。
pub async fn list_project_prefixes(pool: &PgPool) -> anyhow::Result<Vec<(String, String)>> {
    let rows = sqlx::query("SELECT prefix, name FROM tickets_project ORDER BY prefix")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|r| (r.get(0), r.get(1))).collect())
}

/// 外部API経由のチケット作成。project_prefixで対象プロジェクトを特定する。
#[allow(clippy::too_many_arguments)]
pub async fn api_create_external(
    pool: &PgPool,
    project_prefix: &str,
    title: &str,
    description: &str,
    priority: &str,
    ticket_type: &str,
    due_date: Option<chrono::NaiveDate>,
    author_id: i32,
) -> anyhow::Result<Option<ExternalTicketResult>> {
    let project_row = sqlx::query("SELECT id::int4, name FROM tickets_project WHERE prefix = $1")
        .bind(project_prefix)
        .fetch_optional(pool)
        .await?;

    let project_row = match project_row {
        Some(r) => r,
        None => return Ok(None),
    };
    let project_id: i32 = project_row.get(0);
    let project_name: String = project_row.get(1);

    let mut tx = pool.begin().await?;

    let ticket_key = api_generate_ticket_key(&mut tx, project_id).await?;

    let gantt_order: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(gantt_order), 0) + 1 FROM tickets_ticket WHERE project_id = $1"
    )
    .bind(project_id)
    .fetch_one(&mut *tx)
    .await?;

    let ticket_id: i32 = sqlx::query_scalar(
        "INSERT INTO tickets_ticket
            (ticket_key, title, description, status, priority, ticket_type,
             author_id, project_id, due_date, gantt_order, created_at, updated_at)
         VALUES ($1, $2, $3, 'open', $4, $5, $6, $7, $8, $9, NOW(), NOW())
         RETURNING id::int4"
    )
    .bind(&ticket_key)
    .bind(title)
    .bind(description)
    .bind(priority)
    .bind(ticket_type)
    .bind(author_id)
    .bind(project_id)
    .bind(due_date)
    .bind(gantt_order)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO tickets_status_history (old_status, new_status, changed_by_id, changed_at, ticket_id)
         VALUES ('', 'open', $1, NOW(), $2)"
    )
    .bind(author_id)
    .bind(ticket_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Some(ExternalTicketResult {
        id: ticket_id,
        ticket_key,
        project_name,
        status: "open".to_string(),
    }))
}

// =============================================================================
// タスク依存関係(dependencies) — ステップ4
// =============================================================================

const DEPENDENCY_SELECT: &str = "
    SELECT
        d.id::int4, d.from_task_id::int4, d.to_task_id::int4, d.dependency_type, d.created_at,
        ft.ticket_key as from_key, ft.title as from_title,
        tt.ticket_key as to_key, tt.title as to_title,
        cb.id::int4 as cb_id, cb.username as cb_username, cb.email as cb_email, cb.display_name as cb_display_name
     FROM t_task_dependency d
     JOIN tickets_ticket ft ON d.from_task_id = ft.id
     JOIN tickets_ticket tt ON d.to_task_id = tt.id
     LEFT JOIN accounts_user cb ON d.created_by_id = cb.id
";

fn row_to_dependency(row: &sqlx::postgres::PgRow) -> crate::domain::models::dependency_api::TaskDependencyOut {
    use crate::domain::models::dependency_api::TaskDependencyOut;
    let cb_id: Option<i32> = row.get("cb_id");
    TaskDependencyOut {
        id: row.get("id"),
        from_task: row.get("from_task_id"),
        from_task_key: row.get("from_key"),
        from_task_title: row.get("from_title"),
        to_task: row.get("to_task_id"),
        to_task_key: row.get("to_key"),
        to_task_title: row.get("to_title"),
        dependency_type: row.get("dependency_type"),
        created_by: cb_id.map(|_| UserSummaryOut {
            id: row.get("cb_id"),
            username: row.get("cb_username"),
            email: row.get("cb_email"),
            display_name: row.get("cb_display_name"),
        }),
        created_at: row.get("created_at"),
    }
}

/// 指定チケットに関連する依存関係一覧(outgoing + incoming、新しい順)。
pub async fn find_dependencies_for_ticket(
    pool: &PgPool,
    ticket_id: i32,
) -> anyhow::Result<Vec<crate::domain::models::dependency_api::TaskDependencyOut>> {
    let query = format!("{DEPENDENCY_SELECT} WHERE d.from_task_id = $1 OR d.to_task_id = $1 ORDER BY d.created_at DESC");
    let rows = sqlx::query(&query).bind(ticket_id).fetch_all(pool).await?;
    Ok(rows.iter().map(row_to_dependency).collect())
}

pub enum CreateDependencyResult {
    Success(i32),
    SelfReference,
    Duplicate,
    ToTaskNotFound,
}

/// 依存関係を作成する。自己参照・重複チェックはDjangoと同じくアプリ層で行う
/// (DB制約でも二重に保護されている: unique_task_dependency, no_self_dependency)。
pub async fn create_dependency(
    pool: &PgPool,
    from_task_id: i32,
    to_task_id: i32,
    dependency_type: &str,
    created_by: i32,
) -> anyhow::Result<CreateDependencyResult> {
    if from_task_id == to_task_id {
        return Ok(CreateDependencyResult::SelfReference);
    }

    let to_exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM tickets_ticket WHERE id = $1)")
        .bind(to_task_id)
        .fetch_one(pool)
        .await?;
    if !to_exists {
        return Ok(CreateDependencyResult::ToTaskNotFound);
    }

    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM t_task_dependency WHERE from_task_id = $1 AND to_task_id = $2)"
    )
    .bind(from_task_id)
    .bind(to_task_id)
    .fetch_one(pool)
    .await?;
    if exists {
        return Ok(CreateDependencyResult::Duplicate);
    }

    let id: i32 = sqlx::query_scalar(
        "INSERT INTO t_task_dependency (from_task_id, to_task_id, dependency_type, created_by_id, created_at)
         VALUES ($1, $2, $3, $4, NOW())
         RETURNING id::int4"
    )
    .bind(from_task_id)
    .bind(to_task_id)
    .bind(dependency_type)
    .bind(created_by)
    .fetch_one(pool)
    .await?;

    Ok(CreateDependencyResult::Success(id))
}

pub async fn find_dependency_by_id(
    pool: &PgPool,
    dep_id: i32,
) -> anyhow::Result<Option<crate::domain::models::dependency_api::TaskDependencyOut>> {
    let query = format!("{DEPENDENCY_SELECT} WHERE d.id = $1");
    let row = sqlx::query(&query).bind(dep_id).fetch_optional(pool).await?;
    Ok(row.map(|r| row_to_dependency(&r)))
}

/// 依存関係を削除する。ticket_idがfrom/toどちらにも一致しない場合はNotRelatedを返す
/// (Django側の「対象チケットに関連する依存のみ削除可能」というセキュリティチェックを踏襲)。
pub enum DeleteDependencyResult {
    Deleted,
    NotFound,
    NotRelated,
}

pub async fn delete_dependency(pool: &PgPool, dep_id: i32, ticket_id: i32) -> anyhow::Result<DeleteDependencyResult> {
    let row = sqlx::query("SELECT from_task_id::int4, to_task_id::int4 FROM t_task_dependency WHERE id = $1")
        .bind(dep_id)
        .fetch_optional(pool)
        .await?;

    let row = match row {
        Some(r) => r,
        None => return Ok(DeleteDependencyResult::NotFound),
    };

    let from_task_id: i32 = row.get(0);
    let to_task_id: i32 = row.get(1);
    if from_task_id != ticket_id && to_task_id != ticket_id {
        return Ok(DeleteDependencyResult::NotRelated);
    }

    sqlx::query("DELETE FROM t_task_dependency WHERE id = $1").bind(dep_id).execute(pool).await?;
    Ok(DeleteDependencyResult::Deleted)
}

// =============================================================================
// CSVエクスポート — ステップ4
// =============================================================================

pub struct TicketCsvRow {
    pub ticket_key: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub ticket_type: String,
    pub assignees: String,
    pub category: String,
    pub milestone: String,
    pub labels: String,
    pub start_date: String,
    pub due_date: String,
    pub story_points: String,
    pub cycle: String,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn find_tickets_for_csv_export(pool: &PgPool, project_id: i32) -> anyhow::Result<Vec<TicketCsvRow>> {
    let rows = sqlx::query(
        "SELECT
            t.id::int4, t.ticket_key, t.title, t.status, t.priority, t.ticket_type,
            t.start_date, t.due_date, t.story_points, t.created_at, t.updated_at,
            cat.name as category_name, ms.name as milestone_name, cy.name as cycle_name
         FROM tickets_ticket t
         LEFT JOIN tickets_category cat ON t.category_id = cat.id
         LEFT JOIN milestones_milestone ms ON t.milestone_id = ms.id
         LEFT JOIN t_cycle cy ON t.cycle_id = cy.id
         WHERE t.project_id = $1
         ORDER BY t.ticket_key"
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;

    let mut result = Vec::new();
    for row in &rows {
        let ticket_id: i32 = row.get("id");

        let assignees: Vec<String> = sqlx::query_scalar(
            "SELECT COALESCE(NULLIF(u.display_name, ''), u.username)
             FROM tickets_ticket_assignees ta JOIN accounts_user u ON ta.user_id = u.id
             WHERE ta.ticketmodel_id = $1"
        )
        .bind(ticket_id)
        .fetch_all(pool)
        .await?;

        let labels: Vec<String> = sqlx::query_scalar(
            "SELECT l.name FROM tickets_ticket_labels tl JOIN m_label l ON tl.labelmodel_id = l.id
             WHERE tl.ticketmodel_id = $1"
        )
        .bind(ticket_id)
        .fetch_all(pool)
        .await?;

        let start_date: Option<chrono::NaiveDate> = row.get("start_date");
        let due_date: Option<chrono::NaiveDate> = row.get("due_date");
        let story_points: Option<i16> = row.get("story_points");
        let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
        let updated_at: chrono::DateTime<chrono::Utc> = row.get("updated_at");

        result.push(TicketCsvRow {
            ticket_key: row.get("ticket_key"),
            title: row.get("title"),
            status: row.get("status"),
            priority: row.get("priority"),
            ticket_type: row.get("ticket_type"),
            assignees: assignees.join(", "),
            category: row.get::<Option<String>, _>("category_name").unwrap_or_default(),
            milestone: row.get::<Option<String>, _>("milestone_name").unwrap_or_default(),
            labels: labels.join(", "),
            start_date: start_date.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default(),
            due_date: due_date.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default(),
            story_points: story_points.map(|p| p.to_string()).unwrap_or_default(),
            cycle: row.get::<Option<String>, _>("cycle_name").unwrap_or_default(),
            created_at: created_at.format("%Y-%m-%d %H:%M").to_string(),
            updated_at: updated_at.format("%Y-%m-%d %H:%M").to_string(),
        });
    }

    Ok(result)
}

// =============================================================================
// バリデーション関数
// =============================================================================

/// 指定されたassignee_idsがプロジェクトの有効なメンバーであるかを検証
/// メンバーでないuser_idのベクトルを返す（空なら全員メンバー）
pub async fn validate_assignees_are_members(
    pool: &PgPool,
    project_id: i32,
    assignee_ids: &[i32],
) -> anyhow::Result<Vec<i32>> {
    if assignee_ids.is_empty() {
        return Ok(Vec::new());
    }

    // プロジェクトメンバーかつis_activeかつメンバーシップが有効期限内のuser_idを取得
    let member_ids: Vec<i32> = sqlx::query_scalar(
        "SELECT DISTINCT u.id::int4
         FROM accounts_user u
         INNER JOIN tickets_project_membership m ON u.id = m.user_id
         INNER JOIN tickets_project p ON m.project_id = p.id
         WHERE u.is_active = true
           AND m.project_id = $1
           AND (m.end_date IS NULL OR (m.end_date + (p.grace_period_days || ' days')::interval) >= CURRENT_DATE)"
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;

    // assignee_idsのうち、member_idsに含まれないものを見つける
    let non_members: Vec<i32> = assignee_ids
        .iter()
        .copied()
        .filter(|id| !member_ids.contains(id))
        .collect();

    Ok(non_members)
}

/// プロジェクト単位で全チケット(ノード)+全依存関係(エッジ)を一括取得する。
/// 依存関係フロー可視化(React Flow)用。from/to 双方が対象プロジェクトに属する
/// エッジのみを対象とする(プロジェクトを跨ぐ依存は対象外)。
pub async fn find_dependency_graph_for_project(
    pool: &PgPool,
    project_id: i32,
) -> anyhow::Result<crate::domain::models::dependency_api::DependencyGraphOut> {
    use crate::domain::models::dependency_api::{DependencyGraphNodeOut, DependencyGraphOut};

    // ノード: プロジェクト内の全チケット
    let ticket_rows = sqlx::query(
        "SELECT id::int4, ticket_key, title, status, ticket_type, story_points
         FROM tickets_ticket
         WHERE project_id = $1
         ORDER BY id"
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;

    let ticket_ids: Vec<i32> = ticket_rows.iter().map(|r| r.get::<i32, _>("id")).collect();

    // 担当者はM2Mなので別クエリでまとめて取得しRust側でグルーピングする(N+1回避)。
    let assignee_rows = sqlx::query(
        "SELECT ta.ticketmodel_id::int4 as ticket_id, u.id::int4 as user_id, u.username, u.email, u.display_name
         FROM tickets_ticket_assignees ta
         JOIN accounts_user u ON ta.user_id = u.id
         WHERE ta.ticketmodel_id = ANY($1)
         ORDER BY u.id"
    )
    .bind(&ticket_ids)
    .fetch_all(pool)
    .await?;

    let mut assignees_by_ticket: std::collections::HashMap<i32, Vec<UserSummaryOut>> =
        std::collections::HashMap::new();
    for r in &assignee_rows {
        let ticket_id: i32 = r.get("ticket_id");
        assignees_by_ticket.entry(ticket_id).or_default().push(UserSummaryOut {
            id: r.get("user_id"),
            username: r.get("username"),
            email: r.get("email"),
            display_name: r.get("display_name"),
        });
    }

    let nodes: Vec<DependencyGraphNodeOut> = ticket_rows
        .iter()
        .map(|r| {
            let id: i32 = r.get("id");
            DependencyGraphNodeOut {
                id,
                ticket_key: r.get("ticket_key"),
                title: r.get("title"),
                status: r.get("status"),
                ticket_type: r.get("ticket_type"),
                assignees: assignees_by_ticket.remove(&id).unwrap_or_default(),
                story_points: r.get("story_points"),
            }
        })
        .collect();

    // エッジ: 既存 DEPENDENCY_SELECT / row_to_dependency をそのまま再利用
    let edges_query = format!(
        "{DEPENDENCY_SELECT} WHERE ft.project_id = $1 AND tt.project_id = $1 ORDER BY d.created_at DESC"
    );
    let edge_rows = sqlx::query(&edges_query).bind(project_id).fetch_all(pool).await?;
    let edges = edge_rows.iter().map(row_to_dependency).collect();

    Ok(DependencyGraphOut { nodes, edges })
}
