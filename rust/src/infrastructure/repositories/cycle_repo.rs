/// infrastructure/repositories/cycle_repo.rs — サイクル永続化（API用）
///
/// Django /api/v1/cycles/* の実スキーマ (t_cycle) に対応。
/// bigint列は全て ::int4 キャスト。
/// チケット関連の集計（ticketCount, completedCount, totalPoints, completedPoints）を含む。

use sqlx::{PgPool, Row};
use crate::domain::models::cycle_api::{CycleOut, CycleWriteIn};

/// サイクル一覧（フィルタ対応: project, status）。
/// ページネーション無し。
pub async fn find_all_cycles(
    pool: &PgPool,
    project_id: Option<i32>,
    status: Option<&str>,
) -> anyhow::Result<Vec<CycleOut>> {
    // 条件に応じてクエリを構築
    let rows = if let (Some(pid), Some(s)) = (project_id, status) {
        sqlx::query(
            "SELECT
                c.id::int4, c.project_id::int4, c.name, c.number, c.status,
                c.start_date, c.end_date, c.created_at, c.created_by_id::int4
             FROM t_cycle c
             WHERE c.project_id = $1::int4 AND c.status = $2"
        )
        .bind(pid)
        .bind(s)
        .fetch_all(pool)
        .await?
    } else if let Some(pid) = project_id {
        sqlx::query(
            "SELECT
                c.id::int4, c.project_id::int4, c.name, c.number, c.status,
                c.start_date, c.end_date, c.created_at, c.created_by_id::int4
             FROM t_cycle c
             WHERE c.project_id = $1::int4"
        )
        .bind(pid)
        .fetch_all(pool)
        .await?
    } else if let Some(s) = status {
        sqlx::query(
            "SELECT
                c.id::int4, c.project_id::int4, c.name, c.number, c.status,
                c.start_date, c.end_date, c.created_at, c.created_by_id::int4
             FROM t_cycle c
             WHERE c.status = $1"
        )
        .bind(s)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            "SELECT
                c.id::int4, c.project_id::int4, c.name, c.number, c.status,
                c.start_date, c.end_date, c.created_at, c.created_by_id::int4
             FROM t_cycle c"
        )
        .fetch_all(pool)
        .await?
    };

    let mut result = Vec::new();

    for row in rows {
        let cycle_id: i32 = row.get("id");
        let project: i32 = row.get("project_id");
        let created_by_id: Option<i32> = row.get("created_by_id");

        // created_by の詳細情報を取得
        let created_by = if let Some(uid) = created_by_id {
            let user_row = sqlx::query(
                "SELECT id::int4, username, email,
                    COALESCE(display_name, '') as display_name
                 FROM accounts_user WHERE id = $1::int4"
            )
            .bind(uid)
            .fetch_optional(pool)
            .await?;

            user_row.map(|u| crate::domain::models::ticket_api::UserSummaryOut {
                id: u.get("id"),
                username: u.get("username"),
                email: u.get("email"),
                display_name: u.get("display_name"),
            })
        } else {
            None
        };

        // チケット関連の集計
        let ticket_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM tickets_ticket WHERE cycle_id = $1::int4"
        )
        .bind(cycle_id)
        .fetch_one(pool)
        .await?;

        let completed_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM tickets_ticket
             WHERE cycle_id = $1::int4 AND status IN ('closed', 'resolved')"
        )
        .bind(cycle_id)
        .fetch_one(pool)
        .await?;

        let total_points: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(story_points), 0) FROM tickets_ticket
             WHERE cycle_id = $1::int4"
        )
        .bind(cycle_id)
        .fetch_one(pool)
        .await?;

        let completed_points: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(story_points), 0) FROM tickets_ticket
             WHERE cycle_id = $1::int4 AND status IN ('closed', 'resolved')"
        )
        .bind(cycle_id)
        .fetch_one(pool)
        .await?;

        result.push(CycleOut {
            id: cycle_id,
            project,
            name: row.get("name"),
            number: row.get("number"),
            status: row.get("status"),
            start_date: row.get("start_date"),
            end_date: row.get("end_date"),
            created_by,
            created_at: row.get("created_at"),
            ticket_count,
            completed_count,
            total_points,
            completed_points,
        });
    }

    Ok(result)
}

