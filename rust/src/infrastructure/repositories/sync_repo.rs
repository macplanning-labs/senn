//! sync_repo.rs — 差分同期（Local-first）の取得処理
//!
//! 詳細設計: docs/詳細設計書_LocalFirst_コアエンティティ移行.md §2.3
//!
//! - 変更は `(sync_changed_at, id)` の順で cursor の続きから返す（同時刻の行もページ境界で取りこぼさない）
//! - 削除は `sync_tombstones` の `(deleted_at, id)` の順
//! - `NOW()` はトランザクション開始時刻なので、長いトランザクションの行が後からコミットされうる。
//!   最終ページでは cursor を「応答時刻 - 10秒」より先へ進めない（直近10秒は次回もう一度返す）
use chrono::{DateTime, Duration, TimeZone, Utc};
use sqlx::{PgPool, Row};
use std::collections::HashMap;

use crate::domain::models::resource_api::{ProjectOut, ProjectTeamOut};
use crate::domain::models::sync_api::*;
use crate::infrastructure::repositories::ticket_repo::{hydrate_ticket_rows, API_TICKET_SELECT};

/// 直近この秒数の変更・削除は、次回の同期でもう一度返す（コミット順の入れ替わり対策）
const REWIND_SECONDS: i64 = 10;
/// 削除記録の保持日数。これより古い cursor は 410
const TOMBSTONE_RETENTION_DAYS: i64 = 90;
/// 1回に返す削除記録の上限
const TOMBSTONE_PAGE: i64 = 1000;

/// cursor 上の位置（時刻, id）
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Pos {
    at: DateTime<Utc>,
    id: i64,
}

fn epoch() -> DateTime<Utc> {
    Utc.timestamp_opt(0, 0).single().expect("epoch")
}

/// 開始位置を決める。戻り値: (変更の位置, 削除の位置, フル同期か)
fn start_positions(
    cursor: Option<&SyncCursor>,
    server_time: DateTime<Utc>,
) -> Result<(Pos, Pos, bool), SyncError> {
    match cursor {
        Some(c) => {
            if c.d < server_time - Duration::days(TOMBSTONE_RETENTION_DAYS) {
                return Err(SyncError::Expired);
            }
            Ok((Pos { at: c.c, id: c.i }, Pos { at: c.d, id: c.di }, false))
        }
        None => Ok((
            Pos { at: epoch(), id: 0 },
            // フル同期では、同期を始めた以降の削除だけ拾えばよい
            Pos { at: server_time - Duration::seconds(REWIND_SECONDS), id: 0 },
            true,
        )),
    }
}

/// 次に返す cursor を決める。最終ページのときだけ「応答時刻 - 10秒」より先へ進めない
fn next_cursor(last_change: Pos, last_delete: Pos, has_more: bool, server_time: DateTime<Utc>) -> SyncCursor {
    let (c, d) = if has_more {
        (last_change, last_delete)
    } else {
        let floor = Pos { at: server_time - Duration::seconds(REWIND_SECONDS), id: 0 };
        (last_change.min(floor), last_delete.min(floor))
    };
    SyncCursor { c: c.at, i: c.id, d: d.at, di: d.id }
}

async fn server_now(pool: &PgPool) -> Result<DateTime<Utc>, SyncError> {
    Ok(sqlx::query_scalar::<_, DateTime<Utc>>("SELECT NOW()").fetch_one(pool).await?)
}

async fn purge_old_tombstones(pool: &PgPool) -> Result<(), SyncError> {
    sqlx::query("DELETE FROM sync_tombstones WHERE deleted_at < NOW() - make_interval(days => $1)")
        .bind(TOMBSTONE_RETENTION_DAYS as i32)
        .execute(pool)
        .await?;
    Ok(())
}

