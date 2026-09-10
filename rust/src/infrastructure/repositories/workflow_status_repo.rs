/// infrastructure/repositories/workflow_status_repo.rs — Workflow Status 永続化
///
/// t_workflow_status テーブルの CRUD 操作。

use sqlx::{PgPool, Row};

use crate::domain::models::workflow_status_api::*;

const WF_SELECT: &str = "SELECT id::int4 AS id, project_id, team_id, name, slug, category, color, position, is_default
             FROM t_workflow_status";

fn map_wf_row(row: sqlx::postgres::PgRow) -> WorkflowStatusOut {
    let project_id: Option<i64> = row.get("project_id");
    let team_id: Option<i64> = row.get("team_id");
    WorkflowStatusOut {
        id: row.get("id"),
        project: project_id.map(|v| v as i32),
        team_id: team_id.map(|v| v as i32),
        name: row.get("name"),
        slug: row.get("slug"),
        category: row.get("category"),
        color: row.get("color"),
        position: row.get("position"),
        is_default: row.get("is_default"),
    }
}

pub async fn find_all_workflow_statuses(
    pool: &PgPool,
    page: i64,
    project: Option<i32>,
    team: Option<i32>,
) -> anyhow::Result<Vec<WorkflowStatusOut>> {
    const PAGE_SIZE: i64 = 50;
    let page = page.max(1);
    let offset = (page - 1) * PAGE_SIZE;

    let sql = format!(
        "{WF_SELECT}
         WHERE ($1::bigint IS NULL OR project_id = $1)
           AND ($2::bigint IS NULL OR team_id = $2)
         ORDER BY position ASC
         LIMIT $3 OFFSET $4"
    );
    let rows = sqlx::query(&sql)
        .bind(project.map(|p| p as i64))
        .bind(team.map(|t| t as i64))
        .bind(PAGE_SIZE)
        .bind(offset)
        .fetch_all(pool)
        .await?;

    Ok(rows.into_iter().map(map_wf_row).collect())
}

pub async fn count_workflow_statuses(
    pool: &PgPool,
    project: Option<i32>,
    team: Option<i32>,
) -> anyhow::Result<i64> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM t_workflow_status
         WHERE ($1::bigint IS NULL OR project_id = $1)
           AND ($2::bigint IS NULL OR team_id = $2)"
    )
    .bind(project.map(|p| p as i64))
    .bind(team.map(|t| t as i64))
    .fetch_one(pool)
    .await?;

    Ok(count)
}

pub async fn find_workflow_status_by_id(
    pool: &PgPool,
    id: i32,
) -> anyhow::Result<Option<WorkflowStatusOut>> {
    let sql = format!("{WF_SELECT} WHERE id = $1");
    let row_opt = sqlx::query(&sql)
        .bind(id as i64)
        .fetch_optional(pool)
        .await?;

    Ok(row_opt.map(map_wf_row))
}

pub async fn create_workflow_status(
    pool: &PgPool,
    input: &WorkflowStatusWriteIn,
) -> anyhow::Result<i32> {
    let status_id: i32 = sqlx::query_scalar(
        "INSERT INTO t_workflow_status (project_id, team_id, name, slug, category, color, position, is_default)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING id::int4"
    )
    .bind(input.project.map(|p| p as i64))
    .bind(input.team_id.map(|t| t as i64))
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
         SET project_id = $1, team_id = $2, name = $3, slug = $4, category = $5, color = $6, position = $7, is_default = $8
         WHERE id = $9"
    )
    .bind(input.project.map(|p| p as i64))
    .bind(input.team_id.map(|t| t as i64))
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
    let project = input.project.or(existing.project);
    let team_id = input.team_id.or(existing.team_id);
    let name = input.name.as_ref().unwrap_or(&existing.name).clone();
    let slug = input.slug.as_ref().unwrap_or(&existing.slug).clone();
    let category = input.category.as_ref().unwrap_or(&existing.category).clone();
    let color = input.color.as_ref().unwrap_or(&existing.color).clone();
    let position = input.position.unwrap_or(existing.position);
    let is_default = input.is_default.unwrap_or(existing.is_default);

    let rows_affected = sqlx::query(
        "UPDATE t_workflow_status
         SET project_id = $1, team_id = $2, name = $3, slug = $4, category = $5, color = $6, position = $7, is_default = $8
         WHERE id = $9"
    )
    .bind(project.map(|p| p as i64))
    .bind(team_id.map(|t| t as i64))
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

