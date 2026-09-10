/// infrastructure/repositories/wiki_attachment_repo.rs — Wiki添付ファイル永続化

use sqlx::PgPool;
use crate::domain::models::wiki_attachment::WikiAttachment;

pub async fn find_by_wiki_page(pool: &PgPool, wiki_page_id: i32) -> anyhow::Result<Vec<WikiAttachment>> {
    let rows = sqlx::query_as::<_, WikiAttachment>(
        "SELECT wa.id::int4, wa.wiki_page_id::int4, wa.filename,
                wa.file AS file_path, wa.file_size, wa.uploader_id::int4, wa.created_at,
                u.display_name as uploader_name
         FROM wiki_attachment wa
         LEFT JOIN accounts_user u ON wa.uploader_id = u.id
         WHERE wa.wiki_page_id = $1
         ORDER BY wa.created_at DESC"
    ).bind(wiki_page_id).fetch_all(pool).await?;
    Ok(rows)
}

pub async fn create(
    pool: &PgPool, wiki_page_id: i32, uploader_id: i32,
    filename: &str, file_path: &str, file_size: i32,
) -> anyhow::Result<i32> {
    let id = sqlx::query_scalar::<_, i32>(
        "INSERT INTO wiki_attachment (wiki_page_id, uploader_id, filename, file, file_size, created_at)
         VALUES ($1, $2, $3, $4, $5, NOW()) RETURNING id::int4"
    ).bind(wiki_page_id).bind(uploader_id).bind(filename)
     .bind(file_path).bind(file_size)
     .fetch_one(pool).await?;
    Ok(id)
}

pub async fn find_by_id(pool: &PgPool, id: i32) -> anyhow::Result<Option<WikiAttachment>> {
    let row = sqlx::query_as::<_, WikiAttachment>(
        "SELECT wa.id::int4, wa.wiki_page_id::int4, wa.filename,
                wa.file AS file_path, wa.file_size, wa.uploader_id::int4, wa.created_at,
                u.display_name as uploader_name
         FROM wiki_attachment wa
         LEFT JOIN accounts_user u ON wa.uploader_id = u.id
         WHERE wa.id = $1"
    ).bind(id).fetch_optional(pool).await?;
    Ok(row)
}

pub async fn update(
    pool: &PgPool, id: i32, filename: &str, file_path: &str, file_size: i32,
) -> anyhow::Result<bool> {
    let result = sqlx::query(
        "UPDATE wiki_attachment SET filename = $1, file = $2, file_size = $3 WHERE id = $4"
    ).bind(filename).bind(file_path).bind(file_size).bind(id)
     .execute(pool).await?;
    Ok(result.rows_affected() > 0)
}

pub async fn delete(pool: &PgPool, id: i32) -> anyhow::Result<bool> {
    let result = sqlx::query("DELETE FROM wiki_attachment WHERE id = $1")
        .bind(id).execute(pool).await?;
    Ok(result.rows_affected() > 0)
}
