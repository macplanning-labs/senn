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

    // 既存ステータスを取得
    let old_status: Option<String> = sqlx::query_scalar("SELECT status FROM t_cycle WHERE id = $1::int4")
        .bind(id)
        .fetch_optional(pool)
        .await?;

    let old_status = match old_status {
        Some(s) => s,
        None => return Ok(false),
    };

    // ステータス遷移に応じてタイムスタンプを設定
    let result = if old_status != "active" && input.status == "active" {
        // planned → active への遷移: activated_at を設定
        sqlx::query(
            "UPDATE t_cycle SET
                project_id = $1::int4, name = $2, status = $3,
                start_date = $4, end_date = $5, activated_at = NOW()
             WHERE id = $6::int4"
        )
        .bind(input.project)
        .bind(&input.name)
        .bind(&input.status)
        .bind(input.start_date)
        .bind(input.end_date)
        .bind(id)
        .execute(pool)
        .await?
    } else if input.status == "completed" {
        // → completed への遷移: completed_at を設定
        sqlx::query(
            "UPDATE t_cycle SET
                project_id = $1::int4, name = $2, status = $3,
                start_date = $4, end_date = $5, completed_at = NOW()
             WHERE id = $6::int4"
        )
        .bind(input.project)
        .bind(&input.name)
        .bind(&input.status)
        .bind(input.start_date)
        .bind(input.end_date)
        .bind(id)
        .execute(pool)
        .await?
    } else {
        // その他の更新
        sqlx::query(
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
        .await?
    };

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

/// あるサイクルについて、基準時刻(since)から終了境界(until)までのスコープ
/// 追加/削除ポイントを計算する。
///
/// 同一チケットがsince以降に複数回このサイクルへ出入りした場合、各イベントを
/// 単純合算すると二重計上・符号誤りが起きる(例: 追加→削除→再追加で
/// 「追加16pt」と誤計上され、initial_pointsが負になる)。これを防ぐため、
/// チケットごとに「since以降で最初のCycleフィールド変更」だけを見て、
/// その変更の前後関係(このサイクルに入ったのか出たのか)と、現在実際に
/// このサイクルに所属しているかを突き合わせる。追加→削除のように最終的に
/// 所属していなければ加算も減算もされない(意図通り正味ゼロ)。
async fn compute_scope_added_removed(
    pool: &PgPool,
    cycle_id: i32,
    since: chrono::DateTime<chrono::Utc>,
    until: chrono::DateTime<chrono::Utc>,
) -> anyhow::Result<(i64, i64)> {
    let row = sqlx::query(
        "WITH first_change AS (
           SELECT DISTINCT ON (cl.ticket_id) cl.ticket_id, cl.old_value
           FROM tickets_change_log cl
           WHERE cl.field_name = 'Cycle'
             AND cl.changed_at > $2 AND cl.changed_at <= $3
             AND (cl.old_value = $1 OR cl.new_value = $1)
           ORDER BY cl.ticket_id, cl.changed_at ASC
         )
         SELECT
           COALESCE(SUM(CASE WHEN fc.old_value != $1 AND t.cycle_id = $4::int4
                              THEN t.story_points ELSE 0 END), 0)::int8 AS scope_added,
           COALESCE(SUM(CASE WHEN fc.old_value = $1 AND t.cycle_id IS DISTINCT FROM $4::int4
                              THEN t.story_points ELSE 0 END), 0)::int8 AS scope_removed
         FROM first_change fc
         JOIN tickets_ticket t ON t.id = fc.ticket_id"
    )
    .bind(cycle_id.to_string())
    .bind(since)
    .bind(until)
    .bind(cycle_id)
    .fetch_one(pool)
    .await?;

    Ok((row.get("scope_added"), row.get("scope_removed")))
}

