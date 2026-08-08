/// infrastructure/repositories/team_repo.rs — チーム永続化
///
/// Teams (m_team) and Team Memberships (t_team_membership) の CRUD 操作

use sqlx::{PgPool, Row};

use crate::domain::models::team_api::*;
use crate::domain::models::ticket_api::UserSummaryOut;

// =============================================================================
// Teams - 一覧/詳細/CRUD
// =============================================================================

pub async fn find_all_teams(pool: &PgPool, page: i64) -> anyhow::Result<Vec<TeamOut>> {
    const PAGE_SIZE: i64 = 50;
    let page = page.max(1);
    let offset = (page - 1) * PAGE_SIZE;

    let rows = sqlx::query(
        "SELECT
            t.id::int4, t.name, t.slug, t.description, t.icon, t.color, t.slack_webhook_url, t.is_active, t.created_at,
            (SELECT COUNT(*)::int8 FROM t_team_membership WHERE team_id = t.id) as member_count,
            (SELECT COUNT(*)::int8 FROM tickets_project WHERE owner_team_id = t.id) as project_count
         FROM m_team t
         ORDER BY t.name ASC
         LIMIT $1 OFFSET $2"
    )
    .bind(PAGE_SIZE)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    let teams = rows
        .into_iter()
        .map(|row| TeamOut {
            id: row.get(0),
            name: row.get(1),
            slug: row.get(2),
            description: row.get(3),
            icon: row.get(4),
            color: row.get(5),
            slack_webhook_url: row.get(6),
            is_active: row.get(7),
            created_at: row.get(8),
            member_count: row.get(9),
            project_count: row.get(10),
        })
        .collect();

    Ok(teams)
}

pub async fn count_teams(pool: &PgPool) -> anyhow::Result<i64> {
    let row = sqlx::query("SELECT COUNT(*) as count FROM m_team")
        .fetch_one(pool)
        .await?;
    let count: i64 = row.get(0);
    Ok(count)
}

