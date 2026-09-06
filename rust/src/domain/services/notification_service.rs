/// domain/services/notification_service.rs — 通知ビジネスロジック
///
/// In-app 通知（ベル通知）の生成とメール送信トリガー。
/// メール重複防止ロジック含む。

use sqlx::PgPool;

use crate::domain::models::notification::NotificationCategory;
use crate::infrastructure::mail::MailSender;
use crate::infrastructure::repositories::{notification_repo, ticket_repo, user_repo};

/// チケット操作に基づく通知生成
pub async fn notify_ticket_event(
    pool: &PgPool,
    mail_sender: &Option<MailSender>,
    ticket_id: i32,
    actor_id: i32,
    category: NotificationCategory,
    extra_message: &str,
) -> anyhow::Result<()> {
    // チケット情報取得
    let ticket = match ticket_repo::find_by_id(pool, ticket_id).await? {
        Some(t) => t,
        None => return Ok(()),
    };

    let title = format!(
        "{} {} — {}",
        category.icon(),
        category.label(),
        ticket.ticket_key
    );

    let message = if extra_message.is_empty() {
        ticket.title.clone()
    } else {
        format!("{}: {}", ticket.title, extra_message)
    };

    // ウォッチャー全員に通知（自分自身は除外）
    let watchers = ticket_repo::find_watchers(pool, ticket_id).await?;

    for watcher_id in watchers {
        if watcher_id == actor_id {
            continue;
        }

        // In-app 通知作成
        notification_repo::create(
            pool, watcher_id, Some(ticket_id),
            category.as_db_str(), &title, &message,
        ).await?;

        // メール送信（重複防止チェック）
        if let Some(sender) = mail_sender {
            let already_sent = notification_repo::has_recent_log(
                pool, ticket_id, watcher_id, category.as_db_str(), 60,
            ).await?;

            if !already_sent {
                // ユーザーのメール通知設定確認
                if let Some(user) = user_repo::find_by_id(pool, watcher_id).await? {
                    if user.email_notifications_enabled && !user.email.is_empty() {
                        let subject = format!("[WIP] {}", title);
                        let body = format!(
                            "{}\n\nチケット: {}\n{}\n\n---\nこの通知はWIPプロジェクト管理ツールから送信されました。",
                            message, ticket.ticket_key, ticket.title
                        );

                        if let Err(e) = sender.send(&user.email, &subject, &body).await {
                            tracing::warn!("メール送信失敗 (user={}): {}", watcher_id, e);
                        }

                        notification_repo::create_log(
                            pool, ticket_id, watcher_id, category.as_db_str(),
                        ).await?;
                    }
                }
            }
        }
    }

    Ok(())
}

/// コメント追加時の通知
pub async fn notify_comment(
    pool: &PgPool,
    mail_sender: &Option<MailSender>,
    ticket_id: i32,
    author_id: i32,
    comment_body: &str,
) -> anyhow::Result<()> {
    let preview = if comment_body.len() > 100 {
        format!("{}...", &comment_body[..100])
    } else {
        comment_body.to_string()
    };

    notify_ticket_event(
        pool, mail_sender, ticket_id, author_id,
        NotificationCategory::Commented, &preview,
    ).await
}

/// ステータス変更時の通知
pub async fn notify_status_change(
    pool: &PgPool,
    mail_sender: &Option<MailSender>,
    ticket_id: i32,
    actor_id: i32,
    old_status: &str,
    new_status: &str,
) -> anyhow::Result<()> {
    let msg = format!("{} → {}", old_status, new_status);
    notify_ticket_event(
        pool, mail_sender, ticket_id, actor_id,
        NotificationCategory::StatusChanged, &msg,
    ).await
}

/// Cycle 自動完了時の通知
pub async fn notify_cycle_auto_completed(
    pool: &PgPool,
    project_id: i32,
    cycle_id: i32,
    cycle_name: &str,
    carried_over: i64,
    target_cycle_id: Option<i32>,
) -> anyhow::Result<()> {
    // プロジェクトのメンバー user_id 一覧（ORDER BY user_id、最大 100）
    let member_ids: Vec<i32> = sqlx::query_scalar(
        "SELECT DISTINCT m.user_id::int4
         FROM tickets_project_membership m
         WHERE m.project_id = $1::int4
         ORDER BY m.user_id ASC
         LIMIT 100"
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;

    // target_cycle の名前を取得（あれば）
    let target_name = if let Some(target_id) = target_cycle_id {
        let name: Option<String> = sqlx::query_scalar(
            "SELECT name FROM t_cycle WHERE id = $1::int4"
        )
        .bind(target_id)
        .fetch_optional(pool)
        .await?
        .flatten();
        name
    } else {
        None
    };

    let title = format!("📅 Cycle自動完了 — {}", cycle_name);
    let message = if let Some(tname) = target_name {
        format!("未完了 {} 件を次 Cycle「{}」へ持ち越しました", carried_over, tname)
    } else {
        format!("未完了 {} 件を次 Cycle へ持ち越しました", carried_over)
    };

    // 各メンバーに通知を作成
    for user_id in member_ids {
        if let Err(e) = notification_repo::create(
            pool, user_id, None,
            NotificationCategory::CycleAutoCompleted.as_db_str(),
            &title,
            &message,
        ).await {
            tracing::error!(
                "[Cycle自動完了通知] 失敗 user_id={} cycle_id={} project_id={}: {}",
                user_id, cycle_id, project_id, e
            );
        }
    }

    Ok(())
}

/// 担当者設定時の通知
pub async fn notify_assigned(
    pool: &PgPool,
    mail_sender: &Option<MailSender>,
    ticket_id: i32,
    actor_id: i32,
    assignee_name: &str,
) -> anyhow::Result<()> {
    let msg = format!("担当者: {}", assignee_name);
    notify_ticket_event(
        pool, mail_sender, ticket_id, actor_id,
        NotificationCategory::Assigned, &msg,
    ).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;

    /// WIP-000053で報告された「コメント通知が本番で送信されない」の回帰テスト。
    /// notify_ticket_event が参照するテーブル(tickets_ticket_watchers,
    /// notifications_notification 等)が実際のスキーマと一致しており、
    /// ウォッチャーへ通知が作成され、投稿者自身には作成されないことを確認する。
    #[tokio::test]
    async fn notify_comment_creates_notification_for_watcher_but_not_author() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let author = test_support::create_test_user(&pool, "notif-author").await;
        let watcher = test_support::create_test_user(&pool, "notif-watcher").await;
        let project = test_support::create_test_project(&pool, "NTF", author).await;
        let ticket_id = test_support::create_test_ticket(&pool, project, "NTF-T", author).await;

        crate::infrastructure::repositories::ticket_repo::add_watcher(&pool, ticket_id, author)
            .await
            .unwrap();
        crate::infrastructure::repositories::ticket_repo::add_watcher(&pool, ticket_id, watcher)
            .await
            .unwrap();

        notify_comment(&pool, &None, ticket_id, author, "テストコメント").await.unwrap();

        let watcher_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM notifications_notification WHERE user_id = $1 AND ticket_id = $2 AND category = 'commented'"
        )
        .bind(watcher)
        .bind(ticket_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(watcher_count, 1, "ウォッチャーには通知が作成されるはず");

        let author_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM notifications_notification WHERE user_id = $1 AND ticket_id = $2"
        )
        .bind(author)
        .bind(ticket_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(author_count, 0, "コメント投稿者自身には通知を作成しないはず");
    }
}