pub async fn resolve_status_slug_by_category(
    pool: &PgPool,
    project_id: i32,
    category: &str,
) -> anyhow::Result<Option<String>> {
    let slug_opt: Option<String> = sqlx::query_scalar(
        "SELECT slug FROM t_workflow_status
         WHERE project_id = $1 AND category = $2
         ORDER BY position ASC, id ASC
         LIMIT 1"
    )
    .bind(project_id as i64)
    .bind(category)
    .fetch_optional(pool)
    .await?;

    Ok(slug_opt)
}

/// Team マスタ → ワークスペース既定 の順で slug を探す。Project 付きは Project マスタのみ。
pub async fn resolve_status_slug_by_scope(
    pool: &PgPool,
    project_id: Option<i32>,
    team_id: Option<i32>,
    category: &str,
) -> anyhow::Result<Option<String>> {
    if let Some(pid) = project_id {
        return resolve_status_slug_by_category(pool, pid, category).await;
    }
    if let Some(tid) = team_id {
        let slug: Option<String> = sqlx::query_scalar(
            "SELECT slug FROM t_workflow_status
             WHERE team_id = $1 AND project_id IS NULL AND category = $2
             ORDER BY position ASC, id ASC
             LIMIT 1"
        )
        .bind(tid as i64)
        .bind(category)
        .fetch_optional(pool)
        .await?;
        if slug.is_some() {
            return Ok(slug);
        }
    }
    let slug: Option<String> = sqlx::query_scalar(
        "SELECT slug FROM t_workflow_status
         WHERE project_id IS NULL AND team_id IS NULL AND category = $1
         ORDER BY position ASC, id ASC
         LIMIT 1"
    )
    .bind(category)
    .fetch_optional(pool)
    .await?;
    Ok(slug)
}

pub async fn lookup_status_category(
    pool: &PgPool,
    project_id: Option<i32>,
    team_id: Option<i32>,
    slug: &str,
) -> anyhow::Result<Option<String>> {
    if let Some(pid) = project_id {
        return sqlx::query_scalar(
            "SELECT category FROM t_workflow_status WHERE project_id = $1 AND slug = $2"
        )
        .bind(pid as i64)
        .bind(slug)
        .fetch_optional(pool)
        .await
        .map_err(Into::into);
    }
    if let Some(tid) = team_id {
        let cat: Option<String> = sqlx::query_scalar(
            "SELECT category FROM t_workflow_status
             WHERE team_id = $1 AND project_id IS NULL AND slug = $2"
        )
        .bind(tid as i64)
        .bind(slug)
        .fetch_optional(pool)
        .await?;
        if cat.is_some() {
            return Ok(cat);
        }
    }
    let cat: Option<String> = sqlx::query_scalar(
        "SELECT category FROM t_workflow_status
         WHERE project_id IS NULL AND team_id IS NULL AND slug = $1"
    )
    .bind(slug)
    .fetch_optional(pool)
    .await?;
    Ok(cat)
}

pub async fn status_slug_exists_for_scope(
    pool: &PgPool,
    project_id: Option<i32>,
    team_id: Option<i32>,
    slug: &str,
) -> anyhow::Result<bool> {
    Ok(lookup_status_category(pool, project_id, team_id, slug)
        .await?
        .is_some())
}

pub async fn resolve_review_status_slug(
    pool: &PgPool,
    project_id: i32,
) -> anyhow::Result<Option<String>> {
    resolve_review_status_slug_by_scope(pool, Some(project_id), None).await
}

/// Project → Team → ワークスペース既定の順で review 相当 slug を探す。
pub async fn resolve_review_status_slug_by_scope(
    pool: &PgPool,
    project_id: Option<i32>,
    team_id: Option<i32>,
) -> anyhow::Result<Option<String>> {
    if let Some(pid) = project_id {
        if let Some(slug) = pick_review_slug_from_rows(
            &sqlx::query(
                "SELECT slug, category, position FROM t_workflow_status
                 WHERE project_id = $1
                 ORDER BY position ASC, id ASC"
            )
            .bind(pid as i64)
            .fetch_all(pool)
            .await?,
        ) {
            return Ok(Some(slug));
        }
        return Ok(None);
    }

    if let Some(tid) = team_id {
        if let Some(slug) = pick_review_slug_from_rows(
            &sqlx::query(
                "SELECT slug, category, position FROM t_workflow_status
                 WHERE team_id = $1 AND project_id IS NULL
                 ORDER BY position ASC, id ASC"
            )
            .bind(tid as i64)
            .fetch_all(pool)
            .await?,
        ) {
            return Ok(Some(slug));
        }
    }

    Ok(pick_review_slug_from_rows(
        &sqlx::query(
            "SELECT slug, category, position FROM t_workflow_status
             WHERE project_id IS NULL AND team_id IS NULL
             ORDER BY position ASC, id ASC"
        )
        .fetch_all(pool)
        .await?,
    ))
}

