/// infrastructure/repositories/comment_repo.rs — コメント永続化

use sqlx::PgPool;
use crate::domain::models::comment::Comment;

pub async fn find_by_ticket(pool: &PgPool, ticket_id: i32) -> anyhow::Result<Vec<Comment>> {
    let rows = sqlx::query_as::<_, Comment>(
        "SELECT c.id, c.ticket_id, c.author_id, c.body, c.created_at,
                u.display_name as author_name
         FROM t_comments c
         LEFT JOIN m_users u ON c.author_id = u.id
         WHERE c.ticket_id = $1
         ORDER BY c.created_at"
    ).bind(ticket_id).fetch_all(pool).await?;
    Ok(rows)
}

pub async fn create(pool: &PgPool, ticket_id: i32, author_id: i32, body: &str) -> anyhow::Result<i32> {
    let id = sqlx::query_scalar::<_, i32>(
        "INSERT INTO t_comments (ticket_id, author_id, body) VALUES ($1, $2, $3) RETURNING id"
    ).bind(ticket_id).bind(author_id).bind(body).fetch_one(pool).await?;
    Ok(id)
}

pub async fn delete(pool: &PgPool, id: i32) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM t_comments WHERE id=$1").bind(id).execute(pool).await?;
    Ok(())
}
