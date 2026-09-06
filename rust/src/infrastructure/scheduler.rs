use std::time::Duration;
use sqlx::PgPool;
use crate::infrastructure::repositories::cycle_repo;

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
