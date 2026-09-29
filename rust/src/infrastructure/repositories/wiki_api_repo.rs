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

/// ページの所属と作成者（権限判定用）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WikiScope {
    pub project: Option<i32>,
    pub team: Option<i32>,
    pub author_id: i32,
}

/// ページの所属と作成者を取る。ページが無ければ None。
pub async fn find_scope(pool: &PgPool, id: i32) -> anyhow::Result<Option<WikiScope>> {
    let row = sqlx::query(
        "SELECT project_id::int4, team_id::int4, author_id::int4 FROM wiki_page WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| WikiScope {
        project: r.get("project_id"),
        team: r.get("team_id"),
        author_id: r.get("author_id"),
    }))
}

/// この所属のページに書き込めるか。
/// - staff: 常に可
/// - プロジェクトあり: `project_team_repo::can_edit`
/// - チームあり: そのチームの正規メンバー（scoped_project_id が NULL の行）
/// - 両方あり: 両方を満たす必要がある
/// - どちらもなし: 可（共有ページ。削除だけは呼び出し側で別に制限する）
pub async fn can_write_scope(
    pool: &PgPool,
    user_id: i32,
    project: Option<i32>,
    team: Option<i32>,
) -> anyhow::Result<bool> {
    let is_staff: bool = sqlx::query_scalar("SELECT is_staff FROM accounts_user WHERE id = $1")
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .unwrap_or(false);
    if is_staff {
        return Ok(true);
    }
    if let Some(p) = project {
        if !crate::infrastructure::repositories::project_team_repo::can_edit(pool, p, user_id, false).await? {
            return Ok(false);
        }
    }
    if let Some(t) = team {
        let member: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM t_team_membership
                WHERE team_id = $1 AND user_id = $2 AND scoped_project_id IS NULL
             )",
        )
        .bind(t as i64)
        .bind(user_id as i64)
        .fetch_one(pool)
        .await?;
        if !member {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;

    async fn set_role(pool: &PgPool, team_id: i32, user_id: i32, role: &str) {
        sqlx::query(
            "INSERT INTO t_team_membership (team_id, user_id, role, joined_at) VALUES ($1, $2, $3, NOW())",
        )
        .bind(team_id as i64)
        .bind(user_id as i64)
        .bind(role)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn set_guest_role(pool: &PgPool, team_id: i32, user_id: i32, scoped_project_id: i32) {
        sqlx::query(
            "INSERT INTO t_team_membership (team_id, user_id, role, scoped_project_id, joined_at) VALUES ($1, $2, 'member', $3, NOW())",
        )
        .bind(team_id as i64)
        .bind(user_id as i64)
        .bind(scoped_project_id as i64)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn create_wiki_page(
        pool: &PgPool,
        project: Option<i32>,
        team: Option<i32>,
        author_id: i32,
    ) -> i32 {
        let suffix = test_support::unique_suffix();
        let title = format!("wiki_{}", suffix);
        let slug = format!("wiki-{}", suffix);
        sqlx::query_scalar::<_, i32>(
            "INSERT INTO wiki_page (project_id, team_id, title, slug, category, content, author_id, last_editor_id, created_at, updated_at)
             VALUES ($1, $2, $3, $4, 'general', 'content', $5, $5, NOW(), NOW())
             RETURNING id::int4"
        )
        .bind(project.map(|p| p as i64))
        .bind(team.map(|t| t as i64))
        .bind(&title)
        .bind(&slug)
        .bind(author_id as i64)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn create_staff_user(pool: &PgPool, prefix: &str) -> i32 {
        let user_id = test_support::create_test_user(pool, prefix).await;
        sqlx::query("UPDATE accounts_user SET is_staff = true WHERE id = $1")
            .bind(user_id as i64)
            .execute(pool)
            .await
            .unwrap();
        user_id
    }

    #[tokio::test]
    async fn w1_staff_can_write_any_scope() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let owner = test_support::create_test_user(&pool, "w1o").await;
        let staff = create_staff_user(&pool, "w1s").await;
        let project = test_support::create_test_project(&pool, "W1", owner).await;
        let team = sqlx::query_scalar::<_, i32>(
            "SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1"
        )
        .bind(project as i64)
        .fetch_one(&pool)
        .await
        .unwrap();

        // staff can write to any scope
        assert!(can_write_scope(&pool, staff, Some(project), None).await.unwrap());
        assert!(can_write_scope(&pool, staff, None, Some(team)).await.unwrap());
        assert!(can_write_scope(&pool, staff, Some(project), Some(team)).await.unwrap());
        assert!(can_write_scope(&pool, staff, None, None).await.unwrap());
    }

    #[tokio::test]
    async fn w2_project_team_member_can_write_project_scope() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let owner = test_support::create_test_user(&pool, "w2o").await;
        let member = test_support::create_test_user(&pool, "w2m").await;
        let project = test_support::create_test_project(&pool, "W2", owner).await;
        let team = sqlx::query_scalar::<_, i32>(
            "SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1"
        )
        .bind(project as i64)
        .fetch_one(&pool)
        .await
        .unwrap();
        set_role(&pool, team, member, "member").await;

        assert!(can_write_scope(&pool, member, Some(project), None).await.unwrap());
    }

    #[tokio::test]
    async fn w3_non_member_cannot_write_project_scope() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let owner = test_support::create_test_user(&pool, "w3o").await;
        let outsider = test_support::create_test_user(&pool, "w3x").await;
        let project = test_support::create_test_project(&pool, "W3", owner).await;

        assert!(!can_write_scope(&pool, outsider, Some(project), None).await.unwrap());
    }

    #[tokio::test]
    async fn w4_team_member_can_write_team_scope() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let member = test_support::create_test_user(&pool, "w4m").await;
        let team = test_support::create_test_team(&pool, "W4").await;
        set_role(&pool, team, member, "member").await;

        assert!(can_write_scope(&pool, member, None, Some(team)).await.unwrap());
    }

    #[tokio::test]
    async fn w5_non_member_cannot_write_team_scope() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let non_member = test_support::create_test_user(&pool, "w5n").await;
        let team = test_support::create_test_team(&pool, "W5").await;

        assert!(!can_write_scope(&pool, non_member, None, Some(team)).await.unwrap());
    }

    #[tokio::test]
    async fn w6_project_guest_cannot_write_project_scope() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let owner = test_support::create_test_user(&pool, "w6o").await;
        let guest = test_support::create_test_user(&pool, "w6g").await;
        let project = test_support::create_test_project(&pool, "W6", owner).await;
        let team = sqlx::query_scalar::<_, i32>(
            "SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1"
        )
        .bind(project as i64)
        .fetch_one(&pool)
        .await
        .unwrap();
        set_guest_role(&pool, team, guest, project).await;

        assert!(!can_write_scope(&pool, guest, Some(project), None).await.unwrap());
    }

    #[tokio::test]
    async fn w12_project_guest_cannot_write_team_scope() {
        // チーム所属のページには、そのチームのプロジェクトゲスト（scoped_project_id 付き）は書けない
        let Some(pool) = test_support::test_pool().await else { return; };
        let owner = test_support::create_test_user(&pool, "w12o").await;
        let guest = test_support::create_test_user(&pool, "w12g").await;
        let project = test_support::create_test_project(&pool, "W12", owner).await;
        let team = sqlx::query_scalar::<_, i32>(
            "SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1"
        )
        .bind(project as i64)
        .fetch_one(&pool)
        .await
        .unwrap();
        set_guest_role(&pool, team, guest, project).await;

        assert!(!can_write_scope(&pool, guest, None, Some(team)).await.unwrap());
    }

    #[tokio::test]
    async fn w7_both_scope_requires_project_permission() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let owner = test_support::create_test_user(&pool, "w7o").await;
        let outsider = test_support::create_test_user(&pool, "w7x").await;
        let project = test_support::create_test_project(&pool, "W7", owner).await;
        let team = test_support::create_test_team(&pool, "W7T").await;
        set_role(&pool, team, outsider, "member").await;

        // outsider is team member but not project member
        assert!(!can_write_scope(&pool, outsider, Some(project), Some(team)).await.unwrap());
    }

    #[tokio::test]
    async fn w8_both_scope_requires_team_permission() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let owner = test_support::create_test_user(&pool, "w8o").await;
        let member = test_support::create_test_user(&pool, "w8m").await;
        let project = test_support::create_test_project(&pool, "W8", owner).await;
        let project_team = sqlx::query_scalar::<_, i32>(
            "SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1"
        )
        .bind(project as i64)
        .fetch_one(&pool)
        .await
        .unwrap();
        let other_team = test_support::create_test_team(&pool, "W8O").await;
        set_role(&pool, project_team, member, "member").await;

        // member is project member but not team member (on other_team)
        assert!(!can_write_scope(&pool, member, Some(project), Some(other_team)).await.unwrap());
    }

    #[tokio::test]
    async fn w9_both_scope_requires_both_permissions() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let owner = test_support::create_test_user(&pool, "w9o").await;
        let member = test_support::create_test_user(&pool, "w9m").await;
        let project = test_support::create_test_project(&pool, "W9", owner).await;
        let project_team = sqlx::query_scalar::<_, i32>(
            "SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1"
        )
        .bind(project as i64)
        .fetch_one(&pool)
        .await
        .unwrap();
        let team = test_support::create_test_team(&pool, "W9T").await;
        set_role(&pool, project_team, member, "member").await;
        set_role(&pool, team, member, "member").await;

        assert!(can_write_scope(&pool, member, Some(project), Some(team)).await.unwrap());
    }

    #[tokio::test]
    async fn w10_shared_page_allows_non_staff() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let user = test_support::create_test_user(&pool, "w10").await;

        // shared page (no project, no team)
        assert!(can_write_scope(&pool, user, None, None).await.unwrap());
    }

    #[tokio::test]
    async fn w11_find_scope_returns_scope_or_none() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let author = test_support::create_test_user(&pool, "w11").await;
        let owner = test_support::create_test_user(&pool, "w11o").await;
        let project = test_support::create_test_project(&pool, "W11", owner).await;
        let team = sqlx::query_scalar::<_, i32>(
            "SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1"
        )
        .bind(project as i64)
        .fetch_one(&pool)
        .await
        .unwrap();

        // create pages with different scopes
        let project_page = create_wiki_page(&pool, Some(project), None, author).await;
        let team_page = create_wiki_page(&pool, None, Some(team), author).await;
        let shared_page = create_wiki_page(&pool, None, None, author).await;

        // verify find_scope
        let scope = find_scope(&pool, project_page).await.unwrap().unwrap();
        assert_eq!(scope.project, Some(project));
        assert_eq!(scope.team, None);
        assert_eq!(scope.author_id, author);

        let scope = find_scope(&pool, team_page).await.unwrap().unwrap();
        assert_eq!(scope.project, None);
        assert_eq!(scope.team, Some(team));
        assert_eq!(scope.author_id, author);

        let scope = find_scope(&pool, shared_page).await.unwrap().unwrap();
        assert_eq!(scope.project, None);
        assert_eq!(scope.team, None);
        assert_eq!(scope.author_id, author);

        // nonexistent page returns None
        assert!(find_scope(&pool, i32::MAX).await.unwrap().is_none());
    }
}
