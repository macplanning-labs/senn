/// infrastructure/repositories/project_activity_repo.rs — プロジェクト Activity(監査ログ)/ 進捗報告
///
/// - project_activity: 追記のみの時系列ログ。チケット系イベント(追加/除外/完了)は
///   DB トリガー(20260919100001_project_activity_and_updates.sql)が記録し、
///   それ以外(プロジェクト更新・チーム・マイルストーン・進捗投稿)はここの `record` で記録する。
/// - project_updates: 進捗報告(構造化データ。project_activity の payload には丸め込まない)

use serde_json::{json, Value as JsonValue};
use sqlx::PgPool;

use crate::domain::models::project_activity_api::{
    ProjectActivityOut, ProjectActivityPage, ProjectUpdateOut, ProjectUpdatePage,
};
use crate::domain::models::resource_api::ProjectOut;

const DEFAULT_LIMIT: i64 = 30;
const MAX_LIMIT: i64 = 100;

fn normalize_limit(limit: Option<i64>) -> i64 {
    limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

// =============================================================================
// Activity(監査ログ)
// =============================================================================

/// 1件記録する。
pub async fn record(
    pool: &PgPool,
    project_id: i32,
    actor_id: Option<i32>,
    event_type: &str,
    payload: JsonValue,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO project_activity (project_id, actor_id, event_type, payload)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(project_id as i64)
    .bind(actor_id.map(|v| v as i64))
    .bind(event_type)
    .bind(payload)
    .execute(pool)
    .await?;
    Ok(())
}

/// 記録に失敗しても、ユーザーの操作(プロジェクト更新など)自体は失敗させない。
/// 失敗はログに残す。
pub async fn record_best_effort(
    pool: &PgPool,
    project_id: i32,
    actor_id: Option<i32>,
    event_type: &str,
    payload: JsonValue,
) {
    if let Err(e) = record(pool, project_id, actor_id, event_type, payload).await {
        tracing::error!(
            "project_activity の記録に失敗(project_id={project_id}, event_type={event_type}): {e:?}"
        );
    }
}

/// プロジェクト属性の差分を `[{field, from, to}]` で返す。
/// 説明文は長くなり得るため、変更があったことだけを記録する(from/to は持たない)。
pub fn diff_project(old: &ProjectOut, new: &ProjectOut) -> Vec<JsonValue> {
    let mut changes = Vec::new();
    if old.name != new.name {
        changes.push(json!({"field": "name", "from": old.name, "to": new.name}));
    }
    if old.description != new.description {
        changes.push(json!({"field": "description"}));
    }
    if old.status != new.status {
        changes.push(json!({"field": "status", "from": old.status, "to": new.status}));
    }
    if old.priority != new.priority {
        changes.push(json!({"field": "priority", "from": old.priority, "to": new.priority}));
    }
    if old.target_end_date != new.target_end_date {
        changes.push(json!({
            "field": "targetEndDate",
            "from": old.target_end_date.map(|d| d.to_string()),
            "to": new.target_end_date.map(|d| d.to_string()),
        }));
    }
    if old.owner_id != new.owner_id {
        changes.push(json!({"field": "ownerId", "from": old.owner_id, "to": new.owner_id}));
    }
    changes
}

/// 変更前後を比べ、差分があれば `project_updated` として記録する。
pub async fn record_project_changes(
    pool: &PgPool,
    actor_id: Option<i32>,
    old: &ProjectOut,
    new: &ProjectOut,
) {
    let changes = diff_project(old, new);
    if changes.is_empty() {
        return;
    }
    record_best_effort(pool, new.id, actor_id, "project_updated", json!({"changes": changes})).await;
}

/// Activity を新しい順に返す。`before` を渡すと、その id より古いものだけを返す。
pub async fn list_activity(
    pool: &PgPool,
    project_id: i32,
    limit: Option<i64>,
    before: Option<i64>,
) -> anyhow::Result<ProjectActivityPage> {
    let limit = normalize_limit(limit);
    let mut rows = sqlx::query_as::<_, ProjectActivityOut>(
        "SELECT a.id, a.event_type, a.payload, a.actor_id,
                COALESCE(NULLIF(u.display_name, ''), u.username) AS actor_name,
                a.created_at
         FROM project_activity a
         LEFT JOIN accounts_user u ON u.id = a.actor_id
         WHERE a.project_id = $1 AND ($2::bigint IS NULL OR a.id < $2)
         ORDER BY a.id DESC
         LIMIT $3",
    )
    .bind(project_id as i64)
    .bind(before)
    .bind(limit + 1)
    .fetch_all(pool)
    .await?;

    let next = if rows.len() as i64 > limit {
        rows.truncate(limit as usize);
        rows.last().map(|r| r.id)
    } else {
        None
    };
    Ok(ProjectActivityPage { results: rows, next })
}

// =============================================================================
// 進捗報告(project_updates)
// =============================================================================

const UPDATE_SELECT: &str =
    "SELECT p.id, p.project_id, p.health, p.body, p.author_id,
            COALESCE(NULLIF(u.display_name, ''), u.username) AS author_name,
            p.created_at, p.updated_at
     FROM project_updates p
     LEFT JOIN accounts_user u ON u.id = p.author_id";

pub async fn create_update(
    pool: &PgPool,
    project_id: i32,
    author_id: i32,
    health: &str,
    body: &str,
) -> anyhow::Result<ProjectUpdateOut> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO project_updates (project_id, author_id, health, body)
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(project_id as i64)
    .bind(author_id as i64)
    .bind(health)
    .bind(body)
    .fetch_one(pool)
    .await?;
    find_update(pool, project_id, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("created project_update not found"))
}

