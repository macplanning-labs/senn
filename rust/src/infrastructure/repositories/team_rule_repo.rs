/// infrastructure/repositories/team_rule_repo.rs — Team Rules 永続化
///
/// m_team_rule テーブルの CRUD 操作。

use sqlx::{PgPool, Row};

use crate::domain::models::team_rule_api::*;
use crate::domain::models::ticket_api::UserSummaryOut;

pub async fn find_all_team_rules(
    pool: &PgPool,
    page: i64,
    team: Option<i32>,
    category: Option<String>,
    is_active: Option<bool>,
) -> anyhow::Result<Vec<TeamRuleOut>> {
    const PAGE_SIZE: i64 = 50;
    let page = page.max(1);
    let offset = (page - 1) * PAGE_SIZE;

    let rows = if team.is_none() && category.is_none() && is_active.is_none() {
        sqlx::query(
            "SELECT
                r.id::int4, r.title, r.content, r.category, r.sort_order, r.is_active,
                r.created_at, r.updated_at, r.team_id::int4, r.created_by_id::int4,
                t.name as team_name,
                u.id::int4 as user_id, u.username, u.email, u.display_name
             FROM m_team_rule r
             LEFT JOIN m_team t ON r.team_id = t.id
             LEFT JOIN accounts_user u ON r.created_by_id = u.id
             ORDER BY r.sort_order ASC, r.id ASC
             LIMIT $1 OFFSET $2"
        )
        .bind(PAGE_SIZE)
        .bind(offset)
        .fetch_all(pool)
        .await?
    } else if team.is_some() && category.is_none() && is_active.is_none() {
        sqlx::query(
            "SELECT
                r.id::int4, r.title, r.content, r.category, r.sort_order, r.is_active,
                r.created_at, r.updated_at, r.team_id::int4, r.created_by_id::int4,
                t.name as team_name,
                u.id::int4 as user_id, u.username, u.email, u.display_name
             FROM m_team_rule r
             LEFT JOIN m_team t ON r.team_id = t.id
             LEFT JOIN accounts_user u ON r.created_by_id = u.id
             WHERE r.team_id = $1
             ORDER BY r.sort_order ASC, r.id ASC
             LIMIT $2 OFFSET $3"
        )
        .bind(team.unwrap() as i64)
        .bind(PAGE_SIZE)
        .bind(offset)
        .fetch_all(pool)
        .await?
    } else if team.is_none() && category.is_some() && is_active.is_none() {
        sqlx::query(
            "SELECT
                r.id::int4, r.title, r.content, r.category, r.sort_order, r.is_active,
                r.created_at, r.updated_at, r.team_id::int4, r.created_by_id::int4,
                t.name as team_name,
                u.id::int4 as user_id, u.username, u.email, u.display_name
             FROM m_team_rule r
             LEFT JOIN m_team t ON r.team_id = t.id
             LEFT JOIN accounts_user u ON r.created_by_id = u.id
             WHERE r.category = $1
             ORDER BY r.sort_order ASC, r.id ASC
             LIMIT $2 OFFSET $3"
        )
        .bind(&category.unwrap())
        .bind(PAGE_SIZE)
        .bind(offset)
        .fetch_all(pool)
        .await?
    } else if team.is_none() && category.is_none() && is_active.is_some() {
        sqlx::query(
            "SELECT
                r.id::int4, r.title, r.content, r.category, r.sort_order, r.is_active,
                r.created_at, r.updated_at, r.team_id::int4, r.created_by_id::int4,
                t.name as team_name,
                u.id::int4 as user_id, u.username, u.email, u.display_name
             FROM m_team_rule r
             LEFT JOIN m_team t ON r.team_id = t.id
             LEFT JOIN accounts_user u ON r.created_by_id = u.id
             WHERE r.is_active = $1
             ORDER BY r.sort_order ASC, r.id ASC
             LIMIT $2 OFFSET $3"
        )
        .bind(is_active.unwrap())
        .bind(PAGE_SIZE)
        .bind(offset)
        .fetch_all(pool)
        .await?
    } else if team.is_some() && category.is_some() && is_active.is_none() {
        sqlx::query(
            "SELECT
                r.id::int4, r.title, r.content, r.category, r.sort_order, r.is_active,
                r.created_at, r.updated_at, r.team_id::int4, r.created_by_id::int4,
                t.name as team_name,
                u.id::int4 as user_id, u.username, u.email, u.display_name
             FROM m_team_rule r
             LEFT JOIN m_team t ON r.team_id = t.id
             LEFT JOIN accounts_user u ON r.created_by_id = u.id
             WHERE r.team_id = $1 AND r.category = $2
             ORDER BY r.sort_order ASC, r.id ASC
             LIMIT $3 OFFSET $4"
        )
        .bind(team.unwrap() as i64)
        .bind(&category.unwrap())
        .bind(PAGE_SIZE)
        .bind(offset)
        .fetch_all(pool)
        .await?
    } else if team.is_some() && category.is_none() && is_active.is_some() {
        sqlx::query(
            "SELECT
                r.id::int4, r.title, r.content, r.category, r.sort_order, r.is_active,
                r.created_at, r.updated_at, r.team_id::int4, r.created_by_id::int4,
                t.name as team_name,
                u.id::int4 as user_id, u.username, u.email, u.display_name
             FROM m_team_rule r
             LEFT JOIN m_team t ON r.team_id = t.id
             LEFT JOIN accounts_user u ON r.created_by_id = u.id
             WHERE r.team_id = $1 AND r.is_active = $2
             ORDER BY r.sort_order ASC, r.id ASC
             LIMIT $3 OFFSET $4"
        )
        .bind(team.unwrap() as i64)
        .bind(is_active.unwrap())
        .bind(PAGE_SIZE)
        .bind(offset)
        .fetch_all(pool)
        .await?
    } else if team.is_none() && category.is_some() && is_active.is_some() {
        sqlx::query(
            "SELECT
                r.id::int4, r.title, r.content, r.category, r.sort_order, r.is_active,
                r.created_at, r.updated_at, r.team_id::int4, r.created_by_id::int4,
                t.name as team_name,
                u.id::int4 as user_id, u.username, u.email, u.display_name
             FROM m_team_rule r
             LEFT JOIN m_team t ON r.team_id = t.id
             LEFT JOIN accounts_user u ON r.created_by_id = u.id
             WHERE r.category = $1 AND r.is_active = $2
             ORDER BY r.sort_order ASC, r.id ASC
             LIMIT $3 OFFSET $4"
        )
        .bind(&category.unwrap())
        .bind(is_active.unwrap())
        .bind(PAGE_SIZE)
        .bind(offset)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            "SELECT
                r.id::int4, r.title, r.content, r.category, r.sort_order, r.is_active,
                r.created_at, r.updated_at, r.team_id::int4, r.created_by_id::int4,
                t.name as team_name,
                u.id::int4 as user_id, u.username, u.email, u.display_name
             FROM m_team_rule r
             LEFT JOIN m_team t ON r.team_id = t.id
             LEFT JOIN accounts_user u ON r.created_by_id = u.id
             WHERE r.team_id = $1 AND r.category = $2 AND r.is_active = $3
             ORDER BY r.sort_order ASC, r.id ASC
             LIMIT $4 OFFSET $5"
        )
        .bind(team.unwrap() as i64)
        .bind(&category.unwrap())
        .bind(is_active.unwrap())
        .bind(PAGE_SIZE)
        .bind(offset)
        .fetch_all(pool)
        .await?
    };

    let rules = rows
        .into_iter()
        .map(|row| {
            let created_by_id: Option<i32> = row.get(9);
            let created_by = created_by_id.map(|_| UserSummaryOut {
                id: row.get(11),
                username: row.get(12),
                email: row.get(13),
                display_name: row.get(14),
            });

            TeamRuleOut {
                id: row.get(0),
                team: row.get(8),
                team_name: row.get(10),
                title: row.get(1),
                content: row.get(2),
                category: row.get(3),
                sort_order: row.get(4),
                is_active: row.get(5),
                created_by,
                created_at: row.get(6),
                updated_at: row.get(7),
            }
        })
        .collect();

    Ok(rules)
}

