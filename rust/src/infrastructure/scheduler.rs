use std::time::Duration;
use sqlx::PgPool;
use crate::infrastructure::repositories::cycle_repo;

pub fn spawn_cycle_auto_activation(pool: PgPool) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(300));
        loop {
            interval.tick().await;
            match cycle_repo::auto_activate_due_cycles(&pool).await {
                Ok(activated) if !activated.is_empty() => {
                    for (id, project_id, name) in activated {
                        tracing::info!(
                            "cycle auto-activated: id={} project={} name={}",
                            id, project_id, name
                        );
                    }
                }
                Ok(_) => {}
                Err(e) => tracing::error!("cycle auto-activation failed: {:?}", e),
            }
        }
    });
}
