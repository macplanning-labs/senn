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

/// find_all/find_by_id/find_by_key共通のSELECT句。
/// 実スキーマ(tickets_ticket等、Django時代からの命名がそのまま残っている)
/// に合わせている。assigneeは多対多(tickets_ticket_assignees)のため、
/// Ticket構造体の単一assignee_id/assignee_nameには代表1名(最小user_id)の
/// 表示名のみを載せる(assignee_id自体はどの呼び出し元も出力として
/// 参照していないためNULL固定でよい)。
const LEGACY_TICKET_SELECT: &str = "
    SELECT t.id::int4, t.ticket_key, t.title, t.description, t.status, t.priority,
           t.ticket_type, t.parent_id::int4, t.category_id::int4, t.project_id::int4,
           t.author_id::int4, NULL::int4 as assignee_id, t.milestone_id::int4,
           t.start_date, t.due_date, t.gantt_order,
           t.created_at, t.updated_at, t.closed_at,
           au.display_name as author_name,
           (SELECT au2.display_name FROM tickets_ticket_assignees ta
            JOIN accounts_user au2 ON ta.user_id = au2.id
            WHERE ta.ticketmodel_id = t.id ORDER BY au2.id LIMIT 1) as assignee_name,
           cat.name as category_name,
           phase.name as phase_name,
           ms.name as milestone_name,
           proj.name as project_name,
           proj.prefix as project_prefix,
           pt.ticket_key as parent_key
    FROM tickets_ticket t
    LEFT JOIN accounts_user au ON t.author_id = au.id
    LEFT JOIN tickets_category cat ON t.category_id = cat.id
    LEFT JOIN tickets_category phase ON cat.parent_id = phase.id
    LEFT JOIN milestones_milestone ms ON t.milestone_id = ms.id
    LEFT JOIN tickets_project proj ON t.project_id = proj.id
    LEFT JOIN tickets_ticket pt ON t.parent_id = pt.id
";

/// チケット一覧取得（フィルタ付き）
pub async fn find_all(pool: &PgPool, filter: &TicketFilter) -> anyhow::Result<Vec<Ticket>> {
    let mut builder = sqlx::QueryBuilder::<sqlx::Postgres>::new(LEGACY_TICKET_SELECT);
    builder.push(" WHERE 1=1");

    if let Some(ref kw) = filter.keyword {
        if !kw.is_empty() {
            let pattern = format!("%{}%", kw);
            builder.push(" AND (t.title ILIKE ")
                .push_bind(pattern.clone())
                .push(" OR t.description ILIKE ")
                .push_bind(pattern.clone())
                .push(" OR t.ticket_key ILIKE ")
                .push_bind(pattern)
                .push(")");
        }
    }
    if let Some(ref status) = filter.status {
        if !status.is_empty() {
            builder.push(" AND t.status = ").push_bind(status.clone());
        }
    }
    if let Some(ref priority) = filter.priority {
        if !priority.is_empty() {
            builder.push(" AND t.priority = ").push_bind(priority.clone());
        }
    }
    if let Some(aid) = filter.assignee_id {
        builder.push(" AND EXISTS (SELECT 1 FROM tickets_ticket_assignees ta2 WHERE ta2.ticketmodel_id = t.id AND ta2.user_id = ")
            .push_bind(aid)
            .push(")");
    }
    if let Some(pid) = filter.phase_id {
        builder.push(" AND cat.parent_id = ").push_bind(pid);
    }
    if let Some(cid) = filter.category_id {
        builder.push(" AND t.category_id = ").push_bind(cid);
    }
    if let Some(mid) = filter.milestone_id {
        builder.push(" AND t.milestone_id = ").push_bind(mid);
    }
    if let Some(proj_id) = filter.project_id {
        builder.push(" AND t.project_id = ").push_bind(proj_id);
    }

    builder.push(" ORDER BY t.category_id, t.priority DESC, t.due_date NULLS LAST");

    let rows = builder.build_query_as::<Ticket>().fetch_all(pool).await?;

    Ok(rows)
}

/// チケット1件取得（ID指定）
pub async fn find_by_id(pool: &PgPool, id: i32) -> anyhow::Result<Option<Ticket>> {
    let query = format!("{LEGACY_TICKET_SELECT} WHERE t.id = $1");
    let ticket = sqlx::query_as::<_, Ticket>(&query)
        .bind(id)
        .fetch_optional(pool)
        .await?;

    Ok(ticket)
}

/// チケット1件取得（ticket_key 指定）
pub async fn find_by_key(pool: &PgPool, key: &str) -> anyhow::Result<Option<Ticket>> {
    let query = format!("{LEGACY_TICKET_SELECT} WHERE t.ticket_key = $1");
    let ticket = sqlx::query_as::<_, Ticket>(&query)
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
    let gantt_order: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(gantt_order), 0) + 1 FROM tickets_ticket WHERE project_id = $1"
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?;

    let ticket_id = sqlx::query_scalar::<_, i32>(
        "INSERT INTO tickets_ticket (ticket_key, title, description, status, priority, ticket_type,
                                parent_id, category_id, project_id, author_id,
                                milestone_id, start_date, due_date, gantt_order, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, NOW(), NOW())
         RETURNING id::int4"
    )
    .bind(ticket_key).bind(title).bind(description)
    .bind(status).bind(priority).bind(ticket_type)
    .bind(parent_id).bind(category_id).bind(project_id)
    .bind(author_id).bind(milestone_id)
    .bind(start_date).bind(due_date)
    .fetch_one(pool)
    .await?;

    if let Some(aid) = assignee_id {
        sqlx::query("INSERT INTO tickets_ticket_assignees (ticketmodel_id, user_id) VALUES ($1, $2)")
            .bind(ticket_id).bind(aid)
            .execute(pool)
            .await?;
    }

    Ok(ticket_id)
}

/// 次のチケットキーを生成（例: DEMO-000042）
pub async fn generate_next_key(pool: &PgPool, prefix: &str) -> anyhow::Result<String> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tickets_ticket WHERE ticket_key LIKE $1"
    )
    .bind(format!("{}-%", prefix))
    .fetch_one(pool)
    .await?;

    Ok(format!("{}-{:06}", prefix, count + 1))
}

/// ウォッチ追加
pub async fn add_watcher(pool: &PgPool, ticket_id: i32, user_id: i32) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO tickets_ticket_watchers (ticketmodel_id, user_id) VALUES ($1, $2)
         ON CONFLICT DO NOTHING"
    )
    .bind(ticket_id).bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// ウォッチ解除
pub async fn remove_watcher(pool: &PgPool, ticket_id: i32, user_id: i32) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM tickets_ticket_watchers WHERE ticketmodel_id=$1 AND user_id=$2")
        .bind(ticket_id).bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// ウォッチしているか
pub async fn is_watching(pool: &PgPool, ticket_id: i32, user_id: i32) -> anyhow::Result<bool> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM tickets_ticket_watchers WHERE ticketmodel_id=$1 AND user_id=$2)"
    )
    .bind(ticket_id).bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

/// ウォッチャー一覧（user_id のリスト）
pub async fn find_watchers(pool: &PgPool, ticket_id: i32) -> anyhow::Result<Vec<i32>> {
    let ids: Vec<(i32,)> = sqlx::query_as(
        "SELECT user_id::int4 FROM tickets_ticket_watchers WHERE ticketmodel_id=$1"
    )
    .bind(ticket_id)
    .fetch_all(pool)
    .await?;
    Ok(ids.into_iter().map(|(id,)| id).collect())
}

/// 期限到来（当日）または期限超過の未完了チケット × 担当者の組。
/// 担当者は多対多(tickets_ticket_assignees)のため、担当者ごとに1行返す。
#[derive(Debug, sqlx::FromRow)]
pub struct DueTicketAssignee {
    pub ticket_id: i32,
    pub ticket_key: String,
    pub title: String,
    pub due_date: NaiveDate,
    pub user_id: i32,
}

