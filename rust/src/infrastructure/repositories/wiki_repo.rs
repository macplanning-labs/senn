/// infrastructure/repositories/wiki_repo.rs — Wiki 永続化
///
/// WIP-000032のテスト作成中に判明: 本ファイルは元々 t_wiki_pages / m_users /
/// m_projects / t_wiki_revisions という実在しないテーブル名を参照しており、
/// 呼び出すと必ずエラーになっていた(実テーブルは wiki_page / accounts_user /
/// tickets_project / wiki_revision。wiki_api_repo.rs で使われている正しい
/// テーブル名に合わせて修正した)。呼び出し元は wiki_service.rs(Askama用、
/// presentation/handlers/wiki.rsのみが使用)と ai_agent_api.rs
/// (list_wiki_pages)。

use sqlx::PgPool;
use crate::domain::models::wiki::{WikiPage, WikiRevision};

pub async fn find_by_project(pool: &PgPool, project_id: Option<i32>) -> anyhow::Result<Vec<WikiPage>> {
    let rows = if let Some(pid) = project_id {
        sqlx::query_as::<_, WikiPage>(
            "SELECT w.id::int4, w.project_id::int4, w.title, w.slug, w.category, w.content,
                    w.author_id::int4, w.last_editor_id::int4, w.created_at, w.updated_at,
                    au.display_name as author_name,
                    ed.display_name as last_editor_name,
                    p.name as project_name
             FROM wiki_page w
             LEFT JOIN accounts_user au ON w.author_id = au.id
             LEFT JOIN accounts_user ed ON w.last_editor_id = ed.id
             LEFT JOIN tickets_project p ON w.project_id = p.id
             WHERE w.project_id = $1
             ORDER BY w.category, w.title"
        ).bind(pid).fetch_all(pool).await?
    } else {
        // 共有Wiki（project_id IS NULL）
        sqlx::query_as::<_, WikiPage>(
            "SELECT w.id::int4, w.project_id::int4, w.title, w.slug, w.category, w.content,
                    w.author_id::int4, w.last_editor_id::int4, w.created_at, w.updated_at,
                    au.display_name as author_name,
                    ed.display_name as last_editor_name,
                    NULL as project_name
             FROM wiki_page w
             LEFT JOIN accounts_user au ON w.author_id = au.id
             LEFT JOIN accounts_user ed ON w.last_editor_id = ed.id
             WHERE w.project_id IS NULL
             ORDER BY w.category, w.title"
        ).fetch_all(pool).await?
    };
    Ok(rows)
}

pub async fn find_by_slug(pool: &PgPool, project_id: Option<i32>, slug: &str) -> anyhow::Result<Option<WikiPage>> {
    let row = if let Some(pid) = project_id {
        sqlx::query_as::<_, WikiPage>(
            "SELECT w.id::int4, w.project_id::int4, w.title, w.slug, w.category, w.content,
                    w.author_id::int4, w.last_editor_id::int4, w.created_at, w.updated_at,
                    au.display_name as author_name,
                    ed.display_name as last_editor_name,
                    p.name as project_name
             FROM wiki_page w
             LEFT JOIN accounts_user au ON w.author_id = au.id
             LEFT JOIN accounts_user ed ON w.last_editor_id = ed.id
             LEFT JOIN tickets_project p ON w.project_id = p.id
             WHERE w.project_id = $1 AND w.slug = $2"
        ).bind(pid).bind(slug).fetch_optional(pool).await?
    } else {
        sqlx::query_as::<_, WikiPage>(
            "SELECT w.id::int4, w.project_id::int4, w.title, w.slug, w.category, w.content,
                    w.author_id::int4, w.last_editor_id::int4, w.created_at, w.updated_at,
                    au.display_name as author_name,
                    ed.display_name as last_editor_name,
                    NULL as project_name
             FROM wiki_page w
             LEFT JOIN accounts_user au ON w.author_id = au.id
             LEFT JOIN accounts_user ed ON w.last_editor_id = ed.id
             WHERE w.project_id IS NULL AND w.slug = $1"
        ).bind(slug).fetch_optional(pool).await?
    };
    Ok(row)
}

pub async fn create(
    pool: &PgPool, project_id: Option<i32>, title: &str, slug: &str,
    category: &str, content: &str, author_id: i32,
) -> anyhow::Result<i32> {
    let id = sqlx::query_scalar::<_, i32>(
        "INSERT INTO wiki_page (project_id, title, slug, category, content, author_id, last_editor_id, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $6, NOW(), NOW()) RETURNING id::int4"
    ).bind(project_id).bind(title).bind(slug).bind(category).bind(content).bind(author_id)
     .fetch_one(pool).await?;
    Ok(id)
}

