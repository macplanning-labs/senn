/// infrastructure/repositories/saved_view_repo.rs — Saved View 永続化

use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use crate::domain::models::saved_view_api::SavedViewOut;

/// Saved View 一覧取得（owner_id で絞込み）
pub async fn list_by_project_and_owner(
    pool: &PgPool,
    project_id: i64,
    owner_id: i64,
) -> anyhow::Result<Vec<SavedViewOut>> {
    let rows = sqlx::query_as::<_, (i64, i64, String, JsonValue, DateTime<Utc>, DateTime<Utc>)>(
        "SELECT id, project_id, name, filters, created_at, updated_at
         FROM t_saved_view
         WHERE project_id = $1 AND owner_id = $2
         ORDER BY updated_at DESC, id DESC"
    )
    .bind(project_id)
    .bind(owner_id)
    .fetch_all(pool)
    .await?;

    let views = rows
        .into_iter()
        .map(|(id, project, name, filters, created_at, updated_at)| SavedViewOut {
            id,
            project,
            name,
            filters,
            created_at,
            updated_at,
        })
        .collect();

    Ok(views)
}

/// Saved View 作成
pub async fn create(
    pool: &PgPool,
    project_id: i64,
    owner_id: i64,
    name: &str,
    filters: &JsonValue,
) -> anyhow::Result<SavedViewOut> {
    let row = sqlx::query_as::<_, (i64, i64, String, JsonValue, DateTime<Utc>, DateTime<Utc>)>(
        "INSERT INTO t_saved_view (project_id, owner_id, name, filters, created_at, updated_at)
         VALUES ($1, $2, $3, $4, NOW(), NOW())
         RETURNING id, project_id, name, filters, created_at, updated_at"
    )
    .bind(project_id)
    .bind(owner_id)
    .bind(name)
    .bind(filters)
    .fetch_one(pool)
    .await?;

    Ok(SavedViewOut {
        id: row.0,
        project: row.1,
        name: row.2,
        filters: row.3,
        created_at: row.4,
        updated_at: row.5,
    })
}

/// Saved View 更新
pub async fn update(
    pool: &PgPool,
    id: i64,
    owner_id: i64,
    name: Option<&str>,
    filters: Option<&JsonValue>,
) -> anyhow::Result<Option<SavedViewOut>> {
    // 現在の値を取得
    let current = sqlx::query_as::<_, (String, JsonValue)>(
        "SELECT name, filters FROM t_saved_view WHERE id = $1 AND owner_id = $2"
    )
    .bind(id)
    .bind(owner_id)
    .fetch_optional(pool)
    .await?;

    if current.is_none() {
        return Ok(None);
    }

    let (current_name, current_filters) = current.unwrap();
    let new_name = name.unwrap_or(&current_name);
    let new_filters = filters.unwrap_or(&current_filters);

    let row = sqlx::query_as::<_, (i64, i64, String, JsonValue, DateTime<Utc>, DateTime<Utc>)>(
        "UPDATE t_saved_view SET name = $2, filters = $3, updated_at = NOW()
         WHERE id = $1 AND owner_id = $4
         RETURNING id, project_id, name, filters, created_at, updated_at"
    )
    .bind(id)
    .bind(new_name)
    .bind(new_filters)
    .bind(owner_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| SavedViewOut {
        id: r.0,
        project: r.1,
        name: r.2,
        filters: r.3,
        created_at: r.4,
        updated_at: r.5,
    }))
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
    let row = sqlx::query_as::<_, (i64, i64, String, JsonValue, DateTime<Utc>, DateTime<Utc>)>(
        "SELECT id, project_id, name, filters, created_at, updated_at
         FROM t_saved_view
         WHERE id = $1 AND owner_id = $2"
    )
    .bind(id)
    .bind(owner_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| SavedViewOut {
        id: r.0,
        project: r.1,
        name: r.2,
        filters: r.3,
        created_at: r.4,
        updated_at: r.5,
    }))
}
