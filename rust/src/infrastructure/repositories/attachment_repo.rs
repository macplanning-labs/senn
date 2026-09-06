/// infrastructure/repositories/attachment_repo.rs — 添付ファイル永続化

use sqlx::PgPool;
use crate::domain::models::attachment::Attachment;

pub async fn find_by_ticket(pool: &PgPool, ticket_id: i32) -> anyhow::Result<Vec<Attachment>> {
    let rows = sqlx::query_as::<_, Attachment>(
        "SELECT a.id::int4, a.ticket_id::int4, a.comment_id::int4, a.uploader_id::int4, a.filename,
                a.file AS file_path, a.file_size, a.created_at,
                u.display_name as uploader_name
         FROM tickets_attachment a
         LEFT JOIN accounts_user u ON a.uploader_id = u.id
         WHERE a.ticket_id = $1
         ORDER BY a.created_at"
    ).bind(ticket_id).fetch_all(pool).await?;
    Ok(rows)
}

pub async fn create(
    pool: &PgPool, ticket_id: i32, comment_id: Option<i32>,
    uploader_id: i32, filename: &str, file_path: &str, file_size: i32,
) -> anyhow::Result<i32> {
    let id = sqlx::query_scalar::<_, i32>(
        "INSERT INTO tickets_attachment (ticket_id, comment_id, uploader_id, filename, file, file_size, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, NOW()) RETURNING id::int4"
    ).bind(ticket_id).bind(comment_id).bind(uploader_id)
     .bind(filename).bind(file_path).bind(file_size)
     .fetch_one(pool).await?;
    Ok(id)
}

pub async fn delete(pool: &PgPool, id: i32) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM tickets_attachment WHERE id=$1").bind(id).execute(pool).await?;
    Ok(())
}

pub async fn find_by_id(pool: &PgPool, id: i32) -> anyhow::Result<Option<Attachment>> {
    let row = sqlx::query_as::<_, Attachment>(
        "SELECT a.id::int4, a.ticket_id::int4, a.comment_id::int4, a.uploader_id::int4, a.filename,
                a.file AS file_path, a.file_size, a.created_at,
                u.display_name as uploader_name
         FROM tickets_attachment a
         LEFT JOIN accounts_user u ON a.uploader_id = u.id
         WHERE a.id = $1"
    ).bind(id).fetch_optional(pool).await?;
    Ok(row)
}
