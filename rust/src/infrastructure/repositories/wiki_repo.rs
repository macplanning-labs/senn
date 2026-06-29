/// infrastructure/repositories/wiki_repo.rs — Wiki 永続化

use sqlx::PgPool;
use crate::domain::models::wiki::{WikiPage, WikiRevision};

pub async fn find_by_project(pool: &PgPool, project_id: Option<i32>) -> anyhow::Result<Vec<WikiPage>> {
    let rows = if let Some(pid) = project_id {
        sqlx::query_as::<_, WikiPage>(
            "SELECT w.id, w.project_id, w.title, w.slug, w.category, w.content,
                    w.author_id, w.last_editor_id, w.created_at, w.updated_at,
                    au.display_name as author_name,
                    ed.display_name as last_editor_name,
                    p.name as project_name
             FROM t_wiki_pages w
             LEFT JOIN m_users au ON w.author_id = au.id
             LEFT JOIN m_users ed ON w.last_editor_id = ed.id
             LEFT JOIN m_projects p ON w.project_id = p.id
             WHERE w.project_id = $1
             ORDER BY w.category, w.title"
        ).bind(pid).fetch_all(pool).await?
    } else {
        // 共有Wiki（project_id IS NULL）
        sqlx::query_as::<_, WikiPage>(
            "SELECT w.id, w.project_id, w.title, w.slug, w.category, w.content,
                    w.author_id, w.last_editor_id, w.created_at, w.updated_at,
                    au.display_name as author_name,
                    ed.display_name as last_editor_name,
                    NULL as project_name
             FROM t_wiki_pages w
             LEFT JOIN m_users au ON w.author_id = au.id
             LEFT JOIN m_users ed ON w.last_editor_id = ed.id
             WHERE w.project_id IS NULL
             ORDER BY w.category, w.title"
        ).fetch_all(pool).await?
    };
    Ok(rows)
}

pub async fn find_by_slug(pool: &PgPool, project_id: Option<i32>, slug: &str) -> anyhow::Result<Option<WikiPage>> {
    let row = if let Some(pid) = project_id {
        sqlx::query_as::<_, WikiPage>(
            "SELECT w.id, w.project_id, w.title, w.slug, w.category, w.content,
                    w.author_id, w.last_editor_id, w.created_at, w.updated_at,
                    au.display_name as author_name,
                    ed.display_name as last_editor_name,
                    p.name as project_name
             FROM t_wiki_pages w
             LEFT JOIN m_users au ON w.author_id = au.id
             LEFT JOIN m_users ed ON w.last_editor_id = ed.id
             LEFT JOIN m_projects p ON w.project_id = p.id
             WHERE w.project_id = $1 AND w.slug = $2"
        ).bind(pid).bind(slug).fetch_optional(pool).await?
    } else {
        sqlx::query_as::<_, WikiPage>(
            "SELECT w.id, w.project_id, w.title, w.slug, w.category, w.content,
                    w.author_id, w.last_editor_id, w.created_at, w.updated_at,
                    au.display_name as author_name,
                    ed.display_name as last_editor_name,
                    NULL as project_name
             FROM t_wiki_pages w
             LEFT JOIN m_users au ON w.author_id = au.id
             LEFT JOIN m_users ed ON w.last_editor_id = ed.id
             WHERE w.project_id IS NULL AND w.slug = $2"
        ).bind(slug).fetch_optional(pool).await?
    };
    Ok(row)
}

pub async fn create(
    pool: &PgPool, project_id: Option<i32>, title: &str, slug: &str,
    category: &str, content: &str, author_id: i32,
) -> anyhow::Result<i32> {
    let id = sqlx::query_scalar::<_, i32>(
        "INSERT INTO t_wiki_pages (project_id, title, slug, category, content, author_id, last_editor_id)
         VALUES ($1, $2, $3, $4, $5, $6, $6) RETURNING id"
    ).bind(project_id).bind(title).bind(slug).bind(category).bind(content).bind(author_id)
     .fetch_one(pool).await?;
    Ok(id)
}

pub async fn update(
    pool: &PgPool, id: i32, title: &str, category: &str, content: &str, editor_id: i32,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE t_wiki_pages SET title=$2, category=$3, content=$4, last_editor_id=$5, updated_at=NOW()
         WHERE id=$1"
    ).bind(id).bind(title).bind(category).bind(content).bind(editor_id)
     .execute(pool).await?;
    Ok(())
}

pub async fn delete(pool: &PgPool, id: i32) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM t_wiki_pages WHERE id=$1").bind(id).execute(pool).await?;
    Ok(())
}

// --- リビジョン ---

