/// infrastructure/repositories/ticket_link_repo.rs — チケット参照リンク永続化

use sqlx::PgPool;
use crate::domain::models::ticket_link::TicketLink;

pub async fn find_by_ticket(pool: &PgPool, ticket_id: i32) -> anyhow::Result<Vec<TicketLink>> {
    let rows = sqlx::query_as::<_, TicketLink>(
        "SELECT tl.id::int4, tl.ticket_id::int4, tl.url, tl.title, tl.created_by_id::int4, tl.created_at,
                u.display_name as created_by_name
         FROM ticket_link tl
         LEFT JOIN accounts_user u ON tl.created_by_id = u.id
         WHERE tl.ticket_id = $1
         ORDER BY tl.created_at"
    ).bind(ticket_id).fetch_all(pool).await?;
    Ok(rows)
}

pub async fn create(
    pool: &PgPool, ticket_id: i32, url: &str, title: Option<&str>, created_by_id: i32,
) -> anyhow::Result<i32> {
    let id = sqlx::query_scalar::<_, i32>(
        "INSERT INTO ticket_link (ticket_id, url, title, created_by_id, created_at)
         VALUES ($1, $2, $3, $4, NOW()) RETURNING id::int4"
    ).bind(ticket_id).bind(url).bind(title).bind(created_by_id)
     .fetch_one(pool).await?;
    Ok(id)
}

pub async fn delete(pool: &PgPool, id: i32, ticket_id: i32) -> anyhow::Result<bool> {
    let result = sqlx::query("DELETE FROM ticket_link WHERE id = $1 AND ticket_id = $2")
        .bind(id).bind(ticket_id).execute(pool).await?;
    Ok(result.rows_affected() > 0)
}

pub async fn find_by_id(pool: &PgPool, id: i32) -> anyhow::Result<Option<TicketLink>> {
    let row = sqlx::query_as::<_, TicketLink>(
        "SELECT tl.id::int4, tl.ticket_id::int4, tl.url, tl.title, tl.created_by_id::int4, tl.created_at,
                u.display_name as created_by_name
         FROM ticket_link tl
         LEFT JOIN accounts_user u ON tl.created_by_id = u.id
         WHERE tl.id = $1"
    ).bind(id).fetch_optional(pool).await?;
    Ok(row)
}
