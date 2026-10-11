/// GitHub App 連携の永続化(t_github_app / t_github_installation / t_github_setup_state)
///
/// 秘密情報(秘密鍵・クライアントシークレット・Webhook シークレット)は
/// `encrypted_settings`(AES-256-GCM)で暗号化して保存する。
use sqlx::{PgPool, Row};

use super::client::{InstallationInfo, ManifestConversion};
use crate::infrastructure::encrypted_settings::{decrypt_value, encrypt_value};

pub const PURPOSE_MANIFEST: &str = "manifest";
pub const PURPOSE_INSTALL: &str = "install";
const STATE_TTL_MINUTES: i64 = 10;

/// 画面に出してよい App の情報(秘密は含まない)
#[derive(Debug, Clone)]
pub struct AppSummary {
    pub slug: String,
    pub name: String,
    pub html_url: String,
}

#[derive(Debug, Clone)]
pub struct InstallationRow {
    pub installation_id: i64,
    pub account_login: String,
    pub account_type: String,
    pub repository_selection: String,
    pub suspended: bool,
}

pub struct StateRecord {
    pub user_id: i64,
    pub return_path: String,
}

fn random_state() -> String {
    let bytes: [u8; 32] = rand::random();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// ── state(CSRF 対策。使い捨て・10 分) ──

pub async fn create_state(
    pool: &PgPool,
    purpose: &str,
    user_id: i64,
    return_path: &str,
) -> anyhow::Result<String> {
    // 期限切れの掃除(テーブルを肥大させない)
    sqlx::query("DELETE FROM t_github_setup_state WHERE expires_at < now()")
        .execute(pool)
        .await?;
    let state = random_state();
    sqlx::query(
        "INSERT INTO t_github_setup_state (state, purpose, user_id, return_path, expires_at)
         VALUES ($1, $2, $3, $4, now() + ($5 || ' minutes')::interval)",
    )
    .bind(&state)
    .bind(purpose)
    .bind(user_id)
    .bind(return_path)
    .bind(STATE_TTL_MINUTES.to_string())
    .execute(pool)
    .await?;
    Ok(state)
}

/// 単回使用。目的が違う・期限切れ・存在しない場合は None(消費もしない=目的違いは残す)。
pub async fn consume_state(
    pool: &PgPool,
    state: &str,
    purpose: &str,
) -> anyhow::Result<Option<StateRecord>> {
    let row = sqlx::query(
        "DELETE FROM t_github_setup_state
         WHERE state = $1 AND purpose = $2 AND expires_at >= now()
         RETURNING user_id, return_path",
    )
    .bind(state)
    .bind(purpose)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| StateRecord {
        user_id: r.get("user_id"),
        return_path: r.get("return_path"),
    }))
}

// ── App ──

pub async fn app_exists(pool: &PgPool) -> anyhow::Result<bool> {
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM t_github_app")
        .fetch_one(pool)
        .await?;
    Ok(n > 0)
}

pub async fn get_app_summary(pool: &PgPool) -> anyhow::Result<Option<AppSummary>> {
    let row = sqlx::query("SELECT slug, name, html_url FROM t_github_app WHERE id = 1")
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| AppSummary {
        slug: r.get("slug"),
        name: r.get("name"),
        html_url: r.get("html_url"),
    }))
}