pub async fn update(
    pool: &PgPool, id: i32, title: &str, category: &str, content: &str, editor_id: i32,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE wiki_page SET title=$2, category=$3, content=$4, last_editor_id=$5, updated_at=NOW()
         WHERE id=$1"
    ).bind(id).bind(title).bind(category).bind(content).bind(editor_id)
     .execute(pool).await?;
    Ok(())
}

pub async fn delete(pool: &PgPool, id: i32) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM wiki_page WHERE id=$1").bind(id).execute(pool).await?;
    Ok(())
}

// --- リビジョン ---

pub async fn create_revision(
    pool: &PgPool, page_id: i32, content: &str, editor_id: i32, comment: &str,
) -> anyhow::Result<i32> {
    let id = sqlx::query_scalar::<_, i32>(
        "INSERT INTO wiki_revision (page_id, content, editor_id, comment, created_at)
         VALUES ($1, $2, $3, $4, NOW()) RETURNING id::int4"
    ).bind(page_id).bind(content).bind(editor_id).bind(comment)
     .fetch_one(pool).await?;
    Ok(id)
}

pub async fn find_revisions(pool: &PgPool, page_id: i32) -> anyhow::Result<Vec<WikiRevision>> {
    let rows = sqlx::query_as::<_, WikiRevision>(
        "SELECT r.id::int4, r.page_id::int4, r.content, r.editor_id::int4, r.comment, r.created_at,
                u.display_name as editor_name
         FROM wiki_revision r
         LEFT JOIN accounts_user u ON r.editor_id = u.id
         WHERE r.page_id = $1
         ORDER BY r.created_at DESC"
    ).bind(page_id).fetch_all(pool).await?;
    Ok(rows)
}

