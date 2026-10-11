/// infrastructure/repositories/ai_mcp_request_repo.rs — AI Agent を利用した MCP 操作の申請(DEMO-000192)
///
/// 状態: pending(申請中) → approved(承認済み) → issued(発行済み)。pending → rejected / cancelled。
/// 「期限切れ」は状態にせず、approved で approval_expires_at を過ぎたものとして扱う。
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use sqlx::{PgPool, Row};

use crate::domain::models::ticket_api::UserSummaryOut;
use crate::infrastructure::repositories::ai_agent_key_repo;

/// 1人が同時に持てる有効なキーの上限(PC買い替え・複数端末向け)
pub const MAX_ACTIVE_KEYS: i64 = 3;
/// 承認の有効期間(日)。過ぎたら再申請
pub const APPROVAL_VALID_DAYS: i64 = 7;
pub const PURPOSE_MAX_CHARS: usize = 500;

#[derive(Debug, Clone, Serialize)]
pub struct AiMcpRequestOut {
    pub id: i32,
    pub user: UserSummaryOut,
    pub purpose: String,
    /// pending / approved / rejected / issued / cancelled / expired(承認済みで期限切れ。計算で出す)
    pub status: String,
    #[serde(rename = "decidedBy")]
    pub decided_by: Option<UserSummaryOut>,
    #[serde(rename = "decidedAt")]
    pub decided_at: Option<DateTime<Utc>>,
    #[serde(rename = "decisionNote")]
    pub decision_note: String,
    #[serde(rename = "approvalExpiresAt")]
    pub approval_expires_at: Option<DateTime<Utc>>,
    #[serde(rename = "issuedAt")]
    pub issued_at: Option<DateTime<Utc>>,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
}

/// 申請の取得SQL。`$tail` に WHERE 以降を足す(SQLを実行時に組み立てず、コンパイル時に連結する)。
macro_rules! select_sql {
    ($tail:literal) => {
        concat!("
    SELECT r.id::int4, r.purpose, r.status, r.decision_note, r.decided_at, r.approval_expires_at,
           r.issued_at, r.created_at,
           u.id::int4, u.username, u.email, u.display_name,
           d.id::int4, d.username, d.email, d.display_name
    FROM ai_mcp_access_request r
    JOIN accounts_user u ON r.user_id = u.id
    LEFT JOIN accounts_user d ON r.decided_by = d.id
", $tail)
    };
}

fn row_to_out(r: &sqlx::postgres::PgRow, now: DateTime<Utc>) -> AiMcpRequestOut {
    let status: String = r.get(2);
    let expires: Option<DateTime<Utc>> = r.get(5);
    let status = if status == "approved" && expires.is_some_and(|e| e < now) {
        "expired".to_string()
    } else {
        status
    };
    let decided_by_id: Option<i32> = r.get(12);
    AiMcpRequestOut {
        id: r.get(0),
        purpose: r.get(1),
        status,
        decision_note: r.get(3),
        decided_at: r.get(4),
        approval_expires_at: expires,
        issued_at: r.get(6),
        created_at: r.get(7),
        user: UserSummaryOut {
            id: r.get(8),
            username: r.get(9),
            email: r.get(10),
            display_name: r.get(11),
        },
        decided_by: decided_by_id.map(|id| UserSummaryOut {
            id,
            username: r.get(13),
            email: r.get(14),
            display_name: r.get(15),
        }),
    }
}

/// 申請を作る。申請中・承認済みの申請が既にあれば Ok(None)(1人1件)。
pub async fn create(pool: &PgPool, user_id: i32, purpose: &str) -> anyhow::Result<Option<i32>> {
    // 承認済みでも期限切れのものは、再申請できるように先にたたむ
    sqlx::query(
        "UPDATE ai_mcp_access_request SET status = 'cancelled'
         WHERE user_id = $1 AND status = 'approved' AND approval_expires_at < NOW()",
    )
    .bind(user_id)
    .execute(pool)
    .await?;
    let id: Option<i32> = sqlx::query_scalar(
        "INSERT INTO ai_mcp_access_request (user_id, purpose, status) VALUES ($1, $2, 'pending')
         ON CONFLICT DO NOTHING RETURNING id::int4",
    )
    .bind(user_id)
    .bind(purpose)
    .fetch_optional(pool)
    .await?;
    Ok(id)
}

pub async fn find_by_id(pool: &PgPool, id: i32) -> anyhow::Result<Option<AiMcpRequestOut>> {
    let row = sqlx::query(select_sql!(" WHERE r.id = $1"))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    let now = Utc::now();
    Ok(row.map(|r| row_to_out(&r, now)))
}

pub async fn list_for_user(pool: &PgPool, user_id: i32) -> anyhow::Result<Vec<AiMcpRequestOut>> {
    let rows = sqlx::query(select_sql!(
        " WHERE r.user_id = $1 ORDER BY r.created_at DESC LIMIT 50"
    ))
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let now = Utc::now();
    Ok(rows.iter().map(|r| row_to_out(r, now)).collect())
}

/// 管理者向けの一覧。`status` が None なら全件(新しい順)。"pending" なら承認待ちだけ。
pub async fn list_for_admin(
    pool: &PgPool,
    status: Option<&str>,
) -> anyhow::Result<Vec<AiMcpRequestOut>> {
    let rows = sqlx::query(select_sql!(
        " WHERE ($1::text IS NULL OR r.status = $1) ORDER BY r.created_at DESC LIMIT 200"
    ))
    .bind(status)
    .fetch_all(pool)
    .await?;
    let now = Utc::now();
    Ok(rows.iter().map(|r| row_to_out(r, now)).collect())
}

pub async fn count_pending(pool: &PgPool) -> anyhow::Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM ai_mcp_access_request WHERE status = 'pending'")
            .fetch_one(pool)
            .await?,
    )
}