pub async fn count_team_rules(
    pool: &PgPool,
    team: Option<i32>,
    category: Option<String>,
    is_active: Option<bool>,
) -> anyhow::Result<i64> {
    let count: i64 = if team.is_none() && category.is_none() && is_active.is_none() {
        sqlx::query_scalar("SELECT COUNT(*) FROM m_team_rule")
            .fetch_one(pool)
            .await?
    } else if team.is_some() && category.is_none() && is_active.is_none() {
        sqlx::query_scalar("SELECT COUNT(*) FROM m_team_rule WHERE team_id = $1")
            .bind(team.unwrap() as i64)
            .fetch_one(pool)
            .await?
    } else if team.is_none() && category.is_some() && is_active.is_none() {
        sqlx::query_scalar("SELECT COUNT(*) FROM m_team_rule WHERE category = $1")
            .bind(&category.unwrap())
            .fetch_one(pool)
            .await?
    } else if team.is_none() && category.is_none() && is_active.is_some() {
        sqlx::query_scalar("SELECT COUNT(*) FROM m_team_rule WHERE is_active = $1")
            .bind(is_active.unwrap())
            .fetch_one(pool)
            .await?
    } else if team.is_some() && category.is_some() && is_active.is_none() {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM m_team_rule WHERE team_id = $1 AND category = $2"
        )
        .bind(team.unwrap() as i64)
        .bind(&category.unwrap())
        .fetch_one(pool)
        .await?
    } else if team.is_some() && category.is_none() && is_active.is_some() {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM m_team_rule WHERE team_id = $1 AND is_active = $2"
        )
        .bind(team.unwrap() as i64)
        .bind(is_active.unwrap())
        .fetch_one(pool)
        .await?
    } else if team.is_none() && category.is_some() && is_active.is_some() {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM m_team_rule WHERE category = $1 AND is_active = $2"
        )
        .bind(&category.unwrap())
        .bind(is_active.unwrap())
        .fetch_one(pool)
        .await?
    } else {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM m_team_rule WHERE team_id = $1 AND category = $2 AND is_active = $3"
        )
        .bind(team.unwrap() as i64)
        .bind(&category.unwrap())
        .bind(is_active.unwrap())
        .fetch_one(pool)
        .await?
    };

    Ok(count)
}

