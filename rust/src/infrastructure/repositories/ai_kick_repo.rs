/// infrastructure/repositories/ai_kick_repo.rs — 「AIに対応させる」(社内ランナー方式)の永続化
///
/// 設計: docs/design/実装計画_AI自動ワークフローとInsights.md §3.6。
/// - 設定(有効・緊急停止・上限)は `system_settings` に保存する。
/// - ランナーのトークンは**ハッシュだけ**を保存する(生の値は保存しない)。
/// - 回数・予算の集計は `t_ai_run` から数える。**確認と挿入は 1 つのトランザクションで直列に行う**
///   (同時に押されても、上限を超えない)。
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgConnection, PgPool};
use uuid::Uuid;

use super::ai_run_repo::AiRun;
use super::system_settings_repo;
use crate::domain::services::ai_kick_policy::{Caps, Limits, PriorityClass, Snapshot};

pub const KEY_ENABLED: &str = "sys.ai.kick.enabled";
pub const KEY_KILLED: &str = "sys.ai.kick.killed";
pub const KEY_LIMITS: &str = "sys.ai.kick.limits";
/// AI が作業してよいリポジトリ(システム管理者が決める。JSON の文字列配列)
pub const KEY_ALLOWED_REPOS: &str = "sys.ai.kick.allowed_repos";

/// 起動の確認と挿入を直列にするためのロックの鍵(アドバイザリロック)。
const KICK_LOCK_KEY: i64 = 0x5E44_414B_4943_4B01;

// ---- 設定 ------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq)]
pub struct KickSettings {
    pub enabled: bool,
    pub killed: bool,
    /// 保存されていなければ None(=有効にできない・起動できない)
    pub limits: Option<Limits>,
    /// AI が作業してよいリポジトリ(owner/name)。空なら、どのリポジトリでも起動できない。
    /// プロジェクトのオーナーは、この中からしか選べない(ほかのチームのリポジトリを指定させないため)
    pub allowed_repos: Vec<String>,
}

impl KickSettings {
    /// リポジトリが許可されているか(大文字・小文字は区別しない。GitHub と同じ)
    pub fn repo_allowed(&self, repo: &str) -> bool {
        let repo = repo.trim();
        !repo.is_empty()
            && self
                .allowed_repos
                .iter()
                .any(|r| r.eq_ignore_ascii_case(repo))
    }
}

pub async fn get_settings(pool: &PgPool) -> anyhow::Result<KickSettings> {
    let enabled = system_settings_repo::get(pool, KEY_ENABLED)
        .await?
        .as_deref()
        == Some("true");
    let killed = system_settings_repo::get(pool, KEY_KILLED)
        .await?
        .as_deref()
        == Some("true");
    let limits = match system_settings_repo::get(pool, KEY_LIMITS).await? {
        Some(json) if !json.is_empty() => match serde_json::from_str::<Limits>(&json) {
            Ok(l) if l.validate().is_ok() => Some(l),
            // 壊れた・不正な値は「未設定」として扱う(安全側に倒す=起動できない)
            _ => None,
        },
        _ => None,
    };
    // 壊れた値は「空」として扱う(安全側に倒す=起動できない)
    let allowed_repos = system_settings_repo::get(pool, KEY_ALLOWED_REPOS)
        .await?
        .and_then(|json| serde_json::from_str::<Vec<String>>(&json).ok())
        .unwrap_or_default();
    Ok(KickSettings {
        enabled,
        killed,
        limits,
        allowed_repos,
    })
}

pub async fn set_allowed_repos(pool: &PgPool, repos: &[String]) -> anyhow::Result<()> {
    system_settings_repo::set(pool, KEY_ALLOWED_REPOS, &serde_json::to_string(repos)?).await
}

pub async fn set_enabled(pool: &PgPool, enabled: bool) -> anyhow::Result<()> {
    system_settings_repo::set(pool, KEY_ENABLED, if enabled { "true" } else { "false" }).await
}

pub async fn set_killed(pool: &PgPool, killed: bool) -> anyhow::Result<()> {
    system_settings_repo::set(pool, KEY_KILLED, if killed { "true" } else { "false" }).await
}

