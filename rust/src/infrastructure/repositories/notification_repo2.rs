use crate::domain::models::notification_api::NotificationOut;
/// infrastructure/repositories/notification_repo2.rs — 通知永続化（API用）
///
/// Django /api/v1/notifications/* の実スキーマ (notifications_notification) に対応。
/// bigint列は全て ::int4 キャスト。
use sqlx::{PgPool, Postgres, QueryBuilder};

use crate::domain::access::Scope;
use crate::infrastructure::access::scope_sql;

/// 参照先(チケット・Wiki)が、見える範囲にある通知だけに絞る条件(アクセス制御の再設計 C-5)。
/// `t` はチケット、`wp` は Wiki の別名(LEFT JOIN。参照先の無い通知は、そのまま残す)
fn push_visible_target(qb: &mut QueryBuilder<'_, Postgres>, scope: &Scope) {
    qb.push(" AND (n.ticket_id IS NULL OR ");
    scope_sql::push_ticket_visible(qb, "t", scope);
    qb.push(") AND (n.wiki_page_id IS NULL OR ");
    scope_sql::push_wiki_visible(qb, "wp", scope);
    qb.push(")");
}

/// ユーザー宛の全通知を取得(ページネーション無し、ORDER BY priority, created_at DESC)
///
/// `scope` があれば、参照先が今も見える通知だけを返す(公開区分の変更・所属の終了のあとに、
/// 見えなくなったチケットのタイトルを出さない)。
pub async fn find_all_for_user(
    pool: &PgPool,
    user_id: i32,
    scope: Option<&Scope>,
) -> anyhow::Result<Vec<NotificationOut>> {
    let mut qb = QueryBuilder::new(
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
         WHERE n.user_id = ",
    );
    qb.push_bind(user_id).push("::int4 AND n.is_hidden = false");
    if let Some(scope) = scope {
        push_visible_target(&mut qb, scope);
    }
    qb.push(" ORDER BY (CASE WHEN n.category IN ('review_requested', 'mentioned') THEN 0 ELSE 1 END), n.created_at DESC");
    let rows = qb
        .build_query_as::<NotificationOut>()
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
         WHERE id = $1::int4 AND user_id = $2::int4",
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
         WHERE user_id = $1::int4 AND is_read = false",
    )
    .bind(user_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() as i64)
}

/// user_id一致かつis_read=falseかつis_hidden=falseの件数。`scope` があれば、参照先が見える通知だけを数える。
pub async fn unread_count(
    pool: &PgPool,
    user_id: i32,
    scope: Option<&Scope>,
) -> anyhow::Result<i64> {
    let mut qb = QueryBuilder::new(
        "SELECT COUNT(*) FROM notifications_notification n
         LEFT JOIN tickets_ticket t ON n.ticket_id = t.id
         LEFT JOIN wiki_page wp ON n.wiki_page_id = wp.id
         WHERE n.user_id = ",
    );
    qb.push_bind(user_id)
        .push("::int4 AND n.is_read = false AND n.is_hidden = false");
    if let Some(scope) = scope {
        push_visible_target(&mut qb, scope);
    }
    let count: i64 = qb.build_query_scalar().fetch_one(pool).await?;
    Ok(count)
}

/// 指定IDかつuser_id一致の通知をdismiss（is_hidden=true）。
/// 権限チェック(他人の通知をdismissできない)。
/// dismissできたら true、該当なしなら false。
pub async fn dismiss(pool: &PgPool, id: i32, user_id: i32) -> anyhow::Result<bool> {
    let result = sqlx::query(
        "UPDATE notifications_notification SET is_hidden = true
         WHERE id = $1::int4 AND user_id = $2::int4",
    )
    .bind(id)
    .bind(user_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// user_id一致かつis_read=trueかつis_hidden=falseの全通知をdismiss（is_hidden=true）。
/// 更新件数を返す。
pub async fn dismiss_all_read(pool: &PgPool, user_id: i32) -> anyhow::Result<u64> {
    let result = sqlx::query(
        "UPDATE notifications_notification SET is_hidden = true
         WHERE user_id = $1::int4 AND is_read = true AND is_hidden = false",
    )
    .bind(user_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}
