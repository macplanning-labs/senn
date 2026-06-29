/// infrastructure/db.rs — PostgreSQL 接続プール
///
/// sqlx PgPool を作成する。

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

pub async fn create_pool() -> anyhow::Result<PgPool> {
    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("DATABASE_URL must be set"))?;

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await?;

    tracing::info!("✅ Database connected");
    Ok(pool)
}