pub async fn set_limits(pool: &PgPool, limits: &Limits) -> anyhow::Result<()> {
    system_settings_repo::set(pool, KEY_LIMITS, &serde_json::to_string(limits)?).await
}

// ---- プロジェクトの設定 ------------------------------------------------------

#[derive(Debug, Clone, FromRow, PartialEq, Eq)]
pub struct ProjectKick {
    pub ai_kick_enabled: bool,
    pub ai_kick_repo: String,
    pub ai_kick_base_branch: String,
}

pub async fn get_project_kick(
    pool: &PgPool,
    project_id: i32,
) -> anyhow::Result<Option<ProjectKick>> {
    Ok(sqlx::query_as::<_, ProjectKick>(
        "SELECT ai_kick_enabled, ai_kick_repo, ai_kick_base_branch FROM tickets_project WHERE id = $1",
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await?)
}

pub async fn set_project_kick(
    pool: &PgPool,
    project_id: i32,
    enabled: bool,
    repo: &str,
    base_branch: &str,
) -> anyhow::Result<bool> {
    let n = sqlx::query(
        "UPDATE tickets_project SET ai_kick_enabled = $2, ai_kick_repo = $3, ai_kick_base_branch = $4 WHERE id = $1",
    )
    .bind(project_id)
    .bind(enabled)
    .bind(repo)
    .bind(base_branch)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(n > 0)
}

// ---- 起動の確認に使う集計 ------------------------------------------------------

/// 今日(JST)の 0 時(UTC の時刻)を返す SQL の式。
const TODAY_START_JST: &str =
    "(date_trunc('day', NOW() AT TIME ZONE 'Asia/Tokyo') AT TIME ZONE 'Asia/Tokyo')";

pub async fn lock_for_kick(conn: &mut PgConnection) -> anyhow::Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(KICK_LOCK_KEY)
        .execute(conn)
        .await?;
    Ok(())
}

/// 起動の可否の判定に使う現在の状況を集める(トランザクション内で呼ぶ)。
pub async fn snapshot(
    conn: &mut PgConnection,
    settings: &KickSettings,
    project: &ProjectKick,
    project_id: i32,
    ticket_id: i64,
    user_id: i64,
) -> anyhow::Result<Snapshot> {
    let user_hour: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM t_ai_run WHERE queued_at > NOW() - interval '1 hour' AND triggered_by_id = $1",
    )
    .bind(user_id)
    .fetch_one(&mut *conn)
    .await?;
    let user_day: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM t_ai_run WHERE queued_at > NOW() - interval '24 hours' AND triggered_by_id = $1",
    )
    .bind(user_id)
    .fetch_one(&mut *conn)
    .await?;
    let ticket_24h: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM t_ai_run WHERE queued_at > NOW() - interval '24 hours' AND ticket_id = $1",
    )
    .bind(ticket_id)
    .fetch_one(&mut *conn)
    .await?;
    // 取得待ちまたは実行中(完了・停止していない)
    let ticket_active: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM t_ai_run WHERE queued_at IS NOT NULL AND result IS NULL AND stop_requested_at IS NULL AND ticket_id = $1",
    )
    .bind(ticket_id)
    .fetch_one(&mut *conn)
    .await?;
    let queue_len: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM t_ai_run WHERE queued_at IS NOT NULL AND claimed_at IS NULL AND result IS NULL AND stop_requested_at IS NULL",
    )
    .fetch_one(&mut *conn)
    .await?;
    let budget_system = budget_used_system(&mut *conn).await?;
    let budget_project = budget_used_project(&mut *conn, project_id).await?;
    let budget_week = budget_used_system_week(&mut *conn).await?;
    let c = |v: i64| u32::try_from(v.max(0)).unwrap_or(u32::MAX);
    Ok(Snapshot {
        system_enabled: settings.enabled,
        killed: settings.killed,
        project_enabled: project.ai_kick_enabled,
        project_repo_set: !project.ai_kick_repo.trim().is_empty(),
        project_repo_allowed: settings.repo_allowed(&project.ai_kick_repo),
        user_last_hour: c(user_hour),
        user_last_day: c(user_day),
        ticket_last_24h: c(ticket_24h),
        ticket_active: c(ticket_active),
        queue_len: c(queue_len),
        budget_system_used: u64::try_from(budget_system.max(0)).unwrap_or(0),
        budget_project_used: u64::try_from(budget_project.max(0)).unwrap_or(0),
        budget_system_week_used: u64::try_from(budget_week.max(0)).unwrap_or(0),
    })
}