pub async fn find_team_by_id(pool: &PgPool, id: i32) -> anyhow::Result<Option<TeamOut>> {
    let row_opt = sqlx::query(
        "SELECT
            t.id::int4, t.name, t.slug, t.description, t.icon, t.color, t.slack_webhook_url, t.is_active, t.created_at,
            (SELECT COUNT(*)::int8 FROM t_team_membership WHERE team_id = t.id) as member_count,
            (SELECT COUNT(*)::int8 FROM tickets_project WHERE owner_team_id = t.id) as project_count
         FROM m_team t
         WHERE t.id = $1"
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    let team = row_opt.map(|row| TeamOut {
        id: row.get(0),
        name: row.get(1),
        slug: row.get(2),
        description: row.get(3),
        icon: row.get(4),
        color: row.get(5),
        slack_webhook_url: row.get(6),
        is_active: row.get(7),
        created_at: row.get(8),
        member_count: row.get(9),
        project_count: row.get(10),
    });

    Ok(team)
}

/// Djangoの `django.utils.text.slugify(value)`(allow_unicode=Falseの既定動作)相当。
/// categories等とは異なりteamsはASCII限定slugify: 非ASCII文字(日本語など)は削除される。
fn generate_slug(input: &str) -> String {
    let mut result = String::new();
    for c in input.chars() {
        if c.is_ascii_alphanumeric() {
            result.push(c.to_ascii_lowercase());
        } else if (c.is_ascii_whitespace() || c == '-' || c == '_')
            && !result.is_empty()
            && !result.ends_with('-')
        {
            result.push('-');
        }
        // 非ASCII文字(日本語など)はDjangoのASCII限定slugifyと同様に削除する
    }
    let slug = result.trim_matches('-').to_string();

    if slug.is_empty() {
        "team".to_string()
    } else {
        slug
    }
}

pub async fn create_team(pool: &PgPool, input: &TeamWriteIn) -> anyhow::Result<i32> {
    // Slug 生成（入力が空ならname から自動生成）
    let mut slug = if input.slug.is_empty() {
        generate_slug(&input.name)
    } else {
        input.slug.clone()
    };

    // 重複チェック＆連番付与
    let mut counter = 1;
    loop {
        let existing_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM m_team WHERE slug = $1")
            .bind(&slug)
            .fetch_one(pool)
            .await?;

        if existing_count == 0 {
            break;
        }

        let base = if input.slug.is_empty() {
            generate_slug(&input.name)
        } else {
            input.slug.clone()
        };
        slug = format!("{}-{}", base, counter);
        counter += 1;
    }

    let team_id: i32 = sqlx::query_scalar(
        "INSERT INTO m_team (name, slug, description, icon, color, slack_webhook_url, is_active, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())
         RETURNING id::int4"
    )
    .bind(&input.name)
    .bind(&slug)
    .bind(&input.description)
    .bind(&input.icon)
    .bind(&input.color)
    .bind(&input.slack_webhook_url)
    .bind(input.is_active)
    .fetch_one(pool)
    .await?;

    Ok(team_id)
}

pub async fn update_team(pool: &PgPool, id: i32, input: &TeamWriteIn) -> anyhow::Result<bool> {
    let rows_affected = sqlx::query(
        "UPDATE m_team
         SET name = $1, slug = $2, description = $3, icon = $4, color = $5, slack_webhook_url = $6, is_active = $7
         WHERE id = $8"
    )
    .bind(&input.name)
    .bind(&input.slug)
    .bind(&input.description)
    .bind(&input.icon)
    .bind(&input.color)
    .bind(&input.slack_webhook_url)
    .bind(input.is_active)
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected();

    Ok(rows_affected > 0)
}

pub async fn delete_team(pool: &PgPool, id: i32) -> anyhow::Result<bool> {
    let mut tx = pool.begin().await?;

    // m_team_rule は on_delete=CASCADE、そのチームルールを参照するチケットの
    // linked_rules(M2M)も先に削除する必要がある
    sqlx::query(
        "DELETE FROM tickets_ticket_linked_rules WHERE teamrulemodel_id IN
         (SELECT id FROM m_team_rule WHERE team_id = $1)"
    ).bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM m_team_rule WHERE team_id = $1")
        .bind(id).execute(&mut *tx).await?;

    // t_team_membership は on_delete=CASCADE
    sqlx::query("DELETE FROM t_team_membership WHERE team_id = $1")
        .bind(id).execute(&mut *tx).await?;

    // tickets_project.owner_team / tickets_ticket.assigned_team は on_delete=SET_NULL
    sqlx::query("UPDATE tickets_project SET owner_team_id = NULL WHERE owner_team_id = $1")
        .bind(id).execute(&mut *tx).await?;
    sqlx::query("UPDATE tickets_ticket SET assigned_team_id = NULL WHERE assigned_team_id = $1")
        .bind(id).execute(&mut *tx).await?;

    let rows_affected = sqlx::query("DELETE FROM m_team WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();

    tx.commit().await?;
    Ok(rows_affected > 0)
}

// =============================================================================
// Team Memberships
// =============================================================================

pub async fn find_team_members(pool: &PgPool, team_id: i32) -> anyhow::Result<Vec<TeamMembershipOut>> {
    let rows = sqlx::query(
        "SELECT
            tm.id::int4, tm.team_id::int4, tm.user_id::int4, tm.role, tm.joined_at,
            u.id::int4 as user_id, u.username, u.email, u.display_name
         FROM t_team_membership tm
         JOIN accounts_user u ON tm.user_id = u.id
         WHERE tm.team_id = $1
         ORDER BY u.username ASC"
    )
    .bind(team_id)
    .fetch_all(pool)
    .await?;

    let members = rows
        .into_iter()
        .map(|row| {
            let user = UserSummaryOut {
                id: row.get(5),
                username: row.get(6),
                email: row.get(7),
                display_name: row.get(8),
            };
            TeamMembershipOut {
                id: row.get(0),
                team: row.get(1),
                user,
                role: row.get(3),
                joined_at: row.get(4),
            }
        })
        .collect();

    Ok(members)
}

pub async fn check_user_exists(pool: &PgPool, user_id: i32) -> anyhow::Result<bool> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM accounts_user WHERE id = $1")
        .bind(user_id)
        .fetch_one(pool)
        .await?;
    Ok(count > 0)
}

pub async fn check_team_membership_exists(
    pool: &PgPool,
    team_id: i32,
    user_id: i32,
) -> anyhow::Result<bool> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM t_team_membership WHERE team_id = $1 AND user_id = $2"
    )
    .bind(team_id)
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}

pub async fn add_team_member(
    pool: &PgPool,
    team_id: i32,
    user_id: i32,
    role: &str,
) -> anyhow::Result<i32> {
    let membership_id: i32 = sqlx::query_scalar(
        "INSERT INTO t_team_membership (team_id, user_id, role, joined_at)
         VALUES ($1, $2, $3, NOW())
         RETURNING id::int4"
    )
    .bind(team_id)
    .bind(user_id)
    .bind(role)
    .fetch_one(pool)
    .await?;

    Ok(membership_id)
}

pub async fn remove_team_member(pool: &PgPool, team_id: i32, user_id: i32) -> anyhow::Result<bool> {
    let rows_affected = sqlx::query(
        "DELETE FROM t_team_membership WHERE team_id = $1 AND user_id = $2"
    )
    .bind(team_id)
    .bind(user_id)
    .execute(pool)
    .await?
    .rows_affected();

    Ok(rows_affected > 0)
}

pub async fn get_team_member_by_id(
    pool: &PgPool,
    membership_id: i32,
) -> anyhow::Result<Option<TeamMembershipOut>> {
    let row_opt = sqlx::query(
        "SELECT
            tm.id::int4, tm.team_id::int4, tm.user_id::int4, tm.role, tm.joined_at,
            u.id::int4 as user_id, u.username, u.email, u.display_name
         FROM t_team_membership tm
         JOIN accounts_user u ON tm.user_id = u.id
         WHERE tm.id = $1"
    )
    .bind(membership_id)
    .fetch_optional(pool)
    .await?;

    let member = row_opt.map(|row| {
        let user = UserSummaryOut {
            id: row.get(5),
            username: row.get(6),
            email: row.get(7),
            display_name: row.get(8),
        };
        TeamMembershipOut {
            id: row.get(0),
            team: row.get(1),
            user,
            role: row.get(3),
            joined_at: row.get(4),
        }
    });

    Ok(member)
}
