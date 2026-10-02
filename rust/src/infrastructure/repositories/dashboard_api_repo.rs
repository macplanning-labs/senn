use serde_json::{json, Value};
/// infrastructure/repositories/dashboard_api_repo.rs — ダッシュボード JSON API 永続化
///
/// apps/api/views/dashboard.py, apps/tickets/domain/dashboard_service.py,
/// apps/tickets/domain/widget_renderers.py の移植。
/// 既存 infrastructure/repositories/dashboard関連(HTML画面用、存在しない)とは
/// 独立した新規実装。t_dashboard / t_dashboard_widget テーブルを使用。
use sqlx::{PgPool, Postgres, QueryBuilder, Row};

use crate::domain::access::Scope;
use crate::infrastructure::access::scope_sql;

pub async fn is_staff(pool: &PgPool, user_id: i32) -> anyhow::Result<bool> {
    let is_staff: bool = sqlx::query_scalar("SELECT is_staff FROM accounts_user WHERE id = $1")
        .bind(user_id)
        .fetch_one(pool)
        .await?;
    Ok(is_staff)
}

/// 集計の見える範囲(アクセス制御の再設計 D-5。設計書 §5.2「集計の中身は、閲覧者が見えるデータだけで計算する」)
pub enum DashAccess {
    /// 今の判定(staff は全件 / チームの所属・有効なプロジェクト単位の所属)。プロジェクトを指定した
    /// 集計には、今は確認が無い(その動作も今のまま)。フェーズ H で削除する
    Legacy { user_id: i32, staff: bool },
    /// 新しい判定
    Scope(Scope),
}

impl DashAccess {
    fn scope(&self) -> Option<&Scope> {
        match self {
            DashAccess::Scope(s) => Some(s),
            DashAccess::Legacy { .. } => None,
        }
    }
}

/// チケット(別名 `t`)の見える範囲の条件。`project_given` は、プロジェクトを指定した集計か
fn push_ticket_access(
    qb: &mut QueryBuilder<'_, Postgres>,
    access: &DashAccess,
    project_given: bool,
) {
    match access {
        DashAccess::Legacy { .. } if project_given => {
            qb.push("TRUE");
        }
        DashAccess::Legacy { staff: true, .. } => {
            qb.push("TRUE");
        }
        DashAccess::Legacy { user_id, .. } => {
            qb.push(
                "(t.team_id IS NOT NULL AND EXISTS (
                    SELECT 1 FROM t_team_membership tm
                    LEFT JOIN tickets_project sp ON tm.scoped_project_id = sp.id
                    WHERE tm.team_id = t.team_id AND tm.user_id = ",
            )
            .push_bind(*user_id)
            .push(
                " AND (
                        tm.scoped_project_id IS NULL OR
                        (t.project_id IS NOT NULL AND tm.scoped_project_id = t.project_id AND
                            (tm.end_date IS NULL OR NOW()::date <= tm.end_date + (sp.grace_period_days || ' days')::interval))
                    )
                ))",
            );
        }
        DashAccess::Scope(scope) => scope_sql::push_ticket_visible(qb, "t", scope),
    }
}

