/// infrastructure/repositories/ai_mcp_pairing_repo.rs — 端末の接続(DEMO-000192 フェーズ4)
///
/// 利用者の Mac の MCP が作ったキーの「指紋」だけを預かり、申請の承認と同時にキーとして登録する。
/// キーの平文は、このサーバーには届かない。
///
/// 状態: waiting(MCP が開始) → requested(利用者が画面で申請) → approved(承認でキーを登録)
///        requested → rejected(却下・取り下げ) / failed(登録できなかった。上限など)
/// 「期限切れ」は、waiting のまま expires_at を過ぎたもの(状態にしない)。
use chrono::{DateTime, Duration, Utc};
use rand::seq::SliceRandom;
use rand::RngCore;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};

use crate::infrastructure::repositories::ai_mcp_request_repo::MAX_ACTIVE_KEYS;

/// 確認コードの有効期間(分)。申請されるまでの時間
pub const CODE_VALID_MINUTES: i64 = 15;
/// 1時間に作れる確認コードの上限(全体。乱用の歯止め)
pub const MAX_STARTS_PER_HOUR: i64 = 60;
pub const LABEL_MAX_CHARS: usize = 100;

/// 紛らわしい文字(0 O 1 I L)を除いた文字
const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";

/// 確認コード(XXXX-XXXX)を作る
pub fn generate_code() -> String {
    let mut rng = rand::thread_rng();
    let pick = |rng: &mut rand::rngs::ThreadRng| *CODE_ALPHABET.choose(rng).unwrap() as char;
    let a: String = (0..4).map(|_| pick(&mut rng)).collect();
    let b: String = (0..4).map(|_| pick(&mut rng)).collect();
    format!("{a}-{b}")
}

/// 画面で入力・貼り付けされた確認コードを、保存している形(大文字・ハイフン付き)にそろえる
pub fn normalize_code(input: &str) -> String {
    let cleaned: String = input
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if cleaned.len() == 8 {
        format!("{}-{}", &cleaned[..4], &cleaned[4..])
    } else {
        cleaned
    }
}

/// MCP だけが持つ秘密(平文)を作る
pub fn generate_device_secret() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

pub fn hash_secret(secret: &str) -> String {
    let mut h = Sha256::new();
    h.update(secret.as_bytes());
    h.finalize().iter().map(|b| format!("{:02x}", b)).collect()
}

/// 定数時間の比較(長さが違えば false)
fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