/// 利用者が見られるチケットの範囲。`push_ticket_access_sql`（ticket_repo.rs）と同じ判定。
pub async fn ticket_access(pool: &PgPool, user_id: i32) -> anyhow::Result<SyncAccessOut> {
    let is_staff: bool = sqlx::query_scalar("SELECT is_staff FROM accounts_user WHERE id = $1")
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .unwrap_or(false);
    if is_staff {
        return Ok(SyncAccessOut { all: true, team_ids: vec![], scoped_projects: vec![] });
    }

    let rows = sqlx::query(
        "SELECT DISTINCT tm.team_id::int4 AS team_id, tm.scoped_project_id::int4 AS project_id
         FROM t_team_membership tm
         LEFT JOIN tickets_project p ON tm.scoped_project_id = p.id
         WHERE tm.user_id = $1
           AND (
             tm.scoped_project_id IS NULL OR
             (tm.end_date IS NULL OR NOW()::date <= tm.end_date + (p.grace_period_days || ' days')::interval)
           )
         ORDER BY 1, 2",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let mut team_ids = Vec::new();
    let mut scoped_projects = Vec::new();
    for row in rows {
        let team_id: i32 = row.get("team_id");
        match row.get::<Option<i32>, _>("project_id") {
            None => team_ids.push(team_id),
            Some(project_id) => scoped_projects.push(ScopedProjectOut { team_id, project_id }),
        }
    }
    team_ids.dedup();
    Ok(SyncAccessOut { all: false, team_ids, scoped_projects })
}

/// access の範囲に (team_id, project_id) のチケットが入るか
pub fn access_allows(access: &SyncAccessOut, team_id: Option<i32>, project_id: Option<i32>) -> bool {
    if access.all {
        return true;
    }
    let Some(team_id) = team_id else { return false };
    access.team_ids.contains(&team_id)
        || project_id.is_some_and(|pid| {
            access.scoped_projects.iter().any(|s| s.team_id == team_id && s.project_id == pid)
        })
}

/// チケットの差分同期（GET /api/v1/sync/tickets/）
pub async fn sync_tickets(
    pool: &PgPool,
    user_id: i32,
    cursor: Option<SyncCursor>,
    limit: i64,
) -> Result<SyncPageOut<TicketSyncOut>, SyncError> {
    let limit = limit.clamp(1, 1000);
    let server_time = server_now(pool).await?;
    let (change_pos, delete_pos, full) = start_positions(cursor.as_ref(), server_time)?;
    if full {
        purge_old_tombstones(pool).await?;
    }
    let access = ticket_access(pool, user_id).await?;

    // ── 変更（権限では絞らずに取り、見られない行は deleted として返す） ──
    const FROM_MARKER: &str = "\n FROM tickets_ticket t\n";
    debug_assert!(API_TICKET_SELECT.contains(FROM_MARKER));
    let select = API_TICKET_SELECT.replacen(
        FROM_MARKER,
        ",
    t.description AS sync_description,
    t.closed_at AS sync_closed_at,
    t.sync_changed_at AS sync_changed_at,
    t.id::int8 AS sync_id
 FROM tickets_ticket t
",
        1,
    );
    let sql = format!(
        "{select} WHERE (t.sync_changed_at, t.id) > ($1, $2) ORDER BY t.sync_changed_at, t.id LIMIT $3"
    );
    let mut rows = sqlx::query(&sql)
        .bind(change_pos.at)
        .bind(change_pos.id)
        .bind(limit + 1)
        .fetch_all(pool)
        .await?;
    let has_more_changes = rows.len() as i64 > limit;
    rows.truncate(limit as usize);

    let mut last_change = change_pos;
    let mut deleted = Vec::new();
    let mut extras: HashMap<i32, (String, Option<DateTime<Utc>>)> = HashMap::new();
    let mut visible_rows = Vec::with_capacity(rows.len());
    for row in rows {
        let id: i32 = row.get(0);
        let project_id: Option<i32> = row.get(8);
        let team_id: Option<i32> = row.get(37);
        last_change = Pos { at: row.get("sync_changed_at"), id: row.get("sync_id") };
        if access_allows(&access, team_id, project_id) {
            extras.insert(id, (row.get("sync_description"), row.get("sync_closed_at")));
            visible_rows.push(row);
        } else {
            // 見られなくなった（チームを移された等）。キーは漏らさない
            deleted.push(SyncDeletedOut { id: id as i64, key: None });
        }
    }
    let changes = hydrate_ticket_rows(pool, visible_rows)
        .await?
        .into_iter()
        .map(|base| {
            let (description, closed_at) = extras.remove(&base.id).unwrap_or_default();
            TicketSyncOut { base, description, closed_at }
        })
        .collect();

    // ── 削除 ──
    let (tomb, has_more_deletes, last_delete) = fetch_tombstones(pool, "ticket", delete_pos).await?;
    for t in tomb {
        let key = if access_allows(&access, t.team_id, t.project_id) { t.key } else { None };
        deleted.push(SyncDeletedOut { id: t.entity_id, key });
    }

    let has_more = has_more_changes || has_more_deletes;
    Ok(SyncPageOut {
        changes,
        deleted,
        access,
        cursor: next_cursor(last_change, last_delete, has_more, server_time).encode(),
        has_more,
        server_time,
    })
}

struct Tombstone {
    entity_id: i64,
    key: Option<String>,
    team_id: Option<i32>,
    project_id: Option<i32>,
}

async fn fetch_tombstones(
    pool: &PgPool,
    entity: &str,
    from: Pos,
) -> Result<(Vec<Tombstone>, bool, Pos), SyncError> {
    let mut rows = sqlx::query(
        "SELECT id, entity_id, entity_key, team_id::int4 AS team_id, project_id::int4 AS project_id, deleted_at
         FROM sync_tombstones
         WHERE entity = $1 AND (deleted_at, id) > ($2, $3)
         ORDER BY deleted_at, id
         LIMIT $4",
    )
    .bind(entity)
    .bind(from.at)
    .bind(from.id)
    .bind(TOMBSTONE_PAGE + 1)
    .fetch_all(pool)
    .await?;
    let has_more = rows.len() as i64 > TOMBSTONE_PAGE;
    rows.truncate(TOMBSTONE_PAGE as usize);
    let mut last = from;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        last = Pos { at: row.get("deleted_at"), id: row.get("id") };
        out.push(Tombstone {
            entity_id: row.get("entity_id"),
            key: row.get("entity_key"),
            team_id: row.get("team_id"),
            project_id: row.get("project_id"),
        });
    }
    Ok((out, has_more, last))
}