pub async fn get_dashboard_stats(
    pool: &PgPool,
    access: &DashAccess,
    project_id: Option<i32>,
) -> anyhow::Result<Value> {
    let mut qb = QueryBuilder::new(
        "SELECT
            COUNT(*) FILTER (WHERE t.status IN ('open','in_progress'))::int8 AS open_tickets,
            COUNT(*) FILTER (WHERE t.due_date < CURRENT_DATE AND t.status IN ('open','in_progress'))::int8 AS overdue_tickets,
            COUNT(*) FILTER (WHERE t.status = 'closed' AND t.closed_at >= NOW() - INTERVAL '7 days')::int8 AS completed_this_week,
            COUNT(*) FILTER (WHERE t.due_date <= CURRENT_DATE + INTERVAL '3 days' AND t.due_date >= CURRENT_DATE AND t.status IN ('open','in_progress'))::int8 AS due_soon_tickets
         FROM tickets_ticket t
         WHERE ",
    );
    if let Some(pid) = project_id {
        qb.push("t.project_id = ").push_bind(pid).push(" AND ");
    }
    push_ticket_access(&mut qb, access, project_id.is_some());
    let row = qb.build().fetch_one(pool).await?;

    let total_projects: i64 = match access {
        DashAccess::Legacy { staff: true, .. } => {
            sqlx::query_scalar("SELECT COUNT(*) FROM tickets_project")
                .fetch_one(pool)
                .await?
        }
        DashAccess::Legacy { user_id, .. } => {
            sqlx::query_scalar(
                "SELECT COUNT(DISTINCT p.id) FROM tickets_project p
                 JOIN tickets_project_teams pt ON p.id = pt.project_id
                 JOIN t_team_membership tm ON tm.team_id = pt.team_id
                 WHERE tm.user_id = $1 AND (
                     tm.scoped_project_id IS NULL OR
                     (tm.scoped_project_id = p.id AND
                         (tm.end_date IS NULL OR NOW()::date <= tm.end_date + (p.grace_period_days || ' days')::interval))
                 )"
            )
            .bind(*user_id)
            .fetch_one(pool)
            .await?
        }
        DashAccess::Scope(scope) => {
            let mut qb = QueryBuilder::new("SELECT COUNT(*) FROM tickets_project p WHERE ");
            scope_sql::push_project_visible(&mut qb, "p.id", scope);
            qb.build_query_scalar().fetch_one(pool).await?
        }
    };

    Ok(json!({
        "open_tickets": row.get::<i64, _>("open_tickets"),
        "overdue_tickets": row.get::<i64, _>("overdue_tickets"),
        "completed_this_week": row.get::<i64, _>("completed_this_week"),
        "due_soon_tickets": row.get::<i64, _>("due_soon_tickets"),
        "total_projects": total_projects,
    }))
}

/// 自分が担当のチケット。新しい判定では、今も見えるチケットだけ
pub async fn get_my_tickets(
    pool: &PgPool,
    user_id: i32,
    limit: i64,
    scope: Option<&Scope>,
) -> anyhow::Result<Vec<Value>> {
    let mut qb = QueryBuilder::new(
        "SELECT DISTINCT t.id::int4, t.ticket_key, t.title, t.status, t.priority,
            t.due_date, t.updated_at, p.prefix
         FROM tickets_ticket t
         JOIN tickets_ticket_assignees ta ON ta.ticketmodel_id = t.id
         LEFT JOIN tickets_project p ON t.project_id = p.id
         WHERE ta.user_id = ",
    );
    qb.push_bind(user_id)
        .push(" AND t.status IN ('backlog','open','in_progress')");
    if let Some(scope) = scope {
        qb.push(" AND ");
        scope_sql::push_ticket_visible(&mut qb, "t", scope);
    }
    qb.push(" ORDER BY t.updated_at DESC LIMIT ")
        .push_bind(limit);
    let rows = qb.build().fetch_all(pool).await?;

    Ok(rows
        .iter()
        .map(|row| {
            let due_date: Option<chrono::NaiveDate> = row.get("due_date");
            let updated_at: chrono::DateTime<chrono::Utc> = row.get("updated_at");
            json!({
                "id": row.get::<i32, _>("id"),
                "ticket_key": row.get::<String, _>("ticket_key"),
                "title": row.get::<String, _>("title"),
                "status": row.get::<String, _>("status"),
                "priority": row.get::<String, _>("priority"),
                "due_date": due_date.map(|d| d.format("%Y-%m-%d").to_string()),
                "updated_at": updated_at.to_rfc3339(),
                "project_key": row.get::<Option<String>, _>("prefix"),
            })
        })
        .collect())
}

