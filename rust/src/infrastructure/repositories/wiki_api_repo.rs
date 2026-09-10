/// infrastructure/repositories/wiki_api_repo.rs — Wiki JSON API 永続化
///
/// wiki_page / wiki_revision の CRUD + revisions/link-ticket/unlink-ticket。
/// 既存の infrastructure/repositories/wiki_repo.rs (HTML画面用、テーブル名が
/// 実スキーマと不一致で現状未使用)とは別ファイルにして衝突を避ける。

use sqlx::{PgPool, Row};

use crate::domain::models::wiki_api::*;

/// Djangoの `django.utils.text.slugify(value, allow_unicode=True)` 相当。
/// categoriesと同じ考え方でUnicode文字を保持する。
fn generate_slug(input: &str) -> String {
    let lower = input.to_lowercase();
    let mut result = String::new();
    for c in lower.chars() {
        if c.is_alphanumeric() || c == '_' {
            result.push(c);
        } else if !result.ends_with('-') {
            result.push('-');
        }
    }
    let slug = result.trim_matches(|c| c == '-' || c == '_').to_string();

    if slug.is_empty() {
        "page".to_string()
    } else {
        slug
    }
}

/// Markdown → HTML変換 + [[Wikiリンク]]解決。
/// 既存 domain/services/wiki_service.rs の関数を再利用。
fn render_content(content: &str, project_id: Option<i32>) -> String {
    let html = crate::domain::services::wiki_service::render_markdown(content);
    crate::domain::services::wiki_service::resolve_wiki_links(&html, project_id)
}

const LIST_SELECT: &str = "
    SELECT
        w.id::int4, w.title, w.slug, w.category, w.project_id::int4, w.team_id::int4,
        w.created_at, w.updated_at,
        a.id::int4 as a_id, a.username as a_username, a.email as a_email, a.display_name as a_display_name,
        le.id::int4 as le_id, le.username as le_username, le.email as le_email, le.display_name as le_display_name,
        (SELECT COUNT(*) FROM wiki_revision WHERE page_id = w.id)::int8 as revision_count
     FROM wiki_page w
     JOIN accounts_user a ON w.author_id = a.id
     LEFT JOIN accounts_user le ON w.last_editor_id = le.id
";