/// 保存する。既にあれば false(最初の 1 つを守る。上書きしない)。
pub async fn save_app(
    pool: &PgPool,
    conv: &ManifestConversion,
    created_by: i64,
    jwt_secret: &str,
) -> anyhow::Result<bool> {
    let result = sqlx::query(
        "INSERT INTO t_github_app
           (id, app_id, slug, name, html_url, owner_login, client_id,
            client_secret_enc, webhook_secret_enc, private_key_enc, created_by_id)
         VALUES (1, $1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(conv.id)
    .bind(&conv.slug)
    .bind(&conv.name)
    .bind(&conv.html_url)
    .bind(conv.owner.as_ref().map(|o| o.login.as_str()).unwrap_or(""))
    .bind(&conv.client_id)
    .bind(encrypt_value(&conv.client_secret, jwt_secret)?)
    .bind(encrypt_value(&conv.webhook_secret, jwt_secret)?)
    .bind(encrypt_value(&conv.pem, jwt_secret)?)
    .bind(created_by)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn get_webhook_secret(pool: &PgPool, jwt_secret: &str) -> anyhow::Result<Option<String>> {
    let enc: Option<String> =
        sqlx::query_scalar("SELECT webhook_secret_enc FROM t_github_app WHERE id = 1")
            .fetch_optional(pool)
            .await?;
    enc.map(|e| decrypt_value(&e, jwt_secret)).transpose()
}

/// (app_id, 秘密鍵 PEM)
pub async fn get_app_credentials(
    pool: &PgPool,
    jwt_secret: &str,
) -> anyhow::Result<Option<(i64, String)>> {
    let row = sqlx::query("SELECT app_id, private_key_enc FROM t_github_app WHERE id = 1")
        .fetch_optional(pool)
        .await?;
    match row {
        Some(r) => {
            let enc: String = r.get("private_key_enc");
            Ok(Some((r.get("app_id"), decrypt_value(&enc, jwt_secret)?)))
        }
        None => Ok(None),
    }
}

// ── インストール ──

pub async fn upsert_installation(pool: &PgPool, info: &InstallationInfo) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO t_github_installation
           (installation_id, account_login, account_type, repository_selection, suspended_at)
         VALUES ($1, $2, $3, $4, CASE WHEN $5 THEN now() ELSE NULL END)
         ON CONFLICT (installation_id) DO UPDATE SET
           account_login = EXCLUDED.account_login,
           account_type = EXCLUDED.account_type,
           repository_selection = EXCLUDED.repository_selection,
           suspended_at = EXCLUDED.suspended_at,
           updated_at = now()",
    )
    .bind(info.id)
    .bind(&info.account.login)
    .bind(&info.account.account_type)
    .bind(&info.repository_selection)
    .bind(info.suspended_at.is_some())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_installations(pool: &PgPool) -> anyhow::Result<Vec<InstallationRow>> {
    let rows = sqlx::query(
        "SELECT installation_id, account_login, account_type, repository_selection,
                suspended_at IS NOT NULL AS suspended
         FROM t_github_installation ORDER BY account_login",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| InstallationRow {
            installation_id: r.get("installation_id"),
            account_login: r.get("account_login"),
            account_type: r.get("account_type"),
            repository_selection: r.get("repository_selection"),
            suspended: r.get("suspended"),
        })
        .collect())
}

pub async fn installation_exists(pool: &PgPool, installation_id: i64) -> anyhow::Result<bool> {
    let n: i64 =
        sqlx::query_scalar("SELECT count(*) FROM t_github_installation WHERE installation_id = $1")
            .bind(installation_id)
            .fetch_one(pool)
            .await?;
    Ok(n > 0)
}

pub async fn set_installation_suspended(
    pool: &PgPool,
    installation_id: i64,
    suspended: bool,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE t_github_installation
         SET suspended_at = CASE WHEN $2 THEN now() ELSE NULL END, updated_at = now()
         WHERE installation_id = $1",
    )
    .bind(installation_id)
    .bind(suspended)
    .execute(pool)
    .await?;
    Ok(())
}