pub async fn find_team_rule_by_id(pool: &PgPool, id: i32) -> anyhow::Result<Option<TeamRuleOut>> {
    let row_opt = sqlx::query(
        "SELECT
            r.id::int4, r.title, r.content, r.category, r.sort_order, r.is_active,
            r.created_at, r.updated_at, r.team_id::int4, r.created_by_id::int4,
            t.name as team_name,
            u.id::int4 as user_id, u.username, u.email, u.display_name
         FROM m_team_rule r
         LEFT JOIN m_team t ON r.team_id = t.id
         LEFT JOIN accounts_user u ON r.created_by_id = u.id
         WHERE r.id = $1"
    )
    .bind(id as i64)
    .fetch_optional(pool)
    .await?;

    let rule = row_opt.map(|row| {
        let created_by_id: Option<i32> = row.get(9);
        let created_by = created_by_id.map(|_| UserSummaryOut {
            id: row.get(11),
            username: row.get(12),
            email: row.get(13),
            display_name: row.get(14),
        });

        TeamRuleOut {
            id: row.get(0),
            team: row.get(8),
            team_name: row.get(10),
            title: row.get(1),
            content: row.get(2),
            category: row.get(3),
            sort_order: row.get(4),
            is_active: row.get(5),
            created_by,
            created_at: row.get(6),
            updated_at: row.get(7),
        }
    });

    Ok(rule)
}

pub async fn create_team_rule(
    pool: &PgPool,
    input: &TeamRuleWriteIn,
    created_by_id: i32,
) -> anyhow::Result<i32> {
    let rule_id: i32 = sqlx::query_scalar(
        "INSERT INTO m_team_rule (title, content, category, sort_order, is_active, created_by_id, team_id, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())
         RETURNING id::int4"
    )
    .bind(&input.title)
    .bind(&input.content)
    .bind(&input.category)
    .bind(input.sort_order)
    .bind(input.is_active)
    .bind(created_by_id as i64)
    .bind(input.team.map(|t| t as i64))
    .fetch_one(pool)
    .await?;

    Ok(rule_id)
}

pub async fn partial_update_team_rule(
    pool: &PgPool,
    id: i32,
    input: &crate::domain::models::team_rule_api::TeamRuleUpdateIn,
) -> anyhow::Result<bool> {
    // 既存のルールを取得（created_by を保持するため）
    let existing = find_team_rule_by_id(pool, id).await?;
    if existing.is_none() {
        return Ok(false);
    }

    let existing = existing.unwrap();

    // 更新値を決定（指定されない場合は既存値を使用）
    let title = input.title.as_ref().unwrap_or(&existing.title).clone();
    let content = input.content.as_ref().unwrap_or(&existing.content).clone();
    let category = input.category.as_ref().unwrap_or(&existing.category).clone();
    let sort_order = input.sort_order.unwrap_or(existing.sort_order);
    let is_active = input.is_active.unwrap_or(existing.is_active);
    let team = input.team.or(existing.team);

    let rows_affected = sqlx::query(
        "UPDATE m_team_rule
         SET title = $1, content = $2, category = $3, sort_order = $4, is_active = $5, team_id = $6, updated_at = NOW()
         WHERE id = $7"
    )
    .bind(&title)
    .bind(&content)
    .bind(&category)
    .bind(sort_order)
    .bind(is_active)
    .bind(team.map(|t| t as i64))
    .bind(id as i64)
    .execute(pool)
    .await?
    .rows_affected();

    Ok(rows_affected > 0)
}

pub async fn delete_team_rule(pool: &PgPool, id: i32) -> anyhow::Result<bool> {
    let mut tx = pool.begin().await?;

    // tickets_ticket_linked_rules はM2M中間テーブル(DjangoのManyToManyField削除はjoin行を自動除去)
    sqlx::query("DELETE FROM tickets_ticket_linked_rules WHERE teamrulemodel_id = $1")
        .bind(id as i64).execute(&mut *tx).await?;

    let rows_affected = sqlx::query("DELETE FROM m_team_rule WHERE id = $1")
        .bind(id as i64)
        .execute(&mut *tx)
        .await?
        .rows_affected();

    tx.commit().await?;
    Ok(rows_affected > 0)
}