/// 有効なシステム管理者の人数(自己承認を許すかの判定用)
pub async fn count_active_system_admins(pool: &PgPool) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT COUNT(*) FROM accounts_user WHERE is_system_admin = true AND is_active = true",
    )
    .fetch_one(pool)
    .await?)
}

/// 通知の宛先になる、有効なシステム管理者のID
pub async fn active_system_admin_ids(pool: &PgPool) -> anyhow::Result<Vec<i32>> {
    Ok(sqlx::query_scalar(
        "SELECT id::int4 FROM accounts_user WHERE is_system_admin = true AND is_active = true",
    )
    .fetch_all(pool)
    .await?)
}

/// 申請中の申請を承認する。申請中でなければ false。
pub async fn approve(pool: &PgPool, id: i32, decided_by: i32, note: &str) -> anyhow::Result<bool> {
    let expires = Utc::now() + Duration::days(APPROVAL_VALID_DAYS);
    let r = sqlx::query(
        "UPDATE ai_mcp_access_request
         SET status = 'approved', decided_by = $2, decided_at = NOW(), decision_note = $3,
             approval_expires_at = $4
         WHERE id = $1 AND status = 'pending'",
    )
    .bind(id)
    .bind(decided_by)
    .bind(note)
    .bind(expires)
    .execute(pool)
    .await?;
    Ok(r.rows_affected() > 0)
}

/// 申請中の申請を却下する。申請中でなければ false。
pub async fn reject(pool: &PgPool, id: i32, decided_by: i32, note: &str) -> anyhow::Result<bool> {
    let r = sqlx::query(
        "UPDATE ai_mcp_access_request
         SET status = 'rejected', decided_by = $2, decided_at = NOW(), decision_note = $3
         WHERE id = $1 AND status = 'pending'",
    )
    .bind(id)
    .bind(decided_by)
    .bind(note)
    .execute(pool)
    .await?;
    Ok(r.rows_affected() > 0)
}

/// 本人が、申請中の申請を取り下げる。
pub async fn cancel(pool: &PgPool, id: i32, user_id: i32) -> anyhow::Result<bool> {
    let r = sqlx::query(
        "UPDATE ai_mcp_access_request SET status = 'cancelled'
         WHERE id = $1 AND user_id = $2 AND status IN ('pending', 'approved')",
    )
    .bind(id)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(r.rows_affected() > 0)
}

