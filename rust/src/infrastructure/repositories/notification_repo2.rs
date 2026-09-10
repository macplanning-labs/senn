/// infrastructure/repositories/notification_repo2.rs — 通知永続化（API用）
///
/// Django /api/v1/notifications/* の実スキーマ (notifications_notification) に対応。
/// bigint列は全て ::int4 キャスト。

use sqlx::PgPool;
use crate::domain::models::notification_api::NotificationOut;

/// ユーザー宛の全通知を取得（ページネーション無し、ORDER BY created_at DESC）
pub async fn find_all_for_user(pool: &PgPool, user_id: i32) -> anyhow::Result<Vec<NotificationOut>> {
    let rows = sqlx::query_as::<_, NotificationOut>(
        "SELECT
            n.id::int4 as id,
            n.category,
            n.title,
            n.message,
            t.ticket_key as ticket_key,
            proj.prefix as project_key,
            tm.slug as team_slug,
            wp.title as wiki_title,
            n.is_read,
            n.created_at,
            n.ticket_id::int4 as ticket,
            n.wiki_page_id::int4 as wiki_page
         FROM notifications_notification n
         LEFT JOIN tickets_ticket t ON n.ticket_id = t.id
         LEFT JOIN tickets_project proj ON t.project_id = proj.id
         LEFT JOIN m_team tm ON t.team_id = tm.id
         LEFT JOIN wiki_page wp ON n.wiki_page_id = wp.id
         WHERE n.user_id = $1::int4
         ORDER BY n.created_at DESC"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 指定IDかつuser_id一致の通知を既読にマーク。
/// user_idも条件に含めることで権限チェック(他人の通知を既読にできない)。
/// 更新できたら true、該当なしなら false。
pub async fn mark_read(pool: &PgPool, id: i32, user_id: i32) -> anyhow::Result<bool> {
    let result = sqlx::query(
        "UPDATE notifications_notification SET is_read = true
         WHERE id = $1::int4 AND user_id = $2::int4"
    )
    .bind(id)
    .bind(user_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// user_id一致かつis_read=falseの全通知をtrueに更新。
/// 更新件数を返す。
pub async fn mark_all_read(pool: &PgPool, user_id: i32) -> anyhow::Result<i64> {
    let result = sqlx::query(
        "UPDATE notifications_notification SET is_read = true
         WHERE user_id = $1::int4 AND is_read = false"
    )
    .bind(user_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() as i64)
}

/// user_id一致かつis_read=falseの件数。
pub async fn unread_count(pool: &PgPool, user_id: i32) -> anyhow::Result<i64> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications_notification
         WHERE user_id = $1::int4 AND is_read = false"
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    Ok(count)
}
