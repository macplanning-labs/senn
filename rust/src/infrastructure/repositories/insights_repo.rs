/// infrastructure/repositories/insights_repo.rs — Insights 集計(人 vs AI のサイクルタイム・トリアージ時間・処理総量)
///
/// 設計: docs/design/実装計画_AI自動ワークフローとInsights.md §3.1 / §5。
///
/// - 対象は**閲覧者が見えるチケットだけ**(`scope_sql::push_ticket_visible`)。集計は「見えるデータだけで計算」する。
/// - 着手時刻 = ステータス履歴で、分類が started のステータスに**最初に**入った時刻。
/// - 完了時刻 = 同じく completed に最初に入った時刻。履歴が無く `closed_at` だけある(取り込みデータ)ときは `closed_at`。
/// - 区分: `ai` = `t_ai_run` に着手済みの行がある / `human` = それ以外で担当者がいる / `unassigned` = それ以外。
/// - 日付の区切りは JST(`Asia/Tokyo`)。週は月曜始まり。
use chrono::NaiveDate;
use serde::Serialize;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};

use crate::domain::access::Scope;
use crate::infrastructure::access::scope_sql;

const SEGMENTS: [&str; 3] = ["ai", "human", "unassigned"];

/// 集計の絞り込み(期間は JST の日付。`to` を含む)。
#[derive(Debug, Clone, Copy)]
pub struct Filters<'a> {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub project_id: Option<i32>,
    pub team_id: Option<i32>,
    pub ticket_type: Option<&'a str>,
}

/// 集計の粒度。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granularity {
    Week,
    Month,
}

impl Granularity {
    pub fn parse(s: &str) -> Option<Granularity> {
        match s {
            "week" => Some(Granularity::Week),
            "month" => Some(Granularity::Month),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Granularity::Week => "week",
            Granularity::Month => "month",
        }
    }
}

