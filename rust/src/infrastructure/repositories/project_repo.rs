/// infrastructure/repositories/project_repo.rs — プロジェクト永続化

use sqlx::PgPool;
use crate::domain::models::project::Project;

pub async fn find_all(pool: &PgPool) -> anyhow::Result<Vec<Project>> {
    let rows = sqlx::query_as::<_, Project>(
        "SELECT id::int4, name, prefix, description, created_at, owner_id::int4 FROM tickets_project ORDER BY name"
    ).fetch_all(pool).await?;
    Ok(rows)
}

pub async fn find_by_id(pool: &PgPool, id: i32) -> anyhow::Result<Option<Project>> {
    let row = sqlx::query_as::<_, Project>(
        "SELECT id::int4, name, prefix, description, created_at, owner_id::int4 FROM tickets_project WHERE id=$1"
    ).bind(id).fetch_optional(pool).await?;
    Ok(row)
}

pub async fn create(pool: &PgPool, name: &str, prefix: &str, description: &str) -> anyhow::Result<i32> {
    let id = sqlx::query_scalar::<_, i32>(
        "INSERT INTO tickets_project (name, prefix, description) VALUES ($1, $2, $3) RETURNING id"
    ).bind(name).bind(prefix).bind(description).fetch_one(pool).await?;
    Ok(id)
}

pub async fn update(pool: &PgPool, id: i32, name: &str, prefix: &str, description: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE tickets_project SET name=$2, prefix=$3, description=$4 WHERE id=$1")
        .bind(id).bind(name).bind(prefix).bind(description)
        .execute(pool).await?;
    Ok(())
}

pub async fn delete(pool: &PgPool, id: i32) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM tickets_project WHERE id=$1").bind(id).execute(pool).await?;
    Ok(())
}

pub async fn find_by_prefix(pool: &PgPool, prefix: &str) -> anyhow::Result<Option<Project>> {
    let row = sqlx::query_as::<_, Project>(
        "SELECT id::int4, name, prefix, description, created_at, owner_id::int4 FROM tickets_project WHERE prefix=$1"
    ).bind(prefix).fetch_optional(pool).await?;
    Ok(row)
}