/// 指紋(SHA-256 の16進・小文字64文字)の形か
pub fn is_valid_key_hash(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// 表示用の先頭(`senn_ai_` で始まる12文字)の形か
pub fn is_valid_key_prefix(s: &str) -> bool {
    s.len() == 12
        && s.starts_with("senn_ai_")
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

#[derive(Debug, Clone)]
pub struct Pairing {
    pub id: i32,
    pub code: String,
    pub device_secret_hash: String,
    pub key_hash: String,
    pub key_prefix: String,
    pub label: String,
    pub status: String,
    pub message: Option<String>,
    pub user_id: Option<i32>,
    pub request_id: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl Pairing {
    /// 画面・MCP に返す状態。waiting で期限を過ぎたものは expired
    pub fn effective_status(&self, now: DateTime<Utc>) -> &str {
        if self.status == "waiting" && self.expires_at < now {
            "expired"
        } else {
            &self.status
        }
    }
}

const COLUMNS: &str = "id::int4, code, device_secret_hash, key_hash, key_prefix, label, status, message, user_id::int4, request_id::int4, created_at, expires_at";

fn row_to_pairing(r: &sqlx::postgres::PgRow) -> Pairing {
    Pairing {
        id: r.get(0),
        code: r.get(1),
        device_secret_hash: r.get(2),
        key_hash: r.get(3),
        key_prefix: r.get(4),
        label: r.get(5),
        status: r.get(6),
        message: r.get(7),
        user_id: r.get(8),
        request_id: r.get(9),
        created_at: r.get(10),
        expires_at: r.get(11),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum StartError {
    /// 同じキー(指紋)がすでにある
    KeyExists,
    /// 確認コードが重なった(呼び出し側が作り直す)
    CodeCollision,
}

pub struct Started {
    pub code: String,
    pub device_secret: String,
}

/// 1時間に作った確認コードの数が、上限に達しているか(乱用の歯止め。呼び出し側が、開始の前に確認する)
pub async fn too_many_recent_starts(pool: &PgPool) -> anyhow::Result<bool> {
    let recent: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ai_mcp_pairing WHERE created_at > NOW() - INTERVAL '1 hour'",
    )
    .fetch_one(pool)
    .await?;
    Ok(recent >= MAX_STARTS_PER_HOUR)
}

/// MCP が接続を始める。確認コードと、MCP だけが持つ秘密を返す(秘密はここで一度だけ)。
pub async fn start(
    pool: &PgPool,
    key_hash: &str,
    key_prefix: &str,
    label: &str,
) -> anyhow::Result<Result<Started, StartError>> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM ai_mcp_pairing WHERE key_hash = $1)
             OR EXISTS (SELECT 1 FROM ai_agent_api_keys WHERE key_hash = $1)",
    )
    .bind(key_hash)
    .fetch_one(pool)
    .await?;
    if exists {
        return Ok(Err(StartError::KeyExists));
    }
    let code = generate_code();
    let device_secret = generate_device_secret();
    let expires = Utc::now() + Duration::minutes(CODE_VALID_MINUTES);
    let inserted = sqlx::query(
        "INSERT INTO ai_mcp_pairing (code, device_secret_hash, key_hash, key_prefix, label, status, expires_at)
         VALUES ($1, $2, $3, $4, $5, 'waiting', $6)
         ON CONFLICT (code) DO NOTHING",
    )
    .bind(&code)
    .bind(hash_secret(&device_secret))
    .bind(key_hash)
    .bind(key_prefix)
    .bind(label)
    .bind(expires)
    .execute(pool)
    .await?;
    if inserted.rows_affected() == 0 {
        return Ok(Err(StartError::CodeCollision));
    }
    Ok(Ok(Started {
        code,
        device_secret,
    }))
}

pub async fn find_by_code(pool: &PgPool, code: &str) -> anyhow::Result<Option<Pairing>> {
    let row = sqlx::query(&format!(
        "SELECT {COLUMNS} FROM ai_mcp_pairing WHERE code = $1"
    ))
    .bind(normalize_code(code))
    .fetch_optional(pool)
    .await?;
    Ok(row.as_ref().map(row_to_pairing))
}

/// MCP からの状態の問い合わせ。確認コードと秘密の両方が合うときだけ返す(合わなければ None。理由は区別しない)。
pub async fn find_for_device(
    pool: &PgPool,
    code: &str,
    device_secret: &str,
) -> anyhow::Result<Option<Pairing>> {
    let p = find_by_code(pool, code).await?;
    Ok(p.filter(|p| constant_time_eq(&p.device_secret_hash, &hash_secret(device_secret))))
}

#[derive(Debug, PartialEq, Eq)]
pub enum ApplyError {
    NotFound,
    /// すでに申請済み(別の人、または処理済み)
    NotWaiting,
    Expired,
    /// 有効なキーが上限
    TooManyKeys,
    /// 申請中・承認済みの申請が、このコードとは別にすでにある(1人1件)
    RequestExists,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Applied {
    pub request_id: i32,
    /// 申請を新しく作ったか(true なら、管理者への通知が必要)
    pub new_request: bool,
    /// その場でキーを登録できたか(すでに承認済みの申請があった)
    pub registered: bool,
}

/// キーを登録する(承認のあと、または承認済みの申請があるとき)。トランザクション内で呼ぶ。
/// 上限に達していれば、登録せず、接続を failed にして false を返す。
async fn register_key(
    tx: &mut Transaction<'_, Postgres>,
    pairing: &Pairing,
    user_id: i32,
    request_id: i32,
) -> anyhow::Result<bool> {
    sqlx::query("SELECT id FROM accounts_user WHERE id = $1 FOR UPDATE")
        .bind(user_id)
        .execute(&mut **tx)
        .await?;
    let active: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ai_agent_api_keys WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(user_id)
    .fetch_one(&mut **tx)
    .await?;
    if active >= MAX_ACTIVE_KEYS {
        sqlx::query("UPDATE ai_mcp_pairing SET status = 'failed', message = $2 WHERE id = $1")
            .bind(pairing.id)
            .bind(format!(
                "有効なキーが上限({}本)に達しています。設定の「詳細」で、使わないキーを失効してから、もう一度接続してください",
                MAX_ACTIVE_KEYS
            ))
            .execute(&mut **tx)
            .await?;
        return Ok(false);
    }
    let label = if pairing.label.is_empty() {
        None
    } else {
        Some(pairing.label.clone())
    };
    let key_id: i32 = sqlx::query_scalar(
        "INSERT INTO ai_agent_api_keys (user_id, key_hash, key_prefix, label, created_by)
         VALUES ($1, $2, $3, $4, $1) RETURNING id::int4",
    )
    .bind(user_id)
    .bind(&pairing.key_hash)
    .bind(&pairing.key_prefix)
    .bind(label)
    .fetch_one(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE ai_mcp_access_request SET status = 'issued', issued_key_id = $2, issued_at = NOW()
         WHERE id = $1",
    )
    .bind(request_id)
    .bind(key_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE ai_mcp_pairing SET status = 'approved', key_id = $2, user_id = $3, request_id = $4, message = NULL
         WHERE id = $1",
    )
    .bind(pairing.id)
    .bind(key_id)
    .bind(user_id)
    .bind(request_id)
    .execute(&mut **tx)
    .await?;
    Ok(true)
}

/// 利用者が、画面で「申請する」を押した。
/// - 承認済み(未発行)の申請がある → その場でキーを登録する(管理者の承認は、すでに済んでいる)
/// - 申請中の申請がある → その申請に、この接続を結び付ける
/// - どちらも無い → 申請を新しく作って、結び付ける
pub async fn apply(
    pool: &PgPool,
    code: &str,
    user_id: i32,
) -> anyhow::Result<Result<Applied, ApplyError>> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query(&format!(
        "SELECT {COLUMNS} FROM ai_mcp_pairing WHERE code = $1 FOR UPDATE"
    ))
    .bind(normalize_code(code))
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        return Ok(Err(ApplyError::NotFound));
    };
    let pairing = row_to_pairing(&row);
    if pairing.status != "waiting" {
        return Ok(Err(ApplyError::NotWaiting));
    }
    if pairing.expires_at < Utc::now() {
        return Ok(Err(ApplyError::Expired));
    }

    // 期限切れの承認は、再申請できるようにたたむ(申請の作成と同じ扱い)
    sqlx::query(
        "UPDATE ai_mcp_access_request SET status = 'cancelled'
         WHERE user_id = $1 AND status = 'approved' AND approval_expires_at < NOW()",
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await?;

    let open = sqlx::query(
        "SELECT id::int4, status FROM ai_mcp_access_request
         WHERE user_id = $1 AND status IN ('pending', 'approved') FOR UPDATE",
    )
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;

    let result = match open {
        Some(r) => {
            let request_id: i32 = r.get(0);
            let status: String = r.get(1);
            if status == "approved" {
                if !register_key(&mut tx, &pairing, user_id, request_id).await? {
                    tx.commit().await?;
                    return Ok(Err(ApplyError::TooManyKeys));
                }
                Applied {
                    request_id,
                    new_request: false,
                    registered: true,
                }
            } else {
                // 申請中: この接続を結び付ける。すでに別の接続が結び付いていれば、1人1件の規則で断る
                let taken: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM ai_mcp_pairing WHERE request_id = $1 AND status = 'requested')",
                )
                .bind(request_id)
                .fetch_one(&mut *tx)
                .await?;
                if taken {
                    return Ok(Err(ApplyError::RequestExists));
                }
                sqlx::query(
                    "UPDATE ai_mcp_pairing SET status = 'requested', user_id = $2, request_id = $3 WHERE id = $1",
                )
                .bind(pairing.id)
                .bind(user_id)
                .bind(request_id)
                .execute(&mut *tx)
                .await?;
                Applied {
                    request_id,
                    new_request: false,
                    registered: false,
                }
            }
        }
        None => {
            let request_id: i32 = sqlx::query_scalar(
                "INSERT INTO ai_mcp_access_request (user_id, purpose, status)
                 VALUES ($1, $2, 'pending') RETURNING id::int4",
            )
            .bind(user_id)
            .bind(format!("端末の接続: {}", pairing.label))
            .fetch_one(&mut *tx)
            .await?;
            sqlx::query(
                "UPDATE ai_mcp_pairing SET status = 'requested', user_id = $2, request_id = $3 WHERE id = $1",
            )
            .bind(pairing.id)
            .bind(user_id)
            .bind(request_id)
            .execute(&mut *tx)
            .await?;
            Applied {
                request_id,
                new_request: true,
                registered: false,
            }
        }
    };
    tx.commit().await?;
    Ok(Ok(result))
}