/// インストール削除: 行を消し、その連携を無効にする。
pub async fn delete_installation(pool: &PgPool, installation_id: i64) -> anyhow::Result<()> {
    sqlx::query("UPDATE t_git_integration SET is_active = false WHERE installation_id = $1")
        .bind(installation_id)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM t_github_installation WHERE installation_id = $1")
        .bind(installation_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// リポジトリが外された: その連携を無効にする。
pub async fn deactivate_repo_integrations(
    pool: &PgPool,
    installation_id: i64,
    repository_urls: &[String],
) -> anyhow::Result<u64> {
    if repository_urls.is_empty() {
        return Ok(0);
    }
    let r = sqlx::query(
        "UPDATE t_git_integration SET is_active = false
         WHERE installation_id = $1 AND lower(repository_url) = ANY($2)",
    )
    .bind(installation_id)
    .bind(
        repository_urls
            .iter()
            .map(|u| u.to_lowercase())
            .collect::<Vec<_>>(),
    )
    .execute(pool)
    .await?;
    Ok(r.rows_affected())
}

// ── 診断 ──

/// 最後に受信した内容を連携に記録する(画面の診断表示用)。失敗しても処理は止めない。
pub async fn record_delivery(
    pool: &PgPool,
    integration_id: i32,
    event: &str,
    result: &str,
    note: &str,
) {
    let note: String = note.chars().take(300).collect();
    let event: String = event.chars().take(40).collect();
    // 履歴(連携ごとに最新 50 件)。失敗しても、最後の受信の記録は続ける
    if let Err(e) = record_history(pool, integration_id, &event, result, &note).await {
        tracing::warn!("delivery history failed: {:?}", e);
    }
    if let Err(e) = sqlx::query(
        "UPDATE t_git_integration
         SET last_delivery_at = now(), last_delivery_event = $2,
             last_delivery_result = $3, last_delivery_note = $4
         WHERE id = $1",
    )
    .bind(integration_id as i64)
    .bind(event)
    .bind(result)
    .bind(note)
    .execute(pool)
    .await
    {
        tracing::warn!("delivery record failed: {:?}", e);
    }
}

/// 連携ごとに残す受信履歴の件数
pub const HISTORY_KEEP: i64 = 50;

async fn record_history(
    pool: &PgPool,
    integration_id: i32,
    event: &str,
    result: &str,
    note: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO t_git_delivery (integration_id, event, result, note) VALUES ($1, $2, $3, $4)",
    )
    .bind(integration_id as i64)
    .bind(event)
    .bind(result)
    .bind(note)
    .execute(pool)
    .await?;
    // 古いものを消す(増え続けないように)
    sqlx::query(
        "DELETE FROM t_git_delivery
         WHERE integration_id = $1
           AND id NOT IN (
               SELECT id FROM t_git_delivery WHERE integration_id = $1
               ORDER BY received_at DESC, id DESC LIMIT $2)",
    )
    .bind(integration_id as i64)
    .bind(HISTORY_KEEP)
    .execute(pool)
    .await?;
    Ok(())
}

/// 受信履歴(新しい順)。
pub async fn list_history(
    pool: &PgPool,
    integration_id: i32,
    limit: i64,
) -> anyhow::Result<Vec<(String, String, String, chrono::DateTime<chrono::Utc>)>> {
    let rows = sqlx::query_as(
        "SELECT event, result, note, received_at FROM t_git_delivery
         WHERE integration_id = $1 ORDER BY received_at DESC, id DESC LIMIT $2",
    )
    .bind(integration_id as i64)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// App 経由で有効な連携の id(リポジトリ URL の大文字小文字は区別しない)。
pub async fn find_app_integration_id(
    pool: &PgPool,
    repository_url: &str,
) -> anyhow::Result<Option<i32>> {
    let id: Option<i32> = sqlx::query_scalar(
        "SELECT id::int4 FROM t_git_integration
         WHERE lower(repository_url) = lower($1)
           AND installation_id IS NOT NULL AND is_active = true
         ORDER BY id LIMIT 1",
    )
    .bind(repository_url)
    .fetch_optional(pool)
    .await?;
    Ok(id)
}

/// App 経由で既に連携済みのリポジトリ URL(小文字)。1 リポジトリ = App 連携は 1 つなので、
/// どのプロジェクト/チームで連携されていても「連携済み」として扱う。
pub async fn linked_repo_urls(pool: &PgPool) -> anyhow::Result<std::collections::HashSet<String>> {
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT lower(repository_url) FROM t_git_integration WHERE installation_id IS NOT NULL",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().collect())
}

/// t_github_app は環境に 1 行だけ。それを触るテストは、並列に走らないようこのロックを取る。
#[cfg(test)]
pub(crate) static APP_TABLE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{create_test_user, test_pool};

    #[tokio::test]
    async fn delivery_history_keeps_latest_50_newest_first() {
        let Some(pool) = test_pool().await else {
            return;
        };
        // 連携の行が無くても、履歴は連携 id だけで残る
        let id: i32 = 1_000_000_000 + (rand::random::<u32>() % 1_000_000) as i32;
        for i in 0..60 {
            record_delivery(&pool, id, "push", "ok", &format!("n{i}")).await;
        }
        let rows = list_history(&pool, id, 100).await.unwrap();
        assert_eq!(rows.len() as i64, HISTORY_KEEP, "最新 50 件だけ残す");
        assert_eq!(rows.first().unwrap().2, "n59", "新しい順");
        assert_eq!(rows.last().unwrap().2, "n10");
        // 件数を絞って取れる
        assert_eq!(list_history(&pool, id, 20).await.unwrap().len(), 20);
        // 別の連携の履歴には影響しない
        let other = id + 1;
        record_delivery(&pool, other, "check_run", "ignored", "x").await;
        assert_eq!(list_history(&pool, other, 100).await.unwrap().len(), 1);
        assert_eq!(
            list_history(&pool, id, 100).await.unwrap().len() as i64,
            HISTORY_KEEP
        );
    }

    #[tokio::test]
    async fn state_is_single_use_purpose_bound_and_expires() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = create_test_user(&pool, "ghstate").await as i64;

        // 使えるのは 1 回だけ
        let s = create_state(&pool, PURPOSE_INSTALL, user, "/x")
            .await
            .unwrap();
        // 目的が違えば使えず、消費もされない
        assert!(consume_state(&pool, &s, PURPOSE_MANIFEST)
            .await
            .unwrap()
            .is_none());
        let rec = consume_state(&pool, &s, PURPOSE_INSTALL)
            .await
            .unwrap()
            .unwrap();
        assert_eq!((rec.user_id, rec.return_path.as_str()), (user, "/x"));
        assert!(consume_state(&pool, &s, PURPOSE_INSTALL)
            .await
            .unwrap()
            .is_none());

        // 期限が切れたら使えない
        let s = create_state(&pool, PURPOSE_MANIFEST, user, "/")
            .await
            .unwrap();
        sqlx::query("UPDATE t_github_setup_state SET expires_at = now() - interval '1 second' WHERE state = $1")
            .bind(&s)
            .execute(&pool)
            .await
            .unwrap();
        assert!(consume_state(&pool, &s, PURPOSE_MANIFEST)
            .await
            .unwrap()
            .is_none());
    }
}