fn row_to_list(row: &sqlx::postgres::PgRow) -> WikiPageListOut {
    let le_id: Option<i32> = row.get("le_id");
    WikiPageListOut {
        id: row.get("id"),
        title: row.get("title"),
        slug: row.get("slug"),
        category: row.get("category"),
        project: row.get("project_id"),
        team: row.get("team_id"),
        author: UserSummaryOut {
            id: row.get("a_id"),
            username: row.get("a_username"),
            email: row.get("a_email"),
            display_name: row.get("a_display_name"),
        },
        last_editor: le_id.map(|_| UserSummaryOut {
            id: row.get("le_id"),
            username: row.get("le_username"),
            email: row.get("le_email"),
            display_name: row.get("le_display_name"),
        }),
        revision_count: row.get("revision_count"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

pub async fn find_all(
    pool: &PgPool,
    project_id: Option<i32>,
    team_id: Option<i32>,
    category: Option<&str>,
    search: Option<&str>,
    page: i64,
) -> anyhow::Result<Vec<WikiPageListOut>> {
    const PAGE_SIZE: i64 = 50;
    let page = page.max(1);
    let offset = (page - 1) * PAGE_SIZE;
    let search_pattern = search.map(|s| format!("%{}%", s));

    let query = format!(
        "{LIST_SELECT}
         WHERE ($1::int4 IS NULL OR w.project_id = $1)
           AND ($2::int4 IS NULL OR w.team_id = $2)
           AND ($3::text IS NULL OR w.category = $3)
           AND ($4::text IS NULL OR w.title ILIKE $4 OR w.content ILIKE $4)
         ORDER BY w.updated_at DESC
         LIMIT $5 OFFSET $6"
    );

    let rows = sqlx::query(&query)
        .bind(project_id)
        .bind(team_id)
        .bind(category)
        .bind(&search_pattern)
        .bind(PAGE_SIZE)
        .bind(offset)
        .fetch_all(pool)
        .await?;

    Ok(rows.iter().map(row_to_list).collect())
}

pub async fn count_all(
    pool: &PgPool,
    project_id: Option<i32>,
    team_id: Option<i32>,
    category: Option<&str>,
    search: Option<&str>,
) -> anyhow::Result<i64> {
    let search_pattern = search.map(|s| format!("%{}%", s));

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM wiki_page w
         WHERE ($1::int4 IS NULL OR w.project_id = $1)
           AND ($2::int4 IS NULL OR w.team_id = $2)
           AND ($3::text IS NULL OR w.category = $3)
           AND ($4::text IS NULL OR w.title ILIKE $4 OR w.content ILIKE $4)"
    )
    .bind(project_id)
    .bind(team_id)
    .bind(category)
    .bind(&search_pattern)
    .fetch_one(pool)
    .await?;

    Ok(count)
}

pub async fn find_by_id(pool: &PgPool, id: i32) -> anyhow::Result<Option<WikiPageDetailOut>> {
    let row = sqlx::query(
        "SELECT
            w.id::int4, w.title, w.slug, w.category, w.project_id::int4, w.team_id::int4, w.content,
            w.created_at, w.updated_at,
            a.id::int4 as a_id, a.username as a_username, a.email as a_email, a.display_name as a_display_name,
            le.id::int4 as le_id, le.username as le_username, le.email as le_email, le.display_name as le_display_name
         FROM wiki_page w
         JOIN accounts_user a ON w.author_id = a.id
         LEFT JOIN accounts_user le ON w.last_editor_id = le.id
         WHERE w.id = $1"
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    let row = match row {
        Some(r) => r,
        None => return Ok(None),
    };

    let project_id: Option<i32> = row.get("project_id");
    let content: String = row.get("content");
    let le_id: Option<i32> = row.get("le_id");

    let linked_rows = sqlx::query(
        "SELECT t.id::int4, t.ticket_key, t.title, t.status, t.priority
         FROM wiki_page_linked_tickets wlt
         JOIN tickets_ticket t ON wlt.ticketmodel_id = t.id
         WHERE wlt.wikipage_id = $1"
    )
    .bind(id)
    .fetch_all(pool)
    .await?;

    let linked_tickets = linked_rows
        .into_iter()
        .map(|r| LinkedTicketSummaryOut {
            id: r.get("id"),
            ticket_key: r.get("ticket_key"),
            title: r.get("title"),
            status: r.get("status"),
            priority: r.get("priority"),
        })
        .collect();

    Ok(Some(WikiPageDetailOut {
        id: row.get("id"),
        title: row.get("title"),
        slug: row.get("slug"),
        category: row.get("category"),
        project: project_id,
        team: row.get("team_id"),
        rendered_content: render_content(&content, project_id),
        content,
        author: UserSummaryOut {
            id: row.get("a_id"),
            username: row.get("a_username"),
            email: row.get("a_email"),
            display_name: row.get("a_display_name"),
        },
        last_editor: le_id.map(|_| UserSummaryOut {
            id: row.get("le_id"),
            username: row.get("le_username"),
            email: row.get("le_email"),
            display_name: row.get("le_display_name"),
        }),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        linked_tickets,
    }))
}

/// ページ作成(初回リビジョン自動生成)。projectまたはteamスコープ内でslugをユニーク化。
pub async fn create(pool: &PgPool, input: &WikiPageCreateIn, author_id: i32) -> anyhow::Result<i32> {
    let base_slug = generate_slug(&input.title);
    let mut slug = base_slug.clone();
    let mut counter = 1;
    loop {
        let existing: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM wiki_page WHERE slug = $1 AND
             ((project_id IS NULL AND $2::int4 IS NULL AND team_id IS NULL AND $3::int4 IS NULL) OR
              (project_id = $2) OR
              (team_id = $3))"
        )
        .bind(&slug)
        .bind(input.project)
        .bind(input.team)
        .fetch_one(pool)
        .await?;

        if existing == 0 {
            break;
        }
        slug = format!("{}-{}", base_slug, counter);
        counter += 1;
    }

    let mut tx = pool.begin().await?;

    let page_id: i32 = sqlx::query_scalar(
        "INSERT INTO wiki_page (project_id, team_id, title, slug, category, content, author_id, last_editor_id, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $7, NOW(), NOW())
         RETURNING id::int4"
    )
    .bind(input.project)
    .bind(input.team)
    .bind(&input.title)
    .bind(&slug)
    .bind(&input.category)
    .bind(&input.content)
    .bind(author_id)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO wiki_revision (page_id, content, editor_id, comment, created_at)
         VALUES ($1, $2, $3, 'Initial version', NOW())"
    )
    .bind(page_id)
    .bind(&input.content)
    .bind(author_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(page_id)
}

/// ページ更新(リビジョン自動生成)。slugは変更しない。
pub async fn update(pool: &PgPool, id: i32, input: &WikiPageUpdateIn, editor_id: i32) -> anyhow::Result<bool> {
    let mut tx = pool.begin().await?;

    let existing = sqlx::query(
        "SELECT title, category, project_id::int4, team_id::int4, content FROM wiki_page WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;

    let existing = match existing {
        Some(r) => r,
        None => return Ok(false),
    };

    let existing_title: String = existing.get("title");
    let existing_category: String = existing.get("category");
    let existing_project: Option<i32> = existing.get("project_id");
    let existing_team: Option<i32> = existing.get("team_id");
    let existing_content: String = existing.get("content");

    let title = input.title.clone().unwrap_or(existing_title);
    let category = input.category.clone().unwrap_or(existing_category);
    let project = input.project.or(existing_project);
    let team = input.team.or(existing_team);
    let content = input.content.clone().unwrap_or(existing_content);

    sqlx::query(
        "UPDATE wiki_page SET title = $1, category = $2, project_id = $3, team_id = $4, content = $5,
             last_editor_id = $6, updated_at = NOW()
         WHERE id = $7"
    )
    .bind(&title)
    .bind(&category)
    .bind(project)
    .bind(team)
    .bind(&content)
    .bind(editor_id)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO wiki_revision (page_id, content, editor_id, comment, created_at)
         VALUES ($1, $2, $3, $4, NOW())"
    )
    .bind(id)
    .bind(&content)
    .bind(editor_id)
    .bind(&input.revision_comment)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(true)
}

/// ページ削除。wiki_revision/wiki_page_linked_tickets/notifications系(wiki_page参照)を
/// 先に削除してから本体を削除する(DjangoのCASCADEに相当)。
pub async fn delete(pool: &PgPool, id: i32) -> anyhow::Result<bool> {
    let mut tx = pool.begin().await?;

    sqlx::query("DELETE FROM wiki_page_linked_tickets WHERE wikipage_id = $1").bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM notifications_notification WHERE wiki_page_id = $1").bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM notifications_user_read_state WHERE wiki_page_id = $1").bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM wiki_revision WHERE page_id = $1").bind(id).execute(&mut *tx).await?;

    let rows_affected = sqlx::query("DELETE FROM wiki_page WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();

    tx.commit().await?;
    Ok(rows_affected > 0)
}

/// リビジョン一覧(最新50件)。
pub async fn find_revisions(pool: &PgPool, page_id: i32) -> anyhow::Result<Vec<WikiRevisionOut>> {
    let rows = sqlx::query(
        "SELECT
            r.id::int4, r.content, r.comment, r.created_at,
            e.id::int4 as e_id, e.username as e_username, e.email as e_email, e.display_name as e_display_name
         FROM wiki_revision r
         LEFT JOIN accounts_user e ON r.editor_id = e.id
         WHERE r.page_id = $1
         ORDER BY r.created_at DESC
         LIMIT 50"
    )
    .bind(page_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let e_id: Option<i32> = r.get("e_id");
            WikiRevisionOut {
                id: r.get("id"),
                content: r.get("content"),
                editor: e_id.map(|_| UserSummaryOut {
                    id: r.get("e_id"),
                    username: r.get("e_username"),
                    email: r.get("e_email"),
                    display_name: r.get("e_display_name"),
                }),
                comment: r.get("comment"),
                created_at: r.get("created_at"),
            }
        })
        .collect())
}

pub async fn page_exists(pool: &PgPool, id: i32) -> anyhow::Result<bool> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM wiki_page WHERE id = $1)")
        .bind(id)
        .fetch_one(pool)
        .await?;
    Ok(exists)
}

pub async fn ticket_exists(pool: &PgPool, id: i32) -> anyhow::Result<bool> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM tickets_ticket WHERE id = $1)")
        .bind(id)
        .fetch_one(pool)
        .await?;
    Ok(exists)
}

pub async fn link_ticket(pool: &PgPool, page_id: i32, ticket_id: i32) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO wiki_page_linked_tickets (wikipage_id, ticketmodel_id)
         VALUES ($1, $2) ON CONFLICT DO NOTHING"
    )
    .bind(page_id)
    .bind(ticket_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn unlink_ticket(pool: &PgPool, page_id: i32, ticket_id: i32) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM wiki_page_linked_tickets WHERE wikipage_id = $1 AND ticketmodel_id = $2")
        .bind(page_id)
        .bind(ticket_id)
        .execute(pool)
        .await?;
    Ok(())
}