/// 今日(JST)に起動された実行の加重トークンの合計(全体)。
pub async fn budget_used_system(conn: &mut PgConnection) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(&format!(
        "SELECT COALESCE(SUM(weighted_tokens), 0)::int8 FROM t_ai_run
         WHERE queued_at IS NOT NULL AND queued_at >= {TODAY_START_JST}"
    ))
    .fetch_one(conn)
    .await?)
}

/// 直近 7 日間(ローリング)の加重トークンの合計(全体)。
pub async fn budget_used_system_week(conn: &mut PgConnection) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT COALESCE(SUM(weighted_tokens), 0)::int8 FROM t_ai_run
         WHERE queued_at IS NOT NULL AND queued_at >= NOW() - interval '7 days'",
    )
    .fetch_one(conn)
    .await?)
}

/// 同じく、プロジェクトの分。
pub async fn budget_used_project(conn: &mut PgConnection, project_id: i32) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(&format!(
        "SELECT COALESCE(SUM(r.weighted_tokens), 0)::int8
         FROM t_ai_run r JOIN tickets_ticket t ON t.id = r.ticket_id
         WHERE r.queued_at IS NOT NULL AND r.queued_at >= {TODAY_START_JST} AND t.project_id = $1"
    ))
    .bind(i64::from(project_id))
    .fetch_one(conn)
    .await?)
}

/// チケットの優先度(`urgent` / `high` / `medium` / `low`)。
pub async fn ticket_priority(pool: &PgPool, ticket_id: i64) -> anyhow::Result<Option<String>> {
    Ok(
        sqlx::query_scalar("SELECT priority FROM tickets_ticket WHERE id = $1::int8")
            .bind(ticket_id)
            .fetch_optional(pool)
            .await?,
    )
}

/// このチケットが「ブロックしている」未完了のチケットの数(ボトルネックの判定)。
/// 依存関係 `from --blocks--> to` の `to` のうち、完了・キャンセルの分類に入っていないもの。
pub async fn count_open_blocked(pool: &PgPool, ticket_id: i64) -> anyhow::Result<i64> {
    let cat = crate::infrastructure::repositories::insights_repo::category_sql("t.status");
    Ok(sqlx::query_scalar(&format!(
        "SELECT COUNT(*)::int8
         FROM t_task_dependency d JOIN tickets_ticket t ON t.id = d.to_task_id
         WHERE d.from_task_id = $1 AND d.dependency_type = 'blocks'
           AND COALESCE({cat}, '') NOT IN ('completed', 'cancelled')"
    ))
    .bind(ticket_id)
    .fetch_one(pool)
    .await?)
}

// ---- 依頼(キュー) --------------------------------------------------------------

pub struct NewKick<'a> {
    pub run_uuid: Uuid,
    pub ticket_id: i64,
    pub triggered_by_id: i64,
    pub prompt: &'a str,
    pub prompt_hash: &'a str,
    pub repo: &'a str,
    pub base_branch: &'a str,
    pub caps: &'a Caps,
    pub class: PriorityClass,
}

/// 依頼をキューに積む(トランザクション内で呼ぶ)。trigger_type は webhook(SENN からの起動)。
pub async fn insert_kick(conn: &mut PgConnection, k: &NewKick<'_>) -> anyhow::Result<AiRun> {
    Ok(sqlx::query_as::<_, AiRun>(
        "INSERT INTO t_ai_run
            (run_uuid, ticket_id, trigger_type, tool, triggered_by_id, prompt_generated_at, queued_at,
             kick_prompt, kick_prompt_hash, repo, base_branch, caps, priority_class, priority_rank)
         VALUES ($1, $2, 'webhook', 'claude_code', $3, NOW(), NOW(), $4, $5, $6, $7, $8, $9, $10)
         RETURNING *",
    )
    .bind(k.run_uuid)
    .bind(k.ticket_id)
    .bind(k.triggered_by_id)
    .bind(k.prompt)
    .bind(k.prompt_hash)
    .bind(k.repo)
    .bind(k.base_branch)
    .bind(serde_json::to_value(k.caps)?)
    .bind(k.class.as_str())
    .bind(k.class.rank())
    .fetch_one(conn)
    .await?)
}