pub async fn get_recent_activity(
    pool: &PgPool,
    access: &DashAccess,
    limit: i64,
) -> anyhow::Result<Vec<Value>> {
    let mut qb = QueryBuilder::new(
        "SELECT
            h.id::int4, t.id::int4 as ticket_id, t.ticket_key, t.title as ticket_title,
            p.prefix as project_key, h.old_status, h.new_status, h.changed_at,
            COALESCE(NULLIF(u.first_name, ''), u.username) as changed_by
         FROM tickets_status_history h
         JOIN tickets_ticket t ON h.ticket_id = t.id
         LEFT JOIN tickets_project p ON t.project_id = p.id
         LEFT JOIN accounts_user u ON h.changed_by_id = u.id
         WHERE ",
    );
    push_ticket_access(&mut qb, access, false);
    qb.push(" ORDER BY h.changed_at DESC LIMIT ")
        .push_bind(limit);
    let rows = qb.build().fetch_all(pool).await?;

    Ok(rows
        .iter()
        .map(|row| {
            let changed_at: chrono::DateTime<chrono::Utc> = row.get("changed_at");
            let changed_by: Option<String> = row.get("changed_by");
            json!({
                "id": row.get::<i32, _>("id"),
                "ticket_id": row.get::<i32, _>("ticket_id"),
                "ticket_key": row.get::<String, _>("ticket_key"),
                "ticket_title": row.get::<String, _>("ticket_title"),
                "project_key": row.get::<Option<String>, _>("project_key"),
                "old_status": row.get::<String, _>("old_status"),
                "new_status": row.get::<String, _>("new_status"),
                "changed_by": changed_by.unwrap_or_else(|| "System".to_string()),
                "changed_at": changed_at.to_rfc3339(),
            })
        })
        .collect())
}

async fn render_ticket_overview(
    pool: &PgPool,
    project_id: Option<i32>,
    access: &DashAccess,
) -> anyhow::Result<Value> {
    // 合計と、プロジェクト別内訳で、同じ条件を使う
    let filters = |qb: &mut QueryBuilder<'_, Postgres>| {
        if let Some(pid) = project_id {
            qb.push("t.project_id = ").push_bind(pid).push(" AND ");
        }
        push_ticket_access(qb, access, project_id.is_some());
    };

    let mut qb = QueryBuilder::new(
        "SELECT
            COUNT(*) FILTER (WHERE t.status = 'open')::int8 AS open,
            COUNT(*) FILTER (WHERE t.status = 'in_progress')::int8 AS in_progress,
            COUNT(*) FILTER (WHERE t.status = 'resolved')::int8 AS resolved,
            COUNT(*) FILTER (WHERE t.status = 'closed')::int8 AS closed
         FROM tickets_ticket t
         WHERE ",
    );
    filters(&mut qb);
    let row = qb.build().fetch_one(pool).await?;

    let open: i64 = row.get("open");
    let in_progress: i64 = row.get("in_progress");
    let resolved: i64 = row.get("resolved");
    let closed: i64 = row.get("closed");

    let mut qb = QueryBuilder::new(
        "SELECT p.prefix, p.name,
            COUNT(*) FILTER (WHERE t.status = 'open')::int8 AS open,
            COUNT(*) FILTER (WHERE t.status = 'in_progress')::int8 AS in_progress,
            COUNT(*) FILTER (WHERE t.status = 'resolved')::int8 AS resolved,
            COUNT(*) FILTER (WHERE t.status = 'closed')::int8 AS closed
         FROM tickets_ticket t
         JOIN tickets_project p ON p.id = t.project_id
         WHERE ",
    );
    filters(&mut qb);
    qb.push(" GROUP BY p.id, p.prefix, p.name HAVING COUNT(*) > 0 ORDER BY p.prefix");
    let project_rows = qb.build().fetch_all(pool).await?;

    let by_project: Vec<Value> = project_rows
        .iter()
        .map(|r| {
            json!({
                "project_key": r.get::<String, _>("prefix"),
                "project_name": r.get::<String, _>("name"),
                "open": r.get::<i64, _>("open"),
                "in_progress": r.get::<i64, _>("in_progress"),
                "resolved": r.get::<i64, _>("resolved"),
                "closed": r.get::<i64, _>("closed"),
            })
        })
        .collect();

    Ok(json!({
        "open": open,
        "in_progress": in_progress,
        "resolved": resolved,
        "closed": closed,
        "total": open + in_progress + resolved + closed,
        "by_project": by_project,
    }))
}