#[derive(Debug, PartialEq, Eq)]
pub enum IssueError {
    /// 申請が無い、または本人のものでない
    NotFound,
    /// 承認されていない(申請中・却下・取り下げ・発行済み)
    NotApproved,
    /// 承認の期限が切れている
    Expired,
    /// 有効なキーが上限に達している
    TooManyKeys,
}

pub struct IssuedKey {
    pub key_id: i32,
    pub plain_key: String,
    pub key_prefix: String,
}

/// 承認済みの申請から、本人のキーを発行する(1つのトランザクション)。
/// 平文キーはここで作って返すだけで、保存するのはハッシュだけ。
pub async fn issue(
    pool: &PgPool,
    id: i32,
    user_id: i32,
    label: Option<&str>,
) -> anyhow::Result<Result<IssuedKey, IssueError>> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query(
        "SELECT status, approval_expires_at FROM ai_mcp_access_request
         WHERE id = $1 AND user_id = $2 FOR UPDATE",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        return Ok(Err(IssueError::NotFound));
    };
    let status: String = row.get(0);
    let expires: Option<DateTime<Utc>> = row.get(1);
    if status != "approved" {
        return Ok(Err(IssueError::NotApproved));
    }
    if expires.is_some_and(|e| e < Utc::now()) {
        return Ok(Err(IssueError::Expired));
    }
    // 同じ人の発行を直列にして、上限を超えないようにする
    sqlx::query("SELECT id FROM accounts_user WHERE id = $1 FOR UPDATE")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    let active: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ai_agent_api_keys WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await?;
    if active >= MAX_ACTIVE_KEYS {
        return Ok(Err(IssueError::TooManyKeys));
    }

    let plain_key = ai_agent_key_repo::generate_plain_key();
    let key_hash = ai_agent_key_repo::hash_key(&plain_key);
    let key_prefix = ai_agent_key_repo::display_prefix(&plain_key);
    let key_id: i32 = sqlx::query_scalar(
        "INSERT INTO ai_agent_api_keys (user_id, key_hash, key_prefix, label, created_by)
         VALUES ($1, $2, $3, $4, $1) RETURNING id::int4",
    )
    .bind(user_id)
    .bind(&key_hash)
    .bind(&key_prefix)
    .bind(label)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE ai_mcp_access_request SET status = 'issued', issued_key_id = $2, issued_at = NOW()
         WHERE id = $1",
    )
    .bind(id)
    .bind(key_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Ok(IssuedKey {
        key_id,
        plain_key,
        key_prefix,
    }))
}

#[derive(Debug, Clone, Serialize)]
pub struct MyKeyOut {
    pub id: i32,
    #[serde(rename = "keyPrefix")]
    pub key_prefix: String,
    pub label: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "lastUsedAt")]
    pub last_used_at: Option<DateTime<Utc>>,
    #[serde(rename = "revokedAt")]
    pub revoked_at: Option<DateTime<Utc>>,
}

/// 自分のキー一覧(平文は含まない)
pub async fn list_keys_for_user(pool: &PgPool, user_id: i32) -> anyhow::Result<Vec<MyKeyOut>> {
    let rows = sqlx::query(
        "SELECT id::int4, key_prefix, label, created_at, last_used_at, revoked_at
         FROM ai_agent_api_keys WHERE user_id = $1 ORDER BY created_at DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| MyKeyOut {
            id: r.get(0),
            key_prefix: r.get(1),
            label: r.get(2),
            created_at: r.get(3),
            last_used_at: r.get(4),
            revoked_at: r.get(5),
        })
        .collect())
}

/// 自分のキーを失効する。本人のキーでない・失効済みなら false。
pub async fn revoke_own_key(pool: &PgPool, key_id: i32, user_id: i32) -> anyhow::Result<bool> {
    let r = sqlx::query(
        "UPDATE ai_agent_api_keys SET revoked_at = NOW()
         WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL",
    )
    .bind(key_id)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(r.rows_affected() > 0)
}

