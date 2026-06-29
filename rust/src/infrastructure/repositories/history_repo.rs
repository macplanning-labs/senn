/// infrastructure/repositories/history_repo.rs — ステータス変更履歴永続化

use sqlx::PgPool;
use crate::domain::models::ticket::TicketStatusHistory;

/// 履歴記録
pub async fn save(
    pool: &PgPool, ticket_id: i32, old_status: &str, new_status: &str, changed_by_id: i32,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO h_ticket_status (ticket_id, old_status, new_status, changed_by_id)
         VALUES ($1, $2, $3, $4)"
    ).bind(ticket_id).bind(old_status).bind(new_status).bind(changed_by_id)
     .execute(pool).await?;
    Ok(())
}

/// マイルストーン内の履歴取得（バーンダウンチャート用）
pub async fn find_by_milestone(
    pool: &PgPool, milestone_id: i32,
) -> anyhow::Result<Vec<TicketStatusHistory>> {
    let rows = sqlx::query_as::<_, TicketStatusHistory>(
        "SELECT h.id, h.ticket_id, h.old_status, h.new_status, h.changed_by_id, h.changed_at,
                u.display_name as changed_by_name,
                t.ticket_key as ticket_key,
                t.title as ticket_title
         FROM h_ticket_status h
         JOIN t_tickets t ON h.ticket_id = t.id
         LEFT JOIN m_users u ON h.changed_by_id = u.id
         WHERE t.milestone_id = $1
         ORDER BY h.changed_at"
    ).bind(milestone_id).fetch_all(pool).await?;
    Ok(rows)
}

/// チケット単位の履歴取得
pub async fn find_by_ticket(pool: &PgPool, ticket_id: i32) -> anyhow::Result<Vec<TicketStatusHistory>> {
    let rows = sqlx::query_as::<_, TicketStatusHistory>(
        "SELECT h.id, h.ticket_id, h.old_status, h.new_status, h.changed_by_id, h.changed_at,
                u.display_name as changed_by_name,
                t.ticket_key as ticket_key,
                t.title as ticket_title
         FROM h_ticket_status h
         JOIN t_tickets t ON h.ticket_id = t.id
         LEFT JOIN m_users u ON h.changed_by_id = u.id
         WHERE h.ticket_id = $1
         ORDER BY h.changed_at DESC"
    ).bind(ticket_id).fetch_all(pool).await?;
    Ok(rows)
}
