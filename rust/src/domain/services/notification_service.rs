/// domain/services/notification_service.rs — 通知ビジネスロジック
///
/// In-app 通知（ベル通知）の生成とメール送信トリガー。
/// メール重複防止ロジック含む。

use sqlx::PgPool;

use crate::domain::models::notification::NotificationCategory;
use crate::infrastructure::mail::MailSender;
use crate::infrastructure::repositories::{notification_repo, ticket_repo, user_repo};

/// チケット操作に基づく通知生成
#[allow(dead_code)]
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
#[allow(dead_code)]
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
#[allow(dead_code)]
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

/// 担当者設定時の通知
#[allow(dead_code)]
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
