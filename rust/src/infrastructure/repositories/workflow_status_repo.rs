/// infrastructure/repositories/workflow_status_repo.rs — Workflow Status 永続化
///
/// t_workflow_status テーブルの CRUD 操作。

use sqlx::{PgPool, Row};

use crate::domain::models::workflow_status_api::*;

pub async fn find_all_workflow_statuses(
    pool: &PgPool,
    page: i64,
    project: Option<i32>,
) -> anyhow::Result<Vec<WorkflowStatusOut>> {
    const PAGE_SIZE: i64 = 50;
    let page = page.max(1);
    let offset = (page - 1) * PAGE_SIZE;

    let rows = if let Some(project_id) = project {
        sqlx::query(
            "SELECT id::int4, project_id::int4, name, slug, category, color, position, is_default
             FROM t_workflow_status
             WHERE project_id = $1
             ORDER BY position ASC
             LIMIT $2 OFFSET $3"
        )
        .bind(project_id as i64)
        .bind(PAGE_SIZE)
        .bind(offset)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            "SELECT id::int4, project_id::int4, name, slug, category, color, position, is_default
             FROM t_workflow_status
             ORDER BY position ASC
             LIMIT $1 OFFSET $2"
        )
        .bind(PAGE_SIZE)
        .bind(offset)
        .fetch_all(pool)
        .await?
    };

    let statuses = rows
        .into_iter()
        .map(|row| WorkflowStatusOut {
            id: row.get(0),
            project: row.get(1),
            name: row.get(2),
            slug: row.get(3),
            category: row.get(4),
            color: row.get(5),
            position: row.get(6),
            is_default: row.get(7),
        })
        .collect();

    Ok(statuses)
}

pub async fn count_workflow_statuses(
    pool: &PgPool,
    project: Option<i32>,
) -> anyhow::Result<i64> {
    let count: i64 = if let Some(project_id) = project {
        sqlx::query_scalar("SELECT COUNT(*) FROM t_workflow_status WHERE project_id = $1")
            .bind(project_id as i64)
            .fetch_one(pool)
            .await?
    } else {
        sqlx::query_scalar("SELECT COUNT(*) FROM t_workflow_status")
            .fetch_one(pool)
            .await?
    };

    Ok(count)
}

pub async fn find_workflow_status_by_id(
    pool: &PgPool,
    id: i32,
) -> anyhow::Result<Option<WorkflowStatusOut>> {
    let row_opt = sqlx::query(
        "SELECT id::int4, project_id::int4, name, slug, category, color, position, is_default
         FROM t_workflow_status
         WHERE id = $1"
    )
    .bind(id as i64)
    .fetch_optional(pool)
    .await?;

    let status = row_opt.map(|row| WorkflowStatusOut {
        id: row.get(0),
        project: row.get(1),
        name: row.get(2),
        slug: row.get(3),
        category: row.get(4),
        color: row.get(5),
        position: row.get(6),
        is_default: row.get(7),
    });

    Ok(status)
}

pub async fn create_workflow_status(
    pool: &PgPool,
    input: &WorkflowStatusWriteIn,
) -> anyhow::Result<i32> {
    let status_id: i32 = sqlx::query_scalar(
        "INSERT INTO t_workflow_status (project_id, name, slug, category, color, position, is_default)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id::int4"
    )
    .bind(input.project as i64)
    .bind(&input.name)
    .bind(&input.slug)
    .bind(&input.category)
    .bind(&input.color)
    .bind(input.position)
    .bind(input.is_default)
    .fetch_one(pool)
    .await?;

    Ok(status_id)
}

pub async fn update_workflow_status(
    pool: &PgPool,
    id: i32,
    input: &WorkflowStatusWriteIn,
) -> anyhow::Result<bool> {
    let rows_affected = sqlx::query(
        "UPDATE t_workflow_status
         SET project_id = $1, name = $2, slug = $3, category = $4, color = $5, position = $6, is_default = $7
         WHERE id = $8"
    )
    .bind(input.project as i64)
    .bind(&input.name)
    .bind(&input.slug)
    .bind(&input.category)
    .bind(&input.color)
    .bind(input.position)
    .bind(input.is_default)
    .bind(id as i64)
    .execute(pool)
    .await?
    .rows_affected();

    Ok(rows_affected > 0)
}

pub async fn partial_update_workflow_status(
    pool: &PgPool,
    id: i32,
    input: &crate::domain::models::workflow_status_api::WorkflowStatusUpdateIn,
) -> anyhow::Result<bool> {
    // 既存のステータスを取得
    let existing = find_workflow_status_by_id(pool, id).await?;
    if existing.is_none() {
        return Ok(false);
    }

    let existing = existing.unwrap();

    // 更新値を決定（指定されない場合は既存値を使用）
    let project = input.project.unwrap_or(existing.project);
    let name = input.name.as_ref().unwrap_or(&existing.name).clone();
    let slug = input.slug.as_ref().unwrap_or(&existing.slug).clone();
    let category = input.category.as_ref().unwrap_or(&existing.category).clone();
    let color = input.color.as_ref().unwrap_or(&existing.color).clone();
    let position = input.position.unwrap_or(existing.position);
    let is_default = input.is_default.unwrap_or(existing.is_default);

    let rows_affected = sqlx::query(
        "UPDATE t_workflow_status
         SET project_id = $1, name = $2, slug = $3, category = $4, color = $5, position = $6, is_default = $7
         WHERE id = $8"
    )
    .bind(project as i64)
    .bind(&name)
    .bind(&slug)
    .bind(&category)
    .bind(&color)
    .bind(position)
    .bind(is_default)
    .bind(id as i64)
    .execute(pool)
    .await?
    .rows_affected();

    Ok(rows_affected > 0)
}

pub async fn delete_workflow_status(pool: &PgPool, id: i32) -> anyhow::Result<bool> {
    let rows_affected = sqlx::query("DELETE FROM t_workflow_status WHERE id = $1")
        .bind(id as i64)
        .execute(pool)
        .await?
        .rows_affected();

    Ok(rows_affected > 0)
}

pub async fn reorder_workflow_statuses(
    pool: &PgPool,
    order: Vec<i32>,
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;

    for (idx, status_id) in order.iter().enumerate() {
        let result = sqlx::query(
            "UPDATE t_workflow_status
             SET position = $1
             WHERE id = $2"
        )
        .bind(idx as i32)
        .bind(*status_id as i64)
        .execute(&mut *tx)
        .await;

        if let Err(e) = result {
            tracing::error!("Failed to update workflow status position: {:?}", e);
            // 存在しないIDは単に無視
        }
    }

    if let Err(e) = tx.commit().await {
        tracing::error!("Failed to commit workflow status reorder transaction: {:?}", e);
        return Err(e.into());
    }

    Ok(())
}
