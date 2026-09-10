use sqlx::PgPool;

pub const KEY_OLLAMA_MODEL: &str = "ollama_model";
pub const KEY_OLLAMA_TIMEOUT: &str = "ollama_timeout";

pub async fn get(pool: &PgPool, key: &str) -> anyhow::Result<Option<String>> {
    let value: Option<String> = sqlx::query_scalar(
        "SELECT value FROM system_settings WHERE key = $1",
    )
    .bind(key)
    .fetch_optional(pool)
    .await?;
    Ok(value)
}

pub async fn set(pool: &PgPool, key: &str, value: &str) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO system_settings (key, value, updated_at)
        VALUES ($1, $2, NOW())
        ON CONFLICT (key) DO UPDATE
        SET value = EXCLUDED.value, updated_at = NOW()
        "#,
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete(pool: &PgPool, key: &str) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM system_settings WHERE key = $1")
        .bind(key)
        .execute(pool)
        .await?;
    Ok(())
}