/// 申請が承認された。結び付いた接続があれば、キーを登録する。
/// 戻り値: 接続があったか(Some(true)=登録した / Some(false)=上限などで失敗 / None=結び付いた接続なし)
pub async fn on_request_approved(pool: &PgPool, request_id: i32) -> anyhow::Result<Option<bool>> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query(&format!(
        "SELECT {COLUMNS} FROM ai_mcp_pairing WHERE request_id = $1 AND status = 'requested' FOR UPDATE"
    ))
    .bind(request_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let pairing = row_to_pairing(&row);
    let Some(user_id) = pairing.user_id else {
        return Ok(None);
    };
    let ok = register_key(&mut tx, &pairing, user_id, request_id).await?;
    tx.commit().await?;
    Ok(Some(ok))
}

/// 申請が却下・取り下げされた。結び付いた接続を rejected にする。
pub async fn on_request_closed(
    pool: &PgPool,
    request_id: i32,
    message: &str,
) -> anyhow::Result<bool> {
    let r = sqlx::query(
        "UPDATE ai_mcp_pairing SET status = 'rejected', message = $2
         WHERE request_id = $1 AND status = 'requested'",
    )
    .bind(request_id)
    .bind(message)
    .execute(pool)
    .await?;
    Ok(r.rows_affected() > 0)
}

