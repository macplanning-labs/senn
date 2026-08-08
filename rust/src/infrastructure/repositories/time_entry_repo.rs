/// infrastructure/repositories/time_entry_repo.rs — Time Entry 永続化
///
/// t_time_entry テーブルの CRUD 操作。

use sqlx::{PgPool, Row};

use crate::domain::models::time_entry_api::*;
use crate::domain::models::ticket_api::UserSummaryOut;

pub async fn find_all_time_entries(
    pool: &PgPool,
    page: i64,
    ticket: Option<i32>,
    user: Option<i32>,
) -> anyhow::Result<Vec<TimeEntryOut>> {
    const PAGE_SIZE: i64 = 50;
    let page = page.max(1);
    let offset = (page - 1) * PAGE_SIZE;

    let rows = if ticket.is_some() && user.is_some() {
        sqlx::query(
            "SELECT
                t.id::int4, t.ticket_id::int4, t.description, t.start_time, t.end_time,
                t.duration_minutes, t.created_at,
                u.id::int4 as user_id, u.username, u.email, u.display_name
             FROM t_time_entry t
             JOIN accounts_user u ON t.user_id = u.id
             WHERE t.ticket_id = $1 AND t.user_id = $2
             ORDER BY t.created_at DESC
             LIMIT $3 OFFSET $4"
        )
        .bind(ticket.unwrap() as i64)
        .bind(user.unwrap() as i64)
        .bind(PAGE_SIZE)
        .bind(offset)
        .fetch_all(pool)
        .await?
    } else if let Some(ticket_id) = ticket {
        sqlx::query(
            "SELECT
                t.id::int4, t.ticket_id::int4, t.description, t.start_time, t.end_time,
                t.duration_minutes, t.created_at,
                u.id::int4 as user_id, u.username, u.email, u.display_name
             FROM t_time_entry t
             JOIN accounts_user u ON t.user_id = u.id
             WHERE t.ticket_id = $1
             ORDER BY t.created_at DESC
             LIMIT $2 OFFSET $3"
        )
        .bind(ticket_id as i64)
        .bind(PAGE_SIZE)
        .bind(offset)
        .fetch_all(pool)
        .await?
    } else if let Some(user_id) = user {
        sqlx::query(
            "SELECT
                t.id::int4, t.ticket_id::int4, t.description, t.start_time, t.end_time,
                t.duration_minutes, t.created_at,
                u.id::int4 as user_id, u.username, u.email, u.display_name
             FROM t_time_entry t
             JOIN accounts_user u ON t.user_id = u.id
             WHERE t.user_id = $1
             ORDER BY t.created_at DESC
             LIMIT $2 OFFSET $3"
        )
        .bind(user_id as i64)
        .bind(PAGE_SIZE)
        .bind(offset)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            "SELECT
                t.id::int4, t.ticket_id::int4, t.description, t.start_time, t.end_time,
                t.duration_minutes, t.created_at,
                u.id::int4 as user_id, u.username, u.email, u.display_name
             FROM t_time_entry t
             JOIN accounts_user u ON t.user_id = u.id
             ORDER BY t.created_at DESC
             LIMIT $1 OFFSET $2"
        )
        .bind(PAGE_SIZE)
        .bind(offset)
        .fetch_all(pool)
        .await?
    };

    let entries = rows
        .into_iter()
        .map(|row| TimeEntryOut {
            id: row.get(0),
            ticket: row.get(1),
            user: UserSummaryOut {
                id: row.get(7),
                username: row.get(8),
                email: row.get(9),
                display_name: row.get(10),
            },
            description: row.get(2),
            start_time: row.get(3),
            end_time: row.get(4),
            duration_minutes: row.get(5),
            created_at: row.get(6),
        })
        .collect();

    Ok(entries)
}