/// サイクル進捗集計。存在しないサイクルはNoneを返す。
pub async fn get_cycle_progress(pool: &PgPool, cycle_id: i32) -> anyhow::Result<Option<CycleProgressOut>> {
    let cycle_row = sqlx::query("SELECT activated_at, start_date FROM t_cycle WHERE id = $1::int4")
        .bind(cycle_id)
        .fetch_optional(pool)
        .await?;

    let cycle_row = match cycle_row {
        Some(r) => r,
        None => return Ok(None),
    };

    let activated_at: Option<chrono::DateTime<chrono::Utc>> = cycle_row.get("activated_at");
    let start_date: chrono::NaiveDate = cycle_row.get("start_date");
    let since = activated_at.unwrap_or_else(|| {
        chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
            chrono::NaiveDateTime::new(start_date, chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap()),
            chrono::Utc,
        )
    });

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
    let total_points: i64 = row.get("total_points");
    let completion_rate = if ticket_count > 0 {
        let value = completed_count as f64 / ticket_count as f64 * 100.0;
        (value * 10.0).round() / 10.0
    } else {
        0.0
    };

    // スコープ変化計算: (a)(b) 追加/削除(重複防止済み), (c) ポイント変更
    let until = chrono::Utc::now();
    let (scope_added, scope_removed) =
        compute_scope_added_removed(pool, cycle_id, since, until).await?;

    let point_delta: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(ph.new_points - ph.old_points), 0)::int8
         FROM h_task_point_history ph
         JOIN tickets_ticket t ON t.id = ph.ticket_id
         WHERE t.cycle_id = $1::int4
           AND ph.changed_at > $2"
    )
    .bind(cycle_id)
    .bind(since)
    .fetch_one(pool)
    .await?;

    let net_scope_change = scope_added - scope_removed + point_delta;
    let initial_points = total_points - net_scope_change;

    Ok(Some(CycleProgressOut {
        ticket_count,
        completed_count,
        in_progress_count: row.get("in_progress_count"),
        total_points,
        completed_points: row.get("completed_points"),
        completion_rate,
        initial_points,
        scope_added,
        scope_removed,
        scope_change: net_scope_change,
    }))
}