/// 最近更新された Wiki。新しい判定では、見える Wiki だけ(今の判定には確認が無い)
async fn render_recent_wiki(
    pool: &PgPool,
    limit: i64,
    scope: Option<&Scope>,
) -> anyhow::Result<Value> {
    let mut qb = QueryBuilder::new(
        "SELECT w.id::int4, w.title, w.slug, p.prefix,
            w.updated_at, COALESCE(NULLIF(u.first_name, ''), u.username) as last_editor
         FROM wiki_page w
         LEFT JOIN tickets_project p ON w.project_id = p.id
         LEFT JOIN accounts_user u ON w.last_editor_id = u.id
         WHERE ",
    );
    match scope {
        Some(scope) => scope_sql::push_wiki_visible(&mut qb, "w", scope),
        None => {
            qb.push("TRUE");
        }
    }
    qb.push(" ORDER BY w.updated_at DESC LIMIT ")
        .push_bind(limit);
    let rows = qb.build().fetch_all(pool).await?;

    let items: Vec<Value> = rows
        .iter()
        .map(|row| {
            let updated_at: chrono::DateTime<chrono::Utc> = row.get("updated_at");
            json!({
                "id": row.get::<i32, _>("id"),
                "title": row.get::<String, _>("title"),
                "slug": row.get::<String, _>("slug"),
                "projectKey": row.get::<Option<String>, _>("prefix"),
                "updatedAt": updated_at.to_rfc3339(),
                "lastEditor": row.get::<Option<String>, _>("last_editor"),
            })
        })
        .collect();

    Ok(Value::Array(items))
}

