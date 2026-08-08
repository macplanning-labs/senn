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

// =============================================================================
// 高度アクション(progress / complete / velocity / burndown)
// Django: apps/tickets/domain/cycle_service.py の移植。
// 現行Django実装はチケットの"現在の"story_pointsを都度集計するのみで、
// h_task_point_history(ポイント変更履歴)は参照していない。Rust側もまずは
// Djangoと同一の挙動で移植する。
// =============================================================================

use crate::domain::models::cycle_api::{
    BurndownPointOut, CompleteCycleOut, CycleProgressOut, VelocityEntryOut,
};

/// サイクル進捗集計。存在しないサイクルはNoneを返す。
pub async fn get_cycle_progress(pool: &PgPool, cycle_id: i32) -> anyhow::Result<Option<CycleProgressOut>> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM t_cycle WHERE id = $1::int4)")
        .bind(cycle_id)
        .fetch_one(pool)
        .await?;
    if !exists {
        return Ok(None);
    }

    let row = sqlx::query(
        "SELECT
            COUNT(*)::int8 AS ticket_count,
            COUNT(*) FILTER (WHERE status IN ('closed','resolved'))::int8 AS completed_count,
            COUNT(*) FILTER (WHERE status = 'in_progress')::int8 AS in_progress_count,
            COALESCE(SUM(story_points), 0)::int8 AS total_points,
            COALESCE(SUM(story_points) FILTER (WHERE status IN ('closed','resolved')), 0)::int8 AS completed_points
         FROM tickets_ticket WHERE cycle_id = $1::int4"
    )
    .bind(cycle_id)
    .fetch_one(pool)
    .await?;

    let ticket_count: i64 = row.get("ticket_count");
    let completed_count: i64 = row.get("completed_count");
    let completion_rate = if ticket_count > 0 {
        let value = completed_count as f64 / ticket_count as f64 * 100.0;
        (value * 10.0).round() / 10.0
    } else {
        0.0
    };

    Ok(Some(CycleProgressOut {
        ticket_count,
        completed_count,
        in_progress_count: row.get("in_progress_count"),
        total_points: row.get("total_points"),
        completed_points: row.get("completed_points"),
        completion_rate,
    }))
}