pub async fn find_revision_by_id(pool: &PgPool, id: i32) -> anyhow::Result<Option<WikiRevision>> {
    let row = sqlx::query_as::<_, WikiRevision>(
        "SELECT r.id::int4, r.page_id::int4, r.content, r.editor_id::int4, r.comment, r.created_at,
                u.display_name as editor_name
         FROM wiki_revision r
         LEFT JOIN accounts_user u ON r.editor_id = u.id
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
                "SELECT w.id::int4, w.project_id::int4, w.title, w.slug, w.category, w.content,
                        w.author_id::int4, w.last_editor_id::int4, w.created_at, w.updated_at,
                        au.display_name as author_name,
                        ed.display_name as last_editor_name,
                        p.name as project_name
                 FROM wiki_page w
                 LEFT JOIN accounts_user au ON w.author_id = au.id
                 LEFT JOIN accounts_user ed ON w.last_editor_id = ed.id
                 LEFT JOIN tickets_project p ON w.project_id = p.id
                 WHERE w.project_id = $1 AND w.category = $2
                 ORDER BY w.category, w.title"
            ).bind(pid).bind(cat).fetch_all(pool).await?
        }
        (Some(pid), None) => {
            sqlx::query_as::<_, WikiPage>(
                "SELECT w.id::int4, w.project_id::int4, w.title, w.slug, w.category, w.content,
                        w.author_id::int4, w.last_editor_id::int4, w.created_at, w.updated_at,
                        au.display_name as author_name,
                        ed.display_name as last_editor_name,
                        p.name as project_name
                 FROM wiki_page w
                 LEFT JOIN accounts_user au ON w.author_id = au.id
                 LEFT JOIN accounts_user ed ON w.last_editor_id = ed.id
                 LEFT JOIN tickets_project p ON w.project_id = p.id
                 WHERE w.project_id = $1
                 ORDER BY w.category, w.title"
            ).bind(pid).fetch_all(pool).await?
        }
        (None, Some(cat)) => {
            sqlx::query_as::<_, WikiPage>(
                "SELECT w.id::int4, w.project_id::int4, w.title, w.slug, w.category, w.content,
                        w.author_id::int4, w.last_editor_id::int4, w.created_at, w.updated_at,
                        au.display_name as author_name,
                        ed.display_name as last_editor_name,
                        NULL as project_name
                 FROM wiki_page w
                 LEFT JOIN accounts_user au ON w.author_id = au.id
                 LEFT JOIN accounts_user ed ON w.last_editor_id = ed.id
                 WHERE w.project_id IS NULL AND w.category = $1
                 ORDER BY w.category, w.title"
            ).bind(cat).fetch_all(pool).await?
        }
        (None, None) => {
            sqlx::query_as::<_, WikiPage>(
                "SELECT w.id::int4, w.project_id::int4, w.title, w.slug, w.category, w.content,
                        w.author_id::int4, w.last_editor_id::int4, w.created_at, w.updated_at,
                        au.display_name as author_name,
                        ed.display_name as last_editor_name,
                        NULL as project_name
                 FROM wiki_page w
                 LEFT JOIN accounts_user au ON w.author_id = au.id
                 LEFT JOIN accounts_user ed ON w.last_editor_id = ed.id
                 WHERE w.project_id IS NULL
                 ORDER BY w.category, w.title"
            ).fetch_all(pool).await?
        }
    };
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;

    #[tokio::test]
    async fn create_and_find_by_slug_project_scoped() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let author = test_support::create_test_user(&pool, "wiki-author").await;
        let project = test_support::create_test_project(&pool, "WK", author).await;
        let slug = format!("test-page-{}", test_support::unique_suffix());

        let page_id = create(&pool, Some(project), "テストページ", &slug, "general", "本文", author)
            .await
            .unwrap();

        let found = find_by_slug(&pool, Some(project), &slug).await.unwrap();
        assert!(found.is_some());
        let page = found.unwrap();
        assert_eq!(page.id, page_id);
        assert_eq!(page.project_id, Some(project));

        // 別プロジェクト扱い(project_id無し)では見つからないことを確認
        let not_found = find_by_slug(&pool, None, &slug).await.unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn shared_wiki_page_has_no_project() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let author = test_support::create_test_user(&pool, "shared-wiki-author").await;
        let slug = format!("shared-page-{}", test_support::unique_suffix());

        let page_id = create(&pool, None, "共有ページ", &slug, "general", "本文", author)
            .await
            .unwrap();

        let found = find_by_slug(&pool, None, &slug).await.unwrap();
        assert!(found.is_some());
        let page = found.unwrap();
        assert_eq!(page.id, page_id);
        assert_eq!(page.project_id, None);
    }

    #[tokio::test]
    async fn update_changes_content_and_editor() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let author = test_support::create_test_user(&pool, "update-author").await;
        let editor = test_support::create_test_user(&pool, "update-editor").await;
        let project = test_support::create_test_project(&pool, "UPD", author).await;
        let slug = format!("update-page-{}", test_support::unique_suffix());
        let page_id = create(&pool, Some(project), "タイトル", &slug, "general", "旧本文", author)
            .await
            .unwrap();

        update(&pool, page_id, "新タイトル", "general", "新本文", editor).await.unwrap();

        let found = find_by_slug(&pool, Some(project), &slug).await.unwrap().unwrap();
        assert_eq!(found.title, "新タイトル");
        assert_eq!(found.content, "新本文");
        assert_eq!(found.last_editor_id, Some(editor));
    }

    #[tokio::test]
    async fn delete_removes_page() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let author = test_support::create_test_user(&pool, "delete-author").await;
        let project = test_support::create_test_project(&pool, "DEL", author).await;
        let slug = format!("delete-page-{}", test_support::unique_suffix());
        let page_id = create(&pool, Some(project), "削除対象", &slug, "general", "本文", author)
            .await
            .unwrap();

        delete(&pool, page_id).await.unwrap();

        let found = find_by_slug(&pool, Some(project), &slug).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn revisions_are_recorded_and_ordered() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let author = test_support::create_test_user(&pool, "rev-author").await;
        let project = test_support::create_test_project(&pool, "REV", author).await;
        let slug = format!("rev-page-{}", test_support::unique_suffix());
        let page_id = create(&pool, Some(project), "リビジョンテスト", &slug, "general", "v1", author)
            .await
            .unwrap();

        create_revision(&pool, page_id, "v1", author, "初版").await.unwrap();
        create_revision(&pool, page_id, "v2", author, "第2版").await.unwrap();

        let revisions = find_revisions(&pool, page_id).await.unwrap();
        assert_eq!(revisions.len(), 2);
        // created_at DESCなので最新(v2)が先頭
        assert_eq!(revisions[0].content, "v2");
        assert_eq!(revisions[1].content, "v1");
    }
}
