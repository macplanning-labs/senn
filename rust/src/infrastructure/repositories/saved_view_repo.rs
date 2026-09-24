/// infrastructure/repositories/saved_view_repo.rs — Saved View 永続化

use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use crate::domain::models::saved_view_api::SavedViewOut;

type SavedViewRow = (
    i64,
    Option<i64>,
    Option<i64>,
    String,
    JsonValue,
    bool,
    i64,
    DateTime<Utc>,
    DateTime<Utc>,
    String,
);

fn row_to_out(r: SavedViewRow) -> SavedViewOut {
    SavedViewOut {
        id: r.0,
        project: r.1,
        team_id: r.2,
        name: r.3,
        filters: r.4,
        is_shared: r.5,
        owner_id: r.6,
        created_at: r.7,
        updated_at: r.8,
        view_type: r.9,
    }
}

const SAVED_VIEW_SELECT: &str =
    "SELECT id, project_id, team_id, name, filters, is_shared, owner_id, created_at, updated_at, view_type
     FROM t_saved_view";

/// Saved View 一覧取得：所有者の View ＋ 同一 project の共有 View
pub async fn list_by_project_and_owner(
    pool: &PgPool,
    project_id: i64,
    owner_id: i64,
) -> anyhow::Result<Vec<SavedViewOut>> {
    let rows = sqlx::query_as::<_, SavedViewRow>(&format!(
        "{SAVED_VIEW_SELECT}
         WHERE project_id = $1 AND (owner_id = $2 OR is_shared = true)
         ORDER BY updated_at DESC, id DESC"
    ))
    .bind(project_id)
    .bind(owner_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(row_to_out).collect())
}

/// Saved View 一覧取得：所有者の View ＋ 同一 Team の共有 View
pub async fn list_by_team_and_owner(
    pool: &PgPool,
    team_id: i64,
    owner_id: i64,
) -> anyhow::Result<Vec<SavedViewOut>> {
    let rows = sqlx::query_as::<_, SavedViewRow>(&format!(
        "{SAVED_VIEW_SELECT}
         WHERE team_id = $1 AND (owner_id = $2 OR is_shared = true)
         ORDER BY updated_at DESC, id DESC"
    ))
    .bind(team_id)
    .bind(owner_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(row_to_out).collect())
}

/// Saved View 作成（Project スコープ）
pub async fn create(
    pool: &PgPool,
    project_id: i64,
    owner_id: i64,
    name: &str,
    filters: &JsonValue,
    view_type: &str,
    is_shared: bool,
) -> anyhow::Result<SavedViewOut> {
    let row = sqlx::query_as::<_, SavedViewRow>(
        "INSERT INTO t_saved_view (project_id, team_id, owner_id, name, filters, view_type, is_shared, created_at, updated_at)
         VALUES ($1, NULL, $2, $3, $4, $5, $6, NOW(), NOW())
         RETURNING id, project_id, team_id, name, filters, is_shared, owner_id, created_at, updated_at, view_type"
    )
    .bind(project_id)
    .bind(owner_id)
    .bind(name)
    .bind(filters)
    .bind(view_type)
    .bind(is_shared)
    .fetch_one(pool)
    .await?;

    Ok(row_to_out(row))
}

/// Saved View 作成（Team スコープ）
pub async fn create_for_team(
    pool: &PgPool,
    team_id: i64,
    owner_id: i64,
    name: &str,
    filters: &JsonValue,
    view_type: &str,
    is_shared: bool,
) -> anyhow::Result<SavedViewOut> {
    let row = sqlx::query_as::<_, SavedViewRow>(
        "INSERT INTO t_saved_view (project_id, team_id, owner_id, name, filters, view_type, is_shared, created_at, updated_at)
         VALUES (NULL, $1, $2, $3, $4, $5, $6, NOW(), NOW())
         RETURNING id, project_id, team_id, name, filters, is_shared, owner_id, created_at, updated_at, view_type"
    )
    .bind(team_id)
    .bind(owner_id)
    .bind(name)
    .bind(filters)
    .bind(view_type)
    .bind(is_shared)
    .fetch_one(pool)
    .await?;

    Ok(row_to_out(row))
}

