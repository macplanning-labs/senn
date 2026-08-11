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