/// 未読の通知。新しい判定では、参照先のチケットが今も見える物だけ(通知の一覧と同じ規則。設計書 §5.2)
async fn render_unread_notifications(
    pool: &PgPool,
    user_id: i32,
    limit: i64,
    scope: Option<&Scope>,
) -> anyhow::Result<Value> {
    let visible = |qb: &mut QueryBuilder<'_, Postgres>| {
        if let Some(scope) = scope {
            qb.push(" AND (n.ticket_id IS NULL OR EXISTS (SELECT 1 FROM tickets_ticket t WHERE t.id = n.ticket_id AND ");
            scope_sql::push_ticket_visible(qb, "t", scope);
            qb.push("))");
        }
    };
    let mut qb =
        QueryBuilder::new("SELECT COUNT(*) FROM notifications_notification n WHERE n.user_id = ");
    qb.push_bind(user_id).push(" AND n.is_read = false");
    visible(&mut qb);
    let count: i64 = qb.build_query_scalar().fetch_one(pool).await?;

    let mut qb = QueryBuilder::new(
        "SELECT n.id::int4, n.title, n.message, n.category, n.ticket_id::int4, n.created_at
         FROM notifications_notification n
         WHERE n.user_id = ",
    );
    qb.push_bind(user_id).push(" AND n.is_read = false");
    visible(&mut qb);
    qb.push(" ORDER BY n.created_at DESC LIMIT ")
        .push_bind(limit);
    let rows = qb.build().fetch_all(pool).await?;

    let items: Vec<Value> = rows
        .iter()
        .map(|row| {
            let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
            json!({
                "id": row.get::<i32, _>("id"),
                "title": row.get::<String, _>("title"),
                "message": row.get::<String, _>("message"),
                "category": row.get::<String, _>("category"),
                "ticketId": row.get::<Option<i32>, _>("ticket_id"),
                "createdAt": created_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(json!({ "count": count, "items": items }))
}

/// 進行中のサイクル。新しい判定では、見えるチームのサイクルだけ
async fn render_sprint_health(
    pool: &PgPool,
    project_id: Option<i32>,
    scope: Option<&Scope>,
) -> anyhow::Result<Value> {
    let cycle_id: Option<i32> = if let Some(pid) = project_id {
        let mut qb = QueryBuilder::new("SELECT c.id::int4 FROM t_cycle c WHERE c.project_id = ");
        qb.push_bind(pid).push(" AND c.status = 'active'");
        if let Some(scope) = scope {
            qb.push(" AND ");
            scope_sql::push_cycle_visible(&mut qb, "c", scope);
        }
        qb.push(" ORDER BY c.start_date DESC LIMIT 1");
        qb.build_query_scalar().fetch_optional(pool).await?
    } else {
        None
    };

    Ok(json!({ "project_id": project_id, "cycle_id": cycle_id }))
}

/// widget_type + config から該当ウィジェットのデータを生成する(Djangoの WIDGET_RENDERERS 相当)。
pub async fn render_widget_data(
    pool: &PgPool,
    widget_type: &str,
    user_id: i32,
    access: &DashAccess,
    config: &Value,
) -> anyhow::Result<Value> {
    let project_id = config
        .get("project_id")
        .and_then(|v| v.as_i64())
        .map(|v| v as i32);
    let limit = config.get("limit").and_then(|v| v.as_i64()).unwrap_or(10);

    match widget_type {
        "stats_cards" => get_dashboard_stats(pool, access, project_id).await,
        "ticket_overview" => render_ticket_overview(pool, project_id, access).await,
        "recent_activity" => {
            let items = get_recent_activity(
                pool,
                access,
                if config.get("limit").is_some() {
                    limit
                } else {
                    10
                },
            )
            .await?;
            Ok(Value::Array(items))
        }
        "my_tickets" => {
            let items = get_my_tickets(
                pool,
                user_id,
                if config.get("limit").is_some() {
                    limit
                } else {
                    8
                },
                access.scope(),
            )
            .await?;
            Ok(Value::Array(items))
        }
        "recent_wiki" => {
            render_recent_wiki(
                pool,
                if config.get("limit").is_some() {
                    limit
                } else {
                    5
                },
                access.scope(),
            )
            .await
        }
        "unread_notifications" => {
            render_unread_notifications(
                pool,
                user_id,
                if config.get("limit").is_some() {
                    limit
                } else {
                    8
                },
                access.scope(),
            )
            .await
        }
        "sprint_health" => render_sprint_health(pool, project_id, access.scope()).await,
        _ => Ok(json!({})),
    }
}

pub const KNOWN_WIDGET_TYPES: &[&str] = &[
    "stats_cards",
    "ticket_overview",
    "recent_activity",
    "my_tickets",
    "recent_wiki",
    "unread_notifications",
    "sprint_health",
];

const DEFAULT_WIDGETS: &[(&str, i32, i32)] = &[
    ("stats_cards", 0, 2),
    ("ticket_overview", 1, 1),
    ("recent_activity", 2, 1),
    ("my_tickets", 3, 1),
    ("recent_wiki", 4, 1),
    ("unread_notifications", 5, 2),
];

/// デフォルトダッシュボードを取得、無ければ6ウィジェット付きで自動作成する。
pub async fn get_or_create_default_dashboard(pool: &PgPool, user_id: i32) -> anyhow::Result<i32> {
    let existing: Option<i32> = sqlx::query_scalar(
        "SELECT id::int4 FROM t_dashboard WHERE owner_id = $1 AND is_default = true",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    if let Some(id) = existing {
        return Ok(id);
    }

    let mut tx = pool.begin().await?;

    let dashboard_id: i32 = sqlx::query_scalar(
        "INSERT INTO t_dashboard (name, owner_id, is_default, layout, sort_order, created_at, updated_at)
         VALUES ('My Dashboard', $1, true, 'grid-2col', 0, NOW(), NOW())
         RETURNING id::int4"
    )
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await?;

    for (widget_type, position, span) in DEFAULT_WIDGETS {
        sqlx::query(
            "INSERT INTO t_dashboard_widget (dashboard_id, widget_type, position, span, config, is_visible, created_at)
             VALUES ($1, $2, $3, $4, '{}'::jsonb, true, NOW())"
        )
        .bind(dashboard_id)
        .bind(widget_type)
        .bind(position)
        .bind(span)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(dashboard_id)
}

pub async fn serialize_dashboard(
    pool: &PgPool,
    dashboard_id: i32,
    user_id: i32,
    access: &DashAccess,
) -> anyhow::Result<Value> {
    let dash_row =
        sqlx::query("SELECT id::int4, name, layout, is_default FROM t_dashboard WHERE id = $1")
            .bind(dashboard_id)
            .fetch_one(pool)
            .await?;

    let widget_rows = sqlx::query(
        "SELECT id::int4, widget_type, position, span, config
         FROM t_dashboard_widget WHERE dashboard_id = $1 AND is_visible = true ORDER BY position",
    )
    .bind(dashboard_id)
    .fetch_all(pool)
    .await?;

    let mut widgets = Vec::new();
    for w in &widget_rows {
        let widget_type: String = w.get("widget_type");
        let config: Value = w.get("config");
        let data = render_widget_data(pool, &widget_type, user_id, access, &config).await?;
        widgets.push(json!({
            "id": w.get::<i32, _>("id"),
            "widgetType": widget_type,
            "position": w.get::<i32, _>("position"),
            "span": w.get::<i32, _>("span"),
            "config": config,
            "data": data,
        }));
    }

    Ok(json!({
        "id": dash_row.get::<i32, _>("id"),
        "name": dash_row.get::<String, _>("name"),
        "layout": dash_row.get::<String, _>("layout"),
        "isDefault": dash_row.get::<bool, _>("is_default"),
        "widgets": widgets,
    }))
}

pub async fn find_dashboard_owned(
    pool: &PgPool,
    dashboard_id: i32,
    user_id: i32,
) -> anyhow::Result<bool> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM t_dashboard WHERE id = $1 AND owner_id = $2)",
    )
    .bind(dashboard_id)
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

pub async fn list_dashboards(pool: &PgPool, user_id: i32) -> anyhow::Result<Vec<Value>> {
    get_or_create_default_dashboard(pool, user_id).await?;

    let rows = sqlx::query(
        "SELECT d.id::int4, d.name, d.layout, d.is_default,
            (SELECT COUNT(*) FROM t_dashboard_widget w WHERE w.dashboard_id = d.id AND w.is_visible = true)::int8 as widget_count
         FROM t_dashboard d
         WHERE d.owner_id = $1
         ORDER BY d.is_default DESC, d.sort_order, d.created_at DESC"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|row| {
            json!({
                "id": row.get::<i32, _>("id"),
                "name": row.get::<String, _>("name"),
                "layout": row.get::<String, _>("layout"),
                "isDefault": row.get::<bool, _>("is_default"),
                "widgetCount": row.get::<i64, _>("widget_count"),
            })
        })
        .collect())
}

pub async fn create_dashboard(
    pool: &PgPool,
    user_id: i32,
    name: &str,
    layout: &str,
    with_defaults: bool,
) -> anyhow::Result<(i32, String)> {
    let mut tx = pool.begin().await?;

    let dashboard_id: i32 = sqlx::query_scalar(
        "INSERT INTO t_dashboard (name, owner_id, is_default, layout, sort_order, created_at, updated_at)
         VALUES ($1, $2, false, $3, 0, NOW(), NOW())
         RETURNING id::int4"
    )
    .bind(name)
    .bind(user_id)
    .bind(layout)
    .fetch_one(&mut *tx)
    .await?;

    if with_defaults {
        for (widget_type, position, span) in DEFAULT_WIDGETS {
            sqlx::query(
                "INSERT INTO t_dashboard_widget (dashboard_id, widget_type, position, span, config, is_visible, created_at)
                 VALUES ($1, $2, $3, $4, '{}'::jsonb, true, NOW())"
            )
            .bind(dashboard_id)
            .bind(widget_type)
            .bind(position)
            .bind(span)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;
    Ok((dashboard_id, name.to_string()))
}

pub async fn update_dashboard(
    pool: &PgPool,
    dashboard_id: i32,
    user_id: i32,
    name: Option<&str>,
    layout: Option<&str>,
) -> anyhow::Result<Option<(String, String)>> {
    if !find_dashboard_owned(pool, dashboard_id, user_id).await? {
        return Ok(None);
    }

    let existing = sqlx::query("SELECT name, layout FROM t_dashboard WHERE id = $1")
        .bind(dashboard_id)
        .fetch_one(pool)
        .await?;

    let final_name = name.map(|s| s.to_string()).unwrap_or(existing.get("name"));
    let final_layout = layout
        .map(|s| s.to_string())
        .unwrap_or(existing.get("layout"));

    sqlx::query("UPDATE t_dashboard SET name = $1, layout = $2, updated_at = NOW() WHERE id = $3")
        .bind(&final_name)
        .bind(&final_layout)
        .bind(dashboard_id)
        .execute(pool)
        .await?;

    Ok(Some((final_name, final_layout)))
}

pub enum DeleteDashboardResult {
    Deleted,
    NotFound,
    IsDefault,
}

pub async fn delete_dashboard(
    pool: &PgPool,
    dashboard_id: i32,
    user_id: i32,
) -> anyhow::Result<DeleteDashboardResult> {
    let row = sqlx::query("SELECT is_default FROM t_dashboard WHERE id = $1 AND owner_id = $2")
        .bind(dashboard_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await?;

    let row = match row {
        Some(r) => r,
        None => return Ok(DeleteDashboardResult::NotFound),
    };

    let is_default: bool = row.get("is_default");
    if is_default {
        return Ok(DeleteDashboardResult::IsDefault);
    }

    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM t_dashboard_widget WHERE dashboard_id = $1")
        .bind(dashboard_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM t_dashboard WHERE id = $1")
        .bind(dashboard_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    Ok(DeleteDashboardResult::Deleted)
}

pub enum AddWidgetResult {
    Success {
        id: i32,
        widget_type: String,
        position: i32,
    },
    DashboardNotFound,
    UnknownWidgetType,
}

pub async fn add_widget(
    pool: &PgPool,
    dashboard_id: Option<i32>,
    user_id: i32,
    widget_type: &str,
    span: i32,
    config: &Value,
) -> anyhow::Result<AddWidgetResult> {
    let dashboard_id = match dashboard_id {
        Some(id) => {
            if !find_dashboard_owned(pool, id, user_id).await? {
                return Ok(AddWidgetResult::DashboardNotFound);
            }
            id
        }
        None => get_or_create_default_dashboard(pool, user_id).await?,
    };

    if !KNOWN_WIDGET_TYPES.contains(&widget_type) {
        return Ok(AddWidgetResult::UnknownWidgetType);
    }

    let max_pos: Option<i32> =
        sqlx::query_scalar("SELECT MAX(position) FROM t_dashboard_widget WHERE dashboard_id = $1")
            .bind(dashboard_id)
            .fetch_one(pool)
            .await?;
    let position = max_pos.unwrap_or(-1) + 1;

    let widget_id: i32 = sqlx::query_scalar(
        "INSERT INTO t_dashboard_widget (dashboard_id, widget_type, position, span, config, is_visible, created_at)
         VALUES ($1, $2, $3, $4, $5, true, NOW())
         RETURNING id::int4"
    )
    .bind(dashboard_id)
    .bind(widget_type)
    .bind(position)
    .bind(span)
    .bind(config)
    .fetch_one(pool)
    .await?;

    Ok(AddWidgetResult::Success {
        id: widget_id,
        widget_type: widget_type.to_string(),
        position,
    })
}

pub async fn remove_widget(pool: &PgPool, widget_id: i32, user_id: i32) -> anyhow::Result<bool> {
    let rows_affected = sqlx::query(
        "UPDATE t_dashboard_widget SET is_visible = false
         WHERE id = $1 AND dashboard_id IN (SELECT id FROM t_dashboard WHERE owner_id = $2)",
    )
    .bind(widget_id)
    .bind(user_id)
    .execute(pool)
    .await?
    .rows_affected();

    Ok(rows_affected > 0)
}

pub async fn reorder_widgets(
    pool: &PgPool,
    widget_order: &[i32],
    user_id: i32,
) -> anyhow::Result<()> {
    for (idx, widget_id) in widget_order.iter().enumerate() {
        if let Err(e) = sqlx::query(
            "UPDATE t_dashboard_widget SET position = $1
             WHERE id = $2 AND dashboard_id IN (SELECT id FROM t_dashboard WHERE owner_id = $3)",
        )
        .bind(idx as i32)
        .bind(widget_id)
        .bind(user_id)
        .execute(pool)
        .await
        {
            tracing::error!("Failed to reorder widget {}: {:?}", widget_id, e);
        }
    }
    Ok(())
}

/// Team スコープのチケット集計（薄い集計）
/// `scope` があれば(新しい判定)、見えるチームだけ。無ければ今の判定(staff 以外はメンバーシップ必須)
pub async fn get_team_summary(
    pool: &PgPool,
    user_id: i32,
    team_slug: &str,
    scope: Option<&Scope>,
) -> anyhow::Result<Option<Value>> {
    let is_staff: bool = sqlx::query_scalar("SELECT is_staff FROM accounts_user WHERE id = $1")
        .bind(user_id)
        .fetch_one(pool)
        .await?;

    // Team 存在確認。staff 以外はメンバーシップ必須
    let team_row = if let Some(scope) = scope {
        let mut qb = QueryBuilder::new("SELECT m.id::int4 FROM m_team m WHERE m.slug = ");
        qb.push_bind(team_slug).push(" AND ");
        scope_sql::push_team_visible(&mut qb, "m.id", scope);
        qb.build().fetch_optional(pool).await?
    } else if is_staff {
        sqlx::query("SELECT m.id::int4 FROM m_team m WHERE m.slug = $1")
            .bind(team_slug)
            .fetch_optional(pool)
            .await?
    } else {
        sqlx::query(
            "SELECT m.id::int4 FROM m_team m
             WHERE m.slug = $1
             AND EXISTS (
               SELECT 1 FROM t_team_membership tm
               WHERE tm.team_id = m.id AND tm.user_id = $2
                 AND (tm.end_date IS NULL OR tm.end_date >= CURRENT_DATE)
             )",
        )
        .bind(team_slug)
        .bind(user_id)
        .fetch_optional(pool)
        .await?
    };

    let team_row = match team_row {
        Some(r) => r,
        None => return Ok(None),
    };

    let team_id: i32 = team_row.get("id");

    // チケット集計（所属 Team）。canceled は分母に入れない。backlog は open 側に含める
    let stats_row = sqlx::query(
        "SELECT
            COUNT(*) FILTER (WHERE t.status IN ('open', 'backlog'))::int8 AS open,
            COUNT(*) FILTER (WHERE t.status = 'in_progress')::int8 AS in_progress,
            COUNT(*) FILTER (WHERE t.status = 'resolved')::int8 AS resolved,
            COUNT(*) FILTER (WHERE t.status = 'closed')::int8 AS closed
         FROM tickets_ticket t
         WHERE t.team_id = $1
           AND t.status <> 'canceled'",
    )
    .bind(team_id)
    .fetch_one(pool)
    .await?;

    Ok(Some(json!({
        "team_id": team_id,
        "team_slug": team_slug,
        "tickets": {
            "open": stats_row.get::<i64, _>("open"),
            "in_progress": stats_row.get::<i64, _>("in_progress"),
            "resolved": stats_row.get::<i64, _>("resolved"),
            "closed": stats_row.get::<i64, _>("closed"),
            "total": stats_row.get::<i64, _>("open") + stats_row.get::<i64, _>("in_progress") + stats_row.get::<i64, _>("resolved") + stats_row.get::<i64, _>("closed"),
        }
    })))
}
