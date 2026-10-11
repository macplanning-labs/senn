/// infrastructure/repositories/ai_run_repo.rs — AI実行履歴永続化
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

/// t_ai_run の1行。トークン・結果などの列は、Insights(集計)で使う。
#[allow(dead_code)]
#[derive(Debug, Clone, FromRow)]
pub struct AiRun {
    pub id: i64,
    pub run_uuid: Uuid,
    pub ticket_id: i64,
    pub trigger_type: String,
    pub tool: String,
    pub model: String,
    pub triggered_by_id: Option<i64>,
    pub prompt_generated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub result: Option<String>,
    pub error_detail: String,
    pub summary: String,
    pub problem_notified_at: Option<DateTime<Utc>>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_read_tokens: Option<i64>,
    pub cache_creation_tokens: Option<i64>,
    pub usd_equivalent: Option<f64>,
    pub created_at: DateTime<Utc>,
    // ---- 「AIに対応させる」(社内ランナー方式)の列。手動コピーの実行では NULL / 0 ----
    pub queued_at: Option<DateTime<Utc>>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub claimed_by_runner_id: Option<i64>,
    pub kick_prompt: Option<String>,
    pub kick_prompt_hash: Option<String>,
    pub repo: Option<String>,
    pub base_branch: Option<String>,
    pub caps: Option<serde_json::Value>,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub weighted_tokens: i64,
    pub stop_requested_at: Option<DateTime<Utc>>,
    pub stop_reason: Option<String>,
    pub priority_class: String,
    pub priority_rank: i16,
}

/// UUID で実行行を検索。
pub async fn find_by_uuid(pool: &PgPool, uuid: &Uuid) -> anyhow::Result<Option<AiRun>> {
    let row = sqlx::query_as::<_, AiRun>("SELECT * FROM t_ai_run WHERE run_uuid = $1")
        .bind(uuid)
        .fetch_optional(pool)
        .await?;

    Ok(row)
}

pub struct InsertInput {
    pub run_uuid: Uuid,
    pub ticket_id: i64,
    pub trigger_type: Option<String>,
    pub tool: Option<String>,
    pub model: Option<String>,
    pub triggered_by_id: Option<i64>,
}

/// 新規実行行を作成(prompt_generated_at = now, started_at = NULL)。
pub async fn insert(pool: &PgPool, input: &InsertInput) -> anyhow::Result<AiRun> {
    let row = sqlx::query_as::<_, AiRun>(
        "INSERT INTO t_ai_run (run_uuid, ticket_id, trigger_type, tool, model, triggered_by_id, prompt_generated_at)
         VALUES ($1, $2, COALESCE($3, 'manual_copy'), COALESCE($4, 'claude_code'), COALESCE($5, ''), $6, NOW())
         RETURNING *"
    )
    .bind(input.run_uuid)
    .bind(input.ticket_id)
    .bind(&input.trigger_type)
    .bind(&input.tool)
    .bind(&input.model)
    .bind(input.triggered_by_id)
    .fetch_one(pool)
    .await?;

    Ok(row)
}