/// 直近limit件の完了サイクルのベロシティデータ(古い順)。
pub async fn get_velocity_data(pool: &PgPool, project_id: i32, limit: i64) -> anyhow::Result<Vec<VelocityEntryOut>> {
    let cycles = sqlx::query(
        "SELECT id::int4, number, name FROM t_cycle
         WHERE project_id = $1::int4 AND status = 'completed'
         ORDER BY number DESC LIMIT $2"
    )
    .bind(project_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    let mut result = Vec::new();

    // Djangoは reversed(list(completed_cycles)) で古い順に並び替えて返す
    for row in cycles.into_iter().rev() {
        let cycle_id: i32 = row.get("id");

        let stats = sqlx::query(
            "SELECT
                COUNT(*) FILTER (WHERE status IN ('closed','resolved'))::int8 AS completed,
                COALESCE(SUM(story_points) FILTER (WHERE status IN ('closed','resolved')), 0)::int8 AS completed_points,
                COUNT(*) FILTER (WHERE status NOT IN ('closed','resolved','canceled'))::int8 AS carry_over
             FROM tickets_ticket WHERE cycle_id = $1::int4"
        )
        .bind(cycle_id)
        .fetch_one(pool)
        .await?;

        result.push(VelocityEntryOut {
            cycle_id,
            cycle_number: row.get("number"),
            cycle_name: row.get("name"),
            completed_count: stats.get("completed"),
            completed_points: stats.get("completed_points"),
            scope_change: 0,
            carry_over: stats.get("carry_over"),
        });
    }

    Ok(result)
}

pub enum CompleteCycleResult {
    Success(CompleteCycleOut),
    NotFound,
    AlreadyCompleted,
}

/// サイクルを手動完了し、未完了チケットを任意で次サイクルへ移行する。
pub async fn complete_cycle(
    pool: &PgPool,
    cycle_id: i32,
    carry_over_to: Option<i32>,
) -> anyhow::Result<CompleteCycleResult> {
    let mut tx = pool.begin().await?;

    let status: Option<String> = sqlx::query_scalar("SELECT status FROM t_cycle WHERE id = $1::int4")
        .bind(cycle_id)
        .fetch_optional(&mut *tx)
        .await?;

    let status = match status {
        Some(s) => s,
        None => return Ok(CompleteCycleResult::NotFound),
    };

    if status == "completed" {
        return Ok(CompleteCycleResult::AlreadyCompleted);
    }

    let carried_over: i64 = if let Some(target) = carry_over_to {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM tickets_ticket
             WHERE cycle_id = $1::int4 AND status NOT IN ('closed','resolved','canceled')"
        )
        .bind(cycle_id)
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query(
            "UPDATE tickets_ticket SET cycle_id = $1::int4
             WHERE cycle_id = $2::int4 AND status NOT IN ('closed','resolved','canceled')"
        )
        .bind(target)
        .bind(cycle_id)
        .execute(&mut *tx)
        .await?;

        count
    } else {
        0
    };

    sqlx::query("UPDATE t_cycle SET status = 'completed' WHERE id = $1::int4")
        .bind(cycle_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(CompleteCycleResult::Success(CompleteCycleOut {
        completed: true,
        carried_over,
    }))
}

/// バーンダウンチャート用の日次データ。存在しないサイクルはNoneを返す。
pub async fn get_burndown_data(pool: &PgPool, cycle_id: i32) -> anyhow::Result<Option<Vec<BurndownPointOut>>> {
    let cycle_row = sqlx::query("SELECT start_date, end_date FROM t_cycle WHERE id = $1::int4")
        .bind(cycle_id)
        .fetch_optional(pool)
        .await?;

    let cycle_row = match cycle_row {
        Some(r) => r,
        None => return Ok(None),
    };

    let start_date: chrono::NaiveDate = cycle_row.get("start_date");
    let end_date: chrono::NaiveDate = cycle_row.get("end_date");

    let ticket_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tickets_ticket WHERE cycle_id = $1::int4"
    )
    .bind(cycle_id)
    .fetch_one(pool)
    .await?;

    if ticket_count == 0 {
        return Ok(Some(vec![]));
    }

    let total_points: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(story_points), 0) FROM tickets_ticket WHERE cycle_id = $1::int4"
    )
    .bind(cycle_id)
    .fetch_one(pool)
    .await?;

    let duration_days = (end_date - start_date).num_days();
    if duration_days <= 0 {
        return Ok(Some(vec![]));
    }

    // 各チケットの最初の完了日を特定(closed/resolvedへの最初の遷移、サイクル期間内、現在のcycle所属チケットに限る)
    let completion_rows = sqlx::query(
        "SELECT DISTINCT ON (h.ticket_id) h.ticket_id::int4, h.changed_at, t.story_points
         FROM tickets_status_history h
         JOIN tickets_ticket t ON t.id = h.ticket_id
         WHERE t.cycle_id = $1::int4
           AND h.new_status IN ('closed', 'resolved')
           AND h.changed_at::date >= $2 AND h.changed_at::date <= $3
         ORDER BY h.ticket_id, h.changed_at ASC"
    )
    .bind(cycle_id)
    .bind(start_date)
    .bind(end_date)
    .fetch_all(pool)
    .await?;

    let mut completion_by_date: std::collections::HashMap<chrono::NaiveDate, i64> = std::collections::HashMap::new();
    for row in completion_rows {
        let changed_at: chrono::DateTime<chrono::Utc> = row.get("changed_at");
        let points: Option<i16> = row.get("story_points");
        let date = changed_at.date_naive();
        *completion_by_date.entry(date).or_insert(0) += points.unwrap_or(0) as i64;
    }

    let mut result = Vec::new();
    let mut remaining = total_points;

    for day_offset in 0..=duration_days {
        let current_date = start_date + chrono::Duration::days(day_offset);
        let ideal = ((total_points as f64) * (1.0 - (day_offset as f64 / duration_days as f64)) * 10.0).round() / 10.0;

        if let Some(completed) = completion_by_date.get(&current_date) {
            remaining -= completed;
        }

        result.push(BurndownPointOut {
            date: current_date.format("%Y-%m-%d").to_string(),
            ideal,
            actual: remaining,
        });
    }

    Ok(Some(result))
}
