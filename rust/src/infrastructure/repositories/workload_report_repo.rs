/// infrastructure/repositories/workload_report_repo.rs — 稼働レポート集計
///
/// Django apps/api/views/workload_report.py の移植。

use chrono::NaiveDate;
use sqlx::{PgPool, Row};
use serde_json::{json, Value};

pub struct WorkloadReport {
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub total_minutes: i64,
    pub days_with_work: i64,
    pub avg_minutes_per_day: i64,
    pub daily: Vec<Value>,
    pub by_user: Vec<Value>,
    pub by_project: Vec<Value>,
}

pub async fn get_workload_report(
    pool: &PgPool,
    start_date: NaiveDate,
    end_date: NaiveDate,
    project_id: Option<i32>,
) -> anyhow::Result<WorkloadReport> {
    // 日別集計
    let daily_rows = sqlx::query(
        "SELECT
            DATE(te.created_at) AS d,
            COALESCE(SUM(te.duration_minutes), 0)::int8 AS total_minutes,
            COUNT(*)::int8 AS entry_count
         FROM t_time_entry te
         JOIN tickets_ticket t ON te.ticket_id = t.id
         WHERE DATE(te.created_at) >= $1
           AND ($2::int4 IS NULL OR t.project_id = $2)
         GROUP BY DATE(te.created_at)
         ORDER BY d"
    )
    .bind(start_date)
    .bind(project_id)
    .fetch_all(pool)
    .await?;

    let mut daily = Vec::new();
    let mut total_minutes: i64 = 0;
    let mut days_with_work: i64 = 0;
    for row in &daily_rows {
        let d: NaiveDate = row.get("d");
        let m: i64 = row.get("total_minutes");
        total_minutes += m;
        if m > 0 {
            days_with_work += 1;
        }
        daily.push(json!({
            "date": d.format("%Y-%m-%d").to_string(),
            "total_minutes": m,
            "entry_count": row.get::<i64, _>("entry_count"),
        }));
    }

    // ユーザー別集計
    let user_rows = sqlx::query(
        "SELECT
            u.id::int4 AS user_id, u.username,
            COALESCE(u.first_name, u.username) AS display_name,
            COALESCE(SUM(te.duration_minutes), 0)::int8 AS total_minutes,
            COUNT(*)::int8 AS entry_count
         FROM t_time_entry te
         JOIN tickets_ticket t ON te.ticket_id = t.id
         JOIN accounts_user u ON te.user_id = u.id
         WHERE DATE(te.created_at) >= $1
           AND ($2::int4 IS NULL OR t.project_id = $2)
         GROUP BY u.id, u.username, u.first_name
         ORDER BY total_minutes DESC"
    )
    .bind(start_date)
    .bind(project_id)
    .fetch_all(pool)
    .await?;

    let by_user: Vec<Value> = user_rows
        .iter()
        .map(|row| {
            json!({
                "user_id": row.get::<i32, _>("user_id"),
                "username": row.get::<String, _>("username"),
                "display_name": row.get::<String, _>("display_name"),
                "total_minutes": row.get::<i64, _>("total_minutes"),
                "entry_count": row.get::<i64, _>("entry_count"),
            })
        })
        .collect();

    // プロジェクト別集計
    let project_rows = sqlx::query(
        "SELECT
            p.id::int4 AS project_id, p.name AS project_name, p.prefix AS project_prefix,
            COALESCE(SUM(te.duration_minutes), 0)::int8 AS total_minutes,
            COUNT(*)::int8 AS entry_count
         FROM t_time_entry te
         JOIN tickets_ticket t ON te.ticket_id = t.id
         JOIN tickets_project p ON t.project_id = p.id
         WHERE DATE(te.created_at) >= $1
           AND ($2::int4 IS NULL OR t.project_id = $2)
         GROUP BY p.id, p.name, p.prefix
         ORDER BY total_minutes DESC"
    )
    .bind(start_date)
    .bind(project_id)
    .fetch_all(pool)
    .await?;

    let by_project: Vec<Value> = project_rows
        .iter()
        .map(|row| {
            json!({
                "project_id": row.get::<i32, _>("project_id"),
                "project_name": row.get::<String, _>("project_name"),
                "project_prefix": row.get::<String, _>("project_prefix"),
                "total_minutes": row.get::<i64, _>("total_minutes"),
                "entry_count": row.get::<i64, _>("entry_count"),
            })
        })
        .collect();

    let avg_minutes_per_day = if days_with_work > 0 {
        ((total_minutes as f64 / days_with_work as f64).round()) as i64
    } else {
        0
    };

    Ok(WorkloadReport {
        start_date,
        end_date,
        total_minutes,
        days_with_work,
        avg_minutes_per_day,
        daily,
        by_user,
        by_project,
    })
}
