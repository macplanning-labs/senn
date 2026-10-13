use crate::domain::services::ai_kick_service;
use crate::domain::services::ai_mcp_request_service;
use crate::domain::services::ai_run_service;
use crate::domain::services::notification_service;
use crate::infrastructure::mail::MailSender;
use crate::infrastructure::repositories::ai_mcp_request_repo;
use crate::infrastructure::repositories::cycle_repo;
use crate::infrastructure::repositories::notification_repo;
use crate::infrastructure::repositories::user_repo;
use chrono::Utc;
use sqlx::PgPool;
use std::time::Duration;

/// 二要素認証の試行の記録のうち、期限切れの行を消す(DEMO-000153 A-2・R2)。
/// 5 分ごとに、古い順に 1000 件ずつ(最大 10 回まで続けて)消す。総当たりを受けて行が急増しても、1 回の削除が重くならない。
pub fn spawn_mfa_attempt_purge(pool: PgPool) {
    tokio::spawn(async move {
        // 最初の実行は 5 分後。起動の直後は、マイグレーションの適用が終わる前で、テーブルがまだ無いことがある
        let period = Duration::from_secs(300);
        let mut interval = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
        let window = chrono::Duration::minutes(
            crate::domain::services::auth_service::TOTP_LOCK_WINDOW_MINUTES,
        );
        loop {
            interval.tick().await;
            for _ in 0..10 {
                match crate::infrastructure::repositories::user_repo::totp_purge_expired_attempts(
                    &pool, window, 1000,
                )
                .await
                {
                    Ok(n) if n >= 1000 => continue,
                    Ok(_) => break,
                    Err(e) => {
                        tracing::error!(
                            "mfa attempt purge failed: {:?} 処理=二要素認証の試行記録の掃除",
                            e
                        );
                        break;
                    }
                }
            }
        }
    });
}

pub fn spawn_cycle_auto_activation(pool: PgPool) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(300));
        loop {
            interval.tick().await;

            // 先に auto_activate_due_cycles を実行
            match cycle_repo::auto_activate_due_cycles(&pool).await {
                Ok(activated) if !activated.is_empty() => {
                    tracing::info!(
                        "cycle auto-activated: count={} 処理=サイクル自動活性化",
                        activated.len()
                    );
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::error!(
                        "[スケジューラ/サイクル] 処理=サイクル自動活性化 結果=失敗 影響=期日到来サイクルが自動開始されない可能性 | {}",
                        e
                    );
                }
            }

            // 次に auto_complete_overdue_cycles を実行
            match cycle_repo::auto_complete_overdue_cycles(&pool).await {
                Ok(completed) if !completed.is_empty() => {
                    tracing::info!(
                        "cycle auto-completed: count={} 処理=サイクル自動完了",
                        completed.len()
                    );
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::error!(
                        "[スケジューラ/サイクル] 処理=サイクル自動完了 結果=失敗 影響=期限超過サイクルが自動完了されない可能性 | {}",
                        e
                    );
                }
            }
        }
    });
}

/// 担当チケットの期限到来（当日）/超過リマインダーを1時間ごとにチェックする。
/// 実際の通知送信は notify_due_reminder 側のログ判定により1日1回に抑制される。
pub fn spawn_due_date_reminders(pool: PgPool, mail_sender: Option<MailSender>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3600));
        loop {
            interval.tick().await;

            if let Err(e) = notification_service::run_due_date_reminders(&pool, &mail_sender).await
            {
                tracing::error!(
                    "[スケジューラ/期限通知] 処理=期限リマインダー 結果=失敗 影響=期限到来/超過チケットの通知が送信されない可能性 | {}",
                    e
                );
            }
        }
    });
}

/// AI実行履歴の見張りジョブ（未着手/未完了の長時間放置を検知して通知）。
pub fn spawn_ai_run_watchdog(pool: PgPool, ai_username: String) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(600)); // 10分ごと
        loop {
            interval.tick().await;

            // AI ユーザーを取得
            let ai_user = match user_repo::find_by_username(&pool, &ai_username).await {
                Ok(Some(u)) => u,
                Ok(None) => {
                    tracing::warn!(
                        "[スケジューラ/AI実行] AI ユーザー '{}' が見つかりません。スキップ",
                        ai_username
                    );
                    continue;
                }
                Err(e) => {
                    tracing::error!("[スケジューラ/AI実行] AI ユーザー取得失敗 | {}", e);
                    continue;
                }
            };

            let ai_user_id = ai_user.id as i32;
            let start_timeout = std::env::var("SENN_AI_RUN_START_TIMEOUT_MIN")
                .ok()
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(120);
            let finish_timeout = std::env::var("SENN_AI_RUN_FINISH_TIMEOUT_MIN")
                .ok()
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(1440);

            // 「AIに対応させる」(社内ランナー方式)の見張り: ランナーが止まっている・報告が途絶えた・待たせすぎ
            let env_i64 = |k: &str, d: i64| {
                std::env::var(k)
                    .ok()
                    .and_then(|v| v.parse::<i64>().ok())
                    .filter(|v| *v > 0)
                    .unwrap_or(d)
            };
            match ai_kick_service::kick_watchdog_once(
                &pool,
                ai_user_id,
                env_i64("SENN_AI_KICK_UNCLAIMED_MIN", 30),
                env_i64("SENN_AI_KICK_HEARTBEAT_LOST_MIN", 5),
                env_i64("SENN_AI_KICK_EXPIRE_HOURS", 24),
            )
            .await
            {
                Ok(n) if n > 0 => tracing::info!("[スケジューラ/AI実行] 処理=AI 自動実行の見張り 件数={}", n),
                Ok(_) => {}
                Err(e) => tracing::error!(
                    "[スケジューラ/AI実行] 処理=AI 自動実行の見張り 結果=失敗 影響=ランナーの停止が検知されない可能性 | {}",
                    e
                ),
            }

            match ai_run_service::run_watchdog_once(
                &pool,
                ai_user_id,
                start_timeout,
                finish_timeout,
            )
            .await
            {
                Ok(count) if count > 0 => {
                    tracing::info!(
                        "[スケジューラ/AI実行] 見張り実行 処理=AI実行監視 件数={} 未着手タイムアウト={}分 未完了タイムアウト={}分",
                        count, start_timeout, finish_timeout
                    );
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::error!(
                        "[スケジューラ/AI実行] 処理=AI実行監視 結果=失敗 影響=AI実行の放置が検知されない可能性 | {}",
                        e
                    );
                }
            }
        }
    });
}

