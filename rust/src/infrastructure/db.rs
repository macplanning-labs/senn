/// infrastructure/db.rs — PostgreSQL 接続プール
///
/// sqlx PgPool を作成する。

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

pub async fn create_pool() -> anyhow::Result<PgPool> {
    let database_url = resolve_database_url()?;

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await?;

    tracing::info!("✅ Database connected");
    Ok(pool)
}

/// DATABASE_URLが未設定でも、DjangoのDB_*環境変数(環境変数・docker-compose)を
/// 共有しているだけの環境ならそこから接続文字列を組み立てて動作できるようにする。
pub fn resolve_database_url() -> anyhow::Result<String> {
    match std::env::var("DATABASE_URL") {
        Ok(url) => Ok(url),
        Err(_) => build_database_url_from_django_env(),
    }
}

fn build_database_url_from_django_env() -> anyhow::Result<String> {
    let user = std::env::var("DB_USER")
        .map_err(|_| anyhow::anyhow!("DATABASE_URL or DB_USER must be set"))?;
    let password = std::env::var("DB_PASSWORD")
        .map_err(|_| anyhow::anyhow!("DATABASE_URL or DB_PASSWORD must be set"))?;
    let host = std::env::var("DB_HOST").unwrap_or_else(|_| "db".to_string());
    let port = std::env::var("DB_PORT").unwrap_or_else(|_| "5432".to_string());
    let name = std::env::var("DB_NAME")
        .map_err(|_| anyhow::anyhow!("DATABASE_URL or DB_NAME must be set"))?;
    Ok(format!("postgres://{user}:{password}@{host}:{port}/{name}"))
}
