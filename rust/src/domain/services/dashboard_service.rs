/// domain/services/dashboard_service.rs — ダッシュボード集計ロジック
///
/// 現行 Django の dashboard.py（209行）相当のビジネスロジック。
/// - 自分の課題（5フィルタ）
/// - アクティビティフィード（ステータス変更+コメント+チケット作成の時系列マージ）
/// - ステータス統計 + 月間比較
/// - メンバーパフォーマンス
/// - マイルストーン進捗

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use sqlx::PgPool;

use crate::domain::models::ticket::Ticket;

// ---------------------------------------------------------------------------
// ダッシュボード用データ構造
// ---------------------------------------------------------------------------

/// ダッシュボード全体のデータ
#[allow(dead_code)]
pub struct DashboardData {
    pub my_tickets: Vec<Ticket>,
    pub my_filter: String,
    pub my_assigned_count: i64,
    pub my_created_count: i64,
    pub my_overdue_count: i64,
    pub my_due_today_count: i64,
    pub my_due_week_count: i64,
    pub activities: Vec<Activity>,
    pub status_counts: Vec<(String, i64)>,
    pub total_count: i64,
    pub last_month_total: i64,
    pub this_month_total: i64,
    pub member_performance: Vec<MemberPerformance>,
    pub milestone_progress: Vec<MilestoneProgress>,
    pub recent_tickets: Vec<Ticket>,
    pub overdue_count: i64,
}

/// アクティビティフィード項目
pub struct Activity {
    pub activity_type: String, // "status_change" | "comment" | "ticket_created"
    pub timestamp: DateTime<Utc>,
    pub user_name: String,
    pub ticket_key: String,
    pub ticket_title: String,
    pub detail: String,
}

/// メンバーパフォーマンス
pub struct MemberPerformance {
    pub user_name: String,
    pub total: i64,
    pub closed: i64,
}

/// マイルストーン進捗
pub struct MilestoneProgress {
    pub name: String,
    pub due_date: Option<NaiveDate>,
    pub total: i64,
    pub closed: i64,
    pub percent: i32,
}

// ---------------------------------------------------------------------------
// ダッシュボードデータ構築
// ---------------------------------------------------------------------------

pub async fn build_dashboard(
    pool: &PgPool,
    user_id: i32,
    project_id: Option<i32>,
    my_filter: &str,
) -> anyhow::Result<DashboardData> {
    let today = chrono::Local::now().date_naive();

    // --- 自分の課題 ---
    let my_filter_str = if my_filter.is_empty() { "assigned" } else { my_filter };

    let my_tickets = fetch_my_tickets(pool, user_id, project_id, my_filter_str, today).await?;

    // カウント計算
    let my_assigned_count = count_my_tickets(pool, user_id, project_id, "assigned", today).await?;
    let my_created_count = count_my_tickets(pool, user_id, project_id, "created", today).await?;
    let my_overdue_count = count_my_tickets(pool, user_id, project_id, "overdue", today).await?;
    let my_due_today_count = count_my_tickets(pool, user_id, project_id, "due_today", today).await?;
    let my_due_week_count = count_my_tickets(pool, user_id, project_id, "due_this_week", today).await?;

    // --- アクティビティフィード ---
    let activities = fetch_activities(pool, project_id).await?;

    // --- ステータス統計 ---
    let status_counts = fetch_status_counts(pool, project_id).await?;
    let total_count: i64 = status_counts.iter().map(|(_, c)| c).sum();

    // --- 月間比較 ---
    let (last_month_total, this_month_total) = fetch_monthly_comparison(pool, project_id, today).await?;

    // --- 期限超過 ---
    let overdue_count = count_overdue(pool, project_id, today).await?;

    // --- メンバーパフォーマンス ---
    let member_performance = fetch_member_performance(pool, project_id).await?;

    // --- マイルストーン進捗 ---
    let milestone_progress = fetch_milestone_progress(pool, project_id, today).await?;

    // --- 最近のチケット ---
    let recent_tickets = fetch_recent_tickets(pool, project_id).await?;

    Ok(DashboardData {
        my_tickets,
        my_filter: my_filter_str.to_string(),
        my_assigned_count,
        my_created_count,
        my_overdue_count,
        my_due_today_count,
        my_due_week_count,
        activities,
        status_counts,
        total_count,
        last_month_total,
        this_month_total,
        member_performance,
        milestone_progress,
        recent_tickets,
        overdue_count,
    })
}

// ---------------------------------------------------------------------------
// 内部関数
// ---------------------------------------------------------------------------