// ---- 応答の型 ----------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct CycleTimeResult {
    pub from: String,
    pub to: String,
    pub segments: Vec<SegmentStats>,
    pub excluded: ExcludedCount,
    #[serde(rename = "aiRuns")]
    pub ai_runs: AiRunsStats,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct SegmentStats {
    pub segment: String,
    pub count: i64,
    #[serde(rename = "medianSeconds")]
    pub median_seconds: Option<i64>,
    #[serde(rename = "meanSeconds")]
    pub mean_seconds: Option<i64>,
    #[serde(rename = "p90Seconds")]
    pub p90_seconds: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ExcludedCount {
    /// 完了したが、着手の記録が無い(または着手より前に完了している)ため、サイクルタイムに入れなかった件数
    #[serde(rename = "noStart")]
    pub no_start: i64,
}

#[derive(Debug, Serialize)]
pub struct AiRunsStats {
    /// 期間内にプロンプトを作ったが、まだ完了していない実行
    pub unfinished: i64,
    /// 連携エラーになった実行
    pub error: i64,
}

#[derive(Debug, Serialize)]
pub struct TriageTimeResult {
    pub granularity: String,
    pub series: Vec<TriageSeriesEntry>,
    pub waiting: WaitingStats,
}

#[derive(Debug, Serialize)]
pub struct TriageSeriesEntry {
    #[serde(rename = "periodStart")]
    pub period_start: String,
    pub segment: String,
    pub count: i64,
    #[serde(rename = "medianSeconds")]
    pub median_seconds: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct WaitingStats {
    /// まだ一度も着手されていないチケット(キャンセルを除く)
    pub count: i64,
    #[serde(rename = "medianAgeDays")]
    pub median_age_days: Option<i64>,
    #[serde(rename = "maxAgeDays")]
    pub max_age_days: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ThroughputResult {
    pub granularity: String,
    pub series: Vec<ThroughputSeriesEntry>,
    #[serde(rename = "aiShare")]
    pub ai_share: AiShareStats,
}

#[derive(Debug, Serialize)]
pub struct ThroughputSeriesEntry {
    #[serde(rename = "periodStart")]
    pub period_start: String,
    pub segment: String,
    pub tickets: i64,
    pub points: i64,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct AiShareStats {
    #[serde(rename = "ticketsPercent")]
    pub tickets_percent: Option<i64>,
    #[serde(rename = "pointsPercent")]
    pub points_percent: Option<i64>,
}

// ---- AI 使用量 -----------------------------------------------------------

/// AI 使用量の集計結果。**完了した実行のうち、トークン量が分かるものだけ**を数える。
/// 未完了・トークン不明の実行は、数字に混ぜず、`excluded` に件数だけ出す(黙って除外しない)。
#[derive(Debug, Serialize)]
pub struct AiUsageResult {
    pub from: String,
    pub to: String,
    pub granularity: String,
    pub totals: AiUsageTotals,
    pub series: Vec<AiUsageSeriesEntry>,
    pub excluded: AiUsageExcluded,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct AiUsageTotals {
    pub runs: i64,
    pub tickets: i64,
    #[serde(rename = "inputTokens")]
    pub input_tokens: i64,
    #[serde(rename = "outputTokens")]
    pub output_tokens: i64,
    #[serde(rename = "cacheCreationTokens")]
    pub cache_creation_tokens: i64,
    #[serde(rename = "cacheReadTokens")]
    pub cache_read_tokens: i64,
    /// 加重トークン = 入力 + 出力 + キャッシュ作成 + キャッシュ読み取り × 0.1(1 実行ごとに切り捨てて合計)
    #[serde(rename = "weightedTokens")]
    pub weighted_tokens: i64,
    /// API で払った場合の換算額(USD)。請求額ではない。
    #[serde(rename = "usdEquivalent")]
    pub usd_equivalent: f64,
    #[serde(rename = "avgWeightedPerRun")]
    pub avg_weighted_per_run: Option<i64>,
    #[serde(rename = "avgWeightedPerTicket")]
    pub avg_weighted_per_ticket: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct AiUsageSeriesEntry {
    #[serde(rename = "periodStart")]
    pub period_start: String,
    pub runs: i64,
    #[serde(rename = "inputTokens")]
    pub input_tokens: i64,
    #[serde(rename = "outputTokens")]
    pub output_tokens: i64,
    #[serde(rename = "cacheCreationTokens")]
    pub cache_creation_tokens: i64,
    #[serde(rename = "cacheReadTokens")]
    pub cache_read_tokens: i64,
    #[serde(rename = "weightedTokens")]
    pub weighted_tokens: i64,
}

#[derive(Debug, Serialize)]
pub struct AiUsageExcluded {
    /// 期間内にプロンプトを作ったが、まだ終わっていない実行
    pub unfinished: i64,
    /// 終わったが、トークン量が記録されていない実行
    #[serde(rename = "noTokens")]
    pub no_tokens: i64,
}

// ---- SQL の組み立て ------------------------------------------------------

/// ステータス(slug)の分類を引く SQL の式。チケット別名 `t`、`slug` は式。
/// 範囲の決め方は `ticket_repo::completed_status_sql` / `workflow_status_repo::lookup_status_category` と同じ
/// (プロジェクト → チーム → 全体の既定)。引けないときは、標準の slug だけ分類を補う。
pub(crate) fn category_sql(slug: &str) -> String {
    format!(
        "COALESCE(
            CASE WHEN t.project_id IS NOT NULL THEN
                (SELECT w.category FROM t_workflow_status w
                  WHERE w.project_id = t.project_id AND w.slug = {slug} LIMIT 1)
            ELSE COALESCE(
                (SELECT w.category FROM t_workflow_status w
                  WHERE w.team_id = t.team_id AND w.project_id IS NULL AND w.slug = {slug} LIMIT 1),
                (SELECT w.category FROM t_workflow_status w
                  WHERE w.project_id IS NULL AND w.team_id IS NULL AND w.slug = {slug} LIMIT 1))
            END,
            CASE WHEN {slug} IN ('resolved', 'closed') THEN 'completed'
                 WHEN {slug} = 'in_progress' THEN 'started'
                 WHEN {slug} IN ('canceled', 'cancelled') THEN 'cancelled' END)"
    )
}

/// 期間の開始(JST の `from` の 0 時)と終了(JST の `to` の翌日 0 時。含まない)を、SQL の式にして押す。
fn push_period_bounds(qb: &mut QueryBuilder<'_, Postgres>, f: &Filters<'_>) {
    qb.push("(")
        .push_bind(f.from)
        .push("::date)::timestamp AT TIME ZONE 'Asia/Tokyo'");
}
fn push_period_end(qb: &mut QueryBuilder<'_, Postgres>, f: &Filters<'_>) {
    qb.push("((")
        .push_bind(f.to)
        .push("::date + 1)::timestamp AT TIME ZONE 'Asia/Tokyo')");
}

/// 見えるチケットの「着手・完了・区分」を 1 行ずつにした CTE `base` を押す(末尾に `, ` は付けない)。
fn push_base(qb: &mut QueryBuilder<'_, Postgres>, f: &Filters<'_>, scope: &Scope) {
    qb.push(
        "WITH base AS (
            SELECT t.id, t.created_at, t.closed_at, t.story_points,
                ",
    );
    qb.push(category_sql("t.status")).push(
        " AS current_category,
                (SELECT MIN(h.changed_at) FROM tickets_status_history h
                  WHERE h.ticket_id = t.id AND ",
    );
    qb.push(category_sql("h.new_status")).push(
        " = 'started') AS started_at,
                (SELECT MIN(h.changed_at) FROM tickets_status_history h
                  WHERE h.ticket_id = t.id AND ",
    );
    qb.push(category_sql("h.new_status")).push(
        " = 'completed') AS completed_hist_at,
                CASE
                    WHEN EXISTS (SELECT 1 FROM t_ai_run r WHERE r.ticket_id = t.id AND r.started_at IS NOT NULL) THEN 'ai'
                    WHEN EXISTS (SELECT 1 FROM tickets_ticket_assignees a WHERE a.ticketmodel_id = t.id) THEN 'human'
                    ELSE 'unassigned'
                END AS segment
            FROM tickets_ticket t
            WHERE ",
    );
    scope_sql::push_ticket_visible(qb, "t", scope);
    push_ticket_filters(qb, f);
    qb.push(
        "
        ), done AS (
            SELECT base.*, COALESCE(completed_hist_at, closed_at) AS completed_at FROM base
        )",
    );
}

fn push_ticket_filters(qb: &mut QueryBuilder<'_, Postgres>, f: &Filters<'_>) {
    if let Some(p) = f.project_id {
        qb.push(" AND t.project_id = ").push_bind(i64::from(p));
    }
    if let Some(team) = f.team_id {
        qb.push(" AND t.team_id = ").push_bind(i64::from(team));
    }
    if let Some(tt) = f.ticket_type {
        qb.push(" AND t.ticket_type = ").push_bind(tt.to_string());
    }
}

fn trunc_unit(g: Granularity) -> &'static str {
    match g {
        Granularity::Week => "week",
        Granularity::Month => "month",
    }
}

fn opt_secs(v: Option<f64>) -> Option<i64> {
    v.map(|x| x.round() as i64)
}

// ---- 集計 ----------------------------------------------------------------

/// 人 vs AI のサイクルタイム(完了時刻が期間に入るチケット)。
pub async fn cycle_time_stats(
    pool: &PgPool,
    f: &Filters<'_>,
    scope: &Scope,
) -> anyhow::Result<CycleTimeResult> {
    // 区分ごとの件数・中央値・平均・90 パーセンタイル(秒)
    let mut qb = QueryBuilder::<Postgres>::new("");
    push_base(&mut qb, f, scope);
    qb.push(
        "
        SELECT segment,
               COUNT(*)::int8 AS n,
               percentile_cont(0.5) WITHIN GROUP (ORDER BY secs) AS median,
               AVG(secs) AS mean,
               percentile_cont(0.9) WITHIN GROUP (ORDER BY secs) AS p90
        FROM (
            SELECT segment, EXTRACT(EPOCH FROM (completed_at - started_at))::float8 AS secs
            FROM done
            WHERE started_at IS NOT NULL AND completed_at IS NOT NULL
              AND completed_at >= started_at
              AND completed_at >= ",
    );
    push_period_bounds(&mut qb, f);
    qb.push(" AND completed_at < ");
    push_period_end(&mut qb, f);
    qb.push(") s GROUP BY segment");
    let rows = qb.build().fetch_all(pool).await?;

    let mut segments: Vec<SegmentStats> = SEGMENTS
        .iter()
        .map(|s| SegmentStats {
            segment: (*s).to_string(),
            count: 0,
            median_seconds: None,
            mean_seconds: None,
            p90_seconds: None,
        })
        .collect();
    for row in rows {
        let seg: String = row.get("segment");
        if let Some(target) = segments.iter_mut().find(|s| s.segment == seg) {
            target.count = row.get("n");
            target.median_seconds = opt_secs(row.get("median"));
            target.mean_seconds = opt_secs(row.get("mean"));
            target.p90_seconds = opt_secs(row.get("p90"));
        }
    }

    // 完了したが、着手の記録が無い(または着手より前に完了している)ため入れなかった件数
    let mut qb = QueryBuilder::<Postgres>::new("");
    push_base(&mut qb, f, scope);
    qb.push(
        "
        SELECT COUNT(*)::int8 AS n FROM done
        WHERE completed_at IS NOT NULL
          AND (started_at IS NULL OR completed_at < started_at)
          AND completed_at >= ",
    );
    push_period_bounds(&mut qb, f);
    qb.push(" AND completed_at < ");
    push_period_end(&mut qb, f);
    let no_start: i64 = qb.build().fetch_one(pool).await?.get("n");

    let ai_runs = ai_run_counts(pool, f, scope).await?;
    Ok(CycleTimeResult {
        from: f.from.format("%Y-%m-%d").to_string(),
        to: f.to.format("%Y-%m-%d").to_string(),
        segments,
        excluded: ExcludedCount { no_start },
        ai_runs,
    })
}

/// 1 実行の加重トークン(SQL。`r` は t_ai_run)。ランナー側・サーバー側の式と同じ。
const WEIGHTED_SQL: &str = "FLOOR(COALESCE(r.input_tokens, 0) + COALESCE(r.output_tokens, 0)
        + COALESCE(r.cache_creation_tokens, 0) + COALESCE(r.cache_read_tokens, 0) * 0.1)::int8";

/// 「完了していて、トークン量が分かる」実行の条件(SQL)。
const COUNTED_SQL: &str =
    "r.result IS NOT NULL AND r.input_tokens IS NOT NULL AND r.ended_at IS NOT NULL";

/// AI 使用量(トークン・換算額)。完了した時刻(ended_at)が期間内の実行を数える。見えるチケットの実行だけ。
pub async fn ai_usage_stats(
    pool: &PgPool,
    f: &Filters<'_>,
    granularity: Granularity,
    scope: &Scope,
) -> anyhow::Result<AiUsageResult> {
    let unit = trunc_unit(granularity);

    // 合計と、除外した件数
    let mut qb = QueryBuilder::<Postgres>::new(format!(
        "SELECT
            COUNT(*) FILTER (WHERE {COUNTED_SQL})::int8 AS runs,
            COUNT(DISTINCT r.ticket_id) FILTER (WHERE {COUNTED_SQL})::int8 AS tickets,
            COALESCE(SUM(r.input_tokens) FILTER (WHERE {COUNTED_SQL}), 0)::int8 AS input_tokens,
            COALESCE(SUM(r.output_tokens) FILTER (WHERE {COUNTED_SQL}), 0)::int8 AS output_tokens,
            COALESCE(SUM(r.cache_creation_tokens) FILTER (WHERE {COUNTED_SQL}), 0)::int8 AS cache_creation_tokens,
            COALESCE(SUM(r.cache_read_tokens) FILTER (WHERE {COUNTED_SQL}), 0)::int8 AS cache_read_tokens,
            COALESCE(SUM({WEIGHTED_SQL}) FILTER (WHERE {COUNTED_SQL}), 0)::int8 AS weighted_tokens,
            COALESCE(SUM(r.usd_equivalent) FILTER (WHERE {COUNTED_SQL}), 0)::float8 AS usd,
            COUNT(*) FILTER (WHERE r.result IS NOT NULL AND r.input_tokens IS NULL AND r.ended_at IS NOT NULL)::int8 AS no_tokens
         FROM t_ai_run r JOIN tickets_ticket t ON t.id = r.ticket_id
         WHERE r.ended_at IS NOT NULL AND r.ended_at >= "
    ));
    push_period_bounds(&mut qb, f);
    qb.push(" AND r.ended_at < ");
    push_period_end(&mut qb, f);
    qb.push(" AND ");
    scope_sql::push_ticket_visible(&mut qb, "t", scope);
    push_ticket_filters(&mut qb, f);
    let row = qb.build().fetch_one(pool).await?;
    let runs: i64 = row.get("runs");
    let tickets: i64 = row.get("tickets");
    let weighted: i64 = row.get("weighted_tokens");
    let totals = AiUsageTotals {
        runs,
        tickets,
        input_tokens: row.get("input_tokens"),
        output_tokens: row.get("output_tokens"),
        cache_creation_tokens: row.get("cache_creation_tokens"),
        cache_read_tokens: row.get("cache_read_tokens"),
        weighted_tokens: weighted,
        usd_equivalent: row.get("usd"),
        avg_weighted_per_run: (runs > 0).then(|| weighted / runs),
        avg_weighted_per_ticket: (tickets > 0).then(|| weighted / tickets),
    };
    let no_tokens: i64 = row.get("no_tokens");

    // 推移
    let mut qb = QueryBuilder::<Postgres>::new(format!(
        "SELECT to_char(date_trunc('{unit}', r.ended_at AT TIME ZONE 'Asia/Tokyo'), 'YYYY-MM-DD') AS period_start,
            COUNT(*)::int8 AS runs,
            COALESCE(SUM(r.input_tokens), 0)::int8 AS input_tokens,
            COALESCE(SUM(r.output_tokens), 0)::int8 AS output_tokens,
            COALESCE(SUM(r.cache_creation_tokens), 0)::int8 AS cache_creation_tokens,
            COALESCE(SUM(r.cache_read_tokens), 0)::int8 AS cache_read_tokens,
            COALESCE(SUM({WEIGHTED_SQL}), 0)::int8 AS weighted_tokens
         FROM t_ai_run r JOIN tickets_ticket t ON t.id = r.ticket_id
         WHERE {COUNTED_SQL} AND r.ended_at >= "
    ));
    push_period_bounds(&mut qb, f);
    qb.push(" AND r.ended_at < ");
    push_period_end(&mut qb, f);
    qb.push(" AND ");
    scope_sql::push_ticket_visible(&mut qb, "t", scope);
    push_ticket_filters(&mut qb, f);
    qb.push(" GROUP BY 1 ORDER BY 1");
    let rows = qb.build().fetch_all(pool).await?;
    let series = rows
        .iter()
        .map(|r| AiUsageSeriesEntry {
            period_start: r.get("period_start"),
            runs: r.get("runs"),
            input_tokens: r.get("input_tokens"),
            output_tokens: r.get("output_tokens"),
            cache_creation_tokens: r.get("cache_creation_tokens"),
            cache_read_tokens: r.get("cache_read_tokens"),
            weighted_tokens: r.get("weighted_tokens"),
        })
        .collect();

    // 未完了(期間内にプロンプトを作ったもの)。数字には混ぜない
    let unfinished = ai_run_counts(pool, f, scope).await?.unfinished;

    Ok(AiUsageResult {
        from: f.from.to_string(),
        to: f.to.to_string(),
        granularity: granularity.as_str().to_string(),
        totals,
        series,
        excluded: AiUsageExcluded {
            unfinished,
            no_tokens,
        },
    })
}

/// 期間内にプロンプトを作った AI 実行のうち、未完了とエラーの件数(見えるチケットのものだけ)。
async fn ai_run_counts(
    pool: &PgPool,
    f: &Filters<'_>,
    scope: &Scope,
) -> anyhow::Result<AiRunsStats> {
    let mut qb = QueryBuilder::<Postgres>::new(
        "SELECT COUNT(*) FILTER (WHERE r.result IS NULL)::int8 AS unfinished,
                COUNT(*) FILTER (WHERE r.result = 'error')::int8 AS error
         FROM t_ai_run r JOIN tickets_ticket t ON t.id = r.ticket_id
         WHERE ",
    );
    scope_sql::push_ticket_visible(&mut qb, "t", scope);
    push_ticket_filters(&mut qb, f);
    qb.push(" AND r.prompt_generated_at >= ");
    push_period_bounds(&mut qb, f);
    qb.push(" AND r.prompt_generated_at < ");
    push_period_end(&mut qb, f);
    let row = qb.build().fetch_one(pool).await?;
    Ok(AiRunsStats {
        unfinished: row.get("unfinished"),
        error: row.get("error"),
    })
}

/// トリアージ時間(作成 → 着手)。着手時刻が期間に入るチケットを、着手した週(月)ごと・区分ごとに。
pub async fn triage_time_stats(
    pool: &PgPool,
    f: &Filters<'_>,
    granularity: Granularity,
    scope: &Scope,
) -> anyhow::Result<TriageTimeResult> {
    let unit = trunc_unit(granularity);
    let mut qb = QueryBuilder::<Postgres>::new("");
    push_base(&mut qb, f, scope);
    qb.push(format!(
        "
        SELECT to_char(date_trunc('{unit}', started_at AT TIME ZONE 'Asia/Tokyo'), 'YYYY-MM-DD') AS period_start,
               segment,
               COUNT(*)::int8 AS n,
               percentile_cont(0.5) WITHIN GROUP (ORDER BY EXTRACT(EPOCH FROM (started_at - created_at))::float8) AS median
        FROM done
        WHERE started_at IS NOT NULL AND started_at >= created_at
          AND started_at >= "
    ));
    push_period_bounds(&mut qb, f);
    qb.push(" AND started_at < ");
    push_period_end(&mut qb, f);
    qb.push(" GROUP BY 1, segment ORDER BY 1, segment");
    let rows = qb.build().fetch_all(pool).await?;
    let series = rows
        .iter()
        .map(|r| TriageSeriesEntry {
            period_start: r.get("period_start"),
            segment: r.get("segment"),
            count: r.get("n"),
            median_seconds: opt_secs(r.get("median")),
        })
        .collect();

    // まだ一度も着手されていないチケット(期間の終わりまでに作られたもの。キャンセルを除く)
    let mut qb = QueryBuilder::<Postgres>::new("");
    push_base(&mut qb, f, scope);
    qb.push(
        "
        SELECT COUNT(*)::int8 AS n,
               percentile_cont(0.5) WITHIN GROUP (ORDER BY age_days) AS median,
               MAX(age_days) AS max
        FROM (
            SELECT EXTRACT(EPOCH FROM (LEAST(NOW(), ",
    );
    push_period_end(&mut qb, f);
    qb.push(
        ") - created_at))::float8 / 86400.0 AS age_days
            FROM done
            WHERE started_at IS NULL AND completed_at IS NULL
              AND COALESCE(current_category, '') <> 'cancelled'
              AND created_at < ",
    );
    push_period_end(&mut qb, f);
    qb.push(") w");
    let row = qb.build().fetch_one(pool).await?;
    let waiting = WaitingStats {
        count: row.get("n"),
        median_age_days: row
            .get::<Option<f64>, _>("median")
            .map(|x| x.floor() as i64),
        max_age_days: row.get::<Option<f64>, _>("max").map(|x| x.floor() as i64),
    };

    Ok(TriageTimeResult {
        granularity: granularity.as_str().to_string(),
        series,
        waiting,
    })
}

/// 処理総量(完了時刻が期間に入るチケットの件数とポイント)を、完了した週(月)ごと・区分ごとに。
pub async fn throughput_stats(
    pool: &PgPool,
    f: &Filters<'_>,
    granularity: Granularity,
    scope: &Scope,
) -> anyhow::Result<ThroughputResult> {
    let unit = trunc_unit(granularity);
    let mut qb = QueryBuilder::<Postgres>::new("");
    push_base(&mut qb, f, scope);
    qb.push(format!(
        "
        SELECT to_char(date_trunc('{unit}', completed_at AT TIME ZONE 'Asia/Tokyo'), 'YYYY-MM-DD') AS period_start,
               segment,
               COUNT(*)::int8 AS n,
               COALESCE(SUM(COALESCE(story_points, 0)), 0)::int8 AS points
        FROM done
        WHERE completed_at IS NOT NULL AND completed_at >= "
    ));
    push_period_bounds(&mut qb, f);
    qb.push(" AND completed_at < ");
    push_period_end(&mut qb, f);
    qb.push(" GROUP BY 1, segment ORDER BY 1, segment");
    let rows = qb.build().fetch_all(pool).await?;
    let series: Vec<ThroughputSeriesEntry> = rows
        .iter()
        .map(|r| ThroughputSeriesEntry {
            period_start: r.get("period_start"),
            segment: r.get("segment"),
            tickets: r.get("n"),
            points: r.get("points"),
        })
        .collect();
    let ai_share = ai_share(&series);
    Ok(ThroughputResult {
        granularity: granularity.as_str().to_string(),
        series,
        ai_share,
    })
}

/// AI の割合(%)。件数ベースとポイントベース。全体が 0 のときは None。
pub fn ai_share(series: &[ThroughputSeriesEntry]) -> AiShareStats {
    let (mut t_all, mut t_ai, mut p_all, mut p_ai) = (0i64, 0i64, 0i64, 0i64);
    for e in series {
        t_all += e.tickets;
        p_all += e.points;
        if e.segment == "ai" {
            t_ai += e.tickets;
            p_ai += e.points;
        }
    }
    let pct = |part: i64, all: i64| -> Option<i64> {
        (all > 0).then(|| ((part as f64) * 100.0 / (all as f64)).round() as i64)
    };
    AiShareStats {
        tickets_percent: pct(t_ai, t_all),
        points_percent: pct(p_ai, p_all),
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    fn e(seg: &str, tickets: i64, points: i64) -> ThroughputSeriesEntry {
        ThroughputSeriesEntry {
            period_start: "2026-10-05".into(),
            segment: seg.into(),
            tickets,
            points,
        }
    }

    #[test]
    fn ai_share_is_none_when_nothing_completed() {
        assert_eq!(
            ai_share(&[]),
            AiShareStats {
                tickets_percent: None,
                points_percent: None
            }
        );
    }

    #[test]
    fn ai_share_counts_tickets_and_points_separately() {
        let s = ai_share(&[e("ai", 1, 8), e("human", 3, 2), e("unassigned", 0, 0)]);
        assert_eq!(s.tickets_percent, Some(25));
        assert_eq!(s.points_percent, Some(80));
    }

    #[test]
    fn granularity_parse_accepts_only_week_and_month() {
        assert_eq!(Granularity::parse("week"), Some(Granularity::Week));
        assert_eq!(Granularity::parse("month"), Some(Granularity::Month));
        assert_eq!(Granularity::parse("day"), None);
        assert_eq!(Granularity::parse("'; DROP"), None);
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use crate::test_support::{
        add_test_team_member, create_test_private_project, create_test_ticket, create_test_user,
        test_pool, test_viewer,
    };

    // 2026-09-14 は月曜日。期間は 2026-09-01〜2026-09-30(JST)。
    fn period() -> (NaiveDate, NaiveDate) {
        (
            NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 9, 30).unwrap(),
        )
    }

    fn filters<'a>(ticket_type: Option<&'a str>, project_id: Option<i32>) -> Filters<'a> {
        let (from, to) = period();
        Filters {
            from,
            to,
            project_id,
            team_id: None,
            ticket_type,
        }
    }

    /// 標準のワークフロー(open → in_progress → resolved/closed、canceled)をプロジェクトに用意する。
    async fn seed_workflow(pool: &PgPool, project_id: i32) {
        for (slug, category, pos) in [
            ("open", "unstarted", 1),
            ("in_progress", "started", 2),
            ("resolved", "completed", 3),
            ("closed", "completed", 4),
            ("canceled", "cancelled", 5),
        ] {
            sqlx::query(
                "INSERT INTO t_workflow_status (slug, name, category, color, position, is_default, project_id)
                 VALUES ($1, $1, $2, '#000000', $3, false, $4)
                 ON CONFLICT (project_id, slug) WHERE project_id IS NOT NULL DO NOTHING",
            )
            .bind(slug)
            .bind(category)
            .bind(pos)
            .bind(i64::from(project_id))
            .execute(pool)
            .await
            .unwrap();
        }
    }

    /// 閲覧者 1 人・プロジェクト 1 つ(チームは非公開で、閲覧者だけが所属)の環境。
    struct World {
        pool: PgPool,
        user: i32,
        project: i32,
    }

    async fn world(pool: &PgPool) -> World {
        let user = create_test_user(pool, "insights-user").await;
        // 最初から非公開で作る(公開の時間があると、並行する他のテストの集計に混ざる)
        let project = create_test_private_project(pool, "INS", user).await;
        seed_workflow(pool, project).await;
        let team: i32 = sqlx::query_scalar(
            "SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1",
        )
        .bind(project)
        .fetch_one(pool)
        .await
        .unwrap();
        add_test_team_member(pool, team, user, "member").await;
        World {
            pool: pool.clone(),
            user,
            project,
        }
    }

    impl World {
        async fn ticket(&self, created: &str, points: Option<i16>, ticket_type: &str) -> i32 {
            let id = create_test_ticket(&self.pool, self.project, "INS", self.user).await;
            sqlx::query(
                "UPDATE tickets_ticket SET created_at = $2::timestamptz, story_points = $3, ticket_type = $4 WHERE id = $1",
            )
            .bind(id)
            .bind(created)
            .bind(points)
            .bind(ticket_type)
            .execute(&self.pool)
            .await
            .unwrap();
            id
        }

        async fn history(&self, ticket: i32, old: &str, new: &str, at: &str) {
            sqlx::query(
                "INSERT INTO tickets_status_history (ticket_id, old_status, new_status, changed_at)
                 VALUES ($1, $2, $3, $4::timestamptz)",
            )
            .bind(ticket)
            .bind(old)
            .bind(new)
            .bind(at)
            .execute(&self.pool)
            .await
            .unwrap();
        }

        /// 着手と完了の履歴を入れる(時刻は UTC)。
        async fn done(&self, ticket: i32, started: &str, completed: &str) {
            self.history(ticket, "open", "in_progress", started).await;
            self.history(ticket, "in_progress", "resolved", completed)
                .await;
        }

        async fn assign(&self, ticket: i32) {
            sqlx::query(
                "INSERT INTO tickets_ticket_assignees (ticketmodel_id, user_id) VALUES ($1, $2)",
            )
            .bind(ticket)
            .bind(self.user)
            .execute(&self.pool)
            .await
            .unwrap();
        }

        async fn ai_run(&self, ticket: i32, started: bool, result: Option<&str>, prompt_at: &str) {
            sqlx::query(
                "INSERT INTO t_ai_run (run_uuid, ticket_id, started_at, result, prompt_generated_at)
                 VALUES (gen_random_uuid(), $1, CASE WHEN $2 THEN $4::timestamptz END, $3, $4::timestamptz)",
            )
            .bind(ticket)
            .bind(started)
            .bind(result)
            .bind(prompt_at)
            .execute(&self.pool)
            .await
            .unwrap();
        }

        /// 完了した AI 実行(トークンつき)を入れる。tokens = (入力, 出力, キャッシュ作成, キャッシュ読み取り)。
        async fn ai_run_done(
            &self,
            ticket: i32,
            ended: &str,
            result: &str,
            tokens: Option<(i64, i64, i64, i64)>,
            usd: Option<f64>,
        ) {
            let (i, o, cc, cr) = match tokens {
                Some((i, o, cc, cr)) => (Some(i), Some(o), Some(cc), Some(cr)),
                None => (None, None, None, None),
            };
            sqlx::query(
                "INSERT INTO t_ai_run (run_uuid, ticket_id, started_at, ended_at, result, prompt_generated_at,
                                       input_tokens, output_tokens, cache_creation_tokens, cache_read_tokens, usd_equivalent)
                 VALUES (gen_random_uuid(), $1, $2::timestamptz - interval '1 hour', $2::timestamptz, $3, $2::timestamptz - interval '1 hour',
                         $4, $5, $6, $7, $8)",
            )
            .bind(ticket)
            .bind(ended)
            .bind(result)
            .bind(i)
            .bind(o)
            .bind(cc)
            .bind(cr)
            .bind(usd)
            .execute(&self.pool)
            .await
            .unwrap();
        }

        async fn scope(&self) -> Scope {
            test_viewer(&self.pool, self.user).await.scope()
        }
    }

    fn seg<'a>(r: &'a CycleTimeResult, name: &str) -> &'a SegmentStats {
        r.segments.iter().find(|s| s.segment == name).unwrap()
    }

    #[tokio::test]
    async fn cycle_time_by_segment_with_exclusions() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let w = world(&pool).await;

        // AI: 2 時間
        let a = w.ticket("2026-09-01T00:00:00Z", None, "task").await;
        w.done(a, "2026-09-10T01:00:00Z", "2026-09-10T03:00:00Z")
            .await;
        w.ai_run(a, true, Some("resolved"), "2026-09-10T00:30:00Z")
            .await;
        // 人(担当者あり): 1 日 / 3 日
        let b = w.ticket("2026-09-01T00:00:00Z", None, "task").await;
        w.assign(b).await;
        w.done(b, "2026-09-10T00:00:00Z", "2026-09-11T00:00:00Z")
            .await;
        let c = w.ticket("2026-09-01T00:00:00Z", None, "task").await;
        w.assign(c).await;
        w.done(c, "2026-09-10T00:00:00Z", "2026-09-13T00:00:00Z")
            .await;
        // 再オープン(完了→着手→完了)でも「最初の」着手と完了を使う: 2 時間
        let g = w.ticket("2026-09-01T00:00:00Z", None, "task").await;
        w.assign(g).await;
        w.done(g, "2026-09-12T00:00:00Z", "2026-09-12T02:00:00Z")
            .await;
        w.history(g, "resolved", "in_progress", "2026-09-13T00:00:00Z")
            .await;
        w.history(g, "in_progress", "resolved", "2026-09-20T00:00:00Z")
            .await;
        // 未割当: 1 時間
        let d = w.ticket("2026-09-01T00:00:00Z", None, "task").await;
        w.done(d, "2026-09-15T00:00:00Z", "2026-09-15T01:00:00Z")
            .await;
        // 履歴が無く closed_at だけ(取り込みデータ): サイクルタイムに入れず、noStart に数える
        let e = w.ticket("2026-09-01T00:00:00Z", None, "task").await;
        sqlx::query("UPDATE tickets_ticket SET status = 'closed', closed_at = '2026-09-16T00:00:00Z' WHERE id = $1")
            .bind(e)
            .execute(&pool)
            .await
            .unwrap();
        // 期間外(8 月)に完了: 入らない
        let f = w.ticket("2026-08-01T00:00:00Z", None, "task").await;
        w.done(f, "2026-08-10T00:00:00Z", "2026-08-31T00:00:00Z")
            .await;

        let r = cycle_time_stats(&pool, &filters(None, None), &w.scope().await)
            .await
            .unwrap();
        let ai = seg(&r, "ai");
        assert_eq!((ai.count, ai.median_seconds), (1, Some(7200)));
        let human = seg(&r, "human");
        assert_eq!(human.count, 3, "人: b・c・g(再オープンは 1 件として数える)");
        assert_eq!(human.median_seconds, Some(86_400));
        assert_eq!(human.mean_seconds, Some(117_600));
        assert_eq!(
            human.p90_seconds,
            Some(224_640),
            "p90 = 86400 と 259200 の間(線形補間)"
        );
        let un = seg(&r, "unassigned");
        assert_eq!((un.count, un.median_seconds), (1, Some(3600)));
        assert_eq!(r.excluded.no_start, 1, "e は着手の記録が無い");
        assert_eq!(r.segments.len(), 3, "区分は常に 3 つ返す");
    }

    #[tokio::test]
    async fn cycle_time_filters_by_ticket_type_and_project() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let w = world(&pool).await;
        let bug = w.ticket("2026-09-01T00:00:00Z", None, "bug").await;
        w.assign(bug).await;
        w.done(bug, "2026-09-10T00:00:00Z", "2026-09-10T10:00:00Z")
            .await;
        let task = w.ticket("2026-09-01T00:00:00Z", None, "task").await;
        w.assign(task).await;
        w.done(task, "2026-09-10T00:00:00Z", "2026-09-12T00:00:00Z")
            .await;
        let scope = w.scope().await;

        let all = cycle_time_stats(&pool, &filters(None, None), &scope)
            .await
            .unwrap();
        assert_eq!(seg(&all, "human").count, 2);
        let bugs = cycle_time_stats(&pool, &filters(Some("bug"), None), &scope)
            .await
            .unwrap();
        assert_eq!(seg(&bugs, "human").count, 1);
        assert_eq!(seg(&bugs, "human").median_seconds, Some(36_000));
        let other = cycle_time_stats(&pool, &filters(None, Some(w.project + 1_000_000)), &scope)
            .await
            .unwrap();
        assert_eq!(seg(&other, "human").count, 0, "別のプロジェクトは 0 件");
    }

    /// AI 使用量: 完了していて、トークン量が分かる実行だけを数える。未完了・トークン不明・期間外は、数字に混ぜない。
    #[tokio::test]
    async fn ai_usage_counts_only_finished_runs_with_tokens() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let w = world(&pool).await;
        let a = w.ticket("2026-09-01T00:00:00Z", None, "task").await;
        let b = w.ticket("2026-09-01T00:00:00Z", None, "bug").await;

        // 数える: A に 2 回、B に 1 回(同じ週と、別の週)
        w.ai_run_done(
            a,
            "2026-09-10T03:00:00Z",
            "resolved",
            Some((1000, 200, 500, 10_000)),
            Some(1.5),
        )
        .await;
        w.ai_run_done(
            a,
            "2026-09-11T03:00:00Z",
            "blocked",
            Some((100, 50, 0, 0)),
            Some(0.25),
        )
        .await;
        w.ai_run_done(
            b,
            "2026-09-20T03:00:00Z",
            "resolved",
            Some((2000, 400, 0, 20_000)),
            None,
        )
        .await;
        // 数えない: トークン不明 / 未完了 / 期間外(8 月)
        w.ai_run_done(a, "2026-09-12T03:00:00Z", "error", None, None)
            .await;
        w.ai_run(a, true, None, "2026-09-13T00:00:00Z").await;
        w.ai_run_done(
            b,
            "2026-08-10T03:00:00Z",
            "resolved",
            Some((9, 9, 9, 9)),
            Some(9.0),
        )
        .await;
        let scope = w.scope().await;

        let r = ai_usage_stats(&pool, &filters(None, None), Granularity::Week, &scope)
            .await
            .unwrap();
        assert_eq!(r.totals.runs, 3, "数えるのは、完了+トークンありの 3 件だけ");
        assert_eq!(r.totals.tickets, 2);
        assert_eq!(r.totals.input_tokens, 3100);
        assert_eq!(r.totals.output_tokens, 650);
        assert_eq!(r.totals.cache_creation_tokens, 500);
        assert_eq!(r.totals.cache_read_tokens, 30_000);
        // 加重 = 入力 + 出力 + キャッシュ作成 + 読み取り×0.1: (1000+200+500+1000) + (100+50) + (2000+400+0+2000)
        assert_eq!(r.totals.weighted_tokens, 2700 + 150 + 4400);
        assert!(
            (r.totals.usd_equivalent - 1.75).abs() < 1e-9,
            "換算額は、分かるものだけの合計"
        );
        assert_eq!(r.totals.avg_weighted_per_run, Some(7250 / 3));
        assert_eq!(r.totals.avg_weighted_per_ticket, Some(7250 / 2));
        assert_eq!(r.excluded.no_tokens, 1, "トークン不明の件数を出す");
        assert_eq!(r.excluded.unfinished, 1, "未完了の件数を出す");
        // 推移: 9/7 の週に 2 件、9/14 の週に 1 件
        let weeks: Vec<(String, i64)> = r
            .series
            .iter()
            .map(|e| (e.period_start.clone(), e.runs))
            .collect();
        assert_eq!(
            weeks,
            vec![("2026-09-07".to_string(), 2), ("2026-09-14".to_string(), 1)]
        );
        assert_eq!(
            r.series.iter().map(|e| e.weighted_tokens).sum::<i64>(),
            r.totals.weighted_tokens
        );

        // チケット種別で絞る
        let bugs = ai_usage_stats(
            &pool,
            &filters(Some("bug"), None),
            Granularity::Week,
            &scope,
        )
        .await
        .unwrap();
        assert_eq!((bugs.totals.runs, bugs.totals.tickets), (1, 1));
        // 別のプロジェクトは 0 件
        let other = ai_usage_stats(
            &pool,
            &filters(None, Some(w.project + 1_000_000)),
            Granularity::Week,
            &scope,
        )
        .await
        .unwrap();
        assert_eq!(other.totals.runs, 0);
        assert_eq!(
            other.totals.avg_weighted_per_run, None,
            "0 件のとき平均は null"
        );
    }

    /// AI 使用量: 見えないチームの実行は、数字にも、除外の件数にも入らない。
    #[tokio::test]
    async fn ai_usage_never_includes_other_teams_runs() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let x = world(&pool).await;
        let y = world(&pool).await;
        for w in [&x, &y] {
            let t = w.ticket("2026-09-01T00:00:00Z", None, "task").await;
            w.ai_run_done(
                t,
                "2026-09-10T03:00:00Z",
                "resolved",
                Some((1000, 0, 0, 0)),
                Some(1.0),
            )
            .await;
            w.ai_run_done(t, "2026-09-11T03:00:00Z", "error", None, None)
                .await;
            w.ai_run(t, true, None, "2026-09-12T00:00:00Z").await;
        }
        for me in [&x, &y] {
            let scope = me.scope().await;
            let r = ai_usage_stats(&pool, &filters(None, None), Granularity::Week, &scope)
                .await
                .unwrap();
            assert_eq!(r.totals.runs, 1, "自分のチームの実行だけ");
            assert_eq!(r.totals.weighted_tokens, 1000);
            assert_eq!(r.excluded.no_tokens, 1);
            assert_eq!(r.excluded.unfinished, 1);
        }
        // チームに所属しない人には、何も見えない
        let outsider = create_test_user(&pool, "insights-usage-outsider").await;
        let scope = test_viewer(&pool, outsider).await.scope();
        let r = ai_usage_stats(&pool, &filters(None, None), Granularity::Week, &scope)
            .await
            .unwrap();
        assert_eq!(
            (r.totals.runs, r.excluded.no_tokens, r.excluded.unfinished),
            (0, 0, 0)
        );
    }

    /// 最重要: 見えないチームのチケットは、どの集計にも入らない。
    #[tokio::test]
    async fn other_teams_tickets_never_leak_into_any_aggregate() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let x = world(&pool).await; // 閲覧者 X(自分のチームだけ)
        let y = world(&pool).await; // 別のチームの人 Y

        // どちらのチームにも、完了済み・未着手・AI 実行(未完了/エラー)のチケットを作る
        for w in [&x, &y] {
            let done = w.ticket("2026-09-01T00:00:00Z", Some(5), "task").await;
            w.assign(done).await;
            w.done(done, "2026-09-10T00:00:00Z", "2026-09-11T00:00:00Z")
                .await;
            w.ticket("2026-09-02T00:00:00Z", None, "task").await; // 未着手(waiting)
            let ai = w.ticket("2026-09-01T00:00:00Z", Some(3), "task").await;
            w.done(ai, "2026-09-12T00:00:00Z", "2026-09-12T01:00:00Z")
                .await;
            w.ai_run(ai, true, Some("resolved"), "2026-09-12T00:00:00Z")
                .await;
            w.ai_run(ai, false, None, "2026-09-12T00:00:00Z").await; // 未完了
            w.ai_run(ai, true, Some("error"), "2026-09-12T00:00:00Z")
                .await; // エラー
        }

        for (me, label) in [(&x, "X"), (&y, "Y")] {
            let scope = me.scope().await;
            let f = filters(None, None);

            let ct = cycle_time_stats(&pool, &f, &scope).await.unwrap();
            let total: i64 = ct.segments.iter().map(|s| s.count).sum();
            assert_eq!(
                total, 2,
                "{label}: 自分のチームの完了 2 件だけ(他チームの 2 件が混ざらない)"
            );
            assert_eq!(
                ct.ai_runs.unfinished, 1,
                "{label}: 他チームの AI 実行が混ざらない"
            );
            assert_eq!(
                ct.ai_runs.error, 1,
                "{label}: 他チームの AI 実行が混ざらない"
            );

            let tr = triage_time_stats(&pool, &f, Granularity::Week, &scope)
                .await
                .unwrap();
            assert_eq!(tr.waiting.count, 1, "{label}: 他チームの未着手が混ざらない");
            let started: i64 = tr.series.iter().map(|e| e.count).sum();
            assert_eq!(started, 2, "{label}: 自分のチームの着手 2 件だけ");

            let tp = throughput_stats(&pool, &f, Granularity::Week, &scope)
                .await
                .unwrap();
            let tickets: i64 = tp.series.iter().map(|e| e.tickets).sum();
            let points: i64 = tp.series.iter().map(|e| e.points).sum();
            assert_eq!(
                (tickets, points),
                (2, 8),
                "{label}: 件数もポイントも自分のチームだけ"
            );
        }

        // チームに所属しない人には、何も見えない(0 件)
        let outsider = create_test_user(&pool, "insights-outsider").await;
        let scope = test_viewer(&pool, outsider).await.scope();
        let ct = cycle_time_stats(&pool, &filters(None, None), &scope)
            .await
            .unwrap();
        assert_eq!(ct.segments.iter().map(|s| s.count).sum::<i64>(), 0);
        assert_eq!((ct.ai_runs.unfinished, ct.ai_runs.error), (0, 0));
        let tr = triage_time_stats(&pool, &filters(None, None), Granularity::Week, &scope)
            .await
            .unwrap();
        assert_eq!(tr.waiting.count, 0);
        let tp = throughput_stats(&pool, &filters(None, None), Granularity::Week, &scope)
            .await
            .unwrap();
        assert!(tp.series.is_empty());
    }

    #[tokio::test]
    async fn triage_time_and_waiting() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let w = world(&pool).await;
        // 作成→着手: 6 時間 / 1 日(どちらも 2026-09-07 の週)
        let a = w.ticket("2026-09-10T00:00:00Z", None, "task").await;
        w.assign(a).await;
        w.history(a, "open", "in_progress", "2026-09-10T06:00:00Z")
            .await;
        let b = w.ticket("2026-09-08T00:00:00Z", None, "task").await;
        w.assign(b).await;
        w.history(b, "open", "in_progress", "2026-09-09T00:00:00Z")
            .await;
        // まだ着手されていない: 9/1 作成(期間の終わり 10/1 0 時 JST まで 29 日台)
        w.ticket("2026-09-01T00:00:00Z", None, "task").await;
        // キャンセル済みは waiting に入れない
        let c = w.ticket("2026-09-01T00:00:00Z", None, "task").await;
        sqlx::query("UPDATE tickets_ticket SET status = 'canceled' WHERE id = $1")
            .bind(c)
            .execute(&pool)
            .await
            .unwrap();

        let r = triage_time_stats(
            &pool,
            &filters(None, None),
            Granularity::Week,
            &w.scope().await,
        )
        .await
        .unwrap();
        assert_eq!(r.series.len(), 1);
        let e = &r.series[0];
        assert_eq!(e.period_start, "2026-09-07");
        assert_eq!(e.segment, "human");
        assert_eq!(e.count, 2);
        assert_eq!(e.median_seconds, Some(54_000), "6 時間と 24 時間の中央値");
        assert_eq!(r.waiting.count, 1, "未着手は 1 件(キャンセルは除く)");
        assert_eq!(r.waiting.max_age_days, Some(29));
    }

    #[tokio::test]
    async fn throughput_uses_jst_week_boundaries_and_period_edges() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let w = world(&pool).await;
        // 日曜 23:59 JST(= 14:59 UTC)は 9/7 の週 / 月曜 0:01 JST(= 15:01 UTC)は 9/14 の週
        let sun = w.ticket("2026-09-01T00:00:00Z", Some(3), "task").await;
        w.assign(sun).await;
        w.done(sun, "2026-09-10T00:00:00Z", "2026-09-13T14:59:00Z")
            .await;
        let mon = w.ticket("2026-09-01T00:00:00Z", Some(5), "task").await;
        w.done(mon, "2026-09-10T00:00:00Z", "2026-09-13T15:01:00Z")
            .await;
        w.ai_run(mon, true, Some("resolved"), "2026-09-10T00:00:00Z")
            .await;
        // 期間の端: 9/30 23:30 JST(= 14:30 UTC)は入る / 10/1 0:30 JST(= 15:30 UTC)は入らない
        let inside = w.ticket("2026-09-01T00:00:00Z", None, "task").await;
        w.assign(inside).await;
        w.done(inside, "2026-09-10T00:00:00Z", "2026-09-30T14:30:00Z")
            .await;
        let outside = w.ticket("2026-09-01T00:00:00Z", Some(100), "task").await;
        w.assign(outside).await;
        w.done(outside, "2026-09-10T00:00:00Z", "2026-09-30T15:30:00Z")
            .await;

        let r = throughput_stats(
            &pool,
            &filters(None, None),
            Granularity::Week,
            &w.scope().await,
        )
        .await
        .unwrap();
        let cell = |p: &str, s: &str| {
            r.series
                .iter()
                .find(|e| e.period_start == p && e.segment == s)
                .map(|e| (e.tickets, e.points))
        };
        assert_eq!(cell("2026-09-07", "human"), Some((1, 3)));
        assert_eq!(cell("2026-09-14", "ai"), Some((1, 5)));
        assert_eq!(
            cell("2026-09-28", "human"),
            Some((1, 0)),
            "9/30 23:30 JST は期間内(9/28 の週)"
        );
        let all: i64 = r.series.iter().map(|e| e.tickets).sum();
        assert_eq!(all, 3, "10/1 0:30 JST の 1 件(100 点)は期間外");
        assert_eq!(r.ai_share.tickets_percent, Some(33));
        assert_eq!(
            r.ai_share.points_percent,
            Some(63),
            "5 / (3+5) = 62.5% → 63"
        );

        let m = throughput_stats(
            &pool,
            &filters(None, None),
            Granularity::Month,
            &w.scope().await,
        )
        .await
        .unwrap();
        assert!(
            m.series.iter().all(|e| e.period_start == "2026-09-01"),
            "月は月初で区切る"
        );
    }
}
