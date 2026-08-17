/// infrastructure/repositories/notification_repo.rs — 通知永続化

use sqlx::PgPool;
use crate::domain::models::notification::Notification;

pub async fn find_by_user(pool: &PgPool, user_id: i32, limit: i64) -> anyhow::Result<Vec<Notification>> {
    let rows = sqlx::query_as::<_, Notification>(
        "SELECT n.id, n.user_id, n.ticket_id, n.category, n.title, n.message,
                n.is_read, n.created_at,
                t.ticket_key as ticket_key
         FROM t_notifications n
         LEFT JOIN t_tickets t ON n.ticket_id = t.id
         WHERE n.user_id = $1
         ORDER BY n.created_at DESC
         LIMIT $2"
    ).bind(user_id).bind(limit).fetch_all(pool).await?;
    Ok(rows)
}

pub async fn count_unread(pool: &PgPool, user_id: i32) -> anyhow::Result<i64> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM t_notifications WHERE user_id=$1 AND is_read=false"
    ).bind(user_id).fetch_one(pool).await?;
    Ok(count)
}

pub async fn mark_read(pool: &PgPool, id: i32) -> anyhow::Result<()> {
    sqlx::query("UPDATE t_notifications SET is_read=true WHERE id=$1")
        .bind(id).execute(pool).await?;
    Ok(())
}

pub async fn create(
    pool: &PgPool, user_id: i32, ticket_id: Option<i32>,
    category: &str, title: &str, message: &str,
) -> anyhow::Result<i32> {
    let id = sqlx::query_scalar::<_, i32>(
        "INSERT INTO t_notifications (user_id, ticket_id, category, title, message)
         VALUES ($1, $2, $3, $4, $5) RETURNING id"
    ).bind(user_id).bind(ticket_id).bind(category).bind(title).bind(message)
     .fetch_one(pool).await?;
    Ok(id)
}

pub async fn find_by_id(pool: &PgPool, id: i32) -> anyhow::Result<Option<Notification>> {
    let row = sqlx::query_as::<_, Notification>(
        "SELECT n.id, n.user_id, n.ticket_id, n.category, n.title, n.message,
                n.is_read, n.created_at,
                t.ticket_key as ticket_key
         FROM t_notifications n
         LEFT JOIN t_tickets t ON n.ticket_id = t.id
         WHERE n.id = $1"
    ).bind(id).fetch_optional(pool).await?;
    Ok(row)
}

/// 通知ログ記録（メール重複防止）
pub async fn create_log(
    pool: &PgPool, ticket_id: i32, user_id: i32, notification_type: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO h_notification_logs (ticket_id, user_id, notification_type)
         VALUES ($1, $2, $3)"
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
        "SELECT EXISTS(SELECT 1 FROM h_notification_logs
         WHERE ticket_id=$1 AND user_id=$2 AND notification_type=$3
         AND sent_at > NOW() - ($4 || ' minutes')::interval)"
    ).bind(ticket_id).bind(user_id).bind(notification_type).bind(within_minutes)
     .fetch_one(pool).await?;
    Ok(exists)
}

/// 全通知を既読にする
pub async fn mark_all_read(pool: &PgPool, user_id: i32) -> anyhow::Result<u64> {
    let result = sqlx::query(
        "UPDATE t_notifications SET is_read=true WHERE user_id=$1 AND is_read=false"
    ).bind(user_id).execute(pool).await?;
    Ok(result.rows_affected())
}

/// メール通知設定の更新
pub async fn update_email_setting(pool: &PgPool, user_id: i32, enabled: bool) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE m_users SET email_notifications_enabled=$1 WHERE id=$2"
    ).bind(enabled).bind(user_id).execute(pool).await?;
    Ok(())
}