pub async fn count_time_entries(
    pool: &PgPool,
    ticket: Option<i32>,
    user: Option<i32>,
) -> anyhow::Result<i64> {
    let count: i64 = if ticket.is_some() && user.is_some() {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM t_time_entry WHERE ticket_id = $1 AND user_id = $2"
        )
        .bind(ticket.unwrap() as i64)
        .bind(user.unwrap() as i64)
        .fetch_one(pool)
        .await?
    } else if let Some(ticket_id) = ticket {
        sqlx::query_scalar("SELECT COUNT(*) FROM t_time_entry WHERE ticket_id = $1")
            .bind(ticket_id as i64)
            .fetch_one(pool)
            .await?
    } else if let Some(user_id) = user {
        sqlx::query_scalar("SELECT COUNT(*) FROM t_time_entry WHERE user_id = $1")
            .bind(user_id as i64)
            .fetch_one(pool)
            .await?
    } else {
        sqlx::query_scalar("SELECT COUNT(*) FROM t_time_entry")
            .fetch_one(pool)
            .await?
    };

    Ok(count)
}

pub async fn find_time_entry_by_id(
    pool: &PgPool,
    id: i32,
) -> anyhow::Result<Option<TimeEntryOut>> {
    let row_opt = sqlx::query(
        "SELECT
            t.id::int4, t.ticket_id::int4, t.description, t.start_time, t.end_time,
            t.duration_minutes, t.created_at,
            u.id::int4 as user_id, u.username, u.email, u.display_name
         FROM t_time_entry t
         JOIN accounts_user u ON t.user_id = u.id
         WHERE t.id = $1"
    )
    .bind(id as i64)
    .fetch_optional(pool)
    .await?;

    let entry = row_opt.map(|row| TimeEntryOut {
        id: row.get(0),
        ticket: row.get(1),
        user: UserSummaryOut {
            id: row.get(7),
            username: row.get(8),
            email: row.get(9),
            display_name: row.get(10),
        },
        description: row.get(2),
        start_time: row.get(3),
        end_time: row.get(4),
        duration_minutes: row.get(5),
        created_at: row.get(6),
    });

    Ok(entry)
}

pub async fn create_time_entry(
    pool: &PgPool,
    ticket_id: i32,
    user_id: i32,
    description: String,
    start_time: Option<chrono::DateTime<chrono::Utc>>,
    end_time: Option<chrono::DateTime<chrono::Utc>>,
    duration_minutes: i32,
) -> anyhow::Result<i32> {
    let entry_id: i32 = sqlx::query_scalar(
        "INSERT INTO t_time_entry (ticket_id, user_id, description, start_time, end_time, duration_minutes, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, NOW())
         RETURNING id::int4"
    )
    .bind(ticket_id as i64)
    .bind(user_id as i64)
    .bind(&description)
    .bind(start_time)
    .bind(end_time)
    .bind(duration_minutes)
    .fetch_one(pool)
    .await?;

    Ok(entry_id)
}

pub async fn delete_time_entry(pool: &PgPool, id: i32) -> anyhow::Result<bool> {
    let rows_affected = sqlx::query("DELETE FROM t_time_entry WHERE id = $1")
        .bind(id as i64)
        .execute(pool)
        .await?
        .rows_affected();

    Ok(rows_affected > 0)
}

pub async fn get_today_time_entries(
    pool: &PgPool,
    user_id: i32,
) -> anyhow::Result<TimeEntryTodayOut> {
    let row = sqlx::query(
        "SELECT
            COALESCE(SUM(duration_minutes), 0)::int4 as total_minutes,
            COUNT(*)::int4 as entry_count
         FROM t_time_entry
         WHERE user_id = $1 AND DATE(created_at) = CURRENT_DATE"
    )
    .bind(user_id as i64)
    .fetch_one(pool)
    .await?;

    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    Ok(TimeEntryTodayOut {
        date: today,
        total_minutes: row.get(0),
        entry_count: row.get(1),
    })
}