pub async fn find_update(
    pool: &PgPool,
    project_id: i32,
    update_id: i64,
) -> anyhow::Result<Option<ProjectUpdateOut>> {
    let sql = format!("{UPDATE_SELECT} WHERE p.project_id = $1 AND p.id = $2");
    Ok(sqlx::query_as::<_, ProjectUpdateOut>(&sql)
        .bind(project_id as i64)
        .bind(update_id)
        .fetch_optional(pool)
        .await?)
}

pub async fn list_updates(
    pool: &PgPool,
    project_id: i32,
    limit: Option<i64>,
    before: Option<i64>,
    health: Option<&str>,
) -> anyhow::Result<ProjectUpdatePage> {
    let limit = normalize_limit(limit);
    let sql = format!(
        "{UPDATE_SELECT}
         WHERE p.project_id = $1
           AND ($2::bigint IS NULL OR p.id < $2)
           AND ($3::text IS NULL OR p.health = $3)
         ORDER BY p.id DESC
         LIMIT $4"
    );
    let mut rows = sqlx::query_as::<_, ProjectUpdateOut>(&sql)
        .bind(project_id as i64)
        .bind(before)
        .bind(health)
        .bind(limit + 1)
        .fetch_all(pool)
        .await?;

    let next = if rows.len() as i64 > limit {
        rows.truncate(limit as usize);
        rows.last().map(|r| r.id)
    } else {
        None
    };
    Ok(ProjectUpdatePage { results: rows, next })
}

/// 更新。対象が無ければ `None`。
pub async fn edit_update(
    pool: &PgPool,
    project_id: i32,
    update_id: i64,
    health: &str,
    body: &str,
) -> anyhow::Result<Option<ProjectUpdateOut>> {
    let affected = sqlx::query(
        "UPDATE project_updates SET health = $3, body = $4, updated_at = NOW()
         WHERE project_id = $1 AND id = $2",
    )
    .bind(project_id as i64)
    .bind(update_id)
    .bind(health)
    .bind(body)
    .execute(pool)
    .await?
    .rows_affected();
    if affected == 0 {
        return Ok(None);
    }
    find_update(pool, project_id, update_id).await
}

