/// infrastructure/repositories/notification_repo.rs — 通知永続化

use sqlx::PgPool;


pub async fn create(
    pool: &PgPool, user_id: i32, ticket_id: Option<i32>,
    category: &str, title: &str, message: &str,
) -> anyhow::Result<i32> {
    let id = sqlx::query_scalar::<_, i32>(
        "INSERT INTO notifications_notification (user_id, ticket_id, category, title, message, is_read, created_at)
         VALUES ($1, $2, $3, $4, $5, false, NOW()) RETURNING id::int4"
    ).bind(user_id).bind(ticket_id).bind(category).bind(title).bind(message)
     .fetch_one(pool).await?;
    Ok(id)
}

/// 通知ログ記録（メール重複防止）
pub async fn create_log(
    pool: &PgPool, ticket_id: i32, user_id: i32, notification_type: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO notifications_log (ticket_id, user_id, notification_type, sent_at)
         VALUES ($1, $2, $3, NOW())"
    ).bind(ticket_id).bind(user_id).bind(notification_type)
     .execute(pool).await?;
    Ok(())
}

/// 直近で同じ通知を送ったか（重複防止）
pub async fn has_recent_log(
    pool: &PgPool, ticket_id: i32, user_id: i32, notification_type: &str,
    within_minutes: i32,
) -> anyhow::Result<bool> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM notifications_log
         WHERE ticket_id=$1 AND user_id=$2 AND notification_type=$3
         AND sent_at > NOW() - ($4 || ' minutes')::interval)"
    ).bind(ticket_id).bind(user_id).bind(notification_type).bind(within_minutes)
     .fetch_one(pool).await?;
    Ok(exists)
}