// ---- ランナー ----------------------------------------------------------------

/// トークンのハッシュ(SHA-256 の hex)。生のトークンは保存しない。
pub fn hash_token(plain: &str) -> String {
    let d = Sha256::digest(plain.as_bytes());
    d.iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Debug, Clone, FromRow)]
pub struct Runner {
    pub id: i64,
    pub name: String,
    pub token_prefix: String,
    pub signing_secret_enc: String,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub last_seen_at: Option<DateTime<Utc>>,
}

pub async fn create_runner(
    pool: &PgPool,
    name: &str,
    token_hash: &str,
    token_prefix: &str,
    signing_secret_enc: &str,
    created_by: i64,
) -> anyhow::Result<Runner> {
    Ok(sqlx::query_as::<_, Runner>(
        "INSERT INTO t_ai_runner (name, token_hash, token_prefix, signing_secret_enc, created_by_id)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id, name, token_prefix, signing_secret_enc, created_at, revoked_at, last_seen_at",
    )
    .bind(name)
    .bind(token_hash)
    .bind(token_prefix)
    .bind(signing_secret_enc)
    .bind(created_by)
    .fetch_one(pool)
    .await?)
}

/// 失効していないランナーを、トークンのハッシュで探す。
pub async fn find_active_runner(pool: &PgPool, token_hash: &str) -> anyhow::Result<Option<Runner>> {
    Ok(sqlx::query_as::<_, Runner>(
        "SELECT id, name, token_prefix, signing_secret_enc, created_at, revoked_at, last_seen_at
         FROM t_ai_runner WHERE token_hash = $1 AND revoked_at IS NULL",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await?)
}

pub async fn touch_runner(pool: &PgPool, runner_id: i64) -> anyhow::Result<()> {
    sqlx::query("UPDATE t_ai_runner SET last_seen_at = NOW() WHERE id = $1")
        .bind(runner_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_runners(pool: &PgPool) -> anyhow::Result<Vec<Runner>> {
    Ok(sqlx::query_as::<_, Runner>(
        "SELECT id, name, token_prefix, signing_secret_enc, created_at, revoked_at, last_seen_at
         FROM t_ai_runner ORDER BY id",
    )
    .fetch_all(pool)
    .await?)
}

/// 失効させる。失効したランナーが取得して実行中の依頼には、停止を求める。
pub async fn revoke_runner(pool: &PgPool, runner_id: i64) -> anyhow::Result<bool> {
    let mut tx = pool.begin().await?;
    let n = sqlx::query(
        "UPDATE t_ai_runner SET revoked_at = NOW() WHERE id = $1 AND revoked_at IS NULL",
    )
    .bind(runner_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    sqlx::query(
        "UPDATE t_ai_run SET stop_requested_at = NOW(), stop_reason = 'runner_revoked'
         WHERE claimed_by_runner_id = $1 AND result IS NULL AND stop_requested_at IS NULL",
    )
    .bind(runner_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(n > 0)
}

// ---- 取得・ハートビート ---------------------------------------------------------

/// 取得待ちの依頼を、優先度の高い順(同じなら古い順)に 1 件取る(他のランナーと取り合わない)。
/// **このランナーが実行中の依頼があれば取らない**(同時実行は 1 件)。取れなければ None。
pub async fn claim_next(pool: &PgPool, runner_id: i64) -> anyhow::Result<Option<AiRun>> {
    Ok(sqlx::query_as::<_, AiRun>(
        "UPDATE t_ai_run SET claimed_at = NOW(), claimed_by_runner_id = $1, last_heartbeat_at = NOW()
         WHERE id = (
             SELECT id FROM t_ai_run
             WHERE queued_at IS NOT NULL AND claimed_at IS NULL AND result IS NULL AND stop_requested_at IS NULL
               AND NOT EXISTS (
                   SELECT 1 FROM t_ai_run b
                   WHERE b.claimed_by_runner_id = $1 AND b.claimed_at IS NOT NULL AND b.result IS NULL
               )
             ORDER BY priority_rank, queued_at
             LIMIT 1
             FOR UPDATE SKIP LOCKED
         )
         RETURNING *",
    )
    .bind(runner_id)
    .fetch_optional(pool)
    .await?)
}

/// このランナーが取得した実行を取る(他のランナーの実行は取れない)。
pub async fn find_run_of_runner(
    pool: &PgPool,
    runner_id: i64,
    run_uuid: &Uuid,
) -> anyhow::Result<Option<AiRun>> {
    Ok(sqlx::query_as::<_, AiRun>(
        "SELECT * FROM t_ai_run WHERE run_uuid = $1 AND claimed_by_runner_id = $2",
    )
    .bind(run_uuid)
    .bind(runner_id)
    .fetch_optional(pool)
    .await?)
}

/// ハートビート: 使用量と時刻を更新する(未完了の行だけ)。
pub async fn record_heartbeat(
    pool: &PgPool,
    run_id: i64,
    weighted_tokens: i64,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE t_ai_run SET last_heartbeat_at = NOW(), weighted_tokens = GREATEST(weighted_tokens, $2)
         WHERE id = $1 AND result IS NULL",
    )
    .bind(run_id)
    .bind(weighted_tokens)
    .execute(pool)
    .await?;
    Ok(())
}

/// 停止を求める(未完了で、まだ求めていない行だけ。最初の理由を残す)。
pub async fn request_stop(pool: &PgPool, run_id: i64, reason: &str) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE t_ai_run SET stop_requested_at = NOW(), stop_reason = $2
         WHERE id = $1 AND result IS NULL AND stop_requested_at IS NULL",
    )
    .bind(run_id)
    .bind(reason)
    .execute(pool)
    .await?;
    Ok(())
}

/// 完了時に、加重トークンを確定する。
pub async fn set_final_weighted(pool: &PgPool, run_id: i64, weighted: i64) -> anyhow::Result<()> {
    sqlx::query("UPDATE t_ai_run SET weighted_tokens = $2 WHERE id = $1")
        .bind(run_id)
        .bind(weighted.max(0))
        .execute(pool)
        .await?;
    Ok(())
}

/// 取得されていない依頼を、そのまま終わらせる(取り消し・プロジェクトの許可が外れた場合など)。
/// 取得済みの実行には使わない(ランナーが止まってから `finish`/`error` で終える)。
pub async fn finalize_unclaimed(pool: &PgPool, run_id: i64, reason: &str) -> anyhow::Result<bool> {
    let n = sqlx::query(
        "UPDATE t_ai_run
         SET result = 'aborted', ended_at = NOW(), stop_requested_at = COALESCE(stop_requested_at, NOW()),
             stop_reason = COALESCE(stop_reason, $2)
         WHERE id = $1 AND claimed_at IS NULL AND result IS NULL",
    )
    .bind(run_id)
    .bind(reason)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(n > 0)
}

/// 直近に終わった起動(新しい順)が失敗か。失敗 = エラー、または暴走の兆候で止めたもの。
pub async fn recent_failed_flags(pool: &PgPool, n: i64) -> anyhow::Result<Vec<bool>> {
    let rows: Vec<(bool,)> = sqlx::query_as(
        "SELECT (result = 'error' OR (result = 'aborted' AND stop_reason = 'burn_rate'))
         FROM t_ai_run
         WHERE queued_at IS NOT NULL AND result IS NOT NULL
         ORDER BY COALESCE(ended_at, created_at) DESC, id DESC
         LIMIT $1",
    )
    .bind(n)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(f,)| f).collect())
}

/// チケットの最近の実行(画面の状態表示用)。
pub async fn recent_runs_of_ticket(
    pool: &PgPool,
    ticket_id: i64,
    limit: i64,
) -> anyhow::Result<Vec<AiRun>> {
    Ok(sqlx::query_as::<_, AiRun>(
        "SELECT * FROM t_ai_run WHERE ticket_id = $1 ORDER BY created_at DESC, id DESC LIMIT $2",
    )
    .bind(ticket_id)
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

// ---- 見張り ----------------------------------------------------------------------

/// 依頼したが、一定時間たってもランナーが取得しない(ランナーが止まっている可能性)。
pub async fn list_unclaimed_overdue(pool: &PgPool, minutes: i64) -> anyhow::Result<Vec<AiRun>> {
    Ok(sqlx::query_as::<_, AiRun>(
        "SELECT * FROM t_ai_run
         WHERE queued_at IS NOT NULL AND claimed_at IS NULL AND result IS NULL
           AND stop_requested_at IS NULL AND problem_notified_at IS NULL
           AND queued_at < NOW() - ($1 || ' minutes')::interval",
    )
    .bind(minutes)
    .fetch_all(pool)
    .await?)
}

/// 取得されたが、ハートビートが一定時間途絶えた(ランナーが落ちた可能性)。
pub async fn list_heartbeat_lost(pool: &PgPool, minutes: i64) -> anyhow::Result<Vec<AiRun>> {
    Ok(sqlx::query_as::<_, AiRun>(
        "SELECT * FROM t_ai_run
         WHERE claimed_at IS NOT NULL AND result IS NULL
           AND COALESCE(last_heartbeat_at, claimed_at) < NOW() - ($1 || ' minutes')::interval",
    )
    .bind(minutes)
    .fetch_all(pool)
    .await?)
}

/// 一定時間内に接続したランナーがいるか(待機中のランナーも含む。止まっているランナーとの区別に使う)。
pub async fn any_runner_online(pool: &PgPool, within_secs: i64) -> anyhow::Result<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM t_ai_runner
                        WHERE revoked_at IS NULL AND last_seen_at > NOW() - ($1 || ' seconds')::interval)",
    )
    .bind(within_secs)
    .fetch_one(pool)
    .await?)
}