/// AI MCP 申請が 3日以上 pending のままの場合、管理者へ再通知を1回だけ送る(1時間ごと)。
pub fn spawn_ai_mcp_request_reminder(
    pool: PgPool,
    mail_sender: Option<MailSender>,
    base_url: String,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3600)); // 1時間ごと
        loop {
            interval.tick().await;

            let now = Utc::now();
            // 再通知が必要な申請の ID リストを取得
            let reminding_requests = match ai_mcp_request_repo::find_pending_needing_reminder(
                &pool, now,
            )
            .await
            {
                Ok(ids) => ids,
                Err(e) => {
                    tracing::error!(
                        "[スケジューラ/AI MCP] 処理=再通知対象の取得 結果=失敗 影響=3日以上pending の申請の再通知が送信されない可能性 | {}",
                        e
                    );
                    continue;
                }
            };

            if reminding_requests.is_empty() {
                continue;
            }

            for request_id in reminding_requests {
                // 申請の詳細を取得
                let req = match ai_mcp_request_repo::find_by_id(&pool, request_id).await {
                    Ok(Some(r)) => r,
                    Ok(None) => continue,
                    Err(e) => {
                        tracing::warn!(
                            "[スケジューラ/AI MCP] 申請 ID={} の詳細取得失敗: {}",
                            request_id,
                            e
                        );
                        continue;
                    }
                };

                // 申請者の情報を取得
                let requester = match user_repo::find_by_id(&pool, req.user.id).await {
                    Ok(Some(u)) => u,
                    Ok(None) => continue,
                    Err(e) => {
                        tracing::warn!(
                            "[スケジューラ/AI MCP] 申請者 ID={} の取得失敗: {}",
                            req.user.id,
                            e
                        );
                        continue;
                    }
                };

                // mark_reminded で再通知済みマークを付ける（失敗したら通知を送らない）
                match ai_mcp_request_repo::mark_reminded(&pool, request_id).await {
                    Ok(true) => {
                        // 再通知済みマークに成功したら、通知を送る
                        let title = "AI Agent を利用した MCP 操作の申請: (再通知)";
                        let message = format!(
                            "{} さんの申請がまだ処理されていません(3日以上経過)。",
                            if requester.display_name.is_empty() {
                                requester.username.clone()
                            } else {
                                requester.display_name.clone()
                            }
                        );

                        // 管理者全員に In-app 通知を送る
                        if let Ok(admin_ids) =
                            ai_mcp_request_repo::active_system_admin_ids(&pool).await
                        {
                            for admin_id in admin_ids {
                                if let Err(e) = notification_repo::create(
                                    &pool,
                                    admin_id,
                                    None,
                                    "ai_mcp_request",
                                    title,
                                    &message,
                                )
                                .await
                                {
                                    tracing::warn!(
                                        "[スケジューラ/AI MCP] 再通知の In-app 通知作成に失敗 (admin_id={}): {}",
                                        admin_id, e
                                    );
                                }
                            }
                        }

                        // メール通知を送る
                        if let Err(e) = ai_mcp_request_service::send_email_on_reminder(
                            &mail_sender,
                            &pool,
                            &requester,
                            &base_url,
                        )
                        .await
                        {
                            tracing::warn!(
                                "[スケジューラ/AI MCP] 再通知のメール送信に失敗 (user_id={}): {}",
                                req.user.id,
                                e
                            );
                        }
                    }
                    Ok(false) => {
                        // 既に再通知済み(二重送信を防ぐ)
                    }
                    Err(e) => {
                        tracing::error!(
                            "[スケジューラ/AI MCP] 再通知マークの設定に失敗 (request_id={}): {}",
                            request_id,
                            e
                        );
                    }
                }
            }
        }
    });
}