async fn fetch_my_tickets(
    pool: &PgPool, user_id: i32, project_id: Option<i32>,
    filter: &str, today: NaiveDate,
) -> anyhow::Result<Vec<Ticket>> {
    let project_clause = if let Some(pid) = project_id {
        format!("AND t.project_id = {}", pid)
    } else {
        String::new()
    };

    let filter_clause = match filter {
        "assigned" => format!("AND t.assignee_id = {}", user_id),
        "created" => format!("AND t.author_id = {}", user_id),
        "overdue" => format!("AND t.assignee_id = {} AND t.due_date < '{}'", user_id, today),
        "due_today" => format!("AND t.assignee_id = {} AND t.due_date = '{}'", user_id, today),
        "due_this_week" => {
            let week_end = today + Duration::days(6 - today.weekday().num_days_from_monday() as i64);
            format!("AND t.assignee_id = {} AND t.due_date >= '{}' AND t.due_date <= '{}'", user_id, today, week_end)
        }
        _ => format!("AND t.assignee_id = {}", user_id),
    };

    let query = format!(
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
         WHERE t.status != '完了' {} {}
         ORDER BY t.due_date NULLS LAST, t.priority DESC, t.updated_at DESC
         LIMIT 20",
        project_clause, filter_clause,
    );

    let rows = sqlx::query_as::<_, Ticket>(&query)
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

async fn count_my_tickets(
    pool: &PgPool, user_id: i32, project_id: Option<i32>,
    filter: &str, today: NaiveDate,
) -> anyhow::Result<i64> {
    let project_clause = if let Some(pid) = project_id {
        format!("AND t.project_id = {}", pid)
    } else {
        String::new()
    };

    let filter_clause = match filter {
        "assigned" => format!("AND t.assignee_id = {}", user_id),
        "created" => format!("AND t.author_id = {}", user_id),
        "overdue" => format!("AND t.assignee_id = {} AND t.due_date < '{}'", user_id, today),
        "due_today" => format!("AND t.assignee_id = {} AND t.due_date = '{}'", user_id, today),
        "due_this_week" => {
            let week_end = today + Duration::days(6 - today.weekday().num_days_from_monday() as i64);
            format!("AND t.assignee_id = {} AND t.due_date >= '{}' AND t.due_date <= '{}'", user_id, today, week_end)
        }
        _ => format!("AND t.assignee_id = {}", user_id),
    };

    let count: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM t_tickets t WHERE t.status != '完了' {} {}",
        project_clause, filter_clause,
    ))
    .fetch_one(pool)
    .await?;
    Ok(count)
}

async fn fetch_activities(pool: &PgPool, project_id: Option<i32>) -> anyhow::Result<Vec<Activity>> {
    let project_clause = if let Some(pid) = project_id {
        format!("AND t.project_id = {}", pid)
    } else {
        String::new()
    };

    // ステータス変更
    let status_changes: Vec<(DateTime<Utc>, String, String, String, String, String)> = sqlx::query_as(&format!(
        "SELECT h.changed_at, COALESCE(u.display_name, ''), t.ticket_key, t.title,
                h.old_status, h.new_status
         FROM h_ticket_status h
         JOIN t_tickets t ON h.ticket_id = t.id
         LEFT JOIN m_users u ON h.changed_by_id = u.id
         WHERE 1=1 {}
         ORDER BY h.changed_at DESC LIMIT 20",
        project_clause
    ))
    .fetch_all(pool)
    .await?;

    // コメント
    let comments: Vec<(DateTime<Utc>, String, String, String, String)> = sqlx::query_as(&format!(
        "SELECT c.created_at, COALESCE(u.display_name, ''), t.ticket_key, t.title,
                LEFT(c.body, 200)
         FROM t_comments c
         JOIN t_tickets t ON c.ticket_id = t.id
         LEFT JOIN m_users u ON c.author_id = u.id
         WHERE 1=1 {}
         ORDER BY c.created_at DESC LIMIT 20",
        project_clause
    ))
    .fetch_all(pool)
    .await?;

    // 新規チケット
    let new_tickets: Vec<(DateTime<Utc>, String, String, String)> = sqlx::query_as(&format!(
        "SELECT t.created_at, COALESCE(u.display_name, ''), t.ticket_key, t.title
         FROM t_tickets t
         LEFT JOIN m_users u ON t.author_id = u.id
         WHERE 1=1 {}
         ORDER BY t.created_at DESC LIMIT 10",
        project_clause
    ))
    .fetch_all(pool)
    .await?;

    let mut activities = Vec::new();

    for (ts, user, key, title, old_s, new_s) in status_changes {
        activities.push(Activity {
            activity_type: "status_change".to_string(),
            timestamp: ts, user_name: user,
            ticket_key: key, ticket_title: title,
            detail: format!("{} → {}", old_s, new_s),
        });
    }
    for (ts, user, key, title, body) in comments {
        activities.push(Activity {
            activity_type: "comment".to_string(),
            timestamp: ts, user_name: user,
            ticket_key: key, ticket_title: title,
            detail: body,
        });
    }
    for (ts, user, key, title) in new_tickets {
        activities.push(Activity {
            activity_type: "ticket_created".to_string(),
            timestamp: ts, user_name: user,
            ticket_key: key, ticket_title: title,
            detail: String::new(),
        });
    }

    activities.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    activities.truncate(30);

    Ok(activities)
}