fn pick_review_slug_from_rows(rows: &[sqlx::postgres::PgRow]) -> Option<String> {
    let target_slugs = ["in_review", "review", "in-review"];

    for row in rows {
        let category: String = row.get(1);
        let slug: String = row.get(0);
        if category == "started" && target_slugs.contains(&slug.as_str()) {
            return Some(slug);
        }
    }
    for row in rows {
        let category: String = row.get(1);
        let slug: String = row.get(0);
        if category == "review" {
            return Some(slug);
        }
    }
    for row in rows {
        let slug: String = row.get(0);
        if target_slugs.contains(&slug.as_str()) {
            return Some(slug);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;

    #[tokio::test]
    async fn test_resolve_status_slug_by_category_found() {
        let Some(pool) = test_support::test_pool().await else {
            eprintln!("Skipping test: test_pool unavailable (no DB)");
            return;
        };

        // テスト用プロジェクトを作成（t_projects テーブル用）
        let project_id: i32 = sqlx::query_scalar(
            "INSERT INTO tickets_project (name, prefix, description, status, created_at, grace_period_days)
             VALUES ('test_project_wf1', 'WF1', '', 'active', NOW(), 0)
             RETURNING id::int4"
        )
        .fetch_one(&pool)
        .await
        .expect("Failed to create test project");

        // テスト用ステータスを作成
        sqlx::query(
            "INSERT INTO t_workflow_status (project_id, name, slug, category, color, position, is_default)
             VALUES ($1, 'In Progress', 'in_progress', 'started', '#0000ff', 1, false)"
        )
        .bind(project_id as i64)
        .execute(&pool)
        .await
        .expect("Failed to insert workflow status");

        let result = resolve_status_slug_by_category(&pool, project_id, "started")
            .await
            .expect("Failed to query");

        assert_eq!(result, Some("in_progress".to_string()));
    }

    #[tokio::test]
    async fn test_resolve_status_slug_by_category_not_found() {
        let Some(pool) = test_support::test_pool().await else {
            eprintln!("Skipping test: test_pool unavailable (no DB)");
            return;
        };

        // テスト用プロジェクトを作成
        let project_id: i32 = sqlx::query_scalar(
            "INSERT INTO tickets_project (name, prefix, description, status, created_at, grace_period_days)
             VALUES ('test_project_wf2', 'WF2', '', 'active', NOW(), 0)
             RETURNING id::int4"
        )
        .fetch_one(&pool)
        .await
        .expect("Failed to create test project");

        let result = resolve_status_slug_by_category(&pool, project_id, "nonexistent")
            .await
            .expect("Failed to query");

        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_resolve_status_slug_by_category_multiple_same_category() {
        let Some(pool) = test_support::test_pool().await else {
            eprintln!("Skipping test: test_pool unavailable (no DB)");
            return;
        };

        // テスト用プロジェクトを作成
        let project_id: i32 = sqlx::query_scalar(
            "INSERT INTO tickets_project (name, prefix, description, status, created_at, grace_period_days)
             VALUES ('test_project_wf3', 'WF3', '', 'active', NOW(), 0)
             RETURNING id::int4"
        )
        .fetch_one(&pool)
        .await
        .expect("Failed to create test project");

        // 同じカテゴリで複数のステータスを作成（position順）
        sqlx::query(
            "INSERT INTO t_workflow_status (project_id, name, slug, category, color, position, is_default)
             VALUES ($1, 'In Progress', 'in_progress', 'started', '#0000ff', 2, false)"
        )
        .bind(project_id as i64)
        .execute(&pool)
        .await
        .expect("Failed to insert first status");

        sqlx::query(
            "INSERT INTO t_workflow_status (project_id, name, slug, category, color, position, is_default)
             VALUES ($1, 'Started', 'started', 'started', '#ffff00', 1, false)"
        )
        .bind(project_id as i64)
        .execute(&pool)
        .await
        .expect("Failed to insert second status");

        let result = resolve_status_slug_by_category(&pool, project_id, "started")
            .await
            .expect("Failed to query");

        // position 1 の方が選ばれるべき
        assert_eq!(result, Some("started".to_string()));
    }

    #[tokio::test]
    async fn test_resolve_status_slug_by_category_different_projects() {
        let Some(pool) = test_support::test_pool().await else {
            eprintln!("Skipping test: test_pool unavailable (no DB)");
            return;
        };

        // 2つのテスト用プロジェクトを作成
        let project_id_1: i32 = sqlx::query_scalar(
            "INSERT INTO tickets_project (name, prefix, description, status, created_at, grace_period_days)
             VALUES ('test_project_wf4a', 'WF4A', '', 'active', NOW(), 0)
             RETURNING id::int4"
        )
        .fetch_one(&pool)
        .await
        .expect("Failed to create test project 1");

        let project_id_2: i32 = sqlx::query_scalar(
            "INSERT INTO tickets_project (name, prefix, description, status, created_at, grace_period_days)
             VALUES ('test_project_wf4b', 'WF4B', '', 'active', NOW(), 0)
             RETURNING id::int4"
        )
        .fetch_one(&pool)
        .await
        .expect("Failed to create test project 2");

        // プロジェクト1にステータスを作成
        sqlx::query(
            "INSERT INTO t_workflow_status (project_id, name, slug, category, color, position, is_default)
             VALUES ($1, 'In Progress', 'in_progress', 'started', '#0000ff', 1, false)"
        )
        .bind(project_id_1 as i64)
        .execute(&pool)
        .await
        .expect("Failed to insert status for project 1");

        // プロジェクト2にはステータスを作成しない

        let result_1 = resolve_status_slug_by_category(&pool, project_id_1, "started")
            .await
            .expect("Failed to query project 1");
        let result_2 = resolve_status_slug_by_category(&pool, project_id_2, "started")
            .await
            .expect("Failed to query project 2");

        assert_eq!(result_1, Some("in_progress".to_string()));
        assert_eq!(result_2, None);
    }

    #[tokio::test]
    async fn test_resolve_review_status_slug_priority1_started_in_review() {
        let Some(pool) = test_support::test_pool().await else {
            eprintln!("Skipping test: test_pool unavailable (no DB)");
            return;
        };

        let project_id: i32 = sqlx::query_scalar(
            "INSERT INTO tickets_project (name, prefix, description, status, created_at, grace_period_days)
             VALUES ('test_pr_review_1', 'PR1', '', 'active', NOW(), 0)
             RETURNING id::int4"
        )
        .fetch_one(&pool)
        .await
        .expect("Failed to create test project");

        // Priority 1: category = 'started' かつ slug = 'in_review'
        sqlx::query(
            "INSERT INTO t_workflow_status (project_id, name, slug, category, color, position, is_default)
             VALUES ($1, 'In Review', 'in_review', 'started', '#00ff00', 1, false)"
        )
        .bind(project_id as i64)
        .execute(&pool)
        .await
        .expect("Failed to insert status");

        let result = resolve_review_status_slug(&pool, project_id)
            .await
            .expect("Failed to query");

        assert_eq!(result, Some("in_review".to_string()));
    }

    #[tokio::test]
    async fn test_resolve_review_status_slug_priority2_category_review() {
        let Some(pool) = test_support::test_pool().await else {
            eprintln!("Skipping test: test_pool unavailable (no DB)");
            return;
        };

        let project_id: i32 = sqlx::query_scalar(
            "INSERT INTO tickets_project (name, prefix, description, status, created_at, grace_period_days)
             VALUES ('test_pr_review_2', 'PR2', '', 'active', NOW(), 0)
             RETURNING id::int4"
        )
        .fetch_one(&pool)
        .await
        .expect("Failed to create test project");

        // category = 'review'（slug は何でも良い）
        sqlx::query(
            "INSERT INTO t_workflow_status (project_id, name, slug, category, color, position, is_default)
             VALUES ($1, 'Needs Review', 'needs_review', 'review', '#ff00ff', 1, false)"
        )
        .bind(project_id as i64)
        .execute(&pool)
        .await
        .expect("Failed to insert status");

        let result = resolve_review_status_slug(&pool, project_id)
            .await
            .expect("Failed to query");

        assert_eq!(result, Some("needs_review".to_string()));
    }

    #[tokio::test]
    async fn test_resolve_review_status_slug_priority3_slug_only() {
        let Some(pool) = test_support::test_pool().await else {
            eprintln!("Skipping test: test_pool unavailable (no DB)");
            return;
        };

        let project_id: i32 = sqlx::query_scalar(
            "INSERT INTO tickets_project (name, prefix, description, status, created_at, grace_period_days)
             VALUES ('test_pr_review_3', 'PR3', '', 'active', NOW(), 0)
             RETURNING id::int4"
        )
        .fetch_one(&pool)
        .await
        .expect("Failed to create test project");

        // slug = 'review'（category は 'started' でも 'review' でもない）
        sqlx::query(
            "INSERT INTO t_workflow_status (project_id, name, slug, category, color, position, is_default)
             VALUES ($1, 'Review', 'review', 'other', '#ffff00', 1, false)"
        )
        .bind(project_id as i64)
        .execute(&pool)
        .await
        .expect("Failed to insert status");

        let result = resolve_review_status_slug(&pool, project_id)
            .await
            .expect("Failed to query");

        assert_eq!(result, Some("review".to_string()));
    }

    #[tokio::test]
    async fn test_resolve_review_status_slug_no_match() {
        let Some(pool) = test_support::test_pool().await else {
            eprintln!("Skipping test: test_pool unavailable (no DB)");
            return;
        };

        let project_id: i32 = sqlx::query_scalar(
            "INSERT INTO tickets_project (name, prefix, description, status, created_at, grace_period_days)
             VALUES ('test_pr_review_4', 'PR4', '', 'active', NOW(), 0)
             RETURNING id::int4"
        )
        .fetch_one(&pool)
        .await
        .expect("Failed to create test project");

        // 何もマッチしないステータス
        sqlx::query(
            "INSERT INTO t_workflow_status (project_id, name, slug, category, color, position, is_default)
             VALUES ($1, 'In Progress', 'in_progress', 'started', '#0000ff', 1, false)"
        )
        .bind(project_id as i64)
        .execute(&pool)
        .await
        .expect("Failed to insert status");

        let result = resolve_review_status_slug(&pool, project_id)
            .await
            .expect("Failed to query");

        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_resolve_review_status_slug_priority_order() {
        let Some(pool) = test_support::test_pool().await else {
            eprintln!("Skipping test: test_pool unavailable (no DB)");
            return;
        };

        let project_id: i32 = sqlx::query_scalar(
            "INSERT INTO tickets_project (name, prefix, description, status, created_at, grace_period_days)
             VALUES ('test_pr_review_5', 'PR5', '', 'active', NOW(), 0)
             RETURNING id::int4"
        )
        .fetch_one(&pool)
        .await
        .expect("Failed to create test project");

        // Priority 3: slug = 'review'（category = 'other'）
        sqlx::query(
            "INSERT INTO t_workflow_status (project_id, name, slug, category, color, position, is_default)
             VALUES ($1, 'Review', 'review', 'other', '#ffff00', 1, false)"
        )
        .bind(project_id as i64)
        .execute(&pool)
        .await
        .expect("Failed to insert status");

        // Priority 1: category = 'started' かつ slug = 'in_review'
        sqlx::query(
            "INSERT INTO t_workflow_status (project_id, name, slug, category, color, position, is_default)
             VALUES ($1, 'In Review', 'in_review', 'started', '#00ff00', 2, false)"
        )
        .bind(project_id as i64)
        .execute(&pool)
        .await
        .expect("Failed to insert status");

        let result = resolve_review_status_slug(&pool, project_id)
            .await
            .expect("Failed to query");

        // Priority 1 が優先される
        assert_eq!(result, Some("in_review".to_string()));
    }

    #[tokio::test]
    async fn test_resolve_review_status_slug_in_dash_review_variant() {
        let Some(pool) = test_support::test_pool().await else {
            eprintln!("Skipping test: test_pool unavailable (no DB)");
            return;
        };

        let project_id: i32 = sqlx::query_scalar(
            "INSERT INTO tickets_project (name, prefix, description, status, created_at, grace_period_days)
             VALUES ('test_pr_review_6', 'PR6', '', 'active', NOW(), 0)
             RETURNING id::int4"
        )
        .fetch_one(&pool)
        .await
        .expect("Failed to create test project");

        // Priority 1: category = 'started' かつ slug = 'in-review'（ダッシュ表記）
        sqlx::query(
            "INSERT INTO t_workflow_status (project_id, name, slug, category, color, position, is_default)
             VALUES ($1, 'In Review', 'in-review', 'started', '#00ff00', 1, false)"
        )
        .bind(project_id as i64)
        .execute(&pool)
        .await
        .expect("Failed to insert status");

        let result = resolve_review_status_slug(&pool, project_id)
            .await
            .expect("Failed to query");

        assert_eq!(result, Some("in-review".to_string()));
    }
}
