/// infrastructure/repositories/user_repo.rs — ユーザー永続化

use sqlx::PgPool;
use crate::domain::models::user::{User, TotpDevice, WebAuthnCredential};

pub async fn find_all(pool: &PgPool) -> anyhow::Result<Vec<User>> {
    let rows = sqlx::query_as::<_, User>(
        "SELECT id, username, password_hash, display_name, email,
                is_active, is_staff, must_change_password,
                email_notifications_enabled, created_at, updated_at
         FROM m_users WHERE is_active = true
         ORDER BY display_name, username"
    ).fetch_all(pool).await?;
    Ok(rows)
}

pub async fn find_by_id(pool: &PgPool, id: i32) -> anyhow::Result<Option<User>> {
    let row = sqlx::query_as::<_, User>(
        "SELECT id, username, password_hash, display_name, email,
                is_active, is_staff, must_change_password,
                email_notifications_enabled, created_at, updated_at
         FROM m_users WHERE id=$1"
    ).bind(id).fetch_optional(pool).await?;
    Ok(row)
}

pub async fn find_by_username(pool: &PgPool, username: &str) -> anyhow::Result<Option<User>> {
    let row = sqlx::query_as::<_, User>(
        "SELECT id, username, password_hash, display_name, email,
                is_active, is_staff, must_change_password,
                email_notifications_enabled, created_at, updated_at
         FROM m_users WHERE username=$1"
    ).bind(username).fetch_optional(pool).await?;
    Ok(row)
}

pub async fn update_password(pool: &PgPool, id: i32, password_hash: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE m_users SET password_hash=$2, must_change_password=false, updated_at=NOW() WHERE id=$1")
        .bind(id).bind(password_hash).execute(pool).await?;
    Ok(())
}

// --- TOTP ---

pub async fn find_totp(pool: &PgPool, user_id: i32) -> anyhow::Result<Option<TotpDevice>> {
    let row = sqlx::query_as::<_, TotpDevice>(
        "SELECT id, user_id, secret, confirmed, created_at FROM s_totp_devices WHERE user_id=$1"
    ).bind(user_id).fetch_optional(pool).await?;
    Ok(row)
}

pub async fn save_totp(pool: &PgPool, user_id: i32, secret: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO s_totp_devices (user_id, secret) VALUES ($1, $2)
         ON CONFLICT (user_id) DO UPDATE SET secret=$2, confirmed=false"
    ).bind(user_id).bind(secret).execute(pool).await?;
    Ok(())
}

pub async fn confirm_totp(pool: &PgPool, user_id: i32) -> anyhow::Result<()> {
    sqlx::query("UPDATE s_totp_devices SET confirmed=true WHERE user_id=$1")
        .bind(user_id).execute(pool).await?;
    Ok(())
}

pub async fn delete_totp(pool: &PgPool, user_id: i32) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM s_totp_devices WHERE user_id=$1")
        .bind(user_id).execute(pool).await?;
    Ok(())
}

// --- WebAuthn ---

pub async fn find_webauthn_credentials(pool: &PgPool, user_id: i32) -> anyhow::Result<Vec<WebAuthnCredential>> {
    let rows = sqlx::query_as::<_, WebAuthnCredential>(
        "SELECT id, user_id, credential_id, public_key, sign_count, name, created_at
         FROM s_webauthn_credentials WHERE user_id=$1
         ORDER BY created_at"
    ).bind(user_id).fetch_all(pool).await?;
    Ok(rows)
}

pub async fn save_webauthn_credential(
    pool: &PgPool, user_id: i32, credential_id: &[u8],
    public_key: &[u8], name: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO s_webauthn_credentials (user_id, credential_id, public_key, name)
         VALUES ($1, $2, $3, $4)"
    ).bind(user_id).bind(credential_id).bind(public_key).bind(name)
     .execute(pool).await?;
    Ok(())
}

pub async fn delete_webauthn_credential(pool: &PgPool, id: i32) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM s_webauthn_credentials WHERE id=$1")
        .bind(id).execute(pool).await?;
    Ok(())
}
