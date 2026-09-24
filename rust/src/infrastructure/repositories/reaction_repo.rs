/// infrastructure/repositories/reaction_repo.rs — リアクション & カスタム絵文字永続化

use sqlx::PgPool;
use crate::domain::models::reaction::{ReactionRow, CustomEmojiRow};

// === Reaction ===

pub async fn find_by_ticket(pool: &PgPool, ticket_id: i64) -> anyhow::Result<Vec<(ReactionRow, i32, String, String, String)>> {
    let rows = sqlx::query_as::<_, (i64, i64, i64, String, String, sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>, sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>, i32, String, String, String)>(
        "SELECT r.id, r.ticket_id, r.user_id, r.emoji_kind, r.emoji_value, r.created_at, r.updated_at,
                u.id::int4, u.username, u.email, u.display_name
         FROM tickets_reaction r
         JOIN accounts_user u ON r.user_id = u.id
         WHERE r.ticket_id = $1
         ORDER BY r.created_at"
    ).bind(ticket_id).fetch_all(pool).await?;

    Ok(rows.into_iter().map(|(id, ticket_id, user_id, emoji_kind, emoji_value, created_at, updated_at, uid, username, email, display_name)| {
        (
            ReactionRow { id, ticket_id, user_id, emoji_kind, emoji_value, created_at, updated_at },
            uid,
            username,
            email,
            display_name
        )
    }).collect())
}

pub async fn create(
    pool: &PgPool,
    ticket_id: i64,
    user_id: i64,
    emoji_kind: &str,
    emoji_value: &str,
) -> anyhow::Result<i64> {
    let id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO tickets_reaction (ticket_id, user_id, emoji_kind, emoji_value, created_at, updated_at)
         VALUES ($1, $2, $3, $4, NOW(), NOW())
         RETURNING id"
    )
    .bind(ticket_id)
    .bind(user_id)
    .bind(emoji_kind)
    .bind(emoji_value)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub async fn find_by_ticket_user_emoji(
    pool: &PgPool,
    ticket_id: i64,
    user_id: i64,
    emoji_kind: &str,
    emoji_value: &str,
) -> anyhow::Result<Option<(ReactionRow, i32, String, String, String)>> {
    let row = sqlx::query_as::<_, (i64, i64, i64, String, String, sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>, sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>, i32, String, String, String)>(
        "SELECT r.id, r.ticket_id, r.user_id, r.emoji_kind, r.emoji_value, r.created_at, r.updated_at,
                u.id::int4, u.username, u.email, u.display_name
         FROM tickets_reaction r
         JOIN accounts_user u ON r.user_id = u.id
         WHERE r.ticket_id = $1 AND r.user_id = $2 AND r.emoji_kind = $3 AND r.emoji_value = $4"
    )
    .bind(ticket_id)
    .bind(user_id)
    .bind(emoji_kind)
    .bind(emoji_value)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|(id, ticket_id, user_id, emoji_kind, emoji_value, created_at, updated_at, uid, username, email, display_name)| {
        (
            ReactionRow { id, ticket_id, user_id, emoji_kind, emoji_value, created_at, updated_at },
            uid,
            username,
            email,
            display_name
        )
    }))
}

pub async fn delete(pool: &PgPool, id: i64) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM tickets_reaction WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn find_by_id(pool: &PgPool, id: i64) -> anyhow::Result<Option<(ReactionRow, i32, String, String, String)>> {
    let row = sqlx::query_as::<_, (i64, i64, i64, String, String, sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>, sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>, i32, String, String, String)>(
        "SELECT r.id, r.ticket_id, r.user_id, r.emoji_kind, r.emoji_value, r.created_at, r.updated_at,
                u.id::int4, u.username, u.email, u.display_name
         FROM tickets_reaction r
         JOIN accounts_user u ON r.user_id = u.id
         WHERE r.id = $1"
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|(id, ticket_id, user_id, emoji_kind, emoji_value, created_at, updated_at, uid, username, email, display_name)| {
        (
            ReactionRow { id, ticket_id, user_id, emoji_kind, emoji_value, created_at, updated_at },
            uid,
            username,
            email,
            display_name
        )
    }))
}

// === Custom Emoji ===

pub async fn find_emoji_by_project(pool: &PgPool, project_id: i64) -> anyhow::Result<Vec<(CustomEmojiRow, i32, String, String, String)>> {
    let rows = sqlx::query_as::<_, (i64, i64, String, String, String, i64, sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>, i32, String, String, String)>(
        "SELECT e.id, e.project_id, e.slug, e.name, e.image_path, e.uploaded_by, e.created_at,
                u.id::int4, u.username, u.email, u.display_name
         FROM tickets_custom_emoji e
         JOIN accounts_user u ON e.uploaded_by = u.id
         WHERE e.project_id = $1
         ORDER BY e.created_at"
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|(id, project_id, slug, name, image_path, uploaded_by, created_at, uid, username, email, display_name)| {
        (
            CustomEmojiRow { id, project_id, slug, name, image_path, uploaded_by, created_at },
            uid,
            username,
            email,
            display_name
        )
    }).collect())
}

pub async fn create_emoji(
    pool: &PgPool,
    project_id: i64,
    slug: &str,
    name: &str,
    image_path: &str,
    uploaded_by: i64,
) -> anyhow::Result<i64> {
    let id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO tickets_custom_emoji (project_id, slug, name, image_path, uploaded_by, created_at)
         VALUES ($1, $2, $3, $4, $5, NOW())
         RETURNING id"
    )
    .bind(project_id)
    .bind(slug)
    .bind(name)
    .bind(image_path)
    .bind(uploaded_by)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub async fn delete_emoji(pool: &PgPool, id: i64) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM tickets_custom_emoji WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn find_emoji_by_id(pool: &PgPool, id: i64) -> anyhow::Result<Option<(CustomEmojiRow, i32, String, String, String)>> {
    let row = sqlx::query_as::<_, (i64, i64, String, String, String, i64, sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>, i32, String, String, String)>(
        "SELECT e.id, e.project_id, e.slug, e.name, e.image_path, e.uploaded_by, e.created_at,
                u.id::int4, u.username, u.email, u.display_name
         FROM tickets_custom_emoji e
         JOIN accounts_user u ON e.uploaded_by = u.id
         WHERE e.id = $1"
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|(id, project_id, slug, name, image_path, uploaded_by, created_at, uid, username, email, display_name)| {
        (
            CustomEmojiRow { id, project_id, slug, name, image_path, uploaded_by, created_at },
            uid,
            username,
            email,
            display_name
        )
    }))
}

pub async fn find_emoji_by_project_slug(pool: &PgPool, project_id: i64, slug: &str) -> anyhow::Result<Option<i64>> {
    let id = sqlx::query_scalar::<_, i64>(
        "SELECT id FROM tickets_custom_emoji WHERE project_id = $1 AND slug = $2"
    )
    .bind(project_id)
    .bind(slug)
    .fetch_optional(pool)
    .await?;
    Ok(id)
}