pub async fn find_due_or_overdue_with_assignees(pool: &PgPool) -> anyhow::Result<Vec<DueTicketAssignee>> {
    let rows = sqlx::query_as::<_, DueTicketAssignee>(
        "SELECT t.id::int4 AS ticket_id, t.ticket_key, t.title, t.due_date, ta.user_id::int4 AS user_id
         FROM tickets_ticket t
         JOIN tickets_ticket_assignees ta ON ta.ticketmodel_id = t.id
         WHERE t.due_date IS NOT NULL
           AND t.due_date <= CURRENT_DATE
           AND t.status != 'closed'"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
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
    pub cycle: Option<i32>,
    pub category: Option<i32>,
    pub labels: Option<i32>,
    pub parent: Option<i32>,
    pub parent_isnull: Option<bool>,
    pub due_date_gte: Option<NaiveDate>,
    pub due_date_lte: Option<NaiveDate>,
    pub due_date_isnull: Option<bool>,
    pub user_id: Option<i32>,
    pub team_slug: Option<String>,
}

/// 一覧・件数で詳細と同じ OR（staff / 有効 Project メンバー / 所属 Team メンバー）。
/// `project` フィルタとは独立。両方あるときは AND する。
/// L2②: チケットのアクセス可否をチームメンバーシップの1系統に統一(t_team_membership)。
/// scoped_project_id IS NULL の行はチーム全体メンバーとして無条件許可、
/// scoped_project_id が t.project_id と一致する行はProjectゲストとしてend_date/grace_period_daysで期限判定する。
fn push_ticket_access_sql(query: &mut String, param_count: &mut usize) {
    query.push_str(&format!(
        " AND (
                (SELECT is_staff FROM accounts_user WHERE id = ${}) OR
                (t.team_id IS NOT NULL AND
                    EXISTS(SELECT 1 FROM t_team_membership tm
                        LEFT JOIN tickets_project p ON tm.scoped_project_id = p.id
                        WHERE tm.team_id = t.team_id AND tm.user_id = ${} AND (
                            tm.scoped_project_id IS NULL OR
                            (t.project_id IS NOT NULL AND tm.scoped_project_id = t.project_id AND
                                (tm.end_date IS NULL OR NOW()::date <= tm.end_date + (p.grace_period_days || ' days')::interval))
                        )
                    )
                )
            )",
        *param_count,
        *param_count + 1
    ));
    *param_count += 2;
}

