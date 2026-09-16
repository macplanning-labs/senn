/// infrastructure/repositories/notification_preference_repo.rs — ユーザー通知設定(イベント別メールON/OFF)永続化
///
/// 行が存在しないカテゴリはデフォルトON(email_enabled=true)として扱う。

use sqlx::PgPool;

use crate::domain::models::notification::NotificationCategory;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct NotificationPreferenceRow {
    pub category: String,
    pub email_enabled: bool,
}

/// ユーザーの全メール対応カテゴリ分の設定を返す(行が無いカテゴリはデフォルトtrueで埋める)。
/// 返却順は`NotificationCategory::EMAIL_CAPABLE`の宣言順で固定する。
pub async fn list_by_user(pool: &PgPool, user_id: i32) -> anyhow::Result<Vec<NotificationPreferenceRow>> {
    let existing: Vec<NotificationPreferenceRow> = sqlx::query_as(
        "SELECT category, email_enabled FROM accounts_user_notification_preference WHERE user_id = $1"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let mut result = Vec::with_capacity(NotificationCategory::EMAIL_CAPABLE.len());
    for category in NotificationCategory::EMAIL_CAPABLE {
        let email_enabled = existing
            .iter()
            .find(|row| row.category == category.as_db_str())
            .map(|row| row.email_enabled)
            .unwrap_or(true);
        result.push(NotificationPreferenceRow {
            category: category.as_db_str().to_string(),
            email_enabled,
        });
    }
    Ok(result)
}

/// 指定カテゴリのメール通知が有効かどうか(行が無ければtrue扱い)。
pub async fn is_email_enabled(
    pool: &PgPool,
    user_id: i32,
    category: &NotificationCategory,
) -> anyhow::Result<bool> {
    let row: Option<bool> = sqlx::query_scalar(
        "SELECT email_enabled FROM accounts_user_notification_preference WHERE user_id = $1 AND category = $2"
    )
    .bind(user_id)
    .bind(category.as_db_str())
    .fetch_optional(pool)
    .await?;
    Ok(row.unwrap_or(true))
}

/// カテゴリ別設定を作成/更新する(UPSERT)。
pub async fn upsert(pool: &PgPool, user_id: i32, category: &str, email_enabled: bool) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO accounts_user_notification_preference (user_id, category, email_enabled)
         VALUES ($1, $2, $3)
         ON CONFLICT (user_id, category) DO UPDATE SET email_enabled = excluded.email_enabled"
    )
    .bind(user_id)
    .bind(category)
    .bind(email_enabled)
    .execute(pool)
    .await?;
    Ok(())
}
