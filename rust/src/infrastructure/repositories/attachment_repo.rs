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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;

    async fn create_test_comment(pool: &PgPool, ticket_id: i32, author_id: i32) -> i32 {
        sqlx::query_scalar::<_, i32>(
            "INSERT INTO tickets_comment (body, author_id, ticket_id, created_at)
             VALUES ('attachment test', $1, $2, NOW())
             RETURNING id::int4",
        )
        .bind(author_id)
        .bind(ticket_id)
        .fetch_one(pool)
        .await
        .expect("テストコメント作成に失敗")
    }

    #[tokio::test]
    async fn test_create_attachment_without_comment_id() {
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let author = test_support::create_test_user(&pool, "att-none").await;
        let project = test_support::create_test_project(&pool, "ATT", author).await;
        let ticket_id = test_support::create_test_ticket(&pool, project, "ATT-N", author).await;

        let att_id = create(
            &pool,
            ticket_id,
            None,
            author,
            "test.txt",
            "path/test.txt",
            1024,
        )
        .await
        .expect("Failed to create attachment without comment_id");

        let att = find_by_id(&pool, att_id)
            .await
            .expect("Failed to find attachment")
            .expect("Attachment not found");

        assert_eq!(att.comment_id, None);
    }

    #[tokio::test]
    async fn test_create_attachment_with_comment_id() {
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let author = test_support::create_test_user(&pool, "att-cid").await;
        let project = test_support::create_test_project(&pool, "ATC", author).await;
        let ticket_id = test_support::create_test_ticket(&pool, project, "ATT-C", author).await;
        let comment_id = create_test_comment(&pool, ticket_id, author).await;

        let att_id = create(
            &pool,
            ticket_id,
            Some(comment_id),
            author,
            "test.txt",
            "path/test.txt",
            1024,
        )
        .await
        .expect("Failed to create attachment with comment_id");

        let att = find_by_id(&pool, att_id)
            .await
            .expect("Failed to find attachment")
            .expect("Attachment not found");

        assert_eq!(att.comment_id, Some(comment_id));
    }

    #[tokio::test]
    async fn test_find_by_ticket_filters_by_ticket_id() {
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let author = test_support::create_test_user(&pool, "att-filt").await;
        let project = test_support::create_test_project(&pool, "ATF", author).await;
        let ticket_a = test_support::create_test_ticket(&pool, project, "ATT-A", author).await;
        let ticket_b = test_support::create_test_ticket(&pool, project, "ATT-B", author).await;
        let comment_b = create_test_comment(&pool, ticket_b, author).await;

        let att_a = create(
            &pool,
            ticket_a,
            None,
            author,
            "a.txt",
            "path/a.txt",
            1024,
        )
        .await
        .expect("Failed to create attachment on ticket A");
        let att_b = create(
            &pool,
            ticket_b,
            Some(comment_b),
            author,
            "b.txt",
            "path/b.txt",
            2048,
        )
        .await
        .expect("Failed to create attachment on ticket B");

        let atts_a = find_by_ticket(&pool, ticket_a)
            .await
            .expect("Failed to find attachments for ticket A");
        let atts_b = find_by_ticket(&pool, ticket_b)
            .await
            .expect("Failed to find attachments for ticket B");

        assert!(atts_a.iter().any(|a| a.id == att_a && a.comment_id.is_none()));
        assert!(!atts_a.iter().any(|a| a.id == att_b));
        assert!(atts_b.iter().any(|a| a.id == att_b && a.comment_id == Some(comment_b)));
        assert!(!atts_b.iter().any(|a| a.id == att_a));
    }
}