/// Saved View 更新（所有者のみ）
pub async fn update(
    pool: &PgPool,
    id: i64,
    owner_id: i64,
    name: Option<&str>,
    filters: Option<&JsonValue>,
    is_shared: Option<bool>,
) -> anyhow::Result<Option<SavedViewOut>> {
    let current = sqlx::query_as::<_, (String, JsonValue, bool)>(
        "SELECT name, filters, is_shared FROM t_saved_view WHERE id = $1 AND owner_id = $2"
    )
    .bind(id)
    .bind(owner_id)
    .fetch_optional(pool)
    .await?;

    let Some((current_name, current_filters, current_is_shared)) = current else {
        return Ok(None);
    };

    let new_name = name.unwrap_or(&current_name);
    let new_filters = filters.unwrap_or(&current_filters);
    let new_is_shared = is_shared.unwrap_or(current_is_shared);

    let row = sqlx::query_as::<_, SavedViewRow>(
        "UPDATE t_saved_view SET name = $2, filters = $3, is_shared = $4, updated_at = NOW()
         WHERE id = $1 AND owner_id = $5
         RETURNING id, project_id, team_id, name, filters, is_shared, owner_id, created_at, updated_at, view_type"
    )
    .bind(id)
    .bind(new_name)
    .bind(new_filters)
    .bind(new_is_shared)
    .bind(owner_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(row_to_out))
}

/// Saved View 削除
pub async fn delete(
    pool: &PgPool,
    id: i64,
    owner_id: i64,
) -> anyhow::Result<bool> {
    let result = sqlx::query(
        "DELETE FROM t_saved_view WHERE id = $1 AND owner_id = $2"
    )
    .bind(id)
    .bind(owner_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Saved View を ID で取得（owner_id チェック）
pub async fn get_by_id_and_owner(
    pool: &PgPool,
    id: i64,
    owner_id: i64,
) -> anyhow::Result<Option<SavedViewOut>> {
    let row = sqlx::query_as::<_, SavedViewRow>(&format!(
        "{SAVED_VIEW_SELECT}
         WHERE id = $1 AND owner_id = $2"
    ))
    .bind(id)
    .bind(owner_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(row_to_out))
}

/// Saved View 一覧取得：グローバル(project/teamスコープ無し)、所有者の View ＋ 共有 View
pub async fn list_global_and_owner(
    pool: &PgPool,
    owner_id: i64,
    view_type: &str,
) -> anyhow::Result<Vec<SavedViewOut>> {
    let rows = sqlx::query_as::<_, SavedViewRow>(&format!(
        "{SAVED_VIEW_SELECT}
         WHERE project_id IS NULL AND team_id IS NULL AND view_type = $1
           AND (owner_id = $2 OR is_shared = true)
         ORDER BY updated_at DESC, id DESC"
    ))
    .bind(view_type)
    .bind(owner_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(row_to_out).collect())
}

/// Saved View 作成（グローバル、project/teamスコープ無し）
pub async fn create_global(
    pool: &PgPool,
    owner_id: i64,
    name: &str,
    filters: &JsonValue,
    view_type: &str,
    is_shared: bool,
) -> anyhow::Result<SavedViewOut> {
    let row = sqlx::query_as::<_, SavedViewRow>(
        "INSERT INTO t_saved_view (project_id, team_id, owner_id, name, filters, view_type, is_shared, created_at, updated_at)
         VALUES (NULL, NULL, $1, $2, $3, $4, $5, NOW(), NOW())
         RETURNING id, project_id, team_id, name, filters, is_shared, owner_id, created_at, updated_at, view_type"
    )
    .bind(owner_id)
    .bind(name)
    .bind(filters)
    .bind(view_type)
    .bind(is_shared)
    .fetch_one(pool)
    .await?;

    Ok(row_to_out(row))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;

    #[tokio::test]
    async fn create_and_list_global_saved_view() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let owner = test_support::create_test_user(&pool, "saved-view-owner").await as i64;
        let filters = serde_json::json!({ "status": "in_progress" });

        let created = create_global(&pool, owner, "My Global View", &filters, "tickets", false)
            .await
            .unwrap();
        assert_eq!(created.view_type, "tickets");
        assert!(created.project.is_none());
        assert!(created.team_id.is_none());

        let list = list_global_and_owner(&pool, owner, "tickets").await.unwrap();
        assert!(list.iter().any(|v| v.id == created.id));

        let empty_wrong_type = list_global_and_owner(&pool, owner, "projects").await.unwrap();
        assert!(!empty_wrong_type.iter().any(|v| v.id == created.id));
    }
}