/// サイクル詳細取得（IDで検索）。
pub async fn find_cycle_by_id(pool: &PgPool, id: i32) -> anyhow::Result<Option<CycleOut>> {
    // 基本情報を取得
    let row = sqlx::query(
        "SELECT
            id::int4, project_id::int4, name, number, status,
            start_date, end_date, created_at, created_by_id::int4
         FROM t_cycle WHERE id = $1::int4"
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    match row {
        None => Ok(None),
        Some(r) => {
            let cycle_id: i32 = r.get("id");
            let project: i32 = r.get("project_id");
            let created_by_id: Option<i32> = r.get("created_by_id");

            // created_by の詳細情報を取得
            let created_by = if let Some(uid) = created_by_id {
                let user_row = sqlx::query(
                    "SELECT id::int4, username, email,
                        COALESCE(display_name, '') as display_name
                     FROM accounts_user WHERE id = $1::int4"
                )
                .bind(uid)
                .fetch_optional(pool)
                .await?;

                user_row.map(|u| crate::domain::models::ticket_api::UserSummaryOut {
                    id: u.get("id"),
                    username: u.get("username"),
                    email: u.get("email"),
                    display_name: u.get("display_name"),
                })
            } else {
                None
            };

            // チケット関連の集計
            let ticket_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM tickets_ticket WHERE cycle_id = $1::int4"
            )
            .bind(cycle_id)
            .fetch_one(pool)
            .await?;

            let completed_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM tickets_ticket
                 WHERE cycle_id = $1::int4 AND status IN ('closed', 'resolved')"
            )
            .bind(cycle_id)
            .fetch_one(pool)
            .await?;

            let total_points: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(story_points), 0) FROM tickets_ticket
                 WHERE cycle_id = $1::int4"
            )
            .bind(cycle_id)
            .fetch_one(pool)
            .await?;

            let completed_points: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(story_points), 0) FROM tickets_ticket
                 WHERE cycle_id = $1::int4 AND status IN ('closed', 'resolved')"
            )
            .bind(cycle_id)
            .fetch_one(pool)
            .await?;

            Ok(Some(CycleOut {
                id: cycle_id,
                project,
                name: r.get("name"),
                number: r.get("number"),
                status: r.get("status"),
                start_date: r.get("start_date"),
                end_date: r.get("end_date"),
                created_by,
                created_at: r.get("created_at"),
                ticket_count,
                completed_count,
                total_points,
                completed_points,
            }))
        }
    }
}

/// サイクル番号を採番（MAX(number) + 1）。
pub async fn get_next_cycle_number(pool: &PgPool, project_id: i32) -> anyhow::Result<i32> {
    let max_number: Option<i32> = sqlx::query_scalar(
        "SELECT COALESCE(MAX(number), 0) FROM t_cycle WHERE project_id = $1::int4"
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?;

    Ok(max_number.unwrap_or(0) + 1)
}

/// サイクル作成。
/// バリデーション: start_date >= end_date なら anyhow::bail!
pub async fn create_cycle(
    pool: &PgPool,
    input: &CycleWriteIn,
    created_by: i32,
) -> anyhow::Result<i32> {
    // バリデーション
    if input.start_date >= input.end_date {
        anyhow::bail!("開始日は終了日より前でなければなりません。");
    }

    // サイクル番号を採番
    let number = get_next_cycle_number(pool, input.project).await?;

    // INSERT
    let id: i32 = sqlx::query_scalar(
        "INSERT INTO t_cycle (project_id, name, number, status, start_date, end_date, created_by_id, created_at)
         VALUES ($1::int4, $2, $3, $4, $5, $6, $7::int4, NOW())
         RETURNING id::int4"
    )
    .bind(input.project)
    .bind(&input.name)
    .bind(number)
    .bind(&input.status)
    .bind(input.start_date)
    .bind(input.end_date)
    .bind(created_by)
    .fetch_one(pool)
    .await?;

    Ok(id)
}

/// サイクル更新。
/// numberは変更しない（既存値を保持）。
/// バリデーション: start_date >= end_date なら anyhow::bail!
pub async fn update_cycle(pool: &PgPool, id: i32, input: &CycleWriteIn) -> anyhow::Result<bool> {
    // バリデーション
    if input.start_date >= input.end_date {
        anyhow::bail!("開始日は終了日より前でなければなりません。");
    }

    let result = sqlx::query(
        "UPDATE t_cycle SET
            project_id = $1::int4, name = $2, status = $3,
            start_date = $4, end_date = $5
         WHERE id = $6::int4"
    )
    .bind(input.project)
    .bind(&input.name)
    .bind(&input.status)
    .bind(input.start_date)
    .bind(input.end_date)
    .bind(id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// サイクル削除。
pub async fn delete_cycle(pool: &PgPool, id: i32) -> anyhow::Result<bool> {
    let mut tx = pool.begin().await?;

    // tickets_ticket.cycle は on_delete=SET_NULL
    sqlx::query("UPDATE tickets_ticket SET cycle_id = NULL WHERE cycle_id = $1::int4")
        .bind(id).execute(&mut *tx).await?;

    let result = sqlx::query("DELETE FROM t_cycle WHERE id = $1::int4")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(result.rows_affected() > 0)
}
