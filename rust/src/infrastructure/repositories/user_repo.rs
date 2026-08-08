/// infrastructure/repositories/user_repo.rs — ユーザー永続化
///
/// Djangoの実スキーマ(accounts_user, mfa_totp_device, mfa_webauthn_credential)を
/// そのままクエリする。idはDB上bigintだが、アプリ全体でi32を使っているため
/// int4にキャストして取得する(このツールの想定ユーザー数ではi32で十分)。

use sqlx::PgPool;
use crate::domain::models::user::{User, TotpDevice, WebAuthnCredential};

const USER_COLUMNS: &str = "id::int4 AS id, username, password AS password_hash, display_name, email,
        first_name, last_name,
        is_active, is_staff, must_change_password, email_notifications_enabled";

pub async fn find_all(pool: &PgPool) -> anyhow::Result<Vec<User>> {
    let sql = format!(
        "SELECT {USER_COLUMNS} FROM accounts_user WHERE is_active = true ORDER BY username"
    );
    let rows = sqlx::query_as::<_, User>(&sql).fetch_all(pool).await?;
    Ok(rows)
}

pub async fn find_by_id(pool: &PgPool, id: i32) -> anyhow::Result<Option<User>> {
    let sql = format!("SELECT {USER_COLUMNS} FROM accounts_user WHERE id=$1");
    let row = sqlx::query_as::<_, User>(&sql).bind(id).fetch_optional(pool).await?;
    Ok(row)
}

pub async fn find_by_username(pool: &PgPool, username: &str) -> anyhow::Result<Option<User>> {
    let sql = format!("SELECT {USER_COLUMNS} FROM accounts_user WHERE username=$1");
    let row = sqlx::query_as::<_, User>(&sql).bind(username).fetch_optional(pool).await?;
    Ok(row)
}

pub async fn update_password(pool: &PgPool, id: i32, password_hash: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE accounts_user SET password=$2, must_change_password=false WHERE id=$1")
        .bind(id).bind(password_hash).execute(pool).await?;
    Ok(())
}

/// 新規ユーザー登録(Phase 1: /api/v1/auth/register/)。
/// username重複はDB側のUNIQUE制約違反(sqlx::Error)として呼び出し元に伝播する。
pub async fn create_user(
    pool: &PgPool,
    username: &str,
    email: &str,
    password_hash: &str,
    first_name: &str,
    last_name: &str,
) -> anyhow::Result<User> {
    let sql = format!(
        "INSERT INTO accounts_user
            (password, is_superuser, username, first_name, last_name, email,
             is_staff, is_active, date_joined, display_name,
             must_change_password, email_notifications_enabled)
         VALUES ($1, false, $2, $3, $4, $5, false, true, NOW(), '', false, true)
         RETURNING {USER_COLUMNS}"
    );
    let row = sqlx::query_as::<_, User>(&sql)
        .bind(password_hash)
        .bind(username)
        .bind(first_name)
        .bind(last_name)
        .bind(email)
        .fetch_one(pool)
        .await?;
    Ok(row)
}

// --- TOTP ---

const TOTP_COLUMNS: &str = "id::int4 AS id, user_id::int4 AS user_id, secret, confirmed, created_at";

pub async fn find_totp(pool: &PgPool, user_id: i32) -> anyhow::Result<Option<TotpDevice>> {
    let sql = format!("SELECT {TOTP_COLUMNS} FROM mfa_totp_device WHERE user_id=$1");
    let row = sqlx::query_as::<_, TotpDevice>(&sql).bind(user_id).fetch_optional(pool).await?;
    Ok(row)
}

pub async fn save_totp(pool: &PgPool, user_id: i32, secret: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO mfa_totp_device (user_id, secret, confirmed, created_at) VALUES ($1, $2, false, NOW())
         ON CONFLICT (user_id) DO UPDATE SET secret=$2, confirmed=false"
    ).bind(user_id).bind(secret).execute(pool).await?;
    Ok(())
}

pub async fn confirm_totp(pool: &PgPool, user_id: i32) -> anyhow::Result<()> {
    sqlx::query("UPDATE mfa_totp_device SET confirmed=true WHERE user_id=$1")
        .bind(user_id).execute(pool).await?;
    Ok(())
}

pub async fn delete_totp(pool: &PgPool, user_id: i32) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM mfa_totp_device WHERE user_id=$1")
        .bind(user_id).execute(pool).await?;
    Ok(())
}

// --- WebAuthn ---

const WEBAUTHN_COLUMNS: &str =
    "id::int4 AS id, user_id::int4 AS user_id, credential_id, public_key, sign_count, name, created_at";

pub async fn find_webauthn_credentials(pool: &PgPool, user_id: i32) -> anyhow::Result<Vec<WebAuthnCredential>> {
    let sql = format!(
        "SELECT {WEBAUTHN_COLUMNS} FROM mfa_webauthn_credential WHERE user_id=$1 ORDER BY created_at"
    );
    let rows = sqlx::query_as::<_, WebAuthnCredential>(&sql).bind(user_id).fetch_all(pool).await?;
    Ok(rows)
}

pub async fn save_webauthn_credential(
    pool: &PgPool, user_id: i32, credential_id: &[u8],
    public_key: &[u8], name: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO mfa_webauthn_credential (user_id, credential_id, public_key, name, created_at)
         VALUES ($1, $2, $3, $4, NOW())"
    ).bind(user_id).bind(credential_id).bind(public_key).bind(name)
     .execute(pool).await?;
    Ok(())
}

pub async fn delete_webauthn_credential(pool: &PgPool, id: i32) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM mfa_webauthn_credential WHERE id=$1")
        .bind(id).execute(pool).await?;
    Ok(())
}