/// 再通知が必要な申請(pending かつ 3日以上経過 かつ reminded_at IS NULL)の ID 一覧
pub async fn find_pending_needing_reminder(
    pool: &PgPool,
    now: DateTime<Utc>,
) -> anyhow::Result<Vec<i32>> {
    let three_days_ago = now - Duration::days(3);
    Ok(sqlx::query_scalar(
        "SELECT id::int4 FROM ai_mcp_access_request
         WHERE status = 'pending' AND created_at < $1 AND reminded_at IS NULL
         ORDER BY created_at ASC",
    )
    .bind(three_days_ago)
    .fetch_all(pool)
    .await?)
}

/// 申請を「再通知済み」にマークする。status='pending' かつ reminded_at IS NULL のときだけ更新し、成功時に true を返す。
pub async fn mark_reminded(pool: &PgPool, id: i32) -> anyhow::Result<bool> {
    let r = sqlx::query(
        "UPDATE ai_mcp_access_request SET reminded_at = NOW()
         WHERE id = $1 AND status = 'pending' AND reminded_at IS NULL",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(r.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;

    async fn approved_request(pool: &PgPool, user: i32, admin: i32) -> i32 {
        let id = create(pool, user, "テスト").await.unwrap().unwrap();
        assert!(approve(pool, id, admin, "ok").await.unwrap());
        id
    }

    #[tokio::test]
    async fn only_one_open_request_per_user() {
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let user = test_support::create_test_user(&pool, "mcpr1").await;
        let first = create(&pool, user, "a").await.unwrap();
        assert!(first.is_some());
        assert!(create(&pool, user, "b").await.unwrap().is_none());
        // 取り下げれば、また申請できる
        assert!(cancel(&pool, first.unwrap(), user).await.unwrap());
        assert!(create(&pool, user, "c").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn issue_requires_approval_and_works_once() {
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let user = test_support::create_test_user(&pool, "mcpr2").await;
        let admin = test_support::create_test_user(&pool, "mcpa2").await;
        let other = test_support::create_test_user(&pool, "mcpo2").await;
        let id = create(&pool, user, "x").await.unwrap().unwrap();
        // 承認前は発行できない
        assert_eq!(
            issue(&pool, id, user, None).await.unwrap().err(),
            Some(IssueError::NotApproved)
        );
        assert!(approve(&pool, id, admin, "").await.unwrap());
        // 二重承認はできない
        assert!(!approve(&pool, id, admin, "").await.unwrap());
        // 他人は発行できない
        assert_eq!(
            issue(&pool, id, other, None).await.unwrap().err(),
            Some(IssueError::NotFound)
        );
        // 本人は発行でき、そのキーで認証できる
        let key = issue(&pool, id, user, Some("私のMac"))
            .await
            .unwrap()
            .ok()
            .unwrap();
        let hit = ai_agent_key_repo::find_active_by_hash(
            &pool,
            &ai_agent_key_repo::hash_key(&key.plain_key),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(hit.user_id, user);
        assert_eq!(hit.id, key.key_id);
        // 同じ申請から二度は発行できない
        assert_eq!(
            issue(&pool, id, user, None).await.unwrap().err(),
            Some(IssueError::NotApproved)
        );
        let got = find_by_id(&pool, id).await.unwrap().unwrap();
        assert_eq!(got.status, "issued");
    }

    #[tokio::test]
    async fn expired_approval_cannot_issue_and_allows_new_request() {
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let user = test_support::create_test_user(&pool, "mcpr3").await;
        let admin = test_support::create_test_user(&pool, "mcpa3").await;
        let id = approved_request(&pool, user, admin).await;
        sqlx::query("UPDATE ai_mcp_access_request SET approval_expires_at = NOW() - INTERVAL '1 day' WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            issue(&pool, id, user, None).await.unwrap().err(),
            Some(IssueError::Expired)
        );
        assert_eq!(
            find_by_id(&pool, id).await.unwrap().unwrap().status,
            "expired"
        );
        // 期限切れは、再申請できる
        assert!(create(&pool, user, "再").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn active_key_limit_is_enforced_and_revoke_frees_a_slot() {
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let user = test_support::create_test_user(&pool, "mcpr4").await;
        let admin = test_support::create_test_user(&pool, "mcpa4").await;
        let mut first_key = 0;
        for i in 0..MAX_ACTIVE_KEYS {
            let id = approved_request(&pool, user, admin).await;
            let k = issue(&pool, id, user, None).await.unwrap().ok().unwrap();
            if i == 0 {
                first_key = k.key_id;
            }
        }
        let id = approved_request(&pool, user, admin).await;
        assert_eq!(
            issue(&pool, id, user, None).await.unwrap().err(),
            Some(IssueError::TooManyKeys)
        );
        // 他人のキーは失効できない。自分のは失効できる
        let stranger = test_support::create_test_user(&pool, "mcps4").await;
        assert!(!revoke_own_key(&pool, first_key, stranger).await.unwrap());
        assert!(revoke_own_key(&pool, first_key, user).await.unwrap());
        assert!(issue(&pool, id, user, None).await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn reject_only_applies_to_pending() {
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let user = test_support::create_test_user(&pool, "mcpr5").await;
        let admin = test_support::create_test_user(&pool, "mcpa5").await;
        let id = create(&pool, user, "x").await.unwrap().unwrap();
        assert!(reject(&pool, id, admin, "不要").await.unwrap());
        assert!(!reject(&pool, id, admin, "").await.unwrap());
        assert!(!approve(&pool, id, admin, "").await.unwrap());
        assert_eq!(
            issue(&pool, id, user, None).await.unwrap().err(),
            Some(IssueError::NotApproved)
        );
    }

    /// 申請を作り、作成日時を `days` 日前にずらす
    async fn pending_aged(pool: &PgPool, username_prefix: &str, days: i32) -> (i32, i32) {
        let user = test_support::create_test_user(pool, username_prefix).await;
        let id = create(pool, user, "テスト").await.unwrap().unwrap();
        sqlx::query(
            "UPDATE ai_mcp_access_request SET created_at = NOW() - make_interval(days => $2) WHERE id = $1",
        )
        .bind(id)
        .bind(days)
        .execute(pool)
        .await
        .unwrap();
        (id, user)
    }

    #[tokio::test]
    async fn find_pending_needing_reminder_filters_correctly() {
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let (young, _) = pending_aged(&pool, "rem_young", 2).await; // 3日未満 → 対象外
        let (old, _) = pending_aged(&pool, "rem_old", 4).await; // 対象
        let (reminded, _) = pending_aged(&pool, "rem_done", 4).await; // 再通知済み → 対象外
        let (approved, _) = pending_aged(&pool, "rem_appr", 4).await; // 承認済み → 対象外
        let admin = test_support::create_test_user(&pool, "rem_admin").await;
        assert!(mark_reminded(&pool, reminded).await.unwrap());
        assert!(approve(&pool, approved, admin, "").await.unwrap());

        // 判定の基準時刻は、作成のあとに取る(同時刻の申請が境目に紛れないようにする)
        let needing = find_pending_needing_reminder(&pool, Utc::now())
            .await
            .unwrap();
        assert!(needing.contains(&old), "4日前の申請は対象");
        assert!(!needing.contains(&young), "2日前の申請は対象外");
        assert!(!needing.contains(&reminded), "再通知済みは対象外");
        assert!(!needing.contains(&approved), "承認済みは対象外");
    }

    #[tokio::test]
    async fn mark_reminded_works_once() {
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let user = test_support::create_test_user(&pool, "remind_mark_user").await;
        let id = create(&pool, user, "テスト").await.unwrap().unwrap();

        // 1回目は成功
        assert!(mark_reminded(&pool, id).await.unwrap());

        // 2回目は失敗(既に mark 済み)
        assert!(!mark_reminded(&pool, id).await.unwrap());

        // status='pending' でないと mark できない
        let admin = test_support::create_test_user(&pool, "remind_mark_admin").await;
        assert!(reject(&pool, id, admin, "却下").await.unwrap());
        assert!(!mark_reminded(&pool, id).await.unwrap());
    }
}
