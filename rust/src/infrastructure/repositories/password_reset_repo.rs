/// infrastructure/repositories/password_reset_repo.rs — パスワードリセットトークン永続化
///
/// auth-core の `OneTimeTokenStore` トレイトを sqlx で実装する。

use auth_core::domain::one_time_token::{OneTimeToken, OneTimeTokenStore};
use auth_core::error::AuthError;
use sqlx::PgPool;

pub struct PasswordResetTokenStore {
    pool: PgPool,
}

impl PasswordResetTokenStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// 同一ユーザーの未使用トークンを無効化してから新規トークンを保存する。
    pub async fn replace_for_user(
        &self,
        user_id: i32,
        token: &OneTimeToken,
    ) -> Result<(), AuthError> {
        sqlx::query(
            "UPDATE auth_password_reset_token SET used_at = NOW()
             WHERE user_id = $1 AND used_at IS NULL",
        )
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AuthError::Internal(format!("旧トークン無効化エラー: {e}")))?;

        self.save(token, &user_id.to_string()).await
    }
}

#[async_trait::async_trait]
impl OneTimeTokenStore for PasswordResetTokenStore {
    async fn save(&self, token: &OneTimeToken, subject_id: &str) -> Result<(), AuthError> {
        let user_id: i32 = subject_id
            .parse()
            .map_err(|_| AuthError::Internal("subject_id の解析に失敗".into()))?;

        sqlx::query(
            "INSERT INTO auth_password_reset_token (token, user_id, expires_at)
             VALUES ($1, $2, $3)",
        )
        .bind(&token.token)
        .bind(user_id)
        .bind(token.expires_at)
        .execute(&self.pool)
        .await
        .map_err(|e| AuthError::Internal(format!("トークン保存エラー: {e}")))?;

        Ok(())
    }

    async fn consume(&self, token: &str) -> Result<Option<String>, AuthError> {
        let row: Option<(i32,)> = sqlx::query_as(
            "UPDATE auth_password_reset_token
             SET used_at = NOW()
             WHERE token = $1
               AND used_at IS NULL
               AND expires_at > NOW()
             RETURNING user_id::int4",
        )
        .bind(token)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AuthError::Internal(format!("トークン消費エラー: {e}")))?;

        Ok(row.map(|(user_id,)| user_id.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{create_test_user, test_pool, unique_suffix};
    use chrono::Duration;

    #[tokio::test]
    async fn save_and_consume_returns_user_id_once() {
        let Some(pool) = test_pool().await else {
            eprintln!("skip: test DB unavailable");
            return;
        };

        let user_id = create_test_user(&pool, "pwd-reset").await;
        let store = PasswordResetTokenStore::new(pool.clone());
        let token = OneTimeToken::issue(Duration::hours(1));

        store
            .save(&token, &user_id.to_string())
            .await
            .expect("save");

        let consumed = store
            .consume(&token.token)
            .await
            .expect("consume")
            .expect("subject");
        assert_eq!(consumed, user_id.to_string());

        let second = store.consume(&token.token).await.expect("second consume");
        assert!(second.is_none());
    }

    #[tokio::test]
    async fn replace_for_user_invalidates_previous_token() {
        let Some(pool) = test_pool().await else {
            eprintln!("skip: test DB unavailable");
            return;
        };

        let user_id = create_test_user(&pool, "pwd-replace").await;
        let store = PasswordResetTokenStore::new(pool);
        let old_token = OneTimeToken::issue(Duration::hours(1));
        let new_token = OneTimeToken::issue(Duration::hours(1));

        store
            .replace_for_user(user_id, &old_token)
            .await
            .expect("first save");
        store
            .replace_for_user(user_id, &new_token)
            .await
            .expect("replace");

        let old = store.consume(&old_token.token).await.expect("old consume");
        assert!(old.is_none());

        let new = store
            .consume(&new_token.token)
            .await
            .expect("new consume");
        assert_eq!(new, Some(user_id.to_string()));
    }

    #[tokio::test]
    async fn expired_token_is_not_consumed() {
        let Some(pool) = test_pool().await else {
            eprintln!("skip: test DB unavailable");
            return;
        };

        let suffix = unique_suffix();
        let username = format!("pwd-expired-{suffix}");
        let user_id = create_test_user(&pool, &username).await;
        let store = PasswordResetTokenStore::new(pool.clone());
        let token = OneTimeToken::issue(Duration::hours(-1));

        store
            .save(&token, &user_id.to_string())
            .await
            .expect("save");

        let consumed = store.consume(&token.token).await.expect("consume");
        assert!(consumed.is_none());
    }
}