/// 直近limit件の完了サイクルのベロシティデータ(古い順)。
pub async fn get_velocity_data(pool: &PgPool, project_id: i32, limit: i64) -> anyhow::Result<Vec<VelocityEntryOut>> {
    let cycles = sqlx::query(
        "SELECT id::int4, number, name, activated_at, completed_at, start_date FROM t_cycle
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
        let activated_at: Option<chrono::DateTime<chrono::Utc>> = row.get("activated_at");
        let completed_at: Option<chrono::DateTime<chrono::Utc>> = row.get("completed_at");
        let start_date: chrono::NaiveDate = row.get("start_date");

        let since = activated_at.unwrap_or_else(|| {
            chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
                chrono::NaiveDateTime::new(start_date, chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap()),
                chrono::Utc,
            )
        });
        let until = completed_at.unwrap_or_else(chrono::Utc::now);

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

        // スコープ変化計算(重複防止済み)
        let (scope_added, scope_removed) =
            compute_scope_added_removed(pool, cycle_id, since, until).await?;

        let point_delta: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(ph.new_points - ph.old_points), 0)::int8
             FROM h_task_point_history ph
             JOIN tickets_ticket t ON t.id = ph.ticket_id
             WHERE t.cycle_id = $1::int4
               AND ph.changed_at > $2 AND ph.changed_at <= $3"
        )
        .bind(cycle_id)
        .bind(since)
        .bind(until)
        .fetch_one(pool)
        .await?;

        let scope_change = scope_added - scope_removed + point_delta;

        result.push(VelocityEntryOut {
            cycle_id,
            cycle_number: row.get("number"),
            cycle_name: row.get("name"),
            completed_count: stats.get("completed"),
            completed_points: stats.get("completed_points"),
            scope_change,
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

    sqlx::query("UPDATE t_cycle SET status = 'completed', completed_at = NOW() WHERE id = $1::int4")
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
    let cycle_row = sqlx::query("SELECT start_date, end_date, activated_at FROM t_cycle WHERE id = $1::int4")
        .bind(cycle_id)
        .fetch_optional(pool)
        .await?;

    let cycle_row = match cycle_row {
        Some(r) => r,
        None => return Ok(None),
    };

    let start_date: chrono::NaiveDate = cycle_row.get("start_date");
    let end_date: chrono::NaiveDate = cycle_row.get("end_date");
    let activated_at: Option<chrono::DateTime<chrono::Utc>> = cycle_row.get("activated_at");

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

    let since = activated_at.unwrap_or_else(|| {
        chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
            chrono::NaiveDateTime::new(start_date, chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap()),
            chrono::Utc,
        )
    });

    // initial_points を計算(重複防止済み)
    let (scope_added, scope_removed) =
        compute_scope_added_removed(pool, cycle_id, since, chrono::Utc::now()).await?;

    let point_delta: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(ph.new_points - ph.old_points), 0)::int8
         FROM h_task_point_history ph
         JOIN tickets_ticket t ON t.id = ph.ticket_id
         WHERE t.cycle_id = $1::int4
           AND ph.changed_at > $2"
    )
    .bind(cycle_id)
    .bind(since)
    .fetch_one(pool)
    .await?;

    let net_scope_change = scope_added - scope_removed + point_delta;
    let initial_points = total_points - net_scope_change;

    // 各チケットの最初の完了日を特定
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

    // スコープ変化ログを日別に集計。compute_scope_added_removedと同じ理由
    // (同一チケットの複数回出入りによる二重計上防止)で、チケットごとの
    // 「since以降で最初のCycleフィールド変更」の日付にのみ計上する。
    let scope_change_rows = sqlx::query(
        "WITH first_change AS (
           SELECT DISTINCT ON (cl.ticket_id) cl.ticket_id, cl.old_value, cl.changed_at
           FROM tickets_change_log cl
           WHERE cl.field_name = 'Cycle'
             AND cl.changed_at > $2
             AND cl.changed_at >= $3 AND cl.changed_at <= $4
             AND (cl.old_value = $1 OR cl.new_value = $1)
           ORDER BY cl.ticket_id, cl.changed_at ASC
         )
         SELECT
           (fc.changed_at::date) as changed_date,
           COALESCE(SUM(
             CASE WHEN fc.old_value != $1 AND t.cycle_id = $5::int4 THEN t.story_points
                  WHEN fc.old_value = $1 AND t.cycle_id IS DISTINCT FROM $5::int4 THEN -t.story_points
                  ELSE 0 END
           ), 0)::int8 as net_change
         FROM first_change fc
         JOIN tickets_ticket t ON t.id = fc.ticket_id
         GROUP BY (fc.changed_at::date)"
    )
    .bind(cycle_id.to_string())
    .bind(since)
    .bind(start_date.and_hms_opt(0, 0, 0).unwrap())
    .bind(end_date.and_hms_opt(23, 59, 59).unwrap())
    .bind(cycle_id)
    .fetch_all(pool)
    .await?;

    let mut scope_change_by_date: std::collections::HashMap<chrono::NaiveDate, i64> = std::collections::HashMap::new();
    for row in scope_change_rows {
        let date: chrono::NaiveDate = row.get("changed_date");
        let net_change: i64 = row.get("net_change");
        *scope_change_by_date.entry(date).or_insert(0) += net_change;
    }

    // ポイント変更ログを日別に集計
    let point_history_rows = sqlx::query(
        "SELECT (ph.changed_at::date)::text as changed_date, COALESCE(SUM(ph.new_points - ph.old_points), 0)::int8 as delta
         FROM h_task_point_history ph
         JOIN tickets_ticket t ON t.id = ph.ticket_id
         WHERE t.cycle_id = $1::int4
           AND ph.changed_at >= $2 AND ph.changed_at <= $3
         GROUP BY (ph.changed_at::date)::text"
    )
    .bind(cycle_id)
    .bind(start_date.and_hms_opt(0, 0, 0).unwrap())
    .bind(end_date.and_hms_opt(23, 59, 59).unwrap())
    .fetch_all(pool)
    .await?;

    for row in point_history_rows {
        let date_str: String = row.get("changed_date");
        let delta: i64 = row.get("delta");
        if let Ok(date) = chrono::NaiveDate::parse_from_str(&date_str, "%Y-%m-%d") {
            *scope_change_by_date.entry(date).or_insert(0) += delta;
        }
    }

    let mut result = Vec::new();
    let mut remaining = total_points;
    let mut cumulative_scope_change = 0i64;

    for day_offset in 0..=duration_days {
        let current_date = start_date + chrono::Duration::days(day_offset);

        // 累積スコープ変化を更新
        if let Some(change) = scope_change_by_date.get(&current_date) {
            cumulative_scope_change += change;
        }

        let total_scope = initial_points + cumulative_scope_change;
        let ideal = if initial_points > 0 {
            ((initial_points as f64) * (1.0 - (day_offset as f64 / duration_days as f64)) * 10.0).round() / 10.0
        } else {
            0.0
        };

        if let Some(completed) = completion_by_date.get(&current_date) {
            remaining -= completed;
        }

        result.push(BurndownPointOut {
            date: current_date.format("%Y-%m-%d").to_string(),
            ideal,
            actual: remaining,
            total_scope,
        });
    }

    Ok(Some(result))
}

