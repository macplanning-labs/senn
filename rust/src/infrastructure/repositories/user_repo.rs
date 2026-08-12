/// infrastructure/repositories/user_repo.rs — ユーザー永続化
///
/// Djangoの実スキーマ(accounts_user, mfa_totp_device, mfa_webauthn_credential)を
/// そのままクエリする。idはDB上bigintだが、アプリ全体でi32を使っているため
/// int4にキャストして取得する(このツールの想定ユーザー数ではi32で十分)。

use sqlx::PgPool;
use crate::domain::models::user::{User, TotpDevice, WebAuthnCredential};
use webauthn_rs::prelude::Passkey;

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

pub async fn find_project_members(pool: &PgPool, project_id: i32) -> anyhow::Result<Vec<User>> {
    // プロジェクトメンバーのうち、有効期限内のユーザーのみを返す
    // 有効性判定: end_date IS NULL OR (end_date + grace_period_days) >= CURRENT_DATE
    //
    // accounts_user/tickets_project_membership/tickets_projectを直接JOINしてしまうと、
    // 全テーブルにidカラムがあるためUSER_COLUMNS内の無修飾"id"が曖昧参照エラーになる。
    // そのためJOINせず、メンバーシップ判定はサブクエリ(IN)側に閉じ込める。
    let sql = format!(
        "SELECT {USER_COLUMNS}
         FROM accounts_user
         WHERE is_active = true
           AND id IN (
             SELECT m.user_id
             FROM tickets_project_membership m
             INNER JOIN tickets_project p ON p.id = m.project_id
             WHERE m.project_id = $1
               AND (m.end_date IS NULL OR (m.end_date + (p.grace_period_days || ' days')::interval) >= CURRENT_DATE)
           )
         ORDER BY username"
    );
    let rows = sqlx::query_as::<_, User>(&sql)
        .bind(project_id)
        .fetch_all(pool)
        .await?;
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

/// ユーザーの有効/無効を切り替える(Django Admin代替、is_staffのみ呼び出し可能)。
pub async fn set_active(pool: &PgPool, id: i32, is_active: bool) -> anyhow::Result<()> {
    sqlx::query("UPDATE accounts_user SET is_active=$2 WHERE id=$1")
        .bind(id).bind(is_active).execute(pool).await?;
    Ok(())
}

/// ユーザーのプロフィール(メールアドレス・氏名)を更新する(Django Admin代替、
/// is_staffのみ呼び出し可能)。emailのUNIQUE制約違反はsqlx::Errorとして
/// 呼び出し元に伝播する(create_userと同じ扱い)。
pub async fn update_profile(
    pool: &PgPool,
    id: i32,
    email: &str,
    first_name: &str,
    last_name: &str,
) -> anyhow::Result<User> {
    let sql = format!(
        "UPDATE accounts_user SET email=$2, first_name=$3, last_name=$4 WHERE id=$1
         RETURNING {USER_COLUMNS}"
    );
    let row = sqlx::query_as::<_, User>(&sql)
        .bind(id)
        .bind(email)
        .bind(first_name)
        .bind(last_name)
        .fetch_one(pool)
        .await?;
    Ok(row)
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
    // sign_countはNOT NULL制約があるため、初期値0を明示的に指定する必要がある
    // (省略するとDB側の"null value in column sign_count"エラーで保存自体が失敗する)。
    sqlx::query(
        "INSERT INTO mfa_webauthn_credential (user_id, credential_id, public_key, name, sign_count, created_at)
         VALUES ($1, $2, $3, $4, 0, NOW())"
    ).bind(user_id).bind(credential_id).bind(public_key).bind(name)
     .execute(pool).await?;
    Ok(())
}

pub async fn save_webauthn_credential_with_json(
    pool: &PgPool, user_id: i32, credential_id: &[u8],
    public_key: &[u8], passkey_json: &str, name: &str,
) -> anyhow::Result<()> {
    // sign_countはNOT NULL制約があるため、初期値0を明示的に指定する必要がある
    // (省略するとDB側の"null value in column sign_count"エラーで保存自体が失敗する)。
    sqlx::query(
        "INSERT INTO mfa_webauthn_credential (user_id, credential_id, public_key, passkey_json, name, sign_count, created_at)
         VALUES ($1, $2, $3, $4, $5, 0, NOW())"
    ).bind(user_id).bind(credential_id).bind(public_key).bind(passkey_json).bind(name)
     .execute(pool).await?;
    Ok(())
}

pub async fn delete_webauthn_credential(pool: &PgPool, id: i32) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM mfa_webauthn_credential WHERE id=$1")
        .bind(id).execute(pool).await?;
    Ok(())
}

/// 外部API(X-API-Key認証)用: 最初に見つかったstaffユーザーを返す(フォールバック用)。
pub async fn find_first_staff_user(pool: &PgPool) -> anyhow::Result<Option<User>> {
    let sql = format!("SELECT {USER_COLUMNS} FROM accounts_user WHERE is_staff = true ORDER BY id LIMIT 1");
    let user = sqlx::query_as::<_, User>(&sql).fetch_optional(pool).await?;
    Ok(user)
}

/// ログイン検証用: 指定ユーザーのPasskey(passkey_json形式)のみを取得する。
/// (全ユーザーではなく特定の1ユーザーの分のみ。identify_authentication で
/// user_idを特定した後に呼ぶこと)
pub async fn find_passkeys_by_user(pool: &PgPool, user_id: i32) -> anyhow::Result<Vec<Passkey>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT passkey_json FROM mfa_webauthn_credential WHERE user_id = $1 AND passkey_json IS NOT NULL"
    ).bind(user_id).fetch_all(pool).await?;

    Ok(rows.into_iter()
        .filter_map(|(json,)| serde_json::from_str(&json).ok())
        .collect())
}

/// 認証(ログイン)成功後、リプレイ攻撃防止のためsign_countを反映した
/// passkey_jsonで更新する(credential_idで対象行を特定)。
pub async fn update_webauthn_passkey_json(pool: &PgPool, credential_id: &[u8], passkey_json: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE mfa_webauthn_credential SET passkey_json = $2 WHERE credential_id = $1")
        .bind(credential_id).bind(passkey_json).execute(pool).await?;
    Ok(())
}
