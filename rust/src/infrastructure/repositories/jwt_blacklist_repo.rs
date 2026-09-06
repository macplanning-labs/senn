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

/// auth-core の `domain::jwt::TokenBlacklist` トレイト実装（Step 2: 配線）。
///
/// 上記の自由関数 `blacklist` / `is_blacklisted` は既存の呼び出し箇所
/// （`auth_api.rs`）が直接 `&PgPool` を渡す形で使い続けられるようそのまま残し、
/// この構造体はauth-core側のトレイト境界（`&dyn TokenBlacklist`のように抽象化して
/// 受け取りたい箇所）向けの薄いラッパーとして提供する。
pub struct PgJwtBlacklist(pub PgPool);

#[async_trait::async_trait]
impl auth_core::domain::jwt::TokenBlacklist for PgJwtBlacklist {
    async fn blacklist(&self, jti: &str, expires_at: DateTime<Utc>) -> auth_core::Result<()> {
        blacklist(&self.0, jti, expires_at)
            .await
            .map_err(|e| auth_core::AuthError::Internal(e.to_string()))
    }

    async fn is_blacklisted(&self, jti: &str) -> auth_core::Result<bool> {
        is_blacklisted(&self.0, jti)
            .await
            .map_err(|e| auth_core::AuthError::Internal(e.to_string()))
    }
}