/// started_at を設定(未設定の場合のみ。既に設定済みなら何もしない)。tool / model は指定があれば更新する。
pub async fn mark_started(
    pool: &PgPool,
    id: i64,
    tool: Option<&str>,
    model: Option<&str>,
) -> anyhow::Result<bool> {
    let result = sqlx::query(
        "UPDATE t_ai_run
         SET started_at = NOW(), tool = COALESCE($1, tool), model = COALESCE($2, model)
         WHERE id = $3 AND started_at IS NULL",
    )
    .bind(tool)
    .bind(model)
    .bind(id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// AI が使ったトークン量(取れた分だけ。Phase 3 でログから集計する)。
#[derive(Debug, Clone, Copy, Default)]
pub struct TokenUsage {
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_read_tokens: Option<i64>,
    pub cache_creation_tokens: Option<i64>,
    pub usd_equivalent: Option<f64>,
}

/// 完了時に結果・サマリ・トークン数を設定。未完了(result が NULL か error)の行だけを更新し、
/// 更新できたら true(同時に 2 回呼ばれても、片方だけが true になる=二重処理を防ぐ)。
pub async fn mark_finished(
    pool: &PgPool,
    id: i64,
    result: &str,
    summary: &str,
    usage: &TokenUsage,
) -> anyhow::Result<bool> {
    let row_count = sqlx::query(
        "UPDATE t_ai_run SET ended_at = NOW(), started_at = COALESCE(started_at, NOW()), result = $1, summary = $2, input_tokens = $3, output_tokens = $4, cache_read_tokens = $5, cache_creation_tokens = $6, usd_equivalent = $7 WHERE id = $8 AND (result IS NULL OR result = 'error')"
    )
    .bind(result)
    .bind(summary)
    .bind(usage.input_tokens)
    .bind(usage.output_tokens)
    .bind(usage.cache_read_tokens)
    .bind(usage.cache_creation_tokens)
    .bind(usage.usd_equivalent)
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected();

    Ok(row_count > 0)
}

/// エラーを記録(既存の error_detail に追記)。
pub async fn mark_error(pool: &PgPool, id: i64, detail: &str) -> anyhow::Result<bool> {
    let row_count = sqlx::query(
        "UPDATE t_ai_run SET result = CASE WHEN result IS NULL THEN 'error' ELSE result END, error_detail = CASE WHEN error_detail = '' THEN $1 ELSE error_detail || E'\n' || $1 END WHERE id = $2"
    )
    .bind(detail)
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected();

    Ok(row_count > 0)
}

/// problem_notified_at を now に設定(未設定の場合のみ。1回だけ通知する判定に使用)。
pub async fn set_notified(pool: &PgPool, id: i64) -> anyhow::Result<bool> {
    let row_count = sqlx::query(
        "UPDATE t_ai_run SET problem_notified_at = NOW() WHERE id = $1 AND problem_notified_at IS NULL",
    )
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected();

    Ok(row_count > 0)
}

/// 着手がまだ（result IS NULL AND started_at IS NULL）で、prompt_generated_at が minutes 分以上前。
pub async fn list_unstarted_overdue(pool: &PgPool, minutes: i64) -> anyhow::Result<Vec<AiRun>> {
    let rows = sqlx::query_as::<_, AiRun>(
        "SELECT * FROM t_ai_run WHERE result IS NULL AND started_at IS NULL AND problem_notified_at IS NULL AND prompt_generated_at < NOW() - ($1 || ' minutes')::interval ORDER BY prompt_generated_at ASC",
    )
    .bind(minutes)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// 完了がまだ（result IS NULL）で、started_at が minutes 分以上前。
pub async fn list_unfinished_overdue(pool: &PgPool, minutes: i64) -> anyhow::Result<Vec<AiRun>> {
    let rows = sqlx::query_as::<_, AiRun>(
        "SELECT * FROM t_ai_run WHERE result IS NULL AND started_at IS NOT NULL AND problem_notified_at IS NULL AND started_at < NOW() - ($1 || ' minutes')::interval ORDER BY started_at ASC",
    )
    .bind(minutes)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// 再利用可能な未開始実行を検索。ticket_id と triggered_by_id が一致し、
/// started_at IS NULL AND result IS NULL AND trigger_type = 'manual_copy' で、
/// prompt_generated_at が within_minutes 分以内の最新 1 件を返す。
pub async fn find_reusable_unstarted(
    pool: &PgPool,
    ticket_id: i64,
    user_id: i64,
    within_minutes: i64,
) -> anyhow::Result<Option<AiRun>> {
    let row = sqlx::query_as::<_, AiRun>(
        "SELECT * FROM t_ai_run WHERE ticket_id = $1 AND triggered_by_id = $2
         AND started_at IS NULL AND result IS NULL AND trigger_type = 'manual_copy'
         AND prompt_generated_at > NOW() - ($3 || ' minutes')::interval
         ORDER BY prompt_generated_at DESC LIMIT 1",
    )
    .bind(ticket_id)
    .bind(user_id)
    .bind(within_minutes)
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

/// 開始日の設定の SQL。`extra` は、WHERE に足す条件(`t` は tickets_ticket)。
fn start_date_sql(extra: &str) -> String {
    format!(
        "WITH upd AS (
             UPDATE tickets_ticket t
                SET start_date = (now() AT TIME ZONE 'Asia/Tokyo')::date, updated_at = now()
              WHERE t.id = $1::int8 AND t.start_date IS NULL {extra}
          RETURNING t.start_date
         ), logged AS (
             INSERT INTO tickets_change_log (field_name, old_value, new_value, changed_by_id, changed_at, ticket_id)
             SELECT 'start_date', '', upd.start_date::text, $2::int8, now(), $1::int8 FROM upd
         )
         SELECT count(*)::int8 AS n FROM upd"
    )
}

async fn run_start_date(
    pool: &PgPool,
    ticket_id: i32,
    changed_by_id: i32,
    extra: &str,
) -> anyhow::Result<bool> {
    let row = sqlx::query(&start_date_sql(extra))
        .bind(i64::from(ticket_id))
        .bind(i64::from(changed_by_id))
        .fetch_one(pool)
        .await?;
    let n: i64 = sqlx::Row::get(&row, "n");
    Ok(n > 0)
}

/// 開始日が空のときだけ、日本時間の今日を入れ、変更の履歴(tickets_change_log)にも残す。入れたら true。
/// 人が入れた開始日は、変えない。AI が着手したのに開始日が空だと、ガントチャートなどに載らないため。
pub async fn set_start_date_if_missing(
    pool: &PgPool,
    ticket_id: i32,
    changed_by_id: i32,
) -> anyhow::Result<bool> {
    run_start_date(pool, ticket_id, changed_by_id, "").await
}

/// 同じ。ただし、いまの状態が「着手」の分類のときだけ(AI が API で状態を進めたとき用)。
pub async fn set_start_date_if_started(
    pool: &PgPool,
    ticket_id: i32,
    changed_by_id: i32,
) -> anyhow::Result<bool> {
    let cond = format!(
        "AND {} = 'started'",
        crate::infrastructure::repositories::insights_repo::category_sql("t.status")
    );
    run_start_date(pool, ticket_id, changed_by_id, &cond).await
}
