/// domain/services/notification_service.rs — 通知ビジネスロジック
///
/// In-app 通知（ベル通知）の生成とメール送信トリガー。
/// メール重複防止ロジック含む。

use sqlx::PgPool;

use crate::domain::models::notification::NotificationCategory;
use crate::domain::models::user::User;
use crate::infrastructure::mail::MailSender;
use crate::infrastructure::repositories::{notification_preference_repo, notification_repo, ticket_repo, user_repo};
use crate::infrastructure::chat_notifier;

/// メール通知を送ってよいか判定する(マスタースイッチ + カテゴリ別設定の両方を見る)。
/// カテゴリ別設定の取得に失敗した場合は、通知を握りつぶさないようフェイルオープン(true)にする。
async fn should_send_email(pool: &PgPool, user: &User, category: &NotificationCategory) -> bool {
    if !user.email_notifications_enabled || user.email.is_empty() {
        return false;
    }
    match notification_preference_repo::is_email_enabled(pool, user.id, category).await {
        Ok(enabled) => enabled,
        Err(e) => {
            tracing::warn!("通知設定取得失敗 (user={}): {:?}", user.id, e);
            true
        }
    }
}

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

    // チャット通知連携への送信(ウォッチャーの有無に関わらず、イベント単位で1回だけ送信する)
    match crate::infrastructure::repositories::chat_integration_repo::find_active_for_ticket_category(
        pool, ticket_id, category.as_db_str(),
    ).await {
        Ok(integrations) => {
            for integration in integrations {
                let text = format!("{}\n{}", title, message);
                if let Err(e) = chat_notifier::send(&integration, &text).await {
                    tracing::warn!(
                        "チャット通知送信失敗 (integration_id={}, provider={}): {:?}",
                        integration.id, integration.provider, e
                    );
                }
            }
        }
        Err(e) => {
            tracing::warn!("チャット連携取得失敗 (ticket_id={}): {:?}", ticket_id, e);
        }
    }

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
                    if should_send_email(pool, &user, &category).await {
                        let subject = format!("[WIP] {}", title);
                        let body = format!(
                            "{}\n\nチケット: {}\n{}\n\n---\nこの通知は SENN から送信されました。",
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
    let preview = if comment_body.chars().count() > 100 {
        let truncated: String = comment_body.chars().take(100).collect();
        format!("{}...", truncated)
    } else {
        comment_body.to_string()
    };

    notify_ticket_event(
        pool, mail_sender, ticket_id, author_id,
        NotificationCategory::Commented, &preview,
    ).await
}

/// チケットの一般的なフィールド変更時の通知(ステータス変更・担当設定は別カテゴリのため対象外)
pub async fn notify_updated(
    pool: &PgPool,
    mail_sender: &Option<MailSender>,
    ticket_id: i32,
    actor_id: i32,
    changed_fields: &str,
) -> anyhow::Result<()> {
    notify_ticket_event(
        pool, mail_sender, ticket_id, actor_id,
        NotificationCategory::Updated, changed_fields,
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
///
/// Project メンバー経路は残しつつ、所属 Team メンバーも含める（重複排除）。
pub async fn notify_cycle_auto_completed(
    pool: &PgPool,
    project_id: Option<i32>,
    team_id: Option<i32>,
    cycle_id: i32,
    cycle_name: &str,
    carried_over: i64,
    target_cycle_id: Option<i32>,
) -> anyhow::Result<()> {
    let mut member_ids: Vec<i32> = Vec::new();

    if let Some(tid) = team_id {
        // L2: チーム全体メンバーに加え、このProjectに限定されたゲスト(scoped_project_id一致)も対象にする。
        // 他Projectに限定されたゲストは対象外(scoped_project_idが別Projectの行は除外)
        let team_members: Vec<i32> = sqlx::query_scalar(
            "SELECT DISTINCT m.user_id::int4
             FROM t_team_membership m
             WHERE m.team_id = $1::int4
               AND (m.scoped_project_id IS NULL OR m.scoped_project_id = $2::int4)
             ORDER BY m.user_id ASC
             LIMIT 100"
        )
        .bind(tid)
        .bind(project_id)
        .fetch_all(pool)
        .await?;
        member_ids.extend(team_members);
    }

    member_ids.sort_unstable();
    member_ids.dedup();
    if member_ids.len() > 100 {
        member_ids.truncate(100);
    }

    if member_ids.is_empty() {
        tracing::warn!(
            "[Cycle自動完了通知] 通知先なし cycle_id={} project_id={:?} team_id={:?}",
            cycle_id, project_id, team_id
        );
        return Ok(());
    }

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

    for user_id in member_ids {
        if let Err(e) = notification_repo::create(
            pool, user_id, None,
            NotificationCategory::CycleAutoCompleted.as_db_str(),
            &title,
            &message,
        ).await {
            tracing::error!(
                "[Cycle自動完了通知] 失敗 user_id={} cycle_id={} project_id={:?} team_id={:?}: {}",
                user_id, cycle_id, project_id, team_id, e
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

/// レビュー依頼通知
pub async fn notify_review_requested(
    pool: &PgPool,
    mail_sender: &Option<MailSender>,
    ticket_id: i32,
    actor_id: i32,
    reviewer_name: &str,
) -> anyhow::Result<()> {
    let msg = format!("レビュアー: {}", reviewer_name);
    notify_ticket_event(
        pool, mail_sender, ticket_id, actor_id,
        NotificationCategory::ReviewRequested, &msg,
    ).await
}

/// @ユーザー名メンション通知
/// 特定のユーザー1人に対してのみ通知を送る（ウォッチャー全員ではない）
pub async fn notify_mentioned(
    pool: &PgPool,
    mail_sender: &Option<MailSender>,
    ticket_id: i32,
    _actor_id: i32,
    mentioned_user_id: i32,
) -> anyhow::Result<()> {
    // チケット情報取得
    let ticket = match ticket_repo::find_by_id(pool, ticket_id).await? {
        Some(t) => t,
        None => return Ok(()),
    };

    let title = format!(
        "{} メンション — {}",
        NotificationCategory::Mentioned.icon(),
        ticket.ticket_key
    );

    let message = format!("コメントでメンションされました: {}", ticket.title);

    // In-app 通知作成
    notification_repo::create(
        pool, mentioned_user_id, Some(ticket_id),
        NotificationCategory::Mentioned.as_db_str(), &title, &message,
    ).await?;

    // メール送信（重複防止チェック）
    if let Some(sender) = mail_sender {
        let already_sent = notification_repo::has_recent_log(
            pool, ticket_id, mentioned_user_id, NotificationCategory::Mentioned.as_db_str(), 60,
        ).await?;

        if !already_sent {
            // ユーザーのメール通知設定確認
            if let Some(user) = user_repo::find_by_id(pool, mentioned_user_id).await? {
                if should_send_email(pool, &user, &NotificationCategory::Mentioned).await {
                    let subject = format!("[WIP] {}", title);
                    let body = format!(
                        "コメントでメンションされました\n\nチケット: {}\n{}\n\n---\nこの通知は SENN から送信されました。",
                        ticket.ticket_key, ticket.title
                    );

                    if let Err(e) = sender.send(&user.email, &subject, &body).await {
                        tracing::warn!("メール送信失敗 (user={}): {}", mentioned_user_id, e);
                    }

                    notification_repo::create_log(
                        pool, ticket_id, mentioned_user_id, NotificationCategory::Mentioned.as_db_str(),
                    ).await?;
                }
            }
        }
    }

    Ok(())
}

/// 期限到来（当日）/超過リマインダー（担当者本人のみ・1日1回）
///
/// `has_recent_log` を24時間弱のしきい値で流用し、バッチ実行間隔に関わらず
/// 同一チケット×担当者への通知が1日に複数回作成されないようにする。
async fn notify_due_reminder(
    pool: &PgPool,
    mail_sender: &Option<MailSender>,
    ticket_id: i32,
    ticket_key: &str,
    ticket_title: &str,
    user_id: i32,
    category: NotificationCategory,
) -> anyhow::Result<()> {
    if notification_repo::has_recent_log(pool, ticket_id, user_id, category.as_db_str(), 1380).await? {
        return Ok(());
    }

    let title = format!("{} {} — {}", category.icon(), category.label(), ticket_key);
    let message = ticket_title.to_string();

    notification_repo::create(
        pool, user_id, Some(ticket_id), category.as_db_str(), &title, &message,
    ).await?;
    notification_repo::create_log(pool, ticket_id, user_id, category.as_db_str()).await?;

    if let Some(sender) = mail_sender {
        if let Some(user) = user_repo::find_by_id(pool, user_id).await? {
            if should_send_email(pool, &user, &category).await {
                let subject = format!("[WIP] {}", title);
                let body = format!(
                    "{}\n\nチケット: {}\n{}\n\n---\nこの通知は SENN から送信されました。",
                    message, ticket_key, ticket_title
                );

                if let Err(e) = sender.send(&user.email, &subject, &body).await {
                    tracing::warn!("メール送信失敗 (user={}): {}", user_id, e);
                }
            }
        }
    }

    Ok(())
}

/// 期限到来/超過リマインダーのバッチ実行。
/// スケジューラから定期的に呼び出される想定（実際の送信は1日1回に制御される）。
pub async fn run_due_date_reminders(
    pool: &PgPool,
    mail_sender: &Option<MailSender>,
) -> anyhow::Result<()> {
    let targets = ticket_repo::find_due_or_overdue_with_assignees(pool).await?;
    let today = chrono::Local::now().date_naive();

    for t in targets {
        let category = if t.due_date < today {
            NotificationCategory::Overdue
        } else {
            NotificationCategory::DueSoon
        };

        if let Err(e) = notify_due_reminder(
            pool, mail_sender, t.ticket_id, &t.ticket_key, &t.title, t.user_id, category,
        ).await {
            tracing::error!(
                "notify_due_reminder failed ticket_id={} user_id={}: {:?}",
                t.ticket_id, t.user_id, e
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;

    /// DEMO-000053で報告された「コメント通知が本番で送信されない」の回帰テスト。
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

    /// 期限超過チケットの担当者に overdue 通知が作成され、
    /// 同一バッチを2回実行しても1日以内は重複作成されないことを確認する。
    #[tokio::test]
    async fn run_due_date_reminders_notifies_assignee_once_per_day() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let author = test_support::create_test_user(&pool, "due-author").await;
        let assignee = test_support::create_test_user(&pool, "due-assignee").await;
        let project = test_support::create_test_project(&pool, "DUE", author).await;
        let ticket_id = test_support::create_test_ticket(&pool, project, "DUE-T", author).await;

        sqlx::query("UPDATE tickets_ticket SET due_date = CURRENT_DATE - INTERVAL '1 day' WHERE id = $1")
            .bind(ticket_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO tickets_ticket_assignees (ticketmodel_id, user_id) VALUES ($1, $2)")
            .bind(ticket_id)
            .bind(assignee)
            .execute(&pool)
            .await
            .unwrap();

        run_due_date_reminders(&pool, &None).await.unwrap();
        run_due_date_reminders(&pool, &None).await.unwrap();

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM notifications_notification WHERE user_id = $1 AND ticket_id = $2 AND category = 'overdue'"
        )
        .bind(assignee)
        .bind(ticket_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1, "期限超過通知は1日1回のみ作成されるはず");
    }
}