/// プロジェクトの差分同期（GET /api/v1/sync/projects/）。一覧 API と同じく権限では絞らない。
pub async fn sync_projects(
    pool: &PgPool,
    user_id: i32,
    cursor: Option<SyncCursor>,
    limit: i64,
) -> Result<SyncPageOut<ProjectSyncOut>, SyncError> {
    let limit = limit.clamp(1, 1000);
    let server_time = server_now(pool).await?;
    let (change_pos, delete_pos, full) = start_positions(cursor.as_ref(), server_time)?;
    if full {
        purge_old_tombstones(pool).await?;
    }

    // 列の意味は resource_repo::find_all_projects と同じ
    let mut rows = sqlx::query(
        "SELECT
            p.id::int4 AS id, p.name, p.prefix, p.description, p.status, p.priority,
            p.target_end_date, p.created_at, p.updated_at, p.owner_id::int4 AS owner_id,
            (SELECT COUNT(*)::int8 FROM tickets_ticket WHERE project_id = p.id) AS ticket_count,
            (
              SELECT COUNT(DISTINCT tm.user_id)::int8
              FROM tickets_project_teams pt
              JOIN t_team_membership tm ON tm.team_id = pt.team_id
              WHERE pt.project_id = p.id
                AND (tm.scoped_project_id IS NULL OR tm.scoped_project_id = p.id)
            ) AS member_count,
            EXISTS(
              SELECT 1
              FROM tickets_project_teams pt2
              JOIN t_team_membership tm2 ON tm2.team_id = pt2.team_id
              WHERE pt2.project_id = p.id
                AND tm2.user_id = $4
                AND (
                  tm2.scoped_project_id IS NULL
                  OR (
                    tm2.scoped_project_id = p.id
                    AND (tm2.end_date IS NULL OR tm2.end_date + p.grace_period_days >= NOW()::date)
                  )
                )
            ) AS is_member,
            COALESCE(
              (
                SELECT json_agg(json_build_object(
                  'id', t.id, 'name', t.name, 'slug', t.slug, 'icon', t.icon, 'color', t.color,
                  'archived', (t.archived_at IS NOT NULL)
                ) ORDER BY t.name)
                FROM tickets_project_teams pt
                JOIN m_team t ON t.id = pt.team_id
                WHERE pt.project_id = p.id
              ),
              '[]'::json
            ) AS teams,
            p.cycle_auto_complete,
            p.cycle_auto_create_next,
            p.parent_project_id::int4 AS parent_project_id,
            (SELECT COUNT(*)::int8 FROM tickets_project WHERE parent_project_id = p.id) AS child_count,
            COALESCE(
              (SELECT json_agg(rp.roadmap_id::int4 ORDER BY rp.roadmap_id) FROM roadmap_projects rp WHERE rp.project_id = p.id),
              '[]'::json
            ) AS roadmap_ids,
            p.sync_changed_at,
            p.id::int8 AS sync_id
         FROM tickets_project p
         WHERE (p.sync_changed_at, p.id) > ($1, $2)
         ORDER BY p.sync_changed_at, p.id
         LIMIT $3",
    )
    .bind(change_pos.at)
    .bind(change_pos.id)
    .bind(limit + 1)
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let has_more_changes = rows.len() as i64 > limit;
    rows.truncate(limit as usize);

    let mut last_change = change_pos;
    let mut changes = Vec::with_capacity(rows.len());
    for row in rows {
        last_change = Pos { at: row.get("sync_changed_at"), id: row.get("sync_id") };
        let teams: Vec<ProjectTeamOut> = row
            .get::<Option<serde_json::Value>, _>("teams")
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        let roadmap_ids: Vec<i32> = row
            .get::<Option<serde_json::Value>, _>("roadmap_ids")
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        changes.push(ProjectSyncOut {
            base: ProjectOut {
                id: row.get("id"),
                name: row.get("name"),
                prefix: row.get("prefix"),
                description: row.get("description"),
                status: row.get("status"),
                priority: row.get("priority"),
                target_end_date: row.get("target_end_date"),
                ticket_count: row.get("ticket_count"),
                member_count: row.get("member_count"),
                is_member: row.get("is_member"),
                teams,
                owner_id: row.get("owner_id"),
                created_at: row.get("created_at"),
                cycle_auto_complete: row.get("cycle_auto_complete"),
                cycle_auto_create_next: row.get("cycle_auto_create_next"),
                parent_project_id: row.get("parent_project_id"),
                child_count: row.get("child_count"),
                roadmap_ids,
            },
            updated_at: row.get("updated_at"),
        });
    }

    let (tomb, has_more_deletes, last_delete) = fetch_tombstones(pool, "project", delete_pos).await?;
    let deleted = tomb
        .into_iter()
        .map(|t| SyncDeletedOut { id: t.entity_id, key: t.key })
        .collect();

    let has_more = has_more_changes || has_more_deletes;
    Ok(SyncPageOut {
        changes,
        deleted,
        access: SyncAccessOut { all: true, team_ids: vec![], scoped_projects: vec![] },
        cursor: next_cursor(last_change, last_delete, has_more, server_time).encode(),
        has_more,
        server_time,
    })
}

#[cfg(test)]
mod tests {
    use crate::test_support::{create_test_team, create_test_user, test_pool, unique_suffix};
    use super::{sync_tickets, ticket_access, SyncCursor, SyncError};

    #[tokio::test]
    async fn test_t2_ticket_deletion_records_tombstone() {
        let Some(pool) = test_pool().await else { return; };

        let user_id = create_test_user(&pool, "test-user").await;
        let team_id = create_test_team(&pool, "test-team").await;
        let suffix = unique_suffix();
        let ticket_key = format!("T{}", &suffix[..7.min(suffix.len())]);

        // Create ticket with all required columns
        let ticket_id: i32 = sqlx::query_scalar(
            "INSERT INTO tickets_ticket (team_id, author_id, title, description, ticket_key, status, priority, created_at, updated_at, gantt_order, ticket_type)
             VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW(), 0, 'task')
             RETURNING id::int4"
        )
        .bind(team_id)
        .bind(user_id)
        .bind("Test Ticket")
        .bind("Test Description")
        .bind(&ticket_key)
        .bind("open")
        .bind("medium")
        .fetch_one(&pool)
        .await
        .unwrap();

        let fetched_ticket_key: String = sqlx::query_scalar("SELECT ticket_key FROM tickets_ticket WHERE id = $1")
            .bind(ticket_id)
            .fetch_one(&pool)
            .await
            .unwrap();

        // Delete ticket
        let deleted_count = sqlx::query("DELETE FROM tickets_ticket WHERE id = $1")
            .bind(ticket_id)
            .execute(&pool)
            .await
            .unwrap()
            .rows_affected();

        assert_eq!(deleted_count, 1);

