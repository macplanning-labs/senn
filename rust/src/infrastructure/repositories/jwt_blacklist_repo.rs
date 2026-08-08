/// infrastructure/repositories/jwt_blacklist_repo.rs — JWTブラックリスト永続化
///
/// Rust側で自己完結するリフレッシュトークンのブラックリスト。
/// テーブル定義: rust/migrations/20260808000001_jwt_blacklist.sql

use chrono::{DateTime, Utc};
use sqlx::PgPool;

pub async fn blacklist(pool: &PgPool, jti: &str, expires_at: DateTime<Utc>) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO jwt_blacklisted_token (jti, expires_at) VALUES ($1, $2)
         ON CONFLICT (jti) DO NOTHING"
    )
    .bind(jti)
    .bind(expires_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn is_blacklisted(pool: &PgPool, jti: &str) -> anyhow::Result<bool> {
    let row: Option<(i32,)> = sqlx::query_as(
        "SELECT 1 FROM jwt_blacklisted_token WHERE jti = $1"
    )
    .bind(jti)
    .fetch_optional(pool)
    .await?;
    Ok(row.is_some())
}