/// 期限に基づいてplannedサイクルを自動的にactiveに遷移させ、アクティブ化されたサイクル情報を返す。
pub async fn auto_activate_due_cycles(pool: &PgPool) -> anyhow::Result<Vec<(i32, i32, String)>> {
    let activated = sqlx::query(
        "UPDATE t_cycle
         SET status = 'active', activated_at = NOW()
         WHERE status = 'planned' AND start_date <= CURRENT_DATE
         RETURNING id::int4, project_id::int4, name"
    )
    .fetch_all(pool)
    .await?;

    Ok(activated
        .into_iter()
        .map(|row| (row.get(0), row.get(1), row.get(2)))
        .collect())
}

/// 持ち越し先解決の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CarryOverResolve {
    /// 次 Cycle（既存 planned または新規作成）へ持ち越し
    CarryTo(i32),
    /// 持ち越しなしで完了してよい（create_next=false）
    CompleteWithoutCarry,
    /// 次 Cycle を作れず、自動完了自体をスキップする（actor 不在など）
    AbortAutoComplete,
}

/// 持ち越し先 Cycle を解決する。
/// 1. 同 project_id の status='planned' を先頭から探索 → CarryTo(id)
/// 2. 無ければ:
///    - cycle_auto_create_next = false → CompleteWithoutCarry
///    - cycle_auto_create_next = true → 次 Cycle を自動作成して CarryTo(id)
/// 3. created_by / owner / membership(staff 優先) が全て無ければ AbortAutoComplete
pub async fn resolve_carry_over_target(
    pool: &PgPool,
    project_id: i32,
    completed_cycle: &CycleOut,
) -> anyhow::Result<CarryOverResolve> {
    // 1. planned Cycle が存在するか確認
    let planned_cycle: Option<i32> = sqlx::query_scalar(
        "SELECT id::int4 FROM t_cycle
         WHERE project_id = $1::int4 AND status = 'planned'
         ORDER BY start_date ASC, id ASC
         LIMIT 1"
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await?;

    if let Some(id) = planned_cycle {
        return Ok(CarryOverResolve::CarryTo(id));
    }

    // 2. cycle_auto_create_next を確認
    let auto_create_next: bool = sqlx::query_scalar(
        "SELECT cycle_auto_create_next FROM tickets_project WHERE id = $1::int4"
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?;

    if !auto_create_next {
        return Ok(CarryOverResolve::CompleteWithoutCarry);
    }

    // 3. 次 Cycle を自動作成
    // created_by_id を決定
    let creator_id = if let Some(uid) = completed_cycle.created_by.as_ref().map(|u| u.id) {
        Some(uid)
    } else {
        // owner_id を取得
        let owner_id: Option<i32> = sqlx::query_scalar(
            "SELECT owner_id::int4 FROM tickets_project WHERE id = $1::int4"
        )
        .bind(project_id)
        .fetch_one(pool)
        .await?;

        if owner_id.is_some() {
            owner_id
        } else {
            // membership: is_staff 優先、なければ最初のメンバー
            let member_id: Option<i32> = sqlx::query_scalar(
                "SELECT m.user_id::int4
                 FROM tickets_project_membership m
                 JOIN accounts_user u ON u.id = m.user_id
                 WHERE m.project_id = $1::int4
                 ORDER BY u.is_staff DESC, m.user_id ASC
                 LIMIT 1"
            )
            .bind(project_id)
            .fetch_optional(pool)
            .await?;

            member_id
        }
    };

    // creator_id が決まらない場合は自動完了ごとスキップ
    let creator_id = match creator_id {
        Some(uid) => uid,
        None => {
            tracing::error!(
                "[Cycle 自動完了] project_id={} 結果=次Cycle作成失敗→自動完了スキップ 理由=created_by_idが決定不可（creator/owner/membershipが無い）",
                project_id
            );
            return Ok(CarryOverResolve::AbortAutoComplete);
        }
    };

    // 次 Cycle の期間を計算
    let duration_days = (completed_cycle.end_date - completed_cycle.start_date).num_days();
    let next_start = completed_cycle.end_date + chrono::Duration::days(1);
    let duration = if duration_days > 0 {
        duration_days
    } else {
        14
    };
    let next_end = next_start + chrono::Duration::days(duration);

    // 次 Cycle 番号を取得
    let next_number = get_next_cycle_number(pool, project_id).await?;
    let next_name = format!("Cycle {}", next_number);

    // 次 Cycle を作成
    let new_cycle_id: i32 = sqlx::query_scalar(
        "INSERT INTO t_cycle (project_id, name, number, status, start_date, end_date, created_by_id, created_at)
         VALUES ($1::int4, $2, $3, 'planned', $4, $5, $6::int4, NOW())
         RETURNING id::int4"
    )
    .bind(project_id)
    .bind(&next_name)
    .bind(next_number)
    .bind(next_start)
    .bind(next_end)
    .bind(creator_id)
    .fetch_one(pool)
    .await?;

    Ok(CarryOverResolve::CarryTo(new_cycle_id))
}

/// 期限超過の active Cycle を自動完了し、ログエントリを返す。
pub async fn auto_complete_overdue_cycles(
    pool: &PgPool,
) -> anyhow::Result<Vec<crate::domain::models::cycle_api::AutoCompleteLogEntry>> {
    use crate::domain::models::cycle_api::AutoCompleteLogEntry;

    // 対象クエリ
    let overdue_cycles = sqlx::query(
        "SELECT c.id::int4, c.project_id::int4, c.name, c.start_date, c.end_date, c.created_by_id::int4
         FROM t_cycle c
         JOIN tickets_project p ON p.id = c.project_id
         WHERE c.status = 'active'
           AND c.end_date < CURRENT_DATE
           AND p.cycle_auto_complete = true
         ORDER BY c.end_date ASC, c.id ASC"
    )
    .fetch_all(pool)
    .await?;

    let mut result = Vec::new();

    for row in overdue_cycles {
        let cycle_id: i32 = row.get("id");
        let project_id: i32 = row.get("project_id");
        let name: String = row.get("name");
        let start_date: chrono::NaiveDate = row.get("start_date");
        let end_date: chrono::NaiveDate = row.get("end_date");
        let created_by_id: Option<i32> = row.get("created_by_id");

        // completed_cycle 用の CycleOut を構築（最小限）
        let completed_cycle = CycleOut {
            id: cycle_id,
            project: project_id,
            name: name.clone(),
            number: 0, // 使用されない
            status: "active".to_string(),
            start_date,
            end_date,
            created_by: if let Some(uid) = created_by_id {
                // ユーザー情報を取得
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
            },
            created_at: chrono::Utc::now(),
            ticket_count: 0,
            completed_count: 0,
            total_points: 0,
            completed_points: 0,
        };

        // 持ち越し先を解決
        let target_id = match resolve_carry_over_target(pool, project_id, &completed_cycle).await {
            Ok(CarryOverResolve::CarryTo(id)) => Some(id),
            Ok(CarryOverResolve::CompleteWithoutCarry) => None,
            Ok(CarryOverResolve::AbortAutoComplete) => {
                tracing::warn!(
                    "cycle auto-complete aborted: id={} project_id={} 理由=次Cycle作成不可",
                    cycle_id, project_id
                );
                continue;
            }
            Err(e) => {
                tracing::error!(
                    "[Cycle 自動完了] cycle_id={} project_id={} 結果=失敗（持ち越し先解決） | {}",
                    cycle_id, project_id, e
                );
                continue;
            }
        };

        // Cycle を完了
        match complete_cycle(pool, cycle_id, target_id).await {
            Ok(CompleteCycleResult::Success(out)) => {
                tracing::info!(
                    "cycle auto-completed: id={} project_id={} carried_over={} target={:?} 処理=期限超過サイクル自動完了",
                    cycle_id, project_id, out.carried_over, target_id
                );

                // 通知を作成
                if let Err(e) = crate::domain::services::notification_service::notify_cycle_auto_completed(
                    pool,
                    project_id,
                    cycle_id,
                    &name,
                    out.carried_over,
                    target_id,
                ).await {
                    tracing::error!(
                        "[Cycle自動完了通知] 作成失敗 cycle_id={} project_id={}: {}",
                        cycle_id, project_id, e
                    );
                }

                result.push(AutoCompleteLogEntry {
                    cycle_id,
                    project_id,
                    carried_over: out.carried_over,
                    target_cycle_id: target_id,
                });
            }
            Ok(CompleteCycleResult::AlreadyCompleted) => {
                // no-op
                tracing::debug!(
                    "cycle auto-complete skipped: id={} project_id={} 理由=already_completed",
                    cycle_id, project_id
                );
            }
            Ok(CompleteCycleResult::NotFound) => {
                // no-op
                tracing::debug!(
                    "cycle auto-complete skipped: id={} project_id={} 理由=not_found",
                    cycle_id, project_id
                );
            }
            Err(e) => {
                tracing::error!(
                    "[Cycle 自動完了] cycle_id={} project_id={} 結果=失敗（完了処理） | {}",
                    cycle_id, project_id, e
                );
                continue;
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use crate::test_support;

    #[tokio::test]
    async fn test_resolve_carry_over_target_with_planned() {
        // T1: planned あり → 先頭 planned へ持ち越し
        let Some(pool) = test_support::test_pool().await else { return; };
        let user_id = test_support::create_test_user(&pool, "cyc").await;
        let project_id = test_support::create_test_project(&pool, "CYC", user_id).await;

        let today = chrono::Local::now().naive_local().date();
        let active_cycle_id = create_cycle(
            &pool,
            &CycleWriteIn {
                project: project_id,
                name: "Active Cycle".to_string(),
                start_date: today - Duration::days(10),
                end_date: today - Duration::days(1),
                status: "active".to_string(),
            },
            user_id,
        )
        .await
        .expect("Failed to create active cycle");

        let planned_cycle_id = create_cycle(
            &pool,
            &CycleWriteIn {
                project: project_id,
                name: "Planned Cycle".to_string(),
                start_date: today,
                end_date: today + Duration::days(14),
                status: "planned".to_string(),
            },
            user_id,
        )
        .await
        .expect("Failed to create planned cycle");

        let active_cycle = find_cycle_by_id(&pool, active_cycle_id)
            .await
            .expect("Failed to fetch cycle")
            .expect("Active cycle not found");

        let target = resolve_carry_over_target(&pool, project_id, &active_cycle)
            .await
            .expect("resolve_carry_over_target failed");

        assert_eq!(target, CarryOverResolve::CarryTo(planned_cycle_id));
    }

    #[tokio::test]
    async fn test_resolve_carry_over_target_create_next() {
        // T2: planned なし・create_next=true → 新 Cycle 作成
        let Some(pool) = test_support::test_pool().await else { return; };
        let user_id = test_support::create_test_user(&pool, "cyc2").await;
        let project_id = test_support::create_test_project(&pool, "CY2", user_id).await;

        sqlx::query("UPDATE tickets_project SET cycle_auto_create_next = true WHERE id = $1")
            .bind(project_id)
            .execute(&pool)
            .await
            .expect("Failed to set cycle_auto_create_next");

        let today = chrono::Local::now().naive_local().date();
        let active_cycle_id = create_cycle(
            &pool,
            &CycleWriteIn {
                project: project_id,
                name: "Active Cycle".to_string(),
                start_date: today - Duration::days(10),
                end_date: today - Duration::days(1),
                status: "active".to_string(),
            },
            user_id,
        )
        .await
        .expect("Failed to create active cycle");

        let active_cycle = find_cycle_by_id(&pool, active_cycle_id)
            .await
            .expect("Failed to fetch cycle")
            .expect("Active cycle not found");

        let target = resolve_carry_over_target(&pool, project_id, &active_cycle)
            .await
            .expect("resolve_carry_over_target failed");

        let CarryOverResolve::CarryTo(new_id) = target else {
            panic!("expected CarryTo, got {:?}", target);
        };

        let new_cycle = find_cycle_by_id(&pool, new_id)
            .await
            .expect("Failed to fetch cycle")
            .expect("New cycle not found");

        assert_eq!(new_cycle.status, "planned");
        assert!(new_cycle.start_date > active_cycle.end_date);
    }

    #[tokio::test]
    async fn test_resolve_carry_over_target_no_create() {
        // T3: planned なし・create_next=false → CompleteWithoutCarry
        let Some(pool) = test_support::test_pool().await else { return; };
        let user_id = test_support::create_test_user(&pool, "cyc3").await;
        let project_id = test_support::create_test_project(&pool, "CY3", user_id).await;

        sqlx::query("UPDATE tickets_project SET cycle_auto_create_next = false WHERE id = $1")
            .bind(project_id)
            .execute(&pool)
            .await
            .expect("Failed to set cycle_auto_create_next");

        let today = chrono::Local::now().naive_local().date();
        let active_cycle_id = create_cycle(
            &pool,
            &CycleWriteIn {
                project: project_id,
                name: "Active Cycle".to_string(),
                start_date: today - Duration::days(10),
                end_date: today - Duration::days(1),
                status: "active".to_string(),
            },
            user_id,
        )
        .await
        .expect("Failed to create active cycle");

        let active_cycle = find_cycle_by_id(&pool, active_cycle_id)
            .await
            .expect("Failed to fetch cycle")
            .expect("Active cycle not found");

        let target = resolve_carry_over_target(&pool, project_id, &active_cycle)
            .await
            .expect("resolve_carry_over_target failed");

        assert_eq!(target, CarryOverResolve::CompleteWithoutCarry);
    }

    #[tokio::test]
    async fn test_auto_complete_overdue_cycles_disabled() {
        // T4: auto_complete=false → 当該プロジェクトの期限超過は完了しない
        let Some(pool) = test_support::test_pool().await else { return; };
        let user_id = test_support::create_test_user(&pool, "cyc4").await;
        let project_id = test_support::create_test_project(&pool, "CY4", user_id).await;

        sqlx::query("UPDATE tickets_project SET cycle_auto_complete = false WHERE id = $1")
            .bind(project_id)
            .execute(&pool)
            .await
            .expect("Failed to set cycle_auto_complete");

        let today = chrono::Local::now().naive_local().date();
        let overdue_cycle_id = create_cycle(
            &pool,
            &CycleWriteIn {
                project: project_id,
                name: "Overdue Cycle".to_string(),
                start_date: today - Duration::days(10),
                end_date: today - Duration::days(1),
                status: "active".to_string(),
            },
            user_id,
        )
        .await
        .expect("Failed to create overdue cycle");

        let logs = auto_complete_overdue_cycles(&pool)
            .await
            .expect("auto_complete_overdue_cycles failed");

        assert!(!logs.iter().any(|e| e.cycle_id == overdue_cycle_id));

        let cycle = find_cycle_by_id(&pool, overdue_cycle_id)
            .await
            .expect("Failed to fetch cycle")
            .expect("Cycle not found");
        assert_eq!(cycle.status, "active");
    }

    #[tokio::test]
    async fn test_auto_complete_overdue_cycles_already_completed() {
        // T5: 既に completed → 当該 cycle はログに出ない
        let Some(pool) = test_support::test_pool().await else { return; };
        let user_id = test_support::create_test_user(&pool, "cyc5").await;
        let project_id = test_support::create_test_project(&pool, "CY5", user_id).await;

        let today = chrono::Local::now().naive_local().date();
        let cycle_id = create_cycle(
            &pool,
            &CycleWriteIn {
                project: project_id,
                name: "Completed Cycle".to_string(),
                start_date: today - Duration::days(10),
                end_date: today - Duration::days(1),
                status: "completed".to_string(),
            },
            user_id,
        )
        .await
        .expect("Failed to create cycle");

        let logs = auto_complete_overdue_cycles(&pool)
            .await
            .expect("auto_complete_overdue_cycles failed");

        assert!(!logs.iter().any(|e| e.cycle_id == cycle_id));
    }
}