async fn fetch_status_counts(pool: &PgPool, project_id: Option<i32>) -> anyhow::Result<Vec<(String, i64)>> {
    let project_clause = if let Some(pid) = project_id {
        format!("WHERE project_id = {}", pid)
    } else {
        String::new()
    };
    let rows: Vec<(String, i64)> = sqlx::query_as(&format!(
        "SELECT status, COUNT(*) FROM t_tickets {} GROUP BY status ORDER BY status",
        project_clause
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

async fn fetch_monthly_comparison(
    pool: &PgPool, project_id: Option<i32>, today: NaiveDate,
) -> anyhow::Result<(i64, i64)> {
    let this_month_start = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today);
    let last_month_end = this_month_start.pred_opt().unwrap_or(today);
    let last_month_start = NaiveDate::from_ymd_opt(last_month_end.year(), last_month_end.month(), 1).unwrap_or(last_month_end);

    let project_clause = if let Some(pid) = project_id {
        format!("AND project_id = {}", pid)
    } else {
        String::new()
    };

    let last: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM t_tickets WHERE created_at >= '{}' AND created_at < '{}' {}",
        last_month_start, this_month_start, project_clause,
    ))
    .fetch_one(pool)
    .await?;

    let this: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM t_tickets WHERE created_at >= '{}' {}",
        this_month_start, project_clause,
    ))
    .fetch_one(pool)
    .await?;

    Ok((last, this))
}

/// ヘッダーバッジ用の期限超過件数取得（ダッシュボード以外の画面からも呼ばれる）
pub async fn get_overdue_count(pool: &PgPool, project_id: Option<i32>) -> anyhow::Result<i64> {
    let today = chrono::Local::now().date_naive();
    count_overdue(pool, project_id, today).await
}

async fn count_overdue(pool: &PgPool, project_id: Option<i32>, today: NaiveDate) -> anyhow::Result<i64> {
    let project_clause = if let Some(pid) = project_id {
        format!("AND project_id = {}", pid)
    } else {
        String::new()
    };
    let count: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM t_tickets WHERE due_date < '{}' AND status != '完了' {}",
        today, project_clause,
    ))
    .fetch_one(pool)
    .await?;
    Ok(count)
}

async fn fetch_member_performance(pool: &PgPool, project_id: Option<i32>) -> anyhow::Result<Vec<MemberPerformance>> {
    let project_clause = if let Some(pid) = project_id {
        format!("AND t.project_id = {}", pid)
    } else {
        String::new()
    };
    let rows: Vec<(String, i64, i64)> = sqlx::query_as(&format!(
        "SELECT u.display_name,
                COUNT(*) as total,
                COUNT(*) FILTER (WHERE t.status = '完了') as closed
         FROM m_users u
         JOIN t_tickets t ON t.assignee_id = u.id
         WHERE 1=1 {}
         GROUP BY u.id, u.display_name
         ORDER BY closed DESC
         LIMIT 5",
        project_clause
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(name, total, closed)| {
        MemberPerformance { user_name: name, total, closed }
    }).collect())
}

async fn fetch_milestone_progress(
    pool: &PgPool, project_id: Option<i32>, today: NaiveDate,
) -> anyhow::Result<Vec<MilestoneProgress>> {
    let cutoff = today - Duration::days(30);
    let project_clause = if let Some(pid) = project_id {
        format!("AND m.project_id = {}", pid)
    } else {
        String::new()
    };
    let rows: Vec<(String, Option<NaiveDate>, i64, i64)> = sqlx::query_as(&format!(
        "SELECT m.name, m.due_date,
                (SELECT COUNT(*) FROM t_tickets t WHERE t.milestone_id = m.id),
                (SELECT COUNT(*) FROM t_tickets t WHERE t.milestone_id = m.id AND t.status = '完了')
         FROM m_milestones m
         WHERE m.due_date >= '{}' {}
         ORDER BY m.due_date
         LIMIT 5",
        cutoff, project_clause
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(name, due, total, closed)| {
        let percent = if total > 0 { ((closed as f64 / total as f64) * 100.0).round() as i32 } else { 0 };
        MilestoneProgress { name, due_date: due, total, closed, percent }
    }).collect())
}

async fn fetch_recent_tickets(pool: &PgPool, project_id: Option<i32>) -> anyhow::Result<Vec<Ticket>> {
    let project_clause = if let Some(pid) = project_id {
        format!("AND t.project_id = {}", pid)
    } else {
        String::new()
    };
    let rows = sqlx::query_as::<_, Ticket>(&format!(
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
         WHERE 1=1 {}
         ORDER BY t.updated_at DESC LIMIT 10",
        project_clause,
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
