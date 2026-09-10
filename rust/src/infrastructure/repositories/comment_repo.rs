/// infrastructure/repositories/comment_repo.rs — コメント永続化

use sqlx::PgPool;
use crate::domain::models::comment::Comment;

pub async fn find_by_ticket(pool: &PgPool, ticket_id: i32) -> anyhow::Result<Vec<Comment>> {
    let rows = sqlx::query_as::<_, Comment>(
        "SELECT c.id::int4, c.ticket_id::int4, c.author_id::int4, c.body, c.created_at,
                u.display_name as author_name, c.anchor_start, c.anchor_end, c.anchor_quote
         FROM tickets_comment c
         LEFT JOIN accounts_user u ON c.author_id = u.id
         WHERE c.ticket_id = $1
         ORDER BY c.created_at"
    ).bind(ticket_id).fetch_all(pool).await?;
    Ok(rows)
}

pub async fn create(pool: &PgPool, ticket_id: i32, author_id: i32, body: &str) -> anyhow::Result<i32> {
    let id = sqlx::query_scalar::<_, i32>(
        "INSERT INTO tickets_comment (ticket_id, author_id, body, created_at) VALUES ($1, $2, $3, NOW()) RETURNING id::int4"
    ).bind(ticket_id).bind(author_id).bind(body).fetch_one(pool).await?;
    Ok(id)
}

pub async fn delete(pool: &PgPool, id: i32) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM tickets_comment WHERE id=$1").bind(id).execute(pool).await?;
    Ok(())
}