pub async fn create_revision(
    pool: &PgPool, page_id: i32, content: &str, editor_id: i32, comment: &str,
) -> anyhow::Result<i32> {
    let id = sqlx::query_scalar::<_, i32>(
        "INSERT INTO t_wiki_revisions (page_id, content, editor_id, comment)
         VALUES ($1, $2, $3, $4) RETURNING id"
    ).bind(page_id).bind(content).bind(editor_id).bind(comment)
     .fetch_one(pool).await?;
    Ok(id)
}

pub async fn find_revisions(pool: &PgPool, page_id: i32) -> anyhow::Result<Vec<WikiRevision>> {
    let rows = sqlx::query_as::<_, WikiRevision>(
        "SELECT r.id, r.page_id, r.content, r.editor_id, r.comment, r.created_at,
                u.display_name as editor_name
         FROM t_wiki_revisions r
         LEFT JOIN m_users u ON r.editor_id = u.id
         WHERE r.page_id = $1
         ORDER BY r.created_at DESC"
    ).bind(page_id).fetch_all(pool).await?;
    Ok(rows)
}

pub async fn find_revision_by_id(pool: &PgPool, id: i32) -> anyhow::Result<Option<WikiRevision>> {
    let row = sqlx::query_as::<_, WikiRevision>(
        "SELECT r.id, r.page_id, r.content, r.editor_id, r.comment, r.created_at,
                u.display_name as editor_name
         FROM t_wiki_revisions r
         LEFT JOIN m_users u ON r.editor_id = u.id
         WHERE r.id = $1"
    ).bind(id).fetch_optional(pool).await?;
    Ok(row)
}

/// カテゴリーフィルタ付きWikiページ一覧
pub async fn find_by_project_with_category(
    pool: &PgPool, project_id: Option<i32>, category: Option<&str>,
) -> anyhow::Result<Vec<WikiPage>> {
    let rows = match (project_id, category) {
        (Some(pid), Some(cat)) => {
            sqlx::query_as::<_, WikiPage>(
                "SELECT w.id, w.project_id, w.title, w.slug, w.category, w.content,
                        w.author_id, w.last_editor_id, w.created_at, w.updated_at,
                        au.display_name as author_name,
                        ed.display_name as last_editor_name,
                        p.name as project_name
                 FROM t_wiki_pages w
                 LEFT JOIN m_users au ON w.author_id = au.id
                 LEFT JOIN m_users ed ON w.last_editor_id = ed.id
                 LEFT JOIN m_projects p ON w.project_id = p.id
                 WHERE w.project_id = $1 AND w.category = $2
                 ORDER BY w.category, w.title"
            ).bind(pid).bind(cat).fetch_all(pool).await?
        }
        (Some(pid), None) => {
            sqlx::query_as::<_, WikiPage>(
                "SELECT w.id, w.project_id, w.title, w.slug, w.category, w.content,
                        w.author_id, w.last_editor_id, w.created_at, w.updated_at,
                        au.display_name as author_name,
                        ed.display_name as last_editor_name,
                        p.name as project_name
                 FROM t_wiki_pages w
                 LEFT JOIN m_users au ON w.author_id = au.id
                 LEFT JOIN m_users ed ON w.last_editor_id = ed.id
                 LEFT JOIN m_projects p ON w.project_id = p.id
                 WHERE w.project_id = $1
                 ORDER BY w.category, w.title"
            ).bind(pid).fetch_all(pool).await?
        }
        (None, Some(cat)) => {
            sqlx::query_as::<_, WikiPage>(
                "SELECT w.id, w.project_id, w.title, w.slug, w.category, w.content,
                        w.author_id, w.last_editor_id, w.created_at, w.updated_at,
                        au.display_name as author_name,
                        ed.display_name as last_editor_name,
                        NULL as project_name
                 FROM t_wiki_pages w
                 LEFT JOIN m_users au ON w.author_id = au.id
                 LEFT JOIN m_users ed ON w.last_editor_id = ed.id
                 WHERE w.project_id IS NULL AND w.category = $1
                 ORDER BY w.category, w.title"
            ).bind(cat).fetch_all(pool).await?
        }
        (None, None) => {
            sqlx::query_as::<_, WikiPage>(
                "SELECT w.id, w.project_id, w.title, w.slug, w.category, w.content,
                        w.author_id, w.last_editor_id, w.created_at, w.updated_at,
                        au.display_name as author_name,
                        ed.display_name as last_editor_name,
                        NULL as project_name
                 FROM t_wiki_pages w
                 LEFT JOIN m_users au ON w.author_id = au.id
                 LEFT JOIN m_users ed ON w.last_editor_id = ed.id
                 WHERE w.project_id IS NULL
                 ORDER BY w.category, w.title"
            ).fetch_all(pool).await?
        }
    };
    Ok(rows)
}
