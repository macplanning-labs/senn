/// infrastructure/repositories/ai_agent_key_repo.rs — 人ごとのAIエージェント用APIキー永続化
///
/// キー平文はDBに保存しない。SHA-256ハッシュ(key_hash)のみ保存し、認証時はハッシュの完全一致
/// 検索(UNIQUE制約によるO(1) lookup)で照合する。対象は256bitの高エントロピーなランダムトークン
/// であり、bcrypt/argon2のような低速化(ストレッチング)は総当たり耐性の観点で不要かつ、
/// 「提示されたキーからDBの該当行を1発で特定する」という認証フローには不向き(ソルトが都度
/// 変わるため全件比較が必要になる)。詳細は docs/詳細設計書_AIエージェント人ごとAPIキー.md §3.2。
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Utc};
use rand::RngCore;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};

use crate::domain::models::ticket_api::UserSummaryOut;

/// 発行キーの表示用プレフィックス(GitHub PAT等と同様、鍵の識別しやすさのため)。
pub const KEY_PREFIX: &str = "senn_ai_";

/// 新規APIキー(平文)を生成する。戻り値は呼び出し元がユーザーに1度だけ表示するためのものであり、
/// DBにはこの値ではなく `hash_key()` の結果のみを保存すること。
pub fn generate_plain_key() -> String {
    let mut bytes = [0u8; 32]; // 256bit
    rand::thread_rng().fill_bytes(&mut bytes);
    format!("{}{}", KEY_PREFIX, URL_SAFE_NO_PAD.encode(bytes))
}

/// 平文キーのSHA-256ハッシュ(hex文字列、64文字)を計算する。ソルト無し(理由は本ファイル冒頭コメント参照)。
pub fn hash_key(plain: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(plain.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

/// 一覧表示用のプレフィックス(先頭12文字程度。単体では鍵の推測に使えない長さに留める)。
pub fn display_prefix(plain: &str) -> String {
    plain.chars().take(12).collect()
}

/// 認証成功時に必要な最小情報。
#[derive(Debug, Clone, Copy)]
pub struct ActiveKeyAuth {
    pub id: i32,
    pub user_id: i32,
}

/// 有効(revoked_at IS NULL)なキーをハッシュで検索する。認証のホットパスで呼ばれる。
pub async fn find_active_by_hash(
    pool: &PgPool,
    key_hash: &str,
) -> anyhow::Result<Option<ActiveKeyAuth>> {
    let row = sqlx::query(
        "SELECT id::int4, user_id::int4 FROM ai_agent_api_keys WHERE key_hash = $1 AND revoked_at IS NULL",
    )
    .bind(key_hash)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| ActiveKeyAuth {
        id: r.get(0),
        user_id: r.get(1),
    }))
}

/// 最終使用日時を更新する。認証成功後のbest-effort呼び出し想定(失敗しても認証自体は継続してよい)。
pub async fn touch_last_used(pool: &PgPool, id: i32) -> anyhow::Result<()> {
    sqlx::query("UPDATE ai_agent_api_keys SET last_used_at = NOW() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// 新規キーを発行する。呼び出し元は `generate_plain_key()` の結果をハッシュ化してから渡すこと。
pub async fn create(
    pool: &PgPool,
    user_id: i32,
    key_hash: &str,
    key_prefix: &str,
    label: Option<&str>,
    created_by: i32,
) -> anyhow::Result<i32> {
    let id: i32 = sqlx::query_scalar(
        "INSERT INTO ai_agent_api_keys (user_id, key_hash, key_prefix, label, created_by)
         VALUES ($1, $2, $3, $4, $5) RETURNING id::int4",
    )
    .bind(user_id)
    .bind(key_hash)
    .bind(key_prefix)
    .bind(label)
    .bind(created_by)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

#[derive(Debug, Clone, Serialize)]
pub struct AiAgentPersonalKeyOut {
    pub id: i32,
    pub user: UserSummaryOut,
    #[serde(rename = "keyPrefix")]
    pub key_prefix: String,
    pub label: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "lastUsedAt")]
    pub last_used_at: Option<DateTime<Utc>>,
    #[serde(rename = "revokedAt")]
    pub revoked_at: Option<DateTime<Utc>>,
    #[serde(rename = "createdBy")]
    pub created_by: Option<UserSummaryOut>,
}

/// staff向け管理画面用の一覧(平文は含まない)。
pub async fn list_all(pool: &PgPool) -> anyhow::Result<Vec<AiAgentPersonalKeyOut>> {
    let rows = sqlx::query(
        "SELECT k.id::int4, k.key_prefix, k.label, k.created_at, k.last_used_at, k.revoked_at,
                u.id::int4, u.username, u.email, u.display_name,
                cb.id::int4, cb.username, cb.email, cb.display_name
         FROM ai_agent_api_keys k
         JOIN accounts_user u ON k.user_id = u.id
         LEFT JOIN accounts_user cb ON k.created_by = cb.id
         ORDER BY k.created_at DESC",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let created_by_id: Option<i32> = r.get(10);
            AiAgentPersonalKeyOut {
                id: r.get(0),
                key_prefix: r.get(1),
                label: r.get(2),
                created_at: r.get(3),
                last_used_at: r.get(4),
                revoked_at: r.get(5),
                user: UserSummaryOut {
                    id: r.get(6),
                    username: r.get(7),
                    email: r.get(8),
                    display_name: r.get(9),
                },
                created_by: created_by_id.map(|id| UserSummaryOut {
                    id,
                    username: r.get(11),
                    email: r.get(12),
                    display_name: r.get(13),
                }),
            }
        })
        .collect())
}

/// 失効(ソフト削除)。既に失効済み/存在しない場合は false を返す(冪等)。
pub async fn revoke(pool: &PgPool, id: i32) -> anyhow::Result<bool> {
    let result = sqlx::query(
        "UPDATE ai_agent_api_keys SET revoked_at = NOW() WHERE id = $1 AND revoked_at IS NULL",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}