pub async fn delete_update(pool: &PgPool, project_id: i32, update_id: i64) -> anyhow::Result<bool> {
    let affected = sqlx::query("DELETE FROM project_updates WHERE project_id = $1 AND id = $2")
        .bind(project_id as i64)
        .bind(update_id)
        .execute(pool)
        .await?
        .rows_affected();
    Ok(affected > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;
    use chrono::NaiveDate;

    fn project_out(name: &str) -> ProjectOut {
        ProjectOut {
            id: 1,
            name: name.to_string(),
            prefix: "P".to_string(),
            description: "d".to_string(),
            status: "planned".to_string(),
            priority: "medium".to_string(),
            target_end_date: None,
            ticket_count: 0,
            member_count: 0,
            is_member: false,
            teams: vec![],
            owner_id: None,
            created_at: chrono::Utc::now(),
            cycle_auto_complete: true,
            cycle_auto_create_next: true,
            parent_project_id: None,
            child_count: 0,
            roadmap_ids: vec![],
        }
    }

    #[test]
    fn diff_project_returns_empty_when_nothing_changed() {
        let a = project_out("A");
        assert!(diff_project(&a, &a.clone()).is_empty());
    }

    #[test]
    fn diff_project_lists_changed_fields_only() {
        let old = project_out("A");
        let mut new = old.clone();
        new.status = "in_progress".to_string();
        new.target_end_date = NaiveDate::from_ymd_opt(2026, 10, 31);
        new.description = "long text changed".to_string();
        let changes = diff_project(&old, &new);
        let fields: Vec<&str> = changes.iter().map(|c| c["field"].as_str().unwrap()).collect();
        assert_eq!(fields, vec!["description", "status", "targetEndDate"]);
        // 説明文は本文を持たない
        assert!(changes[0].get("from").is_none() && changes[0].get("to").is_none());
        assert_eq!(changes[1]["from"], "planned");
        assert_eq!(changes[1]["to"], "in_progress");
        assert_eq!(changes[2]["to"], "2026-10-31");
    }

    #[tokio::test]
    async fn record_and_list_activity_with_cursor() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let user = test_support::create_test_user(&pool, "act").await;
        let project = test_support::create_test_project(&pool, "ACT", user).await;
        for i in 0..5 {
            record(&pool, project, Some(user), "project_updated", json!({"i": i})).await.unwrap();
        }
        let p1 = list_activity(&pool, project, Some(2), None).await.unwrap();
        assert_eq!(p1.results.len(), 2);
        assert_eq!(p1.results[0].payload["i"], 4, "新しい順");
        assert!(p1.next.is_some());
        let p2 = list_activity(&pool, project, Some(2), p1.next).await.unwrap();
        assert_eq!(p2.results[0].payload["i"], 2);
        let p3 = list_activity(&pool, project, Some(2), p2.next).await.unwrap();
        assert_eq!(p3.results.len(), 1);
        assert!(p3.next.is_none(), "最後のページは next が無い");
    }

    /// トリガー: チケットの追加・完了・プロジェクト付け替え・除外が、書き込み経路によらず記録される
    #[tokio::test]
    async fn ticket_events_are_recorded_by_trigger() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let user = test_support::create_test_user(&pool, "trg").await;
        let p1 = test_support::create_test_project(&pool, "TRA", user).await;
        let p2 = test_support::create_test_project(&pool, "TRB", user).await;
        let ticket = test_support::create_test_ticket(&pool, p1, "TRG", user).await;

        let kinds = |page: &ProjectActivityPage| -> Vec<String> {
            page.results.iter().map(|r| r.event_type.clone()).collect()
        };
        let a1 = list_activity(&pool, p1, None, None).await.unwrap();
        assert_eq!(kinds(&a1), vec!["ticket_added"]);
        assert!(a1.results[0].payload["ticket_key"].as_str().unwrap().starts_with("TRG"));

        // 完了
        sqlx::query("UPDATE tickets_ticket SET status = 'closed' WHERE id = $1")
            .bind(ticket).execute(&pool).await.unwrap();
        // 完了済みの再更新では増えない
        sqlx::query("UPDATE tickets_ticket SET status = 'closed', title = title WHERE id = $1")
            .bind(ticket).execute(&pool).await.unwrap();
        let a1 = list_activity(&pool, p1, None, None).await.unwrap();
        assert_eq!(kinds(&a1), vec!["ticket_completed", "ticket_added"]);

        // 別プロジェクトへ付け替え → 元で removed、先で added
        // (チケットはチームが参加するプロジェクトにのみ紐づけられる制約があるため、
        //  移動先のプロジェクトにも同じチームを参加させる)
        sqlx::query(
            "INSERT INTO tickets_project_teams (project_id, team_id, joined_at)
             SELECT $2, team_id, NOW() FROM tickets_project_teams WHERE project_id = $1
             ON CONFLICT DO NOTHING",
        )
        .bind(p1 as i64).bind(p2 as i64).execute(&pool).await.unwrap();
        sqlx::query("UPDATE tickets_ticket SET project_id = $2 WHERE id = $1")
            .bind(ticket).bind(p2 as i64).execute(&pool).await.unwrap();
        let a1 = list_activity(&pool, p1, None, None).await.unwrap();
        assert_eq!(kinds(&a1)[0], "ticket_removed");
        let a2 = list_activity(&pool, p2, None, None).await.unwrap();
        assert_eq!(kinds(&a2), vec!["ticket_added"]);
    }

    #[tokio::test]
    async fn project_updates_crud_and_health_filter() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let user = test_support::create_test_user(&pool, "upd").await;
        let project = test_support::create_test_project(&pool, "UPD", user).await;

        let a = create_update(&pool, project, user, "on_track", "順調").await.unwrap();
        let b = create_update(&pool, project, user, "at_risk", "遅れ気味").await.unwrap();
        assert_eq!(a.health, "on_track");
        assert!(a.author_name.is_some());

        let all = list_updates(&pool, project, None, None, None).await.unwrap();
        assert_eq!(all.results.iter().map(|u| u.id).collect::<Vec<_>>(), vec![b.id, a.id]);
        let risky = list_updates(&pool, project, None, None, Some("at_risk")).await.unwrap();
        assert_eq!(risky.results.len(), 1);

        let edited = edit_update(&pool, project, a.id, "off_track", "止まった").await.unwrap().unwrap();
        assert_eq!(edited.health, "off_track");
        assert!(edited.updated_at >= a.updated_at);
        // 他プロジェクトの id では編集できない
        assert!(edit_update(&pool, project + 999_999, a.id, "on_track", "x").await.unwrap().is_none());

        assert!(delete_update(&pool, project, a.id).await.unwrap());
        assert!(!delete_update(&pool, project, a.id).await.unwrap());
    }

    #[tokio::test]
    async fn project_updates_reject_invalid_health_at_db_level() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let user = test_support::create_test_user(&pool, "upd2").await;
        let project = test_support::create_test_project(&pool, "UPE", user).await;
        assert!(create_update(&pool, project, user, "great", "x").await.is_err());
    }
}