fn push_team_slug_sql(query: &mut String, param_count: &mut usize) {
    query.push_str(&format!(
        " AND EXISTS(SELECT 1 FROM m_team WHERE id = t.team_id AND lower(slug) = lower(${}))",
        *param_count
    ));
    *param_count += 1;
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
            t.start_date, t.due_date, t.story_points, t.cycle_id::int4,
            t.gantt_order, t.created_at, t.updated_at,
            au.id::int4 as author_id_2, au.username as author_username, au.email as author_email, au.display_name as author_display_name,
            cat.id::int4 as cat_id, cat.name as cat_name, cat.slug as cat_slug, cat.level as cat_level, cat.parent_id::int4 as cat_parent, cat.sort_order as cat_sort, cat.color as cat_color,
            ms.id::int4 as ms_id, ms.name as ms_name, ms.due_date as ms_due_date, ms.description as ms_desc, ms.project_id::int4 as ms_project, ms.created_at as ms_created,
            proj.id::int4 as proj_id,
            tc.name as cycle_name,
            team_m.id::int4 as team_id, team_m.name as team_name, team_m.slug as team_slug, team_m.icon as team_icon, team_m.color as team_color,
            (SELECT COUNT(*) FROM tickets_comment WHERE ticket_id = t.id) as comment_count,
            (SELECT COUNT(*) FROM tickets_ticket WHERE parent_id = t.id) as child_count,
            COALESCE((SELECT COUNT(*) FROM t_time_entry WHERE ticket_id = t.id), 0) as time_spent,
            proj.prefix as list_project_prefix
         FROM tickets_ticket t
         LEFT JOIN accounts_user au ON t.author_id = au.id
         LEFT JOIN tickets_category cat ON t.category_id = cat.id
         LEFT JOIN milestones_milestone ms ON t.milestone_id = ms.id
         LEFT JOIN tickets_project proj ON t.project_id = proj.id
         LEFT JOIN t_cycle tc ON t.cycle_id = tc.id
         LEFT JOIN m_team team_m ON t.team_id = team_m.id
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
    if filter.project.is_some() {
        query.push_str(&format!(" AND t.project_id = ${}", param_count));
        param_count += 1;
    }
    if filter.user_id.is_some() {
        push_ticket_access_sql(&mut query, &mut param_count);
    }

    // project__prefix フィルタ
    if let Some(ref prefix) = filter.project_prefix {
        query.push_str(&format!(
            " AND EXISTS(SELECT 1 FROM tickets_project WHERE id = t.project_id AND prefix = ${})",
            param_count
        ));
        param_count += 1;
    }
    if filter.team_slug.is_some() {
        push_team_slug_sql(&mut query, &mut param_count);
    }

    // milestone フィルタ
    if let Some(ms_id) = filter.milestone {
        query.push_str(&format!(" AND t.milestone_id = ${}", param_count));
        param_count += 1;
    }

    // cycle フィルタ
    if let Some(cycle_id) = filter.cycle {
        query.push_str(&format!(" AND t.cycle_id = ${}", param_count));
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
    if let Some(user_id) = filter.user_id {
        sql_query = sql_query.bind(user_id)
            .bind(user_id);
    }
    if let Some(ref prefix) = filter.project_prefix {
        sql_query = sql_query.bind(prefix.as_str());
    }
    if let Some(ref slug) = filter.team_slug {
        sql_query = sql_query.bind(slug.as_str());
    }
    if let Some(ms_id) = filter.milestone {
        sql_query = sql_query.bind(ms_id);
    }
    if let Some(cycle_id) = filter.cycle {
        sql_query = sql_query.bind(cycle_id);
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

    // reviewers をまとめて取得
    let reviewers_map: std::collections::HashMap<i32, Vec<UserSummaryOut>> =
        if !ticket_ids.is_empty() {
            let reviewers_rows = sqlx::query(
                "SELECT tr.ticketmodel_id::int4, u.id::int4, u.username, u.email, u.display_name
                 FROM tickets_ticket_reviewers tr
                 JOIN accounts_user u ON tr.user_id = u.id
                 WHERE tr.ticketmodel_id = ANY($1)
                 ORDER BY u.id"
            )
            .bind(&ticket_ids)
            .fetch_all(pool)
            .await?;

            let mut map: std::collections::HashMap<i32, Vec<UserSummaryOut>> =
                std::collections::HashMap::new();
            for row in reviewers_rows {
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
                    team_id: None,
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
            let author_username: String = row.get(19);
            let author_email: String = row.get(20);
            let author_display_name: String = row.get(21);

            let category_id_opt: Option<i32> = row.get(7);
            let milestone_id_opt: Option<i32> = row.get(9);
            let project_id: Option<i32> = row.get(8);
            let parent_id: Option<i32> = row.get(10);

            let start_date: Option<NaiveDate> = row.get(11);
            let due_date: Option<NaiveDate> = row.get(12);
            let story_points: Option<i16> = row.get(13);
            let cycle_id: Option<i32> = row.get(14);

            let gantt_order: i32 = row.get(15);
            let created_at: chrono::DateTime<chrono::Utc> = row.get(16);
            let updated_at: chrono::DateTime<chrono::Utc> = row.get(17);

            // SELECT 末尾: team 37-41, aggregates 42-44
            let comment_count: i64 = row.get(42);
            let child_count: i64 = row.get(43);
            let total_time_spent: i64 = row.get(44);
            let project_prefix: Option<String> = row.get(45);

            // Author
            let author = UserSummaryOut {
                id: author_id,
                username: author_username,
                email: author_email,
                display_name: author_display_name,
            };

            // Category
            let category = if let Some(cat_id) = category_id_opt {
                row.get::<Option<i32>, _>(22).and_then(|_| {
                    Some(CategoryOut {
                        id: cat_id,
                        name: row.get(23),
                        slug: row.get(24),
                        level: row.get(25),
                        parent: row.get(26),
                        sort_order: row.get(27),
                        color: row.get(28),
                    })
                })
            } else {
                None
            };

            // Milestone
            let milestone = if let Some(ms_id) = milestone_id_opt {
                row.get::<Option<i32>, _>(29).and_then(|_| {
                    let (open_count, closed_count) = milestone_counts_map
                        .get(&ms_id)
                        .copied()
                        .unwrap_or((0, 0));

                    Some(MilestoneOut {
                        id: ms_id,
                        name: row.get(30),
                        due_date: row.get(31),
                        description: row.get(32),
                        project: row.get(33),
                        open_ticket_count: open_count,
                        closed_ticket_count: closed_count,
                        created_at: row.get(34),
                    })
                })
            } else {
                None
            };

            // Cycle name
            let cycle_name: Option<String> = row.get(36);

            // Team (from tickets_ticket.team_id)
            let team = if let Some(team_id_val) = row.get::<Option<i32>, _>(37) {
                Some(TeamSummaryOut {
                    id: team_id_val,
                    name: row.get(38),
                    slug: row.get(39),
                    icon: row.get(40),
                    color: row.get(41),
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

            // Reviewers
            let reviewers = reviewers_map
                .get(&ticket_id)
                .cloned()
                .unwrap_or_default();

            TicketListOut {
                id: ticket_id,
                ticket_key,
                title,
                status,
                priority,
                ticket_type,
                assignees,
                reviewers,
                author,
                category,
                milestone,
                project: project_id,
                project_prefix,
                parent: parent_id,
                labels,
                start_date,
                due_date,
                story_points,
                cycle: cycle_id,
                cycle_name,
                team,
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
    if filter.user_id.is_some() {
        push_ticket_access_sql(&mut query, &mut param_count);
    }
    if filter.project_prefix.is_some() {
        query.push_str(&format!(
            " AND EXISTS(SELECT 1 FROM tickets_project WHERE id = t.project_id AND prefix = ${})",
            param_count
        ));
        param_count += 1;
    }
    if filter.team_slug.is_some() {
        push_team_slug_sql(&mut query, &mut param_count);
    }
    if filter.milestone.is_some() {
        query.push_str(&format!(" AND t.milestone_id = ${}", param_count));
        param_count += 1;
    }
    if filter.cycle.is_some() {
        query.push_str(&format!(" AND t.cycle_id = ${}", param_count));
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
    if let Some(user_id) = filter.user_id {
        sql_query = sql_query.bind(user_id)
            .bind(user_id);
    }
    if let Some(ref prefix) = filter.project_prefix {
        sql_query = sql_query.bind(prefix.as_str());
    }
    if let Some(ref slug) = filter.team_slug {
        sql_query = sql_query.bind(slug.as_str());
    }
    if let Some(ms_id) = filter.milestone {
        sql_query = sql_query.bind(ms_id);
    }
    if let Some(cycle_id) = filter.cycle {
        sql_query = sql_query.bind(cycle_id);
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
pub async fn api_find_by_key(pool: &PgPool, ticket_key: &str, viewer_user_id: Option<i32>) -> anyhow::Result<Option<TicketDetailOut>> {
    // 基本的なチケット情報を取得
    let base_query_result = sqlx::query(
        "SELECT
            t.id::int4, t.ticket_key, t.title, t.description, t.status, t.priority, t.ticket_type,
            t.author_id::int4, t.category_id::int4, t.project_id::int4, t.milestone_id::int4, t.parent_id::int4,
            t.start_date, t.due_date, t.story_points, t.cycle_id::int4,
            t.gantt_order, t.created_at, t.updated_at, t.closed_at,
            au.id::int4 as author_id_2, au.username as author_username, au.email as author_email, au.display_name as author_display_name,
            cat.id::int4 as cat_id, cat.name as cat_name, cat.slug as cat_slug, cat.level as cat_level, cat.parent_id::int4 as cat_parent, cat.sort_order as cat_sort, cat.color as cat_color,
            ms.id::int4 as ms_id, ms.name as ms_name, ms.due_date as ms_due_date, ms.description as ms_desc, ms.project_id::int4 as ms_project, ms.created_at as ms_created,
            proj.id::int4 as proj_id,
            tc.name as cycle_name,
            team_m.id::int4 as team_id, team_m.name as team_name, team_m.slug as team_slug, team_m.icon as team_icon, team_m.color as team_color,
            (SELECT COUNT(*) FROM tickets_comment WHERE ticket_id = t.id) as comment_count,
            (SELECT COUNT(*) FROM tickets_ticket WHERE parent_id = t.id) as child_count,
            COALESCE((SELECT COUNT(*) FROM t_time_entry WHERE ticket_id = t.id), 0) as time_spent
         FROM tickets_ticket t
         LEFT JOIN accounts_user au ON t.author_id = au.id
         LEFT JOIN tickets_category cat ON t.category_id = cat.id
         LEFT JOIN milestones_milestone ms ON t.milestone_id = ms.id
         LEFT JOIN tickets_project proj ON t.project_id = proj.id
         LEFT JOIN t_cycle tc ON t.cycle_id = tc.id
         LEFT JOIN m_team team_m ON t.team_id = team_m.id
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

    // Reviewers
    let reviewers_rows = sqlx::query(
        "SELECT tr.user_id::int4, u.id::int4, u.username, u.email, u.display_name
         FROM tickets_ticket_reviewers tr
         JOIN accounts_user u ON tr.user_id = u.id
         WHERE tr.ticketmodel_id = $1
         ORDER BY u.id"
    )
    .bind(ticket_id)
    .fetch_all(pool)
    .await?;

    let reviewers: Vec<UserSummaryOut> = reviewers_rows
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
            team_id: None,
            created_at: r.get(5),
            description: r.get(6),
            category: r.get(7),
            is_ai_enabled: r.get(8),
        })
        .collect();

    // Comments
    let comments_rows = sqlx::query(
        "SELECT c.id::int4, c.body, c.created_at, c.author_id::int4, u.id::int4, u.username, u.email, u.display_name, c.updated_at,
                c.anchor_start, c.anchor_end, c.anchor_quote, c.parent_comment_id::int4, (c.deleted_at IS NOT NULL) AS is_deleted,
                (SELECT COUNT(*) FROM tickets_comment r WHERE r.parent_comment_id = c.id)::int4 AS reply_count
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
            let is_deleted: bool = r.get(13);
            let body: String = if is_deleted { String::new() } else { r.get(1) };
            CommentOut {
                id: r.get(0),
                body,
                author,
                created_at: r.get(2),
                updated_at: r.get(8),
                anchor_start: r.get(9),
                anchor_end: r.get(10),
                anchor_quote: r.get(11),
                parent_comment_id: r.get(12),
                is_deleted,
                reply_count: r.get(14),
            }
        })
        .collect();

    // Attachments
    let attachments: Vec<AttachmentOut> = crate::infrastructure::repositories::attachment_repo::find_by_ticket(pool, ticket_id)
        .await?
        .into_iter()
        .map(|att| AttachmentOut {
            id: att.id,
            filename: att.filename.clone(),
            file_size: att.file_size,
            size_display: att.human_size(),
            is_image: att.is_image(),
            created_at: att.created_at,
            uploader: UserSummaryOut {
                id: att.uploader_id,
                username: String::new(),
                email: String::new(),
                display_name: att.uploader_name.clone().unwrap_or_default(),
            },
            file_url: format!("/media/{}", att.file_path),
        })
        .collect();

    // Reference Links
    let links: Vec<TicketLinkOut> = crate::infrastructure::repositories::ticket_link_repo::find_by_ticket(pool, ticket_id)
        .await?
        .into_iter()
        .map(|l| TicketLinkOut {
            id: l.id,
            url: l.url.clone(),
            title: l.title.clone(),
            created_by: UserSummaryOut {
                id: l.created_by_id,
                username: String::new(),
                email: String::new(),
                display_name: l.created_by_name.clone().unwrap_or_default(),
            },
            created_at: l.created_at,
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
    let author_username: String = row.get(21);
    let author_email: String = row.get(22);
    let author_display_name: String = row.get(23);

    let category_id_opt: Option<i32> = row.get(8);
    let category = if let Some(cat_id) = category_id_opt {
        row.get::<Option<i32>, _>(24).and_then(|_| {
            Some(CategoryOut {
                id: cat_id,
                name: row.get(25),
                slug: row.get(26),
                level: row.get(27),
                parent: row.get(28),
                sort_order: row.get(29),
                color: row.get(30),
            })
        })
    } else {
        None
    };

    let milestone = if let Some(ms_id) = milestone_id_opt {
        row.get::<Option<i32>, _>(31).and_then(|_| {
            Some(MilestoneOut {
                id: ms_id,
                name: row.get(32),
                due_date: row.get(33),
                description: row.get(34),
                project: row.get(35),
                open_ticket_count: open_count,
                closed_ticket_count: closed_count,
                created_at: row.get(36),
            })
        })
    } else {
        None
    };

    let project_id: Option<i32> = row.get(9);
    let parent_id: Option<i32> = row.get(11);

    // Team (from tickets_ticket.team_id)
    let team = if let Some(team_id_val) = row.get::<Option<i32>, _>(39) {
        Some(TeamSummaryOut {
            id: team_id_val,
            name: row.get(40),
            slug: row.get(41),
            icon: row.get(42),
            color: row.get(43),
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
        reviewers,
        author: UserSummaryOut {
            id: author_id,
            username: author_username,
            email: author_email,
            display_name: author_display_name,
        },
        category,
        milestone,
        project: project_id,
        project_prefix: None,
        parent: parent_id,
        labels,
        start_date: row.get(12),
        due_date: row.get(13),
        story_points: row.get(14),
        cycle: row.get(15),
        cycle_name: row.get(38),
        team,
        comment_count: row.get(44),
        child_count: row.get(45),
        total_time_spent: row.get(46),
        gantt_order: row.get(16),
        created_at: row.get(17),
        updated_at: row.get(18),
    };

    let is_watching = match viewer_user_id {
        Some(uid) => is_watching(pool, ticket_id, uid).await?,
        None => false,
    };

    Ok(Some(TicketDetailOut {
        base,
        description: row.get(3),
        comments,
        attachments,
        links,
        closed_at: row.get(19),
        linked_rules,
        linked_wiki_pages,
        is_watching,
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

pub async fn get_ticket_project_id(pool: &PgPool, ticket_key: &str) -> anyhow::Result<Option<i32>> {
    let project_id: Option<i32> = sqlx::query_scalar(
        "SELECT project_id::int4 FROM tickets_ticket WHERE ticket_key = $1"
    )
    .bind(ticket_key)
    .fetch_optional(pool)
    .await?;
    Ok(project_id)
}

/// プロジェクト内のルートチケットキー一覧（一括削除用）
pub async fn list_root_ticket_keys_by_project(pool: &PgPool, project_id: i32) -> anyhow::Result<Vec<String>> {
    let keys: Vec<String> = sqlx::query_scalar(
        "SELECT ticket_key FROM tickets_ticket WHERE project_id = $1 AND parent_id IS NULL ORDER BY id"
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    Ok(keys)
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
    team_id: i32,
) -> anyhow::Result<String> {
    // Team から prefix を取得（G6-1: 新規採番は Team prefix のみ）
    let prefix: String = sqlx::query_scalar(
        "SELECT COALESCE(prefix, 'TICKET') FROM m_team WHERE id = $1"
    )
    .bind(team_id)
    .fetch_optional(conn.as_mut())
    .await?
    .unwrap_or_else(|| "TICKET".to_string());

    // アドバイザリロック (hashtext は PostgreSQL 内置関数)
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(&prefix)
        .execute(conn.as_mut())
        .await?;

    // 既存の最大採番を取得。
    // 過去は「ORDER BY id DESC LIMIT 1」で"最後に挿入された行"を最大とみなしていたが、
    // idの挿入順と番号の大小は必ずしも一致しない（例: 別経路でのproject_idなしチケット作成、
    // 過去のキー付け替え等）ため、重複キーによるINSERT失敗(unique constraint violation)が
    // 発生していた(2026-09-12)。prefixで閉じた世界の中で実際に使われている番号のうち
    // 数値として最大のものを常に正として採番する。
    let pattern = format!("{}-%", prefix);
    let existing_keys: Vec<String> = sqlx::query_scalar(
        "SELECT ticket_key FROM tickets_ticket WHERE ticket_key LIKE $1"
    )
    .bind(&pattern)
    .fetch_all(conn.as_mut())
    .await?;

    let suffix_prefix = format!("{}-", prefix);
    let max_num = existing_keys
        .iter()
        .filter_map(|key| key.strip_prefix(suffix_prefix.as_str()))
        .filter_map(|num_part| num_part.parse::<i32>().ok())
        .max()
        .unwrap_or(0);
    let num = max_num + 1;

    Ok(format!("{}-{:06}", prefix, num))
}

/// コメント追加(JSON API用)
pub async fn api_add_comment(
    pool: &PgPool,
    ticket_id: i32,
    author_id: i32,
    body: &str,
    anchor: Option<(i32, i32, String)>, // (start, end, quote)
    parent_comment_id: Option<i32>,
) -> anyhow::Result<CommentOut> {
    let comment_row = if let Some((anchor_start, anchor_end, anchor_quote)) = anchor {
        sqlx::query(
            "INSERT INTO tickets_comment (body, author_id, ticket_id, created_at, anchor_start, anchor_end, anchor_quote, parent_comment_id)
             VALUES ($1, $2, $3, NOW(), $4, $5, $6, $7)
             RETURNING id::int4, body, created_at, author_id::int4, anchor_start, anchor_end, anchor_quote, parent_comment_id::int4"
        )
        .bind(body)
        .bind(author_id)
        .bind(ticket_id)
        .bind(anchor_start)
        .bind(anchor_end)
        .bind(anchor_quote)
        .bind(parent_comment_id)
        .fetch_one(pool)
        .await?
    } else {
        sqlx::query(
            "INSERT INTO tickets_comment (body, author_id, ticket_id, created_at, parent_comment_id)
             VALUES ($1, $2, $3, NOW(), $4)
             RETURNING id::int4, body, created_at, author_id::int4, anchor_start, anchor_end, anchor_quote, parent_comment_id::int4"
        )
        .bind(body)
        .bind(author_id)
        .bind(ticket_id)
        .bind(parent_comment_id)
        .fetch_one(pool)
        .await?
    };

    let comment_id: i32 = comment_row.get(0);
    let comment_body: String = comment_row.get(1);
    let created_at: chrono::DateTime<Utc> = comment_row.get(2);
    let anchor_start: Option<i32> = comment_row.get(4);
    let anchor_end: Option<i32> = comment_row.get(5);
    let anchor_quote: Option<String> = comment_row.get(6);
    let returned_parent_comment_id: Option<i32> = comment_row.get(7);

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
        updated_at: None,
        anchor_start,
        anchor_end,
        anchor_quote,
        parent_comment_id: returned_parent_comment_id,
        is_deleted: false,
        reply_count: 0,
    })
}

/// コメントの所属チケットIDと投稿者IDを取得(編集権限チェック用)
pub async fn api_find_comment_owner(
    pool: &PgPool,
    comment_id: i32,
) -> anyhow::Result<Option<(i32, i32, bool)>> {
    let row = sqlx::query(
        "SELECT ticket_id::int4, author_id::int4, (deleted_at IS NOT NULL) AS is_deleted FROM tickets_comment WHERE id = $1"
    )
    .bind(comment_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| (r.get(0), r.get(1), r.get(2))))
}

/// 返信対象コメントの実効スレッドルートIDを解決する。
/// 対象が既に返信(parent_comment_idがSome)ならその親を返し、
/// トップレベルコメントならそのIDをそのまま返す。ticket_idが一致しない、
/// またはコメントが存在しない場合はNoneを返す。
pub async fn api_resolve_thread_root(
    pool: &PgPool,
    comment_id: i32,
    ticket_id: i32,
) -> anyhow::Result<Option<i32>> {
    let root: Option<(i32,)> = sqlx::query_as(
        "SELECT COALESCE(parent_comment_id, id)::int4 FROM tickets_comment WHERE id = $1 AND ticket_id = $2"
    )
    .bind(comment_id)
    .bind(ticket_id)
    .fetch_optional(pool)
    .await?;
    Ok(root.map(|(id,)| id))
}

/// コメント編集(JSON API用)。呼び出し側で投稿者本人であることを確認済みの前提。
pub async fn api_update_comment(
    pool: &PgPool,
    comment_id: i32,
    body: &str,
) -> anyhow::Result<CommentOut> {
    let row = sqlx::query(
        "UPDATE tickets_comment
         SET body = $1, updated_at = NOW()
         WHERE id = $2
         RETURNING id::int4, body, created_at, author_id::int4, updated_at, anchor_start, anchor_end, anchor_quote"
    )
    .bind(body)
    .bind(comment_id)
    .fetch_one(pool)
    .await?;

    let author_id: i32 = row.get(3);
    let author = sqlx::query_as::<_, UserSummaryOut>(
        "SELECT id::int4, username, email, display_name FROM accounts_user WHERE id = $1"
    )
    .bind(author_id)
    .fetch_one(pool)
    .await?;

    Ok(CommentOut {
        id: row.get(0),
        body: row.get(1),
        author,
        created_at: row.get(2),
        updated_at: row.get(4),
        anchor_start: row.get(5),
        anchor_end: row.get(6),
        anchor_quote: row.get(7),
        parent_comment_id: None,
        is_deleted: false,
        reply_count: 0,
    })
}

/// コメント削除(JSON API用)。論理削除。
pub async fn api_delete_comment(
    pool: &PgPool,
    comment_id: i32,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE tickets_comment SET deleted_at = NOW() WHERE id = $1"
    )
    .bind(comment_id)
    .execute(pool)
    .await?;

    Ok(())
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
    // team_id を決定。project_id のみの場合は参加チームから補完または400
    let team_id = if let Some(team) = input.team_id {
        team
    } else if let Some(project_id) = input.project {
        // project に参加チームが1つだけなら補完。2つ以上なら要求
        let participating_teams: Vec<i32> = sqlx::query_scalar(
            "SELECT team_id FROM tickets_project_teams WHERE project_id = $1 ORDER BY team_id"
        )
        .bind(project_id)
        .fetch_all(conn.as_mut())
        .await?;

        match participating_teams.as_slice() {
            [single_team] => *single_team,
            [] => return Err(anyhow::anyhow!("project has no participating teams")),
            _ => return Err(anyhow::anyhow!("teamId is required when project has multiple teams")),
        }
    } else {
        return Err(anyhow::anyhow!("teamId is required"));
    };

    // project 付きなら、解決後の team_id が参加チームであることをアプリ層でも保証（トリガの前段で 400 相当）
    if let Some(project_id) = input.project {
        let ok: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                SELECT 1 FROM tickets_project_teams
                WHERE project_id = $1 AND team_id = $2
            )"
        )
        .bind(project_id)
        .bind(team_id)
        .fetch_one(conn.as_mut())
        .await?;
        if !ok {
            return Err(anyhow::anyhow!(
                "team_id {} is not a participant of project_id {}",
                team_id,
                project_id
            ));
        }
    }

    // ticket_key を生成（Team prefix のみ。呼び出し元は team_id を渡すこと）
    let ticket_key = api_generate_ticket_key(conn, team_id).await?;

    let gantt_order: i32 = if let Some(project_id) = input.project {
        sqlx::query_scalar(
            "SELECT COALESCE(MAX(gantt_order), 0) + 1 FROM tickets_ticket WHERE project_id = $1"
        )
        .bind(project_id)
        .fetch_one(conn.as_mut())
        .await?
    } else {
        sqlx::query_scalar(
            "SELECT COALESCE(MAX(gantt_order), 0) + 1 FROM tickets_ticket WHERE team_id = $1 AND project_id IS NULL"
        )
        .bind(team_id)
        .fetch_one(conn.as_mut())
        .await?
    };

    // チケットをINSERT
    let ticket_id: i32 = sqlx::query_scalar(
        "INSERT INTO tickets_ticket (
            ticket_key, title, description, status, priority, ticket_type,
            author_id, category_id, project_id, milestone_id, parent_id,
            start_date, due_date, story_points, cycle_id, team_id,
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
    .bind(team_id)
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

    // Reviewers を追加
    for &reviewer_id in &input.reviewers {
        sqlx::query(
            "INSERT INTO tickets_ticket_reviewers (ticketmodel_id, user_id) VALUES ($1, $2)
             ON CONFLICT DO NOTHING"
        )
        .bind(ticket_id)
        .bind(reviewer_id)
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

/// api_update/api_patch が検知した通知対象の変更(呼び出し元が通知送信・ウォッチャー登録に使う)
pub struct TicketChangeEvents {
    pub ticket_id: i32,
    /// ステータスが変わった場合の(旧, 新)
    pub status_change: Option<(String, String)>,
    /// 今回新たに担当者に追加されたユーザーID(通知・ウォッチャー登録対象)
    pub newly_assigned: Vec<i32>,
    /// 今回新たにレビュアーに追加されたユーザーID(通知・ウォッチャー登録対象)
    pub newly_reviewers: Vec<i32>,
    /// ステータス・担当者以外で変更されたフィールドの表示名(タイトルケース)一覧
    pub other_changed_fields: Vec<String>,
}

/// チケット更新(JSON API用、トランザクション必須)
pub async fn api_update(
    conn: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ticket_key: &str,
    input: &TicketWriteIn,
    user_id: i32,
) -> anyhow::Result<Option<TicketChangeEvents>> {
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

    // 更新前のスナップショット(12項目)
    let old_row = sqlx::query(
        "SELECT title, status, priority, ticket_type, category_id::int4, milestone_id::int4,
                description, start_date, due_date, story_points, cycle_id::int4,
                parent_id::int4
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
    let old_parent_id: Option<i32> = old_row.get(11);

    // 更新前の assignees
    let old_assignees: Vec<i32> = sqlx::query_scalar(
        "SELECT user_id::int4 FROM tickets_ticket_assignees WHERE ticketmodel_id = $1 ORDER BY user_id"
    )
    .bind(ticket_id)
    .fetch_all(conn.as_mut())
    .await?;

    // 更新前の reviewers
    let old_reviewers: Vec<i32> = sqlx::query_scalar(
        "SELECT user_id::int4 FROM tickets_ticket_reviewers WHERE ticketmodel_id = $1 ORDER BY user_id"
    )
    .bind(ticket_id)
    .fetch_all(conn.as_mut())
    .await?;

    // 参加チーム制約: project 付きチケットの team_id は参加一覧に含まれること
    let project_id: Option<i32> = sqlx::query_scalar(
        "SELECT project_id::int4 FROM tickets_ticket WHERE id = $1"
    )
    .bind(ticket_id)
    .fetch_one(conn.as_mut())
    .await?;
    let effective_project = input.project.or(project_id);
    let effective_team = input.team_id;
    if let (Some(pid), Some(tid)) = (effective_project, effective_team) {
        let ok: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                SELECT 1 FROM tickets_project_teams
                WHERE project_id = $1 AND team_id = $2
            )"
        )
        .bind(pid)
        .bind(tid)
        .fetch_one(conn.as_mut())
        .await?;
        if !ok {
            return Err(anyhow::anyhow!(
                "team_id {} is not a participant of project_id {}",
                tid,
                pid
            ));
        }
    } else if effective_project.is_some() && effective_team.is_none() {
        return Err(anyhow::anyhow!("teamId is required for project-attached tickets"));
    }

    // UPDATE チケット本体
    sqlx::query(
        "UPDATE tickets_ticket SET
            title = $1, status = $2, priority = $3, ticket_type = $4,
            category_id = $5, milestone_id = $6, cycle_id = $7,
            description = $8, start_date = $9, due_date = $10,
            story_points = $11, team_id = $12, parent_id = $13, updated_at = NOW()
         WHERE id = $14"
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
    .bind(input.team_id)
    .bind(input.parent)
    .bind(ticket_id)
    .execute(conn.as_mut())
    .await?;

    // 中間テーブル全削除→再INSERT (assignees)
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

    // 中間テーブル全削除→再INSERT (reviewers)
    sqlx::query("DELETE FROM tickets_ticket_reviewers WHERE ticketmodel_id = $1")
        .bind(ticket_id)
        .execute(conn.as_mut())
        .await?;
    for &reviewer_id in &input.reviewers {
        sqlx::query(
            "INSERT INTO tickets_ticket_reviewers (ticketmodel_id, user_id) VALUES ($1, $2)"
        )
        .bind(ticket_id)
        .bind(reviewer_id)
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
    let status_change = if old_status != input.status {
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
        Some((old_status.clone(), input.status.clone()))
    } else {
        None
    };

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
        (
            "parent_id",
            old_parent_id.map_or(String::new(), |id| id.to_string()),
            input.parent.map_or(String::new(), |id| id.to_string()),
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

    // ステータス・担当者以外の変更フィールド名一覧(ウォッチャーへの一般更新通知用)
    let other_changed_fields: Vec<String> = changes
        .iter()
        .filter(|(field, old_val, new_val)| *field != "status" && old_val != new_val)
        .map(|(field, _, _)| format_field_name(field))
        .collect();

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
    let newly_assigned: Vec<i32> = sorted_new_assignees
        .iter()
        .copied()
        .filter(|id| !old_assignees.contains(id))
        .collect();
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

    // Reviewers変更ログ
    // Django側は集合(set)として比較しているため、順序の違いだけでは変更とみなさない。
    // old_reviewersはORDER BY user_idで取得済みなのでinput側もソートして比較する。
    let mut sorted_new_reviewers = input.reviewers.clone();
    sorted_new_reviewers.sort_unstable();
    sorted_new_reviewers.dedup();
    let newly_reviewers: Vec<i32> = sorted_new_reviewers
        .iter()
        .copied()
        .filter(|id| !old_reviewers.contains(id))
        .collect();
    if old_reviewers != sorted_new_reviewers {
        let old_usernames = if !old_reviewers.is_empty() {
            let usernames: Vec<String> = sqlx::query_scalar(
                "SELECT username FROM accounts_user WHERE id = ANY($1) ORDER BY id"
            )
            .bind(&old_reviewers)
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

        let new_usernames = if !input.reviewers.is_empty() {
            let usernames: Vec<String> = sqlx::query_scalar(
                "SELECT username FROM accounts_user WHERE id = ANY($1) ORDER BY id"
            )
            .bind(&input.reviewers)
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
        .bind("Reviewers")
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

    Ok(Some(TicketChangeEvents { ticket_id, status_change, newly_assigned, newly_reviewers, other_changed_fields }))
}

/// チケット部分更新(詳細パネルからのインライン編集用)。
/// TicketPatchInで指定されたフィールドのみを更新する(未指定フィールドは触らない)。
pub async fn api_patch(
    conn: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ticket_key: &str,
    input: &TicketPatchIn,
    user_id: i32,
) -> anyhow::Result<Option<TicketChangeEvents>> {
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
                description, start_date, due_date, story_points, cycle_id::int4,
                parent_id::int4, team_id::int4
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
    let old_parent_id: Option<i32> = old_row.get(11);
    let old_team_id: Option<i32> = old_row.get(12);

    let old_assignees: Vec<i32> = sqlx::query_scalar(
        "SELECT user_id::int4 FROM tickets_ticket_assignees WHERE ticketmodel_id = $1 ORDER BY user_id"
    )
    .bind(ticket_id)
    .fetch_all(conn.as_mut())
    .await?;

    let old_reviewers: Vec<i32> = sqlx::query_scalar(
        "SELECT user_id::int4 FROM tickets_ticket_reviewers WHERE ticketmodel_id = $1 ORDER BY user_id"
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
    if let Some(parent) = input.parent {
        builder.push(", parent_id = ").push_bind(parent);
        has_column_update = true;
    }
    if let Some(cycle) = input.cycle {
        builder.push(", cycle_id = ").push_bind(cycle);
        has_column_update = true;
    }
    if let Some(team_id) = input.team_id {
        // 参加チーム制約（project 付きのとき）
        let project_id: Option<i32> = sqlx::query_scalar(
            "SELECT project_id::int4 FROM tickets_ticket WHERE id = $1"
        )
        .bind(ticket_id)
        .fetch_one(conn.as_mut())
        .await?;
        if let Some(pid) = project_id {
            match team_id {
                None => {
                    return Err(anyhow::anyhow!(
                        "teamId is required for project-attached tickets"
                    ));
                }
                Some(tid) => {
                    let ok: bool = sqlx::query_scalar(
                        "SELECT EXISTS(
                            SELECT 1 FROM tickets_project_teams
                            WHERE project_id = $1 AND team_id = $2
                        )"
                    )
                    .bind(pid)
                    .bind(tid)
                    .fetch_one(conn.as_mut())
                    .await?;
                    if !ok {
                        return Err(anyhow::anyhow!(
                            "team_id {} is not a participant of project_id {}",
                            tid,
                            pid
                        ));
                    }
                }
            }
        }
        builder.push(", team_id = ").push_bind(team_id);
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

    if let Some(reviewers) = &input.reviewers {
        sqlx::query("DELETE FROM tickets_ticket_reviewers WHERE ticketmodel_id = $1")
            .bind(ticket_id)
            .execute(conn.as_mut())
            .await?;
        for &reviewer_id in reviewers {
            sqlx::query(
                "INSERT INTO tickets_ticket_reviewers (ticketmodel_id, user_id) VALUES ($1, $2)"
            )
            .bind(ticket_id)
            .bind(reviewer_id)
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
    let mut status_change: Option<(String, String)> = None;
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
            status_change = Some((old_status.clone(), new_status.clone()));
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
    if let Some(parent) = input.parent {
        if old_parent_id != parent {
            changes.push((
                "parent_id",
                old_parent_id.map_or(String::new(), |id| id.to_string()),
                parent.map_or(String::new(), |id| id.to_string()),
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

    // ステータス以外の変更フィールド名一覧(ウォッチャーへの一般更新通知用)
    let other_changed_fields: Vec<String> = changes
        .iter()
        .filter(|(field, _, _)| *field != "status")
        .map(|(field, _, _)| format_field_name(field))
        .collect();

    // Assignees変更ログ(集合として比較、順序違いは無視)
    let mut newly_assigned: Vec<i32> = Vec::new();
    if let Some(assignees) = &input.assignees {
        let mut sorted_new_assignees = assignees.clone();
        sorted_new_assignees.sort_unstable();
        sorted_new_assignees.dedup();
        newly_assigned = sorted_new_assignees
            .iter()
            .copied()
            .filter(|id| !old_assignees.contains(id))
            .collect();
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

    // Reviewers変更ログ(集合として比較、順序違いは無視)
    let mut newly_reviewers: Vec<i32> = Vec::new();
    if let Some(reviewers) = &input.reviewers {
        let mut sorted_new_reviewers = reviewers.clone();
        sorted_new_reviewers.sort_unstable();
        sorted_new_reviewers.dedup();
        newly_reviewers = sorted_new_reviewers
            .iter()
            .copied()
            .filter(|id| !old_reviewers.contains(id))
            .collect();
        if old_reviewers != sorted_new_reviewers {
            let old_usernames = if !old_reviewers.is_empty() {
                let usernames: Vec<String> = sqlx::query_scalar(
                    "SELECT username FROM accounts_user WHERE id = ANY($1) ORDER BY id"
                )
                .bind(&old_reviewers)
                .fetch_all(conn.as_mut())
                .await?;
                if usernames.is_empty() { "(なし)".to_string() } else { usernames.join(", ") }
            } else {
                "(なし)".to_string()
            };

            let new_usernames = if !reviewers.is_empty() {
                let usernames: Vec<String> = sqlx::query_scalar(
                    "SELECT username FROM accounts_user WHERE id = ANY($1) ORDER BY id"
                )
                .bind(reviewers)
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
            .bind("Reviewers")
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

    Ok(Some(TicketChangeEvents { ticket_id, status_change, newly_assigned, newly_reviewers, other_changed_fields }))
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

    // project に参加チームが1つだけなら補完。2つ以上ならエラー
    let participating_teams: Vec<i32> = sqlx::query_scalar(
        "SELECT team_id FROM tickets_project_teams WHERE project_id = $1 ORDER BY team_id"
    )
    .bind(project_id)
    .fetch_all(&mut *tx)
    .await?;

    let team_id = match participating_teams.as_slice() {
        [single_team] => *single_team,
        [] => return Err(anyhow::anyhow!("project has no participating teams")),
        _ => return Err(anyhow::anyhow!("teamId is required when project has multiple teams")),
    };

    let ticket_key = api_generate_ticket_key(&mut tx, team_id).await?;

    let gantt_order: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(gantt_order), 0) + 1 FROM tickets_ticket WHERE project_id = $1"
    )
    .bind(project_id)
    .fetch_one(&mut *tx)
    .await?;

    let ticket_id: i32 = sqlx::query_scalar(
        "INSERT INTO tickets_ticket
            (ticket_key, title, description, status, priority, ticket_type,
             author_id, project_id, due_date, gantt_order, team_id, created_at, updated_at)
         VALUES ($1, $2, $3, 'open', $4, $5, $6, $7, $8, $9, $10, NOW(), NOW())
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
    .bind(team_id)
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
    CircularDependency,
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

    // 循環依存検知: to_task_id から既存の依存グラフを辿って from_task_id に
    // 到達できる場合、この依存を追加すると多段の循環(A→B→C→A等)になる。
    let would_create_cycle: bool = sqlx::query_scalar(
        r#"
        WITH RECURSIVE reachable AS (
            SELECT to_task_id AS task_id FROM t_task_dependency WHERE from_task_id = $1
            UNION
            SELECT d.to_task_id FROM t_task_dependency d
            JOIN reachable r ON d.from_task_id = r.task_id
        )
        SELECT EXISTS (SELECT 1 FROM reachable WHERE task_id = $2)
        "#
    )
    .bind(to_task_id)
    .bind(from_task_id)
    .fetch_one(pool)
    .await?;
    if would_create_cycle {
        return Ok(CreateDependencyResult::CircularDependency);
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

pub async fn find_tickets_for_csv_export(
    pool: &PgPool,
    project_id: i32,
    user_id: i32,
) -> anyhow::Result<Vec<TicketCsvRow>> {
    let mut query = String::from(
        "SELECT
            t.id::int4, t.ticket_key, t.title, t.status, t.priority, t.ticket_type,
            t.start_date, t.due_date, t.story_points, t.created_at, t.updated_at,
            cat.name as category_name, ms.name as milestone_name, cy.name as cycle_name
         FROM tickets_ticket t
         LEFT JOIN tickets_category cat ON t.category_id = cat.id
         LEFT JOIN milestones_milestone ms ON t.milestone_id = ms.id
         LEFT JOIN t_cycle cy ON t.cycle_id = cy.id
         WHERE t.project_id = $1",
    );
    let mut param_count = 2;
    push_ticket_access_sql(&mut query, &mut param_count);
    query.push_str(" ORDER BY t.ticket_key");

    let rows = sqlx::query(&query)
        .bind(project_id)
        .bind(user_id)
        .bind(user_id)
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

    // プロジェクトにアクセスできる(参加チーム全体メンバー、またはこのProjectに限定された
    // 有効期限内のゲスト)かつis_activeなuser_idを取得
    let member_ids: Vec<i32> = sqlx::query_scalar(
        "SELECT DISTINCT u.id::int4
         FROM accounts_user u
         INNER JOIN t_team_membership tm ON u.id = tm.user_id
         INNER JOIN tickets_project_teams pt ON tm.team_id = pt.team_id
         INNER JOIN tickets_project p ON pt.project_id = p.id
         WHERE u.is_active = true
           AND p.id = $1
           AND (
             tm.scoped_project_id IS NULL OR
             (tm.scoped_project_id = $1 AND
                 (tm.end_date IS NULL OR (tm.end_date + (p.grace_period_days || ' days')::interval) >= CURRENT_DATE))
           )"
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

pub async fn validate_reviewers_are_members(
    pool: &PgPool,
    project_id: i32,
    reviewer_ids: &[i32],
) -> anyhow::Result<Vec<i32>> {
    if reviewer_ids.is_empty() {
        return Ok(Vec::new());
    }

    // プロジェクトにアクセスできる(参加チーム全体メンバー、またはこのProjectに限定された
    // 有効期限内のゲスト)かつis_activeなuser_idを取得
    let member_ids: Vec<i32> = sqlx::query_scalar(
        "SELECT DISTINCT u.id::int4
         FROM accounts_user u
         INNER JOIN t_team_membership tm ON u.id = tm.user_id
         INNER JOIN tickets_project_teams pt ON tm.team_id = pt.team_id
         INNER JOIN tickets_project p ON pt.project_id = p.id
         WHERE u.is_active = true
           AND p.id = $1
           AND (
             tm.scoped_project_id IS NULL OR
             (tm.scoped_project_id = $1 AND
                 (tm.end_date IS NULL OR (tm.end_date + (p.grace_period_days || ' days')::interval) >= CURRENT_DATE))
           )"
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;

    // reviewer_idsのうち、member_idsに含まれないものを見つける
    let non_members: Vec<i32> = reviewer_ids
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
    use crate::domain::models::dependency_api::{DependencyGraphNodeOut, DependencyGraphOut, DependencyGraphCycleOut};

    // ノード: プロジェクト内の全チケット
    let ticket_rows = sqlx::query(
        "SELECT t.id::int4, t.ticket_key, t.title, t.status, t.ticket_type, t.story_points,
                t.cycle_id::int4, tc.name AS cycle_name
         FROM tickets_ticket t
         LEFT JOIN t_cycle tc ON t.cycle_id = tc.id
         WHERE t.project_id = $1
         ORDER BY t.id"
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
                cycle: r.get("cycle_id"),
                cycle_name: r.get("cycle_name"),
            }
        })
        .collect();

    // エッジ: 既存 DEPENDENCY_SELECT / row_to_dependency をそのまま再利用
    let edges_query = format!(
        "{DEPENDENCY_SELECT} WHERE ft.project_id = $1 AND tt.project_id = $1 ORDER BY d.created_at DESC"
    );
    let edge_rows = sqlx::query(&edges_query).bind(project_id).fetch_all(pool).await?;
    let edges = edge_rows.iter().map(row_to_dependency).collect();

    // Cycle位置: プロジェクトに属する全Cycleの保存済み座標(未配置ならNULL)
    let cycle_rows = sqlx::query(
        "SELECT id::int4, graph_position_x, graph_position_y
         FROM t_cycle
         WHERE project_id = $1
         ORDER BY id"
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    let cycles: Vec<crate::domain::models::dependency_api::DependencyGraphCycleOut> = cycle_rows
        .iter()
        .map(|r| crate::domain::models::dependency_api::DependencyGraphCycleOut {
            id: r.get("id"),
            graph_position_x: r.get("graph_position_x"),
            graph_position_y: r.get("graph_position_y"),
        })
        .collect();

    Ok(DependencyGraphOut { nodes, edges, cycles })
}

pub async fn find_dependency_graph_for_team(
    pool: &PgPool,
    team_id: i32,
) -> anyhow::Result<crate::domain::models::dependency_api::DependencyGraphOut> {
    use crate::domain::models::dependency_api::{DependencyGraphNodeOut, DependencyGraphOut, DependencyGraphCycleOut};

    // ノード: チーム内の全チケット
    let ticket_rows = sqlx::query(
        "SELECT t.id::int4, t.ticket_key, t.title, t.status, t.ticket_type, t.story_points,
                t.cycle_id::int4, tc.name AS cycle_name
         FROM tickets_ticket t
         LEFT JOIN t_cycle tc ON t.cycle_id = tc.id
         WHERE t.team_id = $1
         ORDER BY t.id"
    )
    .bind(team_id)
    .fetch_all(pool)
    .await?;

    let ticket_ids: Vec<i32> = ticket_rows.iter().map(|r| r.get::<i32, _>("id")).collect();

    // 担当者はM2Mなので別クエリでまとめて取得
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
                cycle: r.get("cycle_id"),
                cycle_name: r.get("cycle_name"),
            }
        })
        .collect();

    // エッジ: 両方のチケットが同じチームに属する依存関係
    let edges_query = format!(
        "{DEPENDENCY_SELECT} WHERE ft.team_id = $1 AND tt.team_id = $1 ORDER BY d.created_at DESC"
    );
    let edge_rows = sqlx::query(&edges_query).bind(team_id).fetch_all(pool).await?;
    let edges = edge_rows.iter().map(row_to_dependency).collect();

    // Cycle位置: チームに属する全Cycleの保存済み座標
    let cycle_rows = sqlx::query(
        "SELECT id::int4, graph_position_x, graph_position_y
         FROM t_cycle
         WHERE team_id = $1
         ORDER BY id"
    )
    .bind(team_id)
    .fetch_all(pool)
    .await?;
    let cycles: Vec<crate::domain::models::dependency_api::DependencyGraphCycleOut> = cycle_rows
        .iter()
        .map(|r| crate::domain::models::dependency_api::DependencyGraphCycleOut {
            id: r.get("id"),
            graph_position_x: r.get("graph_position_x"),
            graph_position_y: r.get("graph_position_y"),
        })
        .collect();

    Ok(DependencyGraphOut { nodes, edges, cycles })
}

pub async fn assert_team_participates_in_project(
    pool: &PgPool,
    project_id: i32,
    team_id: i32,
) -> anyhow::Result<()> {
    let ok: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM tickets_project_teams
            WHERE project_id = $1 AND team_id = $2
        )"
    )
    .bind(project_id)
    .bind(team_id)
    .fetch_one(pool)
    .await?;

    if !ok {
        anyhow::bail!(
            "team_id {} is not a participant of project_id {}",
            team_id,
            project_id
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;

    /// A→B, B→C が既存の状態で C→A を追加しようとすると
    /// 循環(A→B→C→A)になるため拒否されることを確認する。
    #[tokio::test]
    async fn create_dependency_rejects_direct_cycle() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let author = test_support::create_test_user(&pool, "cycle-author").await;
        let project = test_support::create_test_project(&pool, "CYC", author).await;
        let a = test_support::create_test_ticket(&pool, project, "CYC-A", author).await;
        let b = test_support::create_test_ticket(&pool, project, "CYC-B", author).await;
        let c = test_support::create_test_ticket(&pool, project, "CYC-C", author).await;

        let r1 = create_dependency(&pool, a, b, "blocks", author).await.unwrap();
        assert!(matches!(r1, CreateDependencyResult::Success(_)));
        let r2 = create_dependency(&pool, b, c, "blocks", author).await.unwrap();
        assert!(matches!(r2, CreateDependencyResult::Success(_)));

        // C→A を追加すると A→B→C→A の循環になるため拒否されるはず
        let r3 = create_dependency(&pool, c, a, "blocks", author).await.unwrap();
        assert!(matches!(r3, CreateDependencyResult::CircularDependency));
    }

    /// A→B, A→C のように分岐しているだけでは循環にならないことを確認する
    /// (循環検知が誤検知しないことの確認)。
    #[tokio::test]
    async fn create_dependency_allows_non_circular_branch() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let author = test_support::create_test_user(&pool, "branch-author").await;
        let project = test_support::create_test_project(&pool, "BR", author).await;
        let a = test_support::create_test_ticket(&pool, project, "BR-A", author).await;
        let b = test_support::create_test_ticket(&pool, project, "BR-B", author).await;
        let c = test_support::create_test_ticket(&pool, project, "BR-C", author).await;

        let r1 = create_dependency(&pool, a, b, "blocks", author).await.unwrap();
        assert!(matches!(r1, CreateDependencyResult::Success(_)));

        // A→C は A→B と無関係の新規依存であり循環ではない
        let r2 = create_dependency(&pool, a, c, "blocks", author).await.unwrap();
        assert!(matches!(r2, CreateDependencyResult::Success(_)));
    }

    /// 自己参照は循環検知より前の自己参照チェックで拒否されることを確認する。
    #[tokio::test]
    async fn create_dependency_rejects_self_reference() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let author = test_support::create_test_user(&pool, "self-author").await;
        let project = test_support::create_test_project(&pool, "SLF", author).await;
        let a = test_support::create_test_ticket(&pool, project, "SLF-A", author).await;

        let r = create_dependency(&pool, a, a, "blocks", author).await.unwrap();
        assert!(matches!(r, CreateDependencyResult::SelfReference));
    }

    /// 同一(from, to)ペアの重複追加は拒否されることを確認する。
    #[tokio::test]
    async fn create_dependency_rejects_duplicate() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let author = test_support::create_test_user(&pool, "dup-author").await;
        let project = test_support::create_test_project(&pool, "DUP", author).await;
        let a = test_support::create_test_ticket(&pool, project, "DUP-A", author).await;
        let b = test_support::create_test_ticket(&pool, project, "DUP-B", author).await;

        let r1 = create_dependency(&pool, a, b, "blocks", author).await.unwrap();
        assert!(matches!(r1, CreateDependencyResult::Success(_)));
        let r2 = create_dependency(&pool, a, b, "blocks", author).await.unwrap();
        assert!(matches!(r2, CreateDependencyResult::Duplicate));
    }

    /// 存在しないチケットへの依存追加は拒否されることを確認する。
    #[tokio::test]
    async fn create_dependency_rejects_missing_to_task() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let author = test_support::create_test_user(&pool, "missing-author").await;
        let project = test_support::create_test_project(&pool, "MIS", author).await;
        let a = test_support::create_test_ticket(&pool, project, "MIS-A", author).await;

        let r = create_dependency(&pool, a, 999_999_999, "blocks", author).await.unwrap();
        assert!(matches!(r, CreateDependencyResult::ToTaskNotFound));
    }

    /// find_by_id / api_find_by_key の基本的なCRUD往復を確認する。
    #[tokio::test]
    async fn find_by_id_returns_created_ticket() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let author = test_support::create_test_user(&pool, "crud-author").await;
        let project = test_support::create_test_project(&pool, "CRUD", author).await;
        let ticket_id = test_support::create_test_ticket(&pool, project, "CRUD-T", author).await;

        let found = find_by_id(&pool, ticket_id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, ticket_id);
    }

    /// find_all がステータス・プロジェクトIDフィルタで正しく絞り込めることを確認する
    /// (find_all/find_by_id/find_by_keyはDEMO-000032のテスト作成中に、
    /// 実在しないテーブル/カラム名を参照しておりnotify_ticket_event等の
    /// 実行経路が本番でエラーになっていたことが判明したため修正した。
    /// 動的フィルタもformat!()による文字列結合からQueryBuilderのbindに
    /// 修正している)。
    #[tokio::test]
    async fn find_all_filters_by_status_and_project() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let author = test_support::create_test_user(&pool, "filter-author").await;
        let project_a = test_support::create_test_project(&pool, "FA", author).await;
        let project_b = test_support::create_test_project(&pool, "FB", author).await;
        let ticket_open = test_support::create_test_ticket(&pool, project_a, "FIL-OPEN", author).await;
        let ticket_other_project = test_support::create_test_ticket(&pool, project_b, "FIL-OTHER", author).await;

        sqlx::query("UPDATE tickets_ticket SET status = 'closed' WHERE id = $1")
            .bind(ticket_other_project)
            .execute(&pool)
            .await
            .unwrap();

        let mut filter = TicketFilter::default();
        filter.status = Some("open".to_string());
        filter.project_id = Some(project_a);

        let results = find_all(&pool, &filter).await.unwrap();
        assert!(results.iter().any(|t| t.id == ticket_open));
        assert!(results.iter().all(|t| t.project_id == Some(project_a)));
        assert!(results.iter().all(|t| t.status == "open"));
    }

    /// find_dependency_graph_for_project がcycles配列を含み、
    /// 保存済み位置がある場合はgraphPositionX/Yが設定されることを確認する。
    #[tokio::test]
    async fn find_dependency_graph_includes_cycles_with_positions() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let author = test_support::create_test_user(&pool, "dgraph-author").await;
        let project = test_support::create_test_project(&pool, "DG", author).await;

        // テスト用のCycleを作成
        let today = chrono::Local::now().naive_local().date();
        use crate::infrastructure::repositories::cycle_repo::{create_cycle, update_cycle_graph_position};
        use crate::domain::models::cycle_api::CycleWriteIn;

        let cycle_id = create_cycle(
            &pool,
            &CycleWriteIn {
                project: Some(project),
                name: "Test Cycle".to_string(),
                description: String::new(),
                start_date: today,
                end_date: today + chrono::Duration::days(14),
                status: "planned".to_string(),
                team_id: None,
            },
            author,
        )
        .await
        .expect("Failed to create cycle");

        // 依存関係グラフを取得（位置未設定の状態）
        let graph = find_dependency_graph_for_project(&pool, project)
            .await
            .expect("find_dependency_graph_for_project failed");
        assert!(!graph.cycles.is_empty());
        let cycle_entry = graph.cycles.iter().find(|c| c.id == cycle_id).expect("Cycle not found");
        assert!(cycle_entry.graph_position_x.is_none());
        assert!(cycle_entry.graph_position_y.is_none());

        // 位置を保存
        let _ = update_cycle_graph_position(&pool, cycle_id, 100.5, 200.75)
            .await
            .expect("update_cycle_graph_position failed");

        // 再度グラフを取得して位置が保存されていることを確認
        let graph = find_dependency_graph_for_project(&pool, project)
            .await
            .expect("find_dependency_graph_for_project failed");
        let cycle_entry = graph.cycles.iter().find(|c| c.id == cycle_id).expect("Cycle not found");
        assert_eq!(cycle_entry.graph_position_x, Some(100.5));
        assert_eq!(cycle_entry.graph_position_y, Some(200.75));
    }

    /// team_id 列追加後も一覧の集計列（コメント数など）を正しい位置から読むこと。
    /// 列番号がずれると i64 デコードでパニックし、一覧 API が 502 になる。
    #[tokio::test]
    async fn api_find_all_reads_counts_after_team_columns() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let author = test_support::create_test_user(&pool, "list-team-author").await;
        let project = test_support::create_test_project(&pool, "LTEAM", author).await;
        let ticket_no_team = test_support::create_test_ticket(&pool, project, "LTEAM-N", author).await;
        let ticket_with_team = test_support::create_test_ticket(&pool, project, "LTEAM-T", author).await;

        let team_id: i32 = sqlx::query_scalar(
            "INSERT INTO m_team (name, slug, description, icon, color, slack_webhook_url, is_active, created_at)
             VALUES ($1, $2, '', '👥', '#6366f1', '', true, NOW())
             RETURNING id::int4"
        )
        .bind(format!("テストチーム-{}", test_support::unique_suffix()))
        .bind(format!("team-{}", test_support::unique_suffix()))
        .fetch_one(&pool)
        .await
        .unwrap();

        sqlx::query("UPDATE tickets_ticket SET team_id = $1 WHERE id = $2")
            .bind(team_id)
            .bind(ticket_with_team)
            .execute(&pool)
            .await
            .unwrap();

        let filter = ApiTicketFilter {
            project: Some(project),
            ..ApiTicketFilter::default()
        };
        let results = api_find_all(&pool, &filter, "-updated_at", None, 1)
            .await
            .expect("api_find_all should not panic after team columns");

        let no_team = results.iter().find(|t| t.id == ticket_no_team).expect("ticket without team");
        assert!(no_team.team.is_none());
        assert_eq!(no_team.comment_count, 0);
        assert_eq!(no_team.child_count, 0);
        assert_eq!(no_team.total_time_spent, 0);

        let with_team = results.iter().find(|t| t.id == ticket_with_team).expect("ticket with team");
        assert_eq!(with_team.team.as_ref().map(|t| t.id), Some(team_id));
        assert_eq!(with_team.comment_count, 0);
    }
}