#[derive(Debug, Clone, Serialize)]
pub struct PairingView {
    pub code: String,
    pub label: String,
    pub status: String,
    pub message: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
}

impl Pairing {
    pub fn view(&self, now: DateTime<Utc>) -> PairingView {
        PairingView {
            code: self.code.clone(),
            label: self.label.clone(),
            status: self.effective_status(now).to_string(),
            message: self.message.clone(),
            created_at: self.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::repositories::{ai_agent_key_repo, ai_mcp_request_repo};
    use crate::test_support;

    #[test]
    fn code_has_the_expected_shape_and_normalizes() {
        let c = generate_code();
        assert_eq!(c.len(), 9);
        assert_eq!(&c[4..5], "-");
        assert!(c
            .replace('-', "")
            .bytes()
            .all(|b| CODE_ALPHABET.contains(&b)));
        assert_eq!(normalize_code("abcd efgh"), "ABCD-EFGH");
        assert_eq!(normalize_code("ABCD-EFGH"), "ABCD-EFGH");
    }

    #[test]
    fn key_hash_and_prefix_shapes_are_checked() {
        assert!(is_valid_key_hash(&hash_secret("x")));
        assert!(!is_valid_key_hash("ABC"));
        assert!(!is_valid_key_hash(&hash_secret("x").to_uppercase()));
        assert!(is_valid_key_prefix("senn_ai_abcd"));
        assert!(!is_valid_key_prefix("other_abcdef"));
        assert!(!is_valid_key_prefix("senn_ai_abc"));
    }

    struct P {
        code: String,
        secret: String,
        key_hash: String,
    }

    async fn new_pairing(pool: &PgPool) -> P {
        let unique = test_support::unique_suffix();
        let key_hash = hash_secret(&format!("key-{unique}"));
        let s = start(pool, &key_hash, "senn_ai_abcd", "テスト用Mac")
            .await
            .unwrap()
            .ok()
            .unwrap();
        P {
            code: s.code,
            secret: s.device_secret,
            key_hash,
        }
    }

    async fn key_owner(pool: &PgPool, key_hash: &str) -> Option<i32> {
        ai_agent_key_repo::find_active_by_hash(pool, key_hash)
            .await
            .unwrap()
            .map(|k| k.user_id)
    }

    #[tokio::test]
    async fn status_needs_both_the_code_and_the_secret() {
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let p = new_pairing(&pool).await;
        assert!(find_for_device(&pool, &p.code, &p.secret)
            .await
            .unwrap()
            .is_some());
        assert!(find_for_device(&pool, &p.code, "wrong")
            .await
            .unwrap()
            .is_none());
        assert!(find_for_device(&pool, "ZZZZ-ZZZZ", &p.secret)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn applying_creates_a_request_and_approval_registers_the_key() {
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let user = test_support::create_test_user(&pool, "pair1").await;
        let admin = test_support::create_test_user(&pool, "pairadm1").await;
        let p = new_pairing(&pool).await;

        let applied = apply(&pool, &p.code, user).await.unwrap().ok().unwrap();
        assert!(applied.new_request && !applied.registered);
        // 承認の前は、キーは使えない
        assert_eq!(key_owner(&pool, &p.key_hash).await, None);
        let st = find_by_code(&pool, &p.code).await.unwrap().unwrap();
        assert_eq!(st.status, "requested");

        assert!(
            ai_mcp_request_repo::approve(&pool, applied.request_id, admin, "")
                .await
                .unwrap()
        );
        assert_eq!(
            on_request_approved(&pool, applied.request_id)
                .await
                .unwrap(),
            Some(true)
        );
        // 承認の瞬間に、Mac が作ったキーが、その人のキーとして使える
        assert_eq!(key_owner(&pool, &p.key_hash).await, Some(user));
        let st = find_by_code(&pool, &p.code).await.unwrap().unwrap();
        assert_eq!(st.status, "approved");
        let req = ai_mcp_request_repo::find_by_id(&pool, applied.request_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(req.status, "issued");
    }

    #[tokio::test]
    async fn an_already_approved_request_registers_the_key_right_away() {
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let user = test_support::create_test_user(&pool, "pair2").await;
        let admin = test_support::create_test_user(&pool, "pairadm2").await;
        let rid = ai_mcp_request_repo::create(&pool, user, "")
            .await
            .unwrap()
            .unwrap();
        assert!(ai_mcp_request_repo::approve(&pool, rid, admin, "")
            .await
            .unwrap());
        let p = new_pairing(&pool).await;
        let applied = apply(&pool, &p.code, user).await.unwrap().ok().unwrap();
        assert!(applied.registered && !applied.new_request);
        assert_eq!(key_owner(&pool, &p.key_hash).await, Some(user));
    }

    #[tokio::test]
    async fn one_open_request_per_user_and_codes_are_single_use() {
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let user = test_support::create_test_user(&pool, "pair3").await;
        let p1 = new_pairing(&pool).await;
        let p2 = new_pairing(&pool).await;
        assert!(apply(&pool, &p1.code, user).await.unwrap().is_ok());
        // 同じコードは二度使えない
        assert_eq!(
            apply(&pool, &p1.code, user).await.unwrap().err(),
            Some(ApplyError::NotWaiting)
        );
        // 申請中に、別の端末のコードで申請することはできない(1人1件)
        assert_eq!(
            apply(&pool, &p2.code, user).await.unwrap().err(),
            Some(ApplyError::RequestExists)
        );
        assert_eq!(
            apply(&pool, "ZZZZ-ZZZZ", user).await.unwrap().err(),
            Some(ApplyError::NotFound)
        );
    }

    #[tokio::test]
    async fn expired_codes_cannot_be_applied_and_key_hashes_are_unique() {
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let user = test_support::create_test_user(&pool, "pair4").await;
        let p = new_pairing(&pool).await;
        sqlx::query(
            "UPDATE ai_mcp_pairing SET expires_at = NOW() - INTERVAL '1 minute' WHERE code = $1",
        )
        .bind(&p.code)
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            find_for_device(&pool, &p.code, &p.secret)
                .await
                .unwrap()
                .unwrap()
                .view(Utc::now())
                .status,
            "expired"
        );
        assert_eq!(
            apply(&pool, &p.code, user).await.unwrap().err(),
            Some(ApplyError::Expired)
        );
        // 同じ指紋は、もう一度開始できない
        assert_eq!(
            start(&pool, &p.key_hash, "senn_ai_abcd", "x")
                .await
                .unwrap()
                .err(),
            Some(StartError::KeyExists)
        );
    }

    #[tokio::test]
    async fn the_active_key_limit_makes_the_pairing_fail_without_registering() {
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let user = test_support::create_test_user(&pool, "pair5").await;
        let admin = test_support::create_test_user(&pool, "pairadm5").await;
        for i in 0..MAX_ACTIVE_KEYS {
            sqlx::query(
                "INSERT INTO ai_agent_api_keys (user_id, key_hash, key_prefix, created_by) VALUES ($1, $2, 'senn_ai_xxxx', $1)",
            )
            .bind(user)
            .bind(hash_secret(&format!("existing-{i}-{}", test_support::unique_suffix())))
            .execute(&pool)
            .await
            .unwrap();
        }
        let p = new_pairing(&pool).await;
        let applied = apply(&pool, &p.code, user).await.unwrap().ok().unwrap();
        assert!(
            ai_mcp_request_repo::approve(&pool, applied.request_id, admin, "")
                .await
                .unwrap()
        );
        assert_eq!(
            on_request_approved(&pool, applied.request_id)
                .await
                .unwrap(),
            Some(false)
        );
        assert_eq!(key_owner(&pool, &p.key_hash).await, None);
        let st = find_by_code(&pool, &p.code).await.unwrap().unwrap();
        assert_eq!(st.status, "failed");
        assert!(st.message.unwrap().contains("上限"));
    }

    #[tokio::test]
    async fn rejecting_or_cancelling_the_request_closes_the_pairing() {
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let user = test_support::create_test_user(&pool, "pair6").await;
        let admin = test_support::create_test_user(&pool, "pairadm6").await;
        let p = new_pairing(&pool).await;
        let applied = apply(&pool, &p.code, user).await.unwrap().ok().unwrap();
        assert!(
            ai_mcp_request_repo::reject(&pool, applied.request_id, admin, "見送り")
                .await
                .unwrap()
        );
        assert!(
            on_request_closed(&pool, applied.request_id, "却下されました。理由: 見送り")
                .await
                .unwrap()
        );
        let st = find_by_code(&pool, &p.code).await.unwrap().unwrap();
        assert_eq!(st.status, "rejected");
        assert_eq!(key_owner(&pool, &p.key_hash).await, None);
        // 承認の通知が来ても、却下された接続は登録されない
        assert_eq!(
            on_request_approved(&pool, applied.request_id)
                .await
                .unwrap(),
            None
        );
    }
}