/// 取得待ちのまま、長時間(時間)たった依頼。ランナーが生きていても、これ以上は待たせない。
pub async fn list_queue_expired(pool: &PgPool, hours: i64) -> anyhow::Result<Vec<AiRun>> {
    Ok(sqlx::query_as::<_, AiRun>(
        "SELECT * FROM t_ai_run
         WHERE queued_at IS NOT NULL AND claimed_at IS NULL AND result IS NULL AND stop_requested_at IS NULL
           AND queued_at < NOW() - ($1 || ' hours')::interval",
    )
    .bind(hours)
    .fetch_all(pool)
    .await?)
}

/// 管理者(システム管理者)のユーザー ID(通知の宛先)。
pub async fn system_admin_ids(pool: &PgPool) -> anyhow::Result<Vec<i32>> {
    let rows: Vec<(i32,)> = sqlx::query_as(
        "SELECT id::int4 FROM accounts_user WHERE is_system_admin = true AND is_active = true",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(i,)| i).collect())
}

// ---- 監査 --------------------------------------------------------------------------

/// 監査記録(本文・秘密は入れない)。失敗しても処理は止めない。
pub async fn audit(
    pool: &PgPool,
    actor_user_id: Option<i64>,
    actor_kind: &str,
    action: &str,
    detail: serde_json::Value,
) {
    if let Err(e) = sqlx::query(
        "INSERT INTO access_audit_log (actor_user_id, actor_kind, action, detail)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(actor_user_id)
    .bind(actor_kind)
    .bind(action)
    .bind(detail)
    .execute(pool)
    .await
    {
        tracing::warn!(
            "[AI実行] 処理=監査記録 結果=失敗 影響=監査記録なし | {:?}",
            e
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_hash_is_stable_hex_and_does_not_contain_the_token() {
        let h = hash_token("srun_secret-token");
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(!h.contains("secret"));
        assert_eq!(h, hash_token("srun_secret-token"));
        assert_ne!(h, hash_token("srun_secret-token2"));
    }
}