        // Check tombstone record exists
        let tombstone: (String, i64, Option<String>, i64) = sqlx::query_as(
            "SELECT entity, entity_id, entity_key, team_id FROM sync_tombstones WHERE entity = 'ticket' AND entity_id = $1"
        )
        .bind(ticket_id as i64)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(tombstone.0, "ticket");
        assert_eq!(tombstone.1, ticket_id as i64);
        assert_eq!(tombstone.2, Some(fetched_ticket_key));
        assert_eq!(tombstone.3, team_id as i64);
    }

    #[tokio::test]
    async fn test_t2_project_deletion_records_tombstone() {
        let Some(pool) = test_pool().await else { return; };

        let user_id = create_test_user(&pool, "test-user").await;
        let suffix = unique_suffix();
        let prefix = format!("P{}", &suffix[..6.min(suffix.len())]);

        // Create project
        let project_id: i32 = sqlx::query_scalar(
            "INSERT INTO tickets_project (name, prefix, description, created_at, grace_period_days, status, owner_id, priority)
             VALUES ($1, $2, $3, NOW(), 7, 'in_progress', $4, $5)
             RETURNING id::int4"
        )
        .bind("Test Project")
        .bind(&prefix)
        .bind("")
        .bind(user_id)
        .bind("medium")
        .fetch_one(&pool)
        .await
        .unwrap();

        let fetched_prefix: String = sqlx::query_scalar("SELECT prefix FROM tickets_project WHERE id = $1")
            .bind(project_id)
            .fetch_one(&pool)
            .await
            .unwrap();

        // Delete project
        let deleted_count = sqlx::query("DELETE FROM tickets_project WHERE id = $1")
            .bind(project_id)
            .execute(&pool)
            .await
            .unwrap()
            .rows_affected();

        assert_eq!(deleted_count, 1);

        // Check tombstone record exists
        let tombstone: (String, i64, Option<String>, Option<i64>, i64) = sqlx::query_as(
            "SELECT entity, entity_id, entity_key, team_id, project_id FROM sync_tombstones WHERE entity = 'project' AND entity_id = $1"
        )
        .bind(project_id as i64)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(tombstone.0, "project");
        assert_eq!(tombstone.1, project_id as i64);
        assert_eq!(tombstone.2, Some(fetched_prefix));
        assert_eq!(tombstone.3, None); // team_id should be NULL for project
        assert_eq!(tombstone.4, project_id as i64);
    }

    #[tokio::test]
    async fn test_t4_idempotency_key_unique_constraint() {
        let Some(pool) = test_pool().await else { return; };

        let user_id = create_test_user(&pool, "test-user").await;
        let team_id = create_test_team(&pool, "test-team").await;
        let suffix = unique_suffix();
        let ticket_key1 = format!("T{}", &suffix[..6.min(suffix.len())]);
        let ticket_key2 = format!("T2{}", &suffix[..5.min(suffix.len())]);

        let idempotency_key = uuid::Uuid::new_v4();

        // Create first ticket with idempotency key
        let result1 = sqlx::query_scalar::<_, i32>(
            "INSERT INTO tickets_ticket (team_id, author_id, title, description, ticket_key, status, priority, created_at, updated_at, gantt_order, ticket_type, client_request_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW(), 0, 'task', $8)
             RETURNING id::int4"
        )
        .bind(team_id)
        .bind(user_id)
        .bind("Test Ticket")
        .bind("Test Description")
        .bind(&ticket_key1)
        .bind("open")
        .bind("medium")
        .bind(idempotency_key)
        .fetch_one(&pool)
        .await;

        assert!(result1.is_ok(), "First insert should succeed");

        // Try to create second ticket with same idempotency key
        let result2 = sqlx::query_scalar::<_, i32>(
            "INSERT INTO tickets_ticket (team_id, author_id, title, description, ticket_key, status, priority, created_at, updated_at, gantt_order, ticket_type, client_request_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW(), 0, 'task', $8)
             RETURNING id::int4"
        )
        .bind(team_id)
        .bind(user_id)
        .bind("Test Ticket 2")
        .bind("Test Description 2")
        .bind(&ticket_key2)
        .bind("open")
        .bind("medium")
        .bind(idempotency_key)
        .fetch_one(&pool)
        .await;

        assert!(result2.is_err(), "Second insert with same idempotency key should fail");
        let err_str = result2.unwrap_err().to_string();
        assert!(err_str.contains("23505") || err_str.contains("duplicate"), "Error should be UNIQUE constraint violation");

        // Verify only first ticket exists
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM tickets_ticket WHERE client_request_id = $1"
        )
        .bind(idempotency_key)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(count, 1, "Only one ticket should have this idempotency key");
    }


    #[tokio::test]
    async fn test_t2_cursor_expiration() {
        let Some(pool) = test_pool().await else { return; };

        let user_id = create_test_user(&pool, "test-user").await;
        let team_id = create_test_team(&pool, "test-team").await;

        // Mark as staff
        sqlx::query("UPDATE accounts_user SET is_staff = true WHERE id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();

        // Create a cursor with an expired deletion position (> 90 days ago)
        let expired_cursor = SyncCursor {
            c: chrono::Utc::now(),
            i: 0,
            d: chrono::Utc::now() - chrono::Duration::days(100),
            di: 0,
        };

        let result = sync_tickets(&pool, user_id, Some(expired_cursor), 100).await;

        assert!(
            matches!(result, Err(SyncError::Expired)),
            "Should return Expired error for cursor > 90 days old"
        );
    }


    // ── ここから親レビューで追加（T1 / T1b / T3） ──

    /// team だけのチケットを作る（project なし）
    async fn team_ticket(pool: &sqlx::PgPool, team_id: i32, author: i32) -> i32 {
        let key = format!("SY-{}", unique_suffix());
        sqlx::query_scalar(
            "INSERT INTO tickets_ticket (team_id, author_id, title, description, ticket_key, status, priority, created_at, updated_at, gantt_order, ticket_type)
             VALUES ($1, $2, $3, '', $3, 'open', 'medium', NOW(), NOW(), 0, 'task') RETURNING id::int4",
        )
        .bind(team_id)
        .bind(author)
        .bind(&key)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn staff_user(pool: &sqlx::PgPool) -> i32 {
        let u = create_test_user(pool, "sync-staff").await;
        sqlx::query("UPDATE accounts_user SET is_staff = true WHERE id = $1").bind(u).execute(pool).await.unwrap();
        u
    }

    async fn updated_at(pool: &sqlx::PgPool, id: i32) -> chrono::DateTime<chrono::Utc> {
        sqlx::query_scalar("SELECT updated_at FROM tickets_ticket WHERE id = $1").bind(id).fetch_one(pool).await.unwrap()
    }

    async fn sync_changed_at(pool: &sqlx::PgPool, id: i32) -> chrono::DateTime<chrono::Utc> {
        sqlx::query_scalar("SELECT sync_changed_at FROM tickets_ticket WHERE id = $1").bind(id).fetch_one(pool).await.unwrap()
    }

    async fn rewind_updated_at(pool: &sqlx::PgPool, id: i32) {
        sqlx::query("UPDATE tickets_ticket SET updated_at = '2000-01-01T00:00:00Z' WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
        assert_eq!(updated_at(pool, id).await.to_rfc3339(), "2000-01-01T00:00:00+00:00");
    }

    fn recent(t: chrono::DateTime<chrono::Utc>) -> bool {
        t > chrono::Utc::now() - chrono::Duration::minutes(5)
    }

    /// T1: limit=2 で順に取っていくと、作った5件がちょうど1回ずつ返る（ページ境界で取りこぼし・重複なし）
    #[tokio::test]
    async fn t1_paging_returns_each_row_exactly_once() {
        let Some(pool) = test_pool().await else { return; };
        let staff = staff_user(&pool).await;
        let team = create_test_team(&pool, "sync-t1").await;

        // 作成直前の位置から始める（テストDBの他の行を読み飛ばすため）
        let start: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()").fetch_one(&pool).await.unwrap();
        let mut ids = Vec::new();
        for _ in 0..5 {
            ids.push(team_ticket(&pool, team, staff).await);
        }

        let mut cursor = Some(SyncCursor { c: start, i: 0, d: chrono::Utc::now(), di: 0 });
        let mut seen: Vec<i32> = Vec::new();
        for _ in 0..50 {
            let page = sync_tickets(&pool, staff, cursor.clone(), 2).await.expect("sync");
            assert!(page.changes.len() <= 2);
            seen.extend(page.changes.iter().map(|t| t.base.id).filter(|id| ids.contains(id)));
            cursor = Some(SyncCursor::decode(&page.cursor).unwrap());
            if !page.has_more {
                break;
            }
        }
        seen.sort();
        let mut expected = ids.clone();
        expected.sort();
        assert_eq!(seen, expected, "5件がちょうど1回ずつ返ること");
    }

    /// T1b: 見られないチームへ移したチケットは deleted（key なし）で返る。削除したチケットは key 付きで返る
    #[tokio::test]
    async fn t1b_moved_out_ticket_is_reported_as_deleted() {
        let Some(pool) = test_pool().await else { return; };
        let author = create_test_user(&pool, "sync-author").await;
        let member = create_test_user(&pool, "sync-member").await;
        let team_a = create_test_team(&pool, "sync-a").await;
        let team_b = create_test_team(&pool, "sync-b").await;
        sqlx::query("INSERT INTO t_team_membership (team_id, user_id, role, joined_at) VALUES ($1, $2, 'member', NOW())")
            .bind(team_a)
            .bind(member)
            .execute(&pool)
            .await
            .unwrap();

        let access = ticket_access(&pool, member).await.unwrap();
        assert!(!access.all);
        assert_eq!(access.team_ids, vec![team_a]);

        let start: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()").fetch_one(&pool).await.unwrap();
        let moved = team_ticket(&pool, team_a, author).await;
        let removed = team_ticket(&pool, team_a, author).await;
        let removed_key: String = sqlx::query_scalar("SELECT ticket_key FROM tickets_ticket WHERE id = $1").bind(removed).fetch_one(&pool).await.unwrap();

        let first = sync_tickets(&pool, member, Some(SyncCursor { c: start, i: 0, d: chrono::Utc::now(), di: 0 }), 1000).await.unwrap();
        let first_ids: Vec<i32> = first.changes.iter().map(|t| t.base.id).collect();
        assert!(first_ids.contains(&moved) && first_ids.contains(&removed));
        // 付随データは含まない・説明文は含む
        let json = serde_json::to_value(&first.changes[0]).unwrap();
        assert!(json.get("description").is_some() && json.get("comments").is_none());

        sqlx::query("UPDATE tickets_ticket SET team_id = $1 WHERE id = $2").bind(team_b).bind(moved).execute(&pool).await.unwrap();
        sqlx::query("DELETE FROM tickets_ticket WHERE id = $1").bind(removed).execute(&pool).await.unwrap();

        // 前回の cursor から（最終ページの cursor は10秒巻き戻っているので、今の変更も必ず拾える）
        let cursor = SyncCursor::decode(&first.cursor).unwrap();
        let second = sync_tickets(&pool, member, Some(cursor), 1000).await.unwrap();
        assert!(!second.changes.iter().any(|t| t.base.id == moved), "見られない行は changes に入らない");
        let moved_del = second.deleted.iter().find(|d| d.id == moved as i64).expect("移したチケットが deleted に入る");
        assert_eq!(moved_del.key, None, "見られないチケットのキーは返さない");
        let removed_del = second.deleted.iter().find(|d| d.id == removed as i64).expect("削除したチケットが deleted に入る");
        assert_eq!(removed_del.key.as_deref(), Some(removed_key.as_str()));
    }

    /// 最終ページの cursor は「応答時刻 - 10秒」より先へ進まない
    #[tokio::test]
    async fn last_page_cursor_is_rewound() {
        let Some(pool) = test_pool().await else { return; };
        let staff = staff_user(&pool).await;
        let page = sync_tickets(&pool, staff, Some(SyncCursor { c: chrono::Utc::now() - chrono::Duration::seconds(1), i: 0, d: chrono::Utc::now(), di: 0 }), 1000).await.unwrap();
        assert!(!page.has_more);
        let c = SyncCursor::decode(&page.cursor).unwrap();
        assert!(c.c <= page.server_time - chrono::Duration::seconds(10));
        assert!(c.d <= page.server_time - chrono::Duration::seconds(10));
    }

    /// T3: updated_at を明示しない更新・中間テーブルの付け外しで updated_at が動く。表示名変更では動かない
    #[tokio::test]
    async fn t3_updated_at_follows_every_change() {
        let Some(pool) = test_pool().await else { return; };
        let author = create_test_user(&pool, "sync-t3").await;
        let team = create_test_team(&pool, "sync-t3").await;
        let id = team_ticket(&pool, team, author).await;

        // (a) サイクル繰越などと同じ「updated_at を書かない UPDATE」
        rewind_updated_at(&pool, id).await;
        sqlx::query("UPDATE tickets_ticket SET cycle_id = NULL, status = 'in_progress' WHERE id = $1").bind(id).execute(&pool).await.unwrap();
        assert!(recent(updated_at(&pool, id).await), "(a) 列の更新で動く");

        // (b) Webhook の closed_at だけの更新
        rewind_updated_at(&pool, id).await;
        sqlx::query("UPDATE tickets_ticket SET closed_at = NOW() WHERE id = $1").bind(id).execute(&pool).await.unwrap();
        assert!(recent(updated_at(&pool, id).await), "(b) closed_at で動く");

        // (c) 担当者の追加・削除
        rewind_updated_at(&pool, id).await;
        sqlx::query("INSERT INTO tickets_ticket_assignees (ticketmodel_id, user_id) VALUES ($1, $2)").bind(id).bind(author).execute(&pool).await.unwrap();
        assert!(recent(updated_at(&pool, id).await), "(c) 担当者追加で動く");
        rewind_updated_at(&pool, id).await;
        sqlx::query("DELETE FROM tickets_ticket_assignees WHERE ticketmodel_id = $1").bind(id).execute(&pool).await.unwrap();
        assert!(recent(updated_at(&pool, id).await), "(c) 担当者削除で動く");

        // (d) ラベルの付与
        let label: i32 = sqlx::query_scalar(
            "INSERT INTO m_label (name, color, created_at, team_id) VALUES ($1, '#123456', NOW(), $2) RETURNING id::int4",
        )
        .bind(format!("lbl-{}", unique_suffix()))
        .bind(team)
        .fetch_one(&pool)
        .await
        .unwrap();
        rewind_updated_at(&pool, id).await;
        sqlx::query("INSERT INTO tickets_ticket_labels (ticketmodel_id, labelmodel_id) VALUES ($1, $2)").bind(id).bind(label).execute(&pool).await.unwrap();
        assert!(recent(updated_at(&pool, id).await), "(d) ラベル付与で動く");

        // (e) ラベル名の変更: updated_at は動かず、sync_changed_at だけ動く
        rewind_updated_at(&pool, id).await;
        let before_sync = sync_changed_at(&pool, id).await;
        sqlx::query("UPDATE m_label SET name = $1 WHERE id = $2").bind(format!("renamed-{}", unique_suffix())).bind(label).execute(&pool).await.unwrap();
        assert_eq!(updated_at(&pool, id).await.to_rfc3339(), "2000-01-01T00:00:00+00:00", "(e) 表示名変更では updated_at は動かない");
        assert!(sync_changed_at(&pool, id).await > before_sync, "(e) sync_changed_at は動く");

        // (f) アーカイブ済みチームのチケットに付いたラベルの名前変更・チーム名変更がエラーにならない
        sqlx::query("UPDATE m_team SET archived_at = NOW() WHERE id = $1").bind(team).execute(&pool).await.expect("(f) チームのアーカイブ");
        sqlx::query("UPDATE m_label SET name = $1 WHERE id = $2").bind(format!("arch-{}", unique_suffix())).bind(label).execute(&pool).await.expect("(f) ラベル名変更");
        sqlx::query("UPDATE m_team SET name = $1 WHERE id = $2").bind(format!("arch-team-{}", unique_suffix())).bind(team).execute(&pool).await.expect("(f) チーム名変更");
        // アーカイブ済みチームのチケット本体の更新は従来どおり拒否される
        let err = sqlx::query("UPDATE tickets_ticket SET title = 'x' WHERE id = $1").bind(id).execute(&pool).await;
        assert!(err.is_err(), "(f) アーカイブ済みチームのチケット本体は更新できない");
    }

    /// プロジェクトの同期: 作成・参加チーム追加・削除が順に届く
    #[tokio::test]
    async fn projects_sync_changes_and_deletes() {
        let Some(pool) = test_pool().await else { return; };
        let user = create_test_user(&pool, "sync-prj").await;
        let team = create_test_team(&pool, "sync-prj").await;
        let start: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()").fetch_one(&pool).await.unwrap();
        let prefix = format!("SP{}", &unique_suffix()[..6]).to_uppercase();
        let pid: i32 = sqlx::query_scalar(
            "INSERT INTO tickets_project (name, prefix, description, created_at, grace_period_days, status, priority) VALUES ($1, $1, '', NOW(), 7, 'in_progress', 'medium') RETURNING id::int4",
        )
        .bind(&prefix)
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO tickets_project_teams (project_id, team_id, joined_at) VALUES ($1, $2, NOW())").bind(pid).bind(team).execute(&pool).await.unwrap();

        let page = super::sync_projects(&pool, user, Some(SyncCursor { c: start, i: 0, d: chrono::Utc::now(), di: 0 }), 1000).await.unwrap();
        let p = page.changes.iter().find(|p| p.base.id == pid).expect("作成したプロジェクトが届く");
        assert_eq!(p.base.teams.len(), 1, "参加チームが入っている");
        assert!(page.access.all);

        let cursor = SyncCursor::decode(&page.cursor).unwrap();
        sqlx::query("DELETE FROM tickets_project_teams WHERE project_id = $1").bind(pid).execute(&pool).await.unwrap();
        sqlx::query("DELETE FROM tickets_project WHERE id = $1").bind(pid).execute(&pool).await.unwrap();
        let page2 = super::sync_projects(&pool, user, Some(cursor), 1000).await.unwrap();
        let del = page2.deleted.iter().find(|d| d.id == pid as i64).expect("削除が届く");
        assert_eq!(del.key.as_deref(), Some(prefix.as_str()));
    }

    /// T6 のフィクスチャ生成（通常の cargo test では動かさない）。
    ///
    /// 同じ行データに対して一覧 API（api_find_all）の結果を条件ごとに記録し、
    /// フロントエンドの端末側絞り込み（repos/ticketQuery.ts）と比べるためのファイルを書き出す。
    ///   PARITY_FIXTURE_OUT=../frontend/src/shared/sync/repos/__fixtures__/ticketQueryParity.json \
    ///     cargo test generate_ticket_query_parity_fixture -- --ignored
    #[tokio::test]
    #[ignore]
    async fn generate_ticket_query_parity_fixture() {
        use crate::infrastructure::repositories::ticket_repo::{api_find_all, ApiTicketFilter};
        let Some(pool) = test_pool().await else { panic!("TEST_DATABASE_URL が必要です") };
        let Ok(out) = std::env::var("PARITY_FIXTURE_OUT") else { panic!("PARITY_FIXTURE_OUT が必要です") };

        let staff = staff_user(&pool).await;
        let u1 = create_test_user(&pool, "par-u1").await;
        let u2 = create_test_user(&pool, "par-u2").await;
        let team = create_test_team(&pool, "parity").await;
        let slug: String = sqlx::query_scalar("SELECT slug FROM m_team WHERE id = $1").bind(team).fetch_one(&pool).await.unwrap();
        let prefix = format!("PQ{}", &unique_suffix()[..5]).to_uppercase();
        let project: i32 = sqlx::query_scalar(
            "INSERT INTO tickets_project (name, prefix, description, created_at, grace_period_days, status, priority) VALUES ($1, $1, '', NOW(), 7, 'in_progress', 'medium') RETURNING id::int4",
        ).bind(&prefix).fetch_one(&pool).await.unwrap();
        sqlx::query("INSERT INTO tickets_project_teams (project_id, team_id, joined_at) VALUES ($1, $2, NOW())").bind(project).bind(team).execute(&pool).await.unwrap();
        let milestone: i32 = sqlx::query_scalar(
            "INSERT INTO milestones_milestone (name, due_date, description, created_at, project_id) VALUES ('M1', '2026-10-01', '', NOW(), $1) RETURNING id::int4",
        ).bind(project).fetch_one(&pool).await.unwrap();
        let mut labels = Vec::new();
        for n in ["bug", "ui"] {
            labels.push(sqlx::query_scalar::<_, i32>(
                "INSERT INTO m_label (name, color, created_at, team_id) VALUES ($1, '#111111', NOW(), $2) RETURNING id::int4",
            ).bind(format!("{n}-{}", unique_suffix())).bind(team).fetch_one(&pool).await.unwrap());
        }

        let statuses = ["backlog", "open", "in_progress", "resolved", "closed"];
        let priorities = ["low", "medium", "high", "urgent"];
        let mut ids: Vec<i32> = Vec::new();
        for i in 0..32i32 {
            let key = format!("{prefix}-{:06}", i + 1);
            let with_project = i % 3 != 0;
            let due: Option<chrono::NaiveDate> = if i % 4 == 0 { None } else { chrono::NaiveDate::from_ymd_opt(2026, 9, 20 + (i % 7) as u32) };
            let title = if i % 5 == 0 { format!("Login 画面の修正 {i}") } else { format!("タスク {i}") };
            let description = if i % 6 == 0 { "API の遅延を調べる" } else { "" };
            let id: i32 = sqlx::query_scalar(
                "INSERT INTO tickets_ticket (ticket_key, title, description, status, priority, ticket_type, project_id, author_id, team_id, created_at, updated_at, gantt_order, due_date, milestone_id, parent_id)
                 VALUES ($1, $2, $3, $4, $5, 'task', $6, $7, $8, NOW(), NOW(), $9, $10, $11, NULL) RETURNING id::int4",
            )
            .bind(&key)
            .bind(&title)
            .bind(description)
            .bind(statuses[(i % 5) as usize])
            .bind(priorities[(i % 4) as usize])
            .bind(if with_project { Some(project) } else { None })
            .bind(staff)
            .bind(team)
            .bind(i % 4) // 同値が多い並び順
            .bind(due)
            .bind(if with_project && i % 2 == 0 { Some(milestone) } else { None })
            .fetch_one(&pool)
            .await
            .unwrap();
            if i % 2 == 0 {
                sqlx::query("INSERT INTO tickets_ticket_assignees (ticketmodel_id, user_id) VALUES ($1, $2)").bind(id).bind(u1).execute(&pool).await.unwrap();
            }
            if i % 3 == 1 {
                sqlx::query("INSERT INTO tickets_ticket_assignees (ticketmodel_id, user_id) VALUES ($1, $2)").bind(id).bind(u2).execute(&pool).await.unwrap();
            }
            if i % 4 == 1 {
                sqlx::query("INSERT INTO tickets_ticket_labels (ticketmodel_id, labelmodel_id) VALUES ($1, $2)").bind(id).bind(labels[0]).execute(&pool).await.unwrap();
            }
            if i % 5 == 2 {
                sqlx::query("INSERT INTO tickets_ticket_labels (ticketmodel_id, labelmodel_id) VALUES ($1, $2)").bind(id).bind(labels[1]).execute(&pool).await.unwrap();
            }
            ids.push(id);
        }
        // 親子（先頭のチケットを親に）と、作成・更新日時（同時刻を含む）を最後に確定させる
        for (n, id) in ids.iter().enumerate() {
            let parent = if n > 0 && n % 7 == 0 { Some(ids[0]) } else { None };
            let created = chrono::Utc::now() - chrono::Duration::hours(100 - n as i64);
            let updated = chrono::Utc::now() - chrono::Duration::minutes(((n as i64) % 9) * 10) - chrono::Duration::microseconds(n as i64 % 3);
            sqlx::query("UPDATE tickets_ticket SET parent_id = $1, created_at = $2, updated_at = $3 WHERE id = $4")
                .bind(parent).bind(created).bind(updated).bind(id).execute(&pool).await.unwrap();
        }

        // 行データ（同期 API と同じ形）
        let page = sync_tickets(&pool, staff, None, 1000).await.unwrap();
        let mut tickets: Vec<serde_json::Value> = page
            .changes
            .iter()
            .filter(|t| ids.contains(&t.base.id))
            .map(|t| serde_json::to_value(t).unwrap())
            .collect();
        assert_eq!(tickets.len(), ids.len());
        tickets.sort_by_key(|t| t["id"].as_i64());

        let s = |v: &str| Some(v.to_string());
        let mut cases: Vec<(String, serde_json::Value, ApiTicketFilter, String, Option<String>)> = Vec::new();
        let orderings = ["-updated_at", "updated_at", "-created_at", "created_at", "due_date", "-due_date", "priority", "-priority", "gantt_order", "-gantt_order", "unknown"];
        for o in orderings {
            cases.push((format!("ordering {o}"), serde_json::json!({"ordering": o}), ApiTicketFilter::default(), o.to_string(), None));
        }
        let mut f = ApiTicketFilter::default(); f.status = Some(vec!["open".into()]);
        cases.push(("status".into(), serde_json::json!({"status": "open"}), f, "-updated_at".into(), None));
        let mut f = ApiTicketFilter::default(); f.status = Some(vec!["open".into(), "in_progress".into()]);
        cases.push(("status__in".into(), serde_json::json!({"status__in": "open,in_progress"}), f, "-updated_at".into(), None));
        let mut f = ApiTicketFilter::default(); f.priority = Some(vec!["high".into(), "urgent".into()]);
        cases.push(("priority__in".into(), serde_json::json!({"priority__in": "high,urgent", "ordering": "priority"}), f, "priority".into(), None));
        let mut f = ApiTicketFilter::default(); f.assignees = Some(u1);
        cases.push(("assignees".into(), serde_json::json!({"assignees": u1}), f, "-updated_at".into(), None));
        let mut f = ApiTicketFilter::default(); f.labels = Some(labels[0]);
        cases.push(("labels".into(), serde_json::json!({"labels": labels[0]}), f, "-updated_at".into(), None));
        let mut f = ApiTicketFilter::default(); f.project = Some(project);
        cases.push(("project".into(), serde_json::json!({"project": project}), f, "-updated_at".into(), None));
        let mut f = ApiTicketFilter::default(); f.project_prefix = s(&prefix);
        cases.push(("project__prefix".into(), serde_json::json!({"project__prefix": prefix, "ordering": "gantt_order"}), f, "gantt_order".into(), None));
        let mut f = ApiTicketFilter::default(); f.milestone = Some(milestone);
        cases.push(("milestone".into(), serde_json::json!({"milestone": milestone}), f, "-updated_at".into(), None));
        let mut f = ApiTicketFilter::default(); f.parent = Some(ids[0]);
        cases.push(("parent".into(), serde_json::json!({"parent": ids[0]}), f, "-updated_at".into(), None));
        let mut f = ApiTicketFilter::default(); f.parent_isnull = Some(true);
        cases.push(("parent__isnull".into(), serde_json::json!({"parent__isnull": true}), f, "-updated_at".into(), None));
        let mut f = ApiTicketFilter::default(); f.due_date_gte = chrono::NaiveDate::from_ymd_opt(2026, 9, 22); f.due_date_lte = chrono::NaiveDate::from_ymd_opt(2026, 9, 24);
        cases.push(("due range".into(), serde_json::json!({"due_date__gte": "2026-09-22", "due_date__lte": "2026-09-24", "ordering": "due_date"}), f, "due_date".into(), None));
        let mut f = ApiTicketFilter::default(); f.due_date_isnull = Some(false);
        cases.push(("due_date__isnull=false".into(), serde_json::json!({"due_date__isnull": false, "ordering": "-due_date"}), f, "-due_date".into(), None));
        cases.push(("search title".into(), serde_json::json!({"search": "login"}), ApiTicketFilter::default(), "-updated_at".into(), s("login")));
        cases.push(("search description".into(), serde_json::json!({"search": "api の"}), ApiTicketFilter::default(), "-updated_at".into(), s("api の")));
        cases.push(("search key".into(), serde_json::json!({"search": format!("{}-00001", prefix.to_lowercase())}), ApiTicketFilter::default(), "-updated_at".into(), Some(format!("{}-00001", prefix.to_lowercase()))));
        let mut f = ApiTicketFilter::default(); f.status = Some(vec!["open".into(), "backlog".into(), "in_progress".into()]); f.assignees = Some(u1);
        cases.push(("my issues".into(), serde_json::json!({"status__in": "open,backlog,in_progress", "assignees": u1}), f, "-updated_at".into(), None));

        let mut out_cases = Vec::new();
        for (name, mut params, mut filter, sort, search) in cases {
            filter.team_slug = Some(slug.clone());
            filter.user_id = Some(staff);
            params["team_slug"] = serde_json::Value::String(slug.clone());
            let rows = api_find_all(&pool, &filter, &sort, search.as_deref(), 1).await.unwrap();
            let expected: Vec<String> = rows.into_iter().map(|t| t.ticket_key).collect();
            assert!(expected.len() < 50, "1ページに収まる件数にすること");
            out_cases.push(serde_json::json!({"name": name, "params": params, "expected": expected}));
        }
        let doc = serde_json::json!({
            "note": "生成: rust/src/infrastructure/repositories/sync_repo.rs generate_ticket_query_parity_fixture",
            "tickets": tickets,
            "cases": out_cases,
        });
        std::fs::write(&out, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
        println!("wrote {out}");
    }
}
