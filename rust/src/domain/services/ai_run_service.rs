/// domain/services/ai_run_service.rs — AI実行ライフサイクル管理
///
/// AI(Claude Code 等)の「着手・完了・失敗」を `t_ai_run` に記録し、チケットのステータス・
/// コメント・通知に反映する。設計: docs/design/実装計画_AI自動ワークフローとInsights.md §3.1 / §3.5。
///
/// - 着手: チケットを started 分類へ(履歴・closed_at は既存の遷移関数が処理)。
/// - 完了(resolved): resolved へ。**closed にはしない**(本番確認後に人が閉じる)。
/// - 失敗/未報告: チケットにコメントし、プロンプトを作った人(いなければ担当者・作成者)へ通知。1実行につき1回。
use crate::domain::services::git_webhook_service;
use crate::infrastructure::repositories::{
    ai_run_repo, notification_repo, ticket_repo, workflow_status_repo,
};
use chrono::Utc;
use sqlx::PgPool;
use std::fmt;
use uuid::Uuid;

/// エラー型。
#[derive(Debug)]
pub enum AiRunError {
    NotFound,
    Conflict(String),
    BadRequest(String),
    Internal(anyhow::Error),
}

impl fmt::Display for AiRunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AiRunError::NotFound => write!(f, "実行が見つかりません"),
            AiRunError::Conflict(msg) => write!(f, "{}", msg),
            AiRunError::BadRequest(msg) => write!(f, "{}", msg),
            AiRunError::Internal(e) => write!(f, "Internal error: {}", e),
        }
    }
}

fn internal<E: Into<anyhow::Error>>(e: E) -> AiRunError {
    AiRunError::Internal(e.into())
}

/// start_run の入力。
#[derive(Debug, Default)]
pub struct StartInput {
    pub ticket_key: String,
    pub tool: Option<String>,
    pub model: Option<String>,
    pub trigger_type: Option<String>,
    pub triggered_by_id: Option<i32>,
}

/// finish_run の入力。result は resolved|blocked|aborted のみ。
#[derive(Debug)]
pub struct FinishInput<'a> {
    pub result: &'a str,
    pub summary: &'a str,
    pub usage: ai_run_repo::TokenUsage,
}

#[derive(Debug)]
pub struct StartOutcome {
    pub ticket_key: String,
    pub started_at: chrono::DateTime<Utc>,
    /// この呼び出しで着手を記録した(=初回)なら true。2 回目以降は false。
    pub created: bool,
    /// 遷移後のチケットのステータス(slug)。
    pub ticket_status: String,
}

#[derive(Debug)]
pub struct FinishOutcome {
    pub result: String,
    pub ended_at: chrono::DateTime<Utc>,
    /// 遷移後のチケットのステータス(slug)。
    pub ticket_status: String,
}

const FINAL_RESULTS: [&str; 3] = ["resolved", "blocked", "aborted"];

struct TicketCtx {
    status: String,
    project_id: Option<i32>,
    team_id: Option<i32>,
}

async fn ticket_ctx(pool: &PgPool, ticket_id: i32) -> Result<TicketCtx, AiRunError> {
    let row: Option<(String, Option<i32>, Option<i32>)> = sqlx::query_as(
        "SELECT status, project_id::int4, team_id::int4 FROM tickets_ticket WHERE id = $1::int4",
    )
    .bind(ticket_id)
    .fetch_optional(pool)
    .await
    .map_err(internal)?;
    let (status, project_id, team_id) = row.ok_or(AiRunError::NotFound)?;
    Ok(TicketCtx {
        status,
        project_id,
        team_id,
    })
}

async fn current_status(pool: &PgPool, ticket_id: i32) -> Result<String, AiRunError> {
    Ok(ticket_ctx(pool, ticket_id).await?.status)
}

/// ステータスの遷移を試みる。失敗しても記録そのものは止めず、ログに残す
/// (git 連携と同じ流儀。単調性ガード=完了/キャンセル済みからは戻さない、は遷移関数が持つ)。
async fn move_ticket(pool: &PgPool, ticket_id: i32, ai_user_id: i32, slug: &str, trigger: &str) {
    if let Err(e) =
        git_webhook_service::apply_git_status_transition(pool, ticket_id, slug, ai_user_id, trigger)
            .await
    {
        tracing::warn!(
            "[AI実行] 処理=ステータス遷移 結果=失敗 影響=チケット状態が未更新 | ticket_id={} slug={} {:?}",
            ticket_id,
            slug,
            e
        );
    }
}

/// 完了に使う slug。そのスコープに "resolved" があり分類が completed ならそれ(AI は closed にしない)。
/// 無ければ completed 分類の先頭 slug。
async fn pick_resolved_slug(pool: &PgPool, ticket_id: i32, ctx: &TicketCtx) -> Option<String> {
    let cat =
        workflow_status_repo::lookup_status_category(pool, ctx.project_id, ctx.team_id, "resolved")
            .await
            .ok()
            .flatten();
    if cat.as_deref() == Some("completed") {
        return Some("resolved".to_string());
    }
    git_webhook_service::resolve_target_slug_for_ticket(pool, ticket_id, "completed")
        .await
        .ok()
        .flatten()
}

/// 要対応(blocked)に使う slug。そのスコープに "blocked" という slug があるときだけ(無ければ遷移しない)。
async fn pick_blocked_slug(pool: &PgPool, ctx: &TicketCtx) -> Option<String> {
    let cat =
        workflow_status_repo::lookup_status_category(pool, ctx.project_id, ctx.team_id, "blocked")
            .await
            .ok()
            .flatten();
    cat.map(|_| "blocked".to_string())
}

/// 実行を開始する。同じ run_uuid の 2 回目以降は何も変えない(冪等)。
pub async fn start_run(
    pool: &PgPool,
    ai_user_id: i32,
    run_uuid: Uuid,
    input: StartInput,
) -> Result<StartOutcome, AiRunError> {
    let StartInput {
        ticket_key,
        tool,
        model,
        trigger_type,
        triggered_by_id,
    } = input;
    let ticket_key = ticket_key.as_str();
    let ticket_id = ticket_repo::resolve_ticket_id(pool, ticket_key)
        .await
        .map_err(internal)?
        .ok_or(AiRunError::NotFound)?;

    // 既存の行(SENN がプロンプト生成時に作った行、または 2 回目の呼び出し)
    let existing = ai_run_repo::find_by_uuid(pool, &run_uuid)
        .await
        .map_err(internal)?;
    let run = match existing {
        Some(r) => r,
        None => {
            let input = ai_run_repo::InsertInput {
                run_uuid,
                ticket_id: ticket_id as i64,
                trigger_type: trigger_type.clone(),
                tool: tool.clone(),
                model: model.clone(),
                triggered_by_id: triggered_by_id.map(i64::from),
            };
            match ai_run_repo::insert(pool, &input).await {
                Ok(r) => r,
                // 同時に 2 回呼ばれて、片方が先に作った場合は、その行を使う
                Err(e) => ai_run_repo::find_by_uuid(pool, &run_uuid)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| internal(e))?,
            }
        }
    };

    if run.ticket_id != i64::from(ticket_id) {
        return Err(AiRunError::Conflict(
            "この実行UUIDは別のチケットで使われています".to_string(),
        ));
    }

    // 着手を記録(初回だけ true)
    let started_now = ai_run_repo::mark_started(pool, run.id, tool.as_deref(), model.as_deref())
        .await
        .map_err(internal)?;

    if started_now {
        let ctx = ticket_ctx(pool, ticket_id).await?;
        let category = workflow_status_repo::lookup_status_category(
            pool,
            ctx.project_id,
            ctx.team_id,
            &ctx.status,
        )
        .await
        .map_err(internal)?
        .unwrap_or_default();
        // 完了・キャンセル済みでなければ、開始日を入れる(未設定のときだけ。人が入れた開始日は変えない)。
        // AI が着手しても開始日が空のままだと、ガントチャートなどに載らないため
        if !["completed", "cancelled"].contains(&category.as_str()) {
            if let Err(e) =
                ai_run_repo::set_start_date_if_missing(pool, ticket_id, ai_user_id).await
            {
                tracing::warn!(
                    "[AI実行] 処理=開始日の設定 結果=失敗 影響=開始日が空のまま | ticket_id={} {:?}",
                    ticket_id,
                    e
                );
            }
        }
        // すでに着手済み・完了・キャンセルの分類なら動かさない
        if !["started", "completed", "cancelled"].contains(&category.as_str()) {
            match git_webhook_service::resolve_target_slug_for_ticket(pool, ticket_id, "started")
                .await
                .map_err(internal)?
            {
                Some(slug) => move_ticket(pool, ticket_id, ai_user_id, &slug, "ai_start").await,
                None => tracing::warn!(
                    "[AI実行] 処理=着手の遷移 結果=スキップ 影響=チケット状態が未更新 | ticket_id={} 理由=started 分類のステータスが無い",
                    ticket_id
                ),
            }
        }
    }

    let run = ai_run_repo::find_by_uuid(pool, &run_uuid)
        .await
        .map_err(internal)?
        .ok_or(AiRunError::NotFound)?;
    Ok(StartOutcome {
        ticket_key: ticket_key.to_string(),
        started_at: run.started_at.unwrap_or_else(Utc::now),
        created: started_now,
        ticket_status: current_status(pool, ticket_id).await?,
    })
}

/// 実行を完了する。result は resolved|blocked|aborted のみ。
/// 先に「未完了の行だけを更新」して権利を取り、そのあとで遷移・コメントを行う
/// (同時に 2 回呼ばれても、遷移・コメントが二重にならない)。
pub async fn finish_run(
    pool: &PgPool,
    ai_user_id: i32,
    run_uuid: Uuid,
    input: FinishInput<'_>,
) -> Result<FinishOutcome, AiRunError> {
    let FinishInput {
        result,
        summary,
        usage,
    } = input;
    if !FINAL_RESULTS.contains(&result) {
        return Err(AiRunError::BadRequest(
            "result は resolved, blocked, aborted のいずれかを指定してください".to_string(),
        ));
    }

    let run = ai_run_repo::find_by_uuid(pool, &run_uuid)
        .await
        .map_err(internal)?
        .ok_or(AiRunError::NotFound)?;
    let ticket_id = run.ticket_id as i32;

    let claimed = ai_run_repo::mark_finished(pool, run.id, result, summary, &usage)
        .await
        .map_err(internal)?;

    if claimed {
        let ctx = ticket_ctx(pool, ticket_id).await?;
        match result {
            "resolved" => {
                if let Some(slug) = pick_resolved_slug(pool, ticket_id, &ctx).await {
                    move_ticket(pool, ticket_id, ai_user_id, &slug, "ai_finish").await;
                }
            }
            "blocked" => {
                if let Some(slug) = pick_blocked_slug(pool, &ctx).await {
                    move_ticket(pool, ticket_id, ai_user_id, &slug, "ai_finish").await;
                }
            }
            _ => {} // aborted: 動かさない
        }

        let done = ai_run_repo::find_by_uuid(pool, &run_uuid)
            .await
            .map_err(internal)?
            .ok_or(AiRunError::NotFound)?;
        let heading = match result {
            "resolved" => "AI作業が完了しました(解決済み)。",
            "blocked" => "AI作業が止まりました(要対応)。",
            _ => "AI作業を中断しました。",
        };
        let body = format!(
            "{}\n\n{}\n\n実行ID: {} / 所要時間: {}",
            heading,
            summary,
            run_uuid,
            format_duration(&done)
        );
        add_ai_comment(pool, ticket_id, ai_user_id, &body).await;
    }

    let done = ai_run_repo::find_by_uuid(pool, &run_uuid)
        .await
        .map_err(internal)?
        .ok_or(AiRunError::NotFound)?;
    Ok(FinishOutcome {
        result: done.result.clone().unwrap_or_else(|| result.to_string()),
        ended_at: done.ended_at.unwrap_or_else(Utc::now),
        ticket_status: current_status(pool, ticket_id).await?,
    })
}

/// 連携のエラーを報告する。run が無くても ticket_key があれば、エラーの行を新規に作って記録する。
pub async fn report_error(
    pool: &PgPool,
    ai_user_id: i32,
    run_uuid: Uuid,
    ticket_key: Option<&str>,
    detail: &str,
) -> Result<(), AiRunError> {
    let run = match ai_run_repo::find_by_uuid(pool, &run_uuid)
        .await
        .map_err(internal)?
    {
        Some(r) => r,
        None => {
            let key = ticket_key.ok_or(AiRunError::NotFound)?;
            let ticket_id = ticket_repo::resolve_ticket_id(pool, key)
                .await
                .map_err(internal)?
                .ok_or(AiRunError::NotFound)?;
            let input = ai_run_repo::InsertInput {
                run_uuid,
                ticket_id: i64::from(ticket_id),
                trigger_type: None,
                tool: None,
                model: None,
                triggered_by_id: None,
            };
            match ai_run_repo::insert(pool, &input).await {
                Ok(r) => r,
                Err(e) => ai_run_repo::find_by_uuid(pool, &run_uuid)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| internal(e))?,
            }
        }
    };

    ai_run_repo::mark_error(pool, run.id, detail)
        .await
        .map_err(internal)?;

    let body = format!("⚠ AI連携エラー: {}\n\n実行ID: {}", detail, run_uuid);
    let message = format!("AI連携エラー: {} (実行ID: {})", detail, run_uuid);
    post_problem(pool, ai_user_id, &run, &body, "AI連携エラー", &message).await;
    Ok(())
}

/// 見張り: 「プロンプト作成後に着手の報告が無い」「着手後に完了の報告が無い」実行を見つけ、
/// コメントと通知を 1 回だけ出す。result は変えない(あとから finish を受け付けるため)。
/// 処理した件数を返す。
pub async fn run_watchdog_once(
    pool: &PgPool,
    ai_user_id: i32,
    start_timeout_min: i64,
    finish_timeout_min: i64,
) -> Result<usize, AiRunError> {
    let mut count = 0;

    let unstarted = ai_run_repo::list_unstarted_overdue(pool, start_timeout_min)
        .await
        .map_err(internal)?;
    for run in unstarted {
        let body = format!(
            "⚠ AI連携の確認: プロンプトを作成してから{}分たちましたが、AIから着手の報告がありません。貼り付け忘れか、連携コマンドの失敗の可能性があります。\n\n実行ID: {}",
            start_timeout_min, run.run_uuid
        );
        let message = format!("AIから着手の報告がありません (実行ID: {})", run.run_uuid);
        if post_problem(pool, ai_user_id, &run, &body, "AI連携の確認", &message).await {
            count += 1;
        }
    }

    let unfinished = ai_run_repo::list_unfinished_overdue(pool, finish_timeout_min)
        .await
        .map_err(internal)?;
    for run in unfinished {
        let body = format!(
            "⚠ AI連携の確認: 着手から{}分たちましたが、完了の報告がありません。\n\n実行ID: {}",
            finish_timeout_min, run.run_uuid
        );
        let message = format!("AIから完了の報告がありません (実行ID: {})", run.run_uuid);
        if post_problem(pool, ai_user_id, &run, &body, "AI連携の確認", &message).await {
            count += 1;
        }
    }

    Ok(count)
}

/// 問題のコメントと通知を、1実行につき 1 回だけ出す。出したら true。
async fn post_problem(
    pool: &PgPool,
    ai_user_id: i32,
    run: &ai_run_repo::AiRun,
    comment_body: &str,
    title: &str,
    message: &str,
) -> bool {
    match ai_run_repo::set_notified(pool, run.id).await {
        Ok(true) => {}
        Ok(false) => return false,
        Err(e) => {
            tracing::error!(
                "[AI実行] 処理=通知済みの記録 結果=失敗 影響=通知なし | {:?}",
                e
            );
            return false;
        }
    }
    let ticket_id = run.ticket_id as i32;
    add_ai_comment(pool, ticket_id, ai_user_id, comment_body).await;
    for user_id in notify_recipients(pool, run, ai_user_id).await {
        if let Err(e) =
            notification_repo::create(pool, user_id, Some(ticket_id), "updated", title, message)
                .await
        {
            tracing::error!("[AI実行] 処理=通知作成 結果=失敗 影響=通知なし | {:?}", e);
        }
    }
    true
}

/// 通知の宛先。プロンプトを作った人。いなければ(CLI から直接 start した等)担当者と作成者。
async fn notify_recipients(pool: &PgPool, run: &ai_run_repo::AiRun, ai_user_id: i32) -> Vec<i32> {
    if let Some(id) = run.triggered_by_id {
        return vec![id as i32];
    }
    let rows: Vec<(i32,)> = sqlx::query_as(
        "SELECT DISTINCT u FROM (
             SELECT user_id::int4 AS u FROM tickets_ticket_assignees WHERE ticketmodel_id = $1
             UNION
             SELECT author_id::int4 FROM tickets_ticket WHERE id = $1
         ) x WHERE u <> $2",
    )
    .bind(run.ticket_id)
    .bind(ai_user_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    rows.into_iter().map(|(u,)| u).collect()
}

async fn add_ai_comment(pool: &PgPool, ticket_id: i32, ai_user_id: i32, body: &str) {
    if let Err(e) = ticket_repo::api_add_comment(
        pool,
        ticket_id,
        ai_user_id,
        body,
        None,
        None,
        Some(ai_user_id),
    )
    .await
    {
        tracing::error!(
            "[AI実行] 処理=コメント追加 結果=失敗 影響=コメントなし | {:?}",
            e
        );
    }
}

/// 所要時間を「X分Y秒」で返す(着手から終了、終了前なら現在まで)。
fn format_duration(run: &ai_run_repo::AiRun) -> String {
    let start = run.started_at.unwrap_or_else(Utc::now);
    let end = run.ended_at.unwrap_or_else(Utc::now);
    let secs = end.signed_duration_since(start).num_seconds().max(0);
    format!("{}分{}秒", secs / 60, secs % 60)
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use crate::test_support::{
        create_test_project, create_test_ticket, create_test_user, test_pool,
    };

    /// テスト用のプロジェクトに標準のワークフロー(open → in_progress → resolved → closed)を用意する。
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

    struct Env {
        pool: PgPool,
        ai_user: i32,
        author: i32,
        project: i32,
    }

    async fn env() -> Option<Env> {
        let pool = test_pool().await?;
        let ai_user = create_test_user(&pool, "ai-run-bot").await;
        let author = create_test_user(&pool, "ai-run-author").await;
        let project = create_test_project(&pool, "AIR", author).await;
        seed_workflow(&pool, project).await;
        Some(Env {
            pool,
            ai_user,
            author,
            project,
        })
    }

    async fn new_ticket(e: &Env) -> (i32, String) {
        let id = create_test_ticket(&e.pool, e.project, "AIR", e.author).await;
        let key: String = sqlx::query_scalar("SELECT ticket_key FROM tickets_ticket WHERE id = $1")
            .bind(id)
            .fetch_one(&e.pool)
            .await
            .unwrap();
        (id, key)
    }

    async fn status_of(pool: &PgPool, ticket_id: i32) -> String {
        sqlx::query_scalar("SELECT status FROM tickets_ticket WHERE id = $1")
            .bind(ticket_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn count(pool: &PgPool, sql: &str, id: i32) -> i64 {
        sqlx::query_scalar(sql)
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn comments(pool: &PgPool, ticket_id: i32) -> i64 {
        count(
            pool,
            "SELECT COUNT(*) FROM tickets_comment WHERE ticket_id = $1",
            ticket_id,
        )
        .await
    }

    async fn history(pool: &PgPool, ticket_id: i32) -> i64 {
        count(
            pool,
            "SELECT COUNT(*) FROM tickets_status_history WHERE ticket_id = $1",
            ticket_id,
        )
        .await
    }

    async fn notifications(pool: &PgPool, user_id: i32) -> i64 {
        count(
            pool,
            "SELECT COUNT(*) FROM notifications_notification WHERE user_id = $1",
            user_id,
        )
        .await
    }

    async fn start(e: &Env, uuid: Uuid, key: &str) -> Result<StartOutcome, AiRunError> {
        start_run(
            &e.pool,
            e.ai_user,
            uuid,
            StartInput {
                ticket_key: key.to_string(),
                ..Default::default()
            },
        )
        .await
    }

    async fn finish(e: &Env, uuid: Uuid, result: &str) -> Result<FinishOutcome, AiRunError> {
        finish_run(
            &e.pool,
            e.ai_user,
            uuid,
            FinishInput {
                result,
                summary: "要約",
                usage: Default::default(),
            },
        )
        .await
    }

    #[tokio::test]
    async fn start_moves_ticket_and_is_idempotent() {
        let Some(e) = env().await else { return };
        let (tid, key) = new_ticket(&e).await;
        let uuid = Uuid::new_v4();

        let a = start(&e, uuid, &key).await.unwrap();
        assert!(a.created);
        assert_eq!(a.ticket_status, "in_progress");
        assert_eq!(status_of(&e.pool, tid).await, "in_progress");
        assert_eq!(history(&e.pool, tid).await, 1);

        let b = start(&e, uuid, &key).await.unwrap();
        assert!(!b.created, "2回目は何も変えない");
        assert_eq!(history(&e.pool, tid).await, 1, "履歴が増えない");
        assert_eq!(a.started_at, b.started_at, "着手時刻が変わらない");
    }

    #[tokio::test]
    async fn start_sets_start_date_only_when_missing_and_logs_it() {
        let Some(e) = env().await else { return };
        let (tid, key) = new_ticket(&e).await;
        let start_date = |pool: sqlx::PgPool| async move {
            sqlx::query_scalar::<_, Option<chrono::NaiveDate>>(
                "SELECT start_date FROM tickets_ticket WHERE id = $1",
            )
            .bind(tid)
            .fetch_one(&pool)
            .await
            .unwrap()
        };
        assert_eq!(start_date(e.pool.clone()).await, None, "最初は空");

        let uuid = Uuid::new_v4();
        start(&e, uuid, &key).await.unwrap();
        let jst_today = sqlx::query_scalar::<_, chrono::NaiveDate>(
            "SELECT (now() AT TIME ZONE 'Asia/Tokyo')::date",
        )
        .fetch_one(&e.pool)
        .await
        .unwrap();
        assert_eq!(
            start_date(e.pool.clone()).await,
            Some(jst_today),
            "AI が着手したら、開始日(日本時間の今日)が入る"
        );
        let logs: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM tickets_change_log WHERE ticket_id = $1 AND field_name = 'start_date'",
        )
        .bind(tid)
        .fetch_one(&e.pool)
        .await
        .unwrap();
        assert_eq!(logs, 1, "変更の履歴に残る");

        // 2 回目の開始(同じ実行)では、何も変えない
        start(&e, uuid, &key).await.unwrap();
        let logs: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM tickets_change_log WHERE ticket_id = $1 AND field_name = 'start_date'",
        )
        .bind(tid)
        .fetch_one(&e.pool)
        .await
        .unwrap();
        assert_eq!(logs, 1, "履歴が増えない");
    }

    #[tokio::test]
    async fn start_keeps_a_start_date_that_a_human_already_set() {
        let Some(e) = env().await else { return };
        let (tid, key) = new_ticket(&e).await;
        sqlx::query("UPDATE tickets_ticket SET start_date = '2026-01-15' WHERE id = $1")
            .bind(tid)
            .execute(&e.pool)
            .await
            .unwrap();
        start(&e, Uuid::new_v4(), &key).await.unwrap();
        let d: Option<chrono::NaiveDate> =
            sqlx::query_scalar("SELECT start_date FROM tickets_ticket WHERE id = $1")
                .bind(tid)
                .fetch_one(&e.pool)
                .await
                .unwrap();
        assert_eq!(
            d,
            chrono::NaiveDate::from_ymd_opt(2026, 1, 15),
            "人が入れた開始日は変えない"
        );
    }

    #[tokio::test]
    async fn start_does_not_set_start_date_on_a_completed_ticket() {
        let Some(e) = env().await else { return };
        let (tid, key) = new_ticket(&e).await;
        sqlx::query("UPDATE tickets_ticket SET status = 'closed' WHERE id = $1")
            .bind(tid)
            .execute(&e.pool)
            .await
            .unwrap();
        start(&e, Uuid::new_v4(), &key).await.unwrap();
        let d: Option<chrono::NaiveDate> =
            sqlx::query_scalar("SELECT start_date FROM tickets_ticket WHERE id = $1")
                .bind(tid)
                .fetch_one(&e.pool)
                .await
                .unwrap();
        assert_eq!(d, None, "完了済みのチケットには、開始日を入れない");
    }

    #[tokio::test]
    async fn start_rejects_uuid_used_for_another_ticket() {
        let Some(e) = env().await else { return };
        let (_, key1) = new_ticket(&e).await;
        let (_, key2) = new_ticket(&e).await;
        let uuid = Uuid::new_v4();
        start(&e, uuid, &key1).await.unwrap();
        assert!(matches!(
            start(&e, uuid, &key2).await,
            Err(AiRunError::Conflict(_))
        ));
    }

    #[tokio::test]
    async fn start_unknown_ticket_is_not_found() {
        let Some(e) = env().await else { return };
        assert!(matches!(
            start(&e, Uuid::new_v4(), "NOPE-999999").await,
            Err(AiRunError::NotFound)
        ));
    }

    #[tokio::test]
    async fn start_does_not_reopen_completed_ticket() {
        let Some(e) = env().await else { return };
        let (tid, key) = new_ticket(&e).await;
        sqlx::query("UPDATE tickets_ticket SET status = 'closed' WHERE id = $1")
            .bind(tid)
            .execute(&e.pool)
            .await
            .unwrap();
        start(&e, Uuid::new_v4(), &key).await.unwrap();
        assert_eq!(
            status_of(&e.pool, tid).await,
            "closed",
            "完了済みは戻さない"
        );
        assert_eq!(history(&e.pool, tid).await, 0);
    }

    #[tokio::test]
    async fn finish_resolved_goes_to_resolved_not_closed_and_is_idempotent() {
        let Some(e) = env().await else { return };
        let (tid, key) = new_ticket(&e).await;
        let uuid = Uuid::new_v4();
        start(&e, uuid, &key).await.unwrap();

        let a = finish(&e, uuid, "resolved").await.unwrap();
        assert_eq!(a.result, "resolved");
        assert_eq!(
            a.ticket_status, "resolved",
            "AI は resolved まで。closed にしない"
        );
        let closed_at: Option<chrono::DateTime<Utc>> =
            sqlx::query_scalar("SELECT closed_at FROM tickets_ticket WHERE id = $1")
                .bind(tid)
                .fetch_one(&e.pool)
                .await
                .unwrap();
        assert!(closed_at.is_some(), "完了分類に入ったので closed_at が入る");
        assert_eq!(comments(&e.pool, tid).await, 1);
        let h = history(&e.pool, tid).await;
        assert_eq!(h, 2, "open→in_progress、in_progress→resolved");

        let b = finish(&e, uuid, "resolved").await.unwrap();
        assert_eq!(b.result, "resolved");
        assert_eq!(comments(&e.pool, tid).await, 1, "コメントが増えない");
        assert_eq!(history(&e.pool, tid).await, h, "履歴が増えない");
    }

    #[tokio::test]
    async fn finish_concurrent_calls_comment_once() {
        let Some(e) = env().await else { return };
        let (tid, key) = new_ticket(&e).await;
        let uuid = Uuid::new_v4();
        start(&e, uuid, &key).await.unwrap();
        let (a, b) = tokio::join!(finish(&e, uuid, "resolved"), finish(&e, uuid, "resolved"));
        a.unwrap();
        b.unwrap();
        assert_eq!(
            comments(&e.pool, tid).await,
            1,
            "同時に呼んでもコメントは1件"
        );
        assert_eq!(history(&e.pool, tid).await, 2);
    }

    #[tokio::test]
    async fn finish_blocked_and_aborted_do_not_complete_ticket() {
        let Some(e) = env().await else { return };
        let (tid, key) = new_ticket(&e).await;
        let u1 = Uuid::new_v4();
        start(&e, u1, &key).await.unwrap();
        finish(&e, u1, "blocked").await.unwrap();
        assert_eq!(
            status_of(&e.pool, tid).await,
            "in_progress",
            "blocked の slug が無ければ動かさない"
        );

        let (tid2, key2) = new_ticket(&e).await;
        let u2 = Uuid::new_v4();
        start(&e, u2, &key2).await.unwrap();
        finish(&e, u2, "aborted").await.unwrap();
        assert_eq!(status_of(&e.pool, tid2).await, "in_progress");
        assert_eq!(comments(&e.pool, tid2).await, 1);
    }

    #[tokio::test]
    async fn finish_validates_result_and_run() {
        let Some(e) = env().await else { return };
        let (_, key) = new_ticket(&e).await;
        let uuid = Uuid::new_v4();
        start(&e, uuid, &key).await.unwrap();
        assert!(matches!(
            finish(&e, uuid, "closed").await,
            Err(AiRunError::BadRequest(_))
        ));
        assert!(matches!(
            finish(&e, Uuid::new_v4(), "resolved").await,
            Err(AiRunError::NotFound)
        ));
    }

    #[tokio::test]
    async fn finish_without_start_fills_started_at() {
        let Some(e) = env().await else { return };
        let (tid, _) = new_ticket(&e).await;
        let uuid = Uuid::new_v4();
        // SENN がプロンプト作成時に作る行(着手の報告はまだ)
        ai_run_repo::insert(
            &e.pool,
            &ai_run_repo::InsertInput {
                run_uuid: uuid,
                ticket_id: i64::from(tid),
                trigger_type: None,
                tool: None,
                model: None,
                triggered_by_id: None,
            },
        )
        .await
        .unwrap();
        finish(&e, uuid, "resolved").await.unwrap();
        let run = ai_run_repo::find_by_uuid(&e.pool, &uuid)
            .await
            .unwrap()
            .unwrap();
        assert!(run.started_at.is_some() && run.ended_at.is_some());
        assert_eq!(status_of(&e.pool, tid).await, "resolved");
    }

    #[tokio::test]
    async fn error_comments_once_and_notifies_triggerer() {
        let Some(e) = env().await else { return };
        let (tid, key) = new_ticket(&e).await;
        let uuid = Uuid::new_v4();
        let person = create_test_user(&e.pool, "ai-run-person").await;
        start_run(
            &e.pool,
            e.ai_user,
            uuid,
            StartInput {
                ticket_key: key.clone(),
                triggered_by_id: Some(person),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        report_error(
            &e.pool,
            e.ai_user,
            uuid,
            None,
            "トークンログの読み込みに失敗",
        )
        .await
        .unwrap();
        report_error(&e.pool, e.ai_user, uuid, None, "もう一度")
            .await
            .unwrap();

        assert_eq!(comments(&e.pool, tid).await, 1, "コメントは1回だけ");
        assert_eq!(notifications(&e.pool, person).await, 1, "通知も1回だけ");
        let run = ai_run_repo::find_by_uuid(&e.pool, &uuid)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(run.result.as_deref(), Some("error"));
        assert!(
            run.error_detail.contains("読み込みに失敗") && run.error_detail.contains("もう一度")
        );
    }

    #[tokio::test]
    async fn error_without_run_creates_it_and_falls_back_to_author() {
        let Some(e) = env().await else { return };
        let (tid, key) = new_ticket(&e).await;
        let uuid = Uuid::new_v4();
        assert!(matches!(
            report_error(&e.pool, e.ai_user, uuid, None, "x").await,
            Err(AiRunError::NotFound)
        ));
        report_error(&e.pool, e.ai_user, uuid, Some(&key), "start に失敗しました")
            .await
            .unwrap();
        let run = ai_run_repo::find_by_uuid(&e.pool, &uuid)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(run.result.as_deref(), Some("error"));
        assert_eq!(comments(&e.pool, tid).await, 1);
        assert_eq!(
            notifications(&e.pool, e.author).await,
            1,
            "宛先が無ければ作成者へ"
        );
    }

    #[tokio::test]
    async fn finish_after_error_is_accepted() {
        let Some(e) = env().await else { return };
        let (tid, key) = new_ticket(&e).await;
        let uuid = Uuid::new_v4();
        start(&e, uuid, &key).await.unwrap();
        report_error(&e.pool, e.ai_user, uuid, None, "一時的な失敗")
            .await
            .unwrap();
        finish(&e, uuid, "resolved").await.unwrap();
        assert_eq!(status_of(&e.pool, tid).await, "resolved");
        let run = ai_run_repo::find_by_uuid(&e.pool, &uuid)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(run.result.as_deref(), Some("resolved"));
    }

    #[tokio::test]
    async fn watchdog_flags_overdue_runs_once() {
        let Some(e) = env().await else { return };
        let mk = |tid: i32| ai_run_repo::InsertInput {
            run_uuid: Uuid::new_v4(),
            ticket_id: i64::from(tid),
            trigger_type: None,
            tool: None,
            model: None,
            triggered_by_id: None,
        };
        let (t_unstarted, _) = new_ticket(&e).await;
        let (t_unfinished, _) = new_ticket(&e).await;
        let (t_fresh, _) = new_ticket(&e).await;
        let (t_done, _) = new_ticket(&e).await;

        let old = ai_run_repo::insert(&e.pool, &mk(t_unstarted))
            .await
            .unwrap();
        sqlx::query(
            "UPDATE t_ai_run SET prompt_generated_at = NOW() - interval '3 hours' WHERE id = $1",
        )
        .bind(old.id)
        .execute(&e.pool)
        .await
        .unwrap();

        let unfinished = ai_run_repo::insert(&e.pool, &mk(t_unfinished))
            .await
            .unwrap();
        sqlx::query("UPDATE t_ai_run SET started_at = NOW() - interval '30 hours' WHERE id = $1")
            .bind(unfinished.id)
            .execute(&e.pool)
            .await
            .unwrap();

        ai_run_repo::insert(&e.pool, &mk(t_fresh)).await.unwrap();

        let done = ai_run_repo::insert(&e.pool, &mk(t_done)).await.unwrap();
        sqlx::query("UPDATE t_ai_run SET prompt_generated_at = NOW() - interval '3 hours', started_at = NOW() - interval '2 hours', ended_at = NOW(), result = 'resolved' WHERE id = $1")
            .bind(done.id).execute(&e.pool).await.unwrap();

        let n = run_watchdog_once(&e.pool, e.ai_user, 120, 1440)
            .await
            .unwrap();
        assert!(
            n >= 2,
            "未着手と未完了の2件は検出される(他のテストの行が混ざることがあるので以上で判定)"
        );
        assert_eq!(comments(&e.pool, t_unstarted).await, 1);
        assert_eq!(comments(&e.pool, t_unfinished).await, 1);
        assert_eq!(comments(&e.pool, t_fresh).await, 0, "新しい実行は対象外");
        assert_eq!(comments(&e.pool, t_done).await, 0, "完了済みは対象外");

        run_watchdog_once(&e.pool, e.ai_user, 120, 1440)
            .await
            .unwrap();
        assert_eq!(comments(&e.pool, t_unstarted).await, 1, "2回目は増えない");

        // 見張りが通知したあとでも、完了の報告は受け付ける
        let uuid = unfinished.run_uuid;
        finish(&e, uuid, "resolved").await.unwrap();
        assert_eq!(status_of(&e.pool, t_unfinished).await, "resolved");
    }
}

/// チケット作業用プロンプトの末尾に付ける SENN 記録手順ブロック。
/// AI(Claude Code 等)が作業の開始・完了・失敗時に呼ぶべき MCP ツール・シェル
/// コマンドと、チケットの closed 禁止など重要な指示を含む。
pub fn build_instruction_block(ticket_key: &str, run_uuid: &Uuid, tool: &str) -> String {
    format!(
        r#"---
## SENN への作業記録(必須)
このチケットの作業は SENN に記録されます。次の手順を必ず実行してください。実行ID: {}

1. 作業を始める前(最初の操作): SENN の MCP ツール `senn_ai_run_start` を、ticket_key="{}"、run_uuid="{}"、tool="{}" で呼ぶ。MCP ツールが使えない場合は、シェルで `senn-ai-run start {} --run {} --tool {}` を実行する。
2. 作業が終わったら: `senn_ai_run_finish`(run_uuid="{}"、result、summary)を呼ぶ(シェルなら `senn-ai-run finish {} --result <結果> --summary "<要約>"`)。
   - result: resolved=実装・テスト・PR 作成まで終えた / blocked=人の判断や確認が必要で止まった / aborted=中断した。
   - summary: 何をしたかを 2〜3 行で(専門用語を避け、担当者が読める言葉で)。
   - マージと本番反映は人が行う。resolved にしても、チケットを closed にはしないこと。
3. 失敗したとき(コマンドが使えない・エラーになった): `senn_ai_run_error`(run_uuid、detail)を呼ぶ(シェルなら `senn-ai-run error {} --detail "<内容>"`)。それも使えない場合は、最終報告にその旨を書く。
"#,
        run_uuid,
        ticket_key,
        run_uuid,
        tool,
        ticket_key,
        run_uuid,
        tool,
        run_uuid,
        run_uuid,
        run_uuid,
    )
}

#[cfg(test)]
mod build_instruction_block_tests {
    use super::*;

    #[test]
    fn contains_required_elements() {
        let ticket_key = "ABC-123";
        let run_uuid = Uuid::new_v4();
        let tool = "claude_code";

        let block = build_instruction_block(ticket_key, &run_uuid, tool);

        // 重要な要素が含まれているか確認
        assert!(block.contains("ABC-123"), "ticket_key が含まれている");
        assert!(
            block.contains(&run_uuid.to_string()),
            "run_uuid が含まれている"
        );
        assert!(block.contains("claude_code"), "tool が含まれている");
        assert!(
            block.contains("senn_ai_run_start"),
            "開始コマンドが含まれている"
        );
        assert!(
            block.contains("senn_ai_run_finish"),
            "完了コマンドが含まれている"
        );
        assert!(
            block.contains("senn_ai_run_error"),
            "エラーコマンドが含まれている"
        );
        assert!(
            block.contains("closed にはしないこと"),
            "closed 禁止の文言が含まれている"
        );

        // 長さは 1,500 文字以内
        assert!(
            block.len() <= 1500,
            "ブロックサイズが 1,500 文字以内(実際: {})",
            block.len()
        );
    }

    #[test]
    fn different_tools() {
        let ticket_key = "PROJ-456";
        let run_uuid = Uuid::new_v4();

        let claude_code_block = build_instruction_block(ticket_key, &run_uuid, "claude_code");
        assert!(claude_code_block.contains("claude_code"));

        let cursor_block = build_instruction_block(ticket_key, &run_uuid, "cursor");
        assert!(cursor_block.contains("cursor"));
        assert!(!cursor_block.contains("claude_code"));
    }
}
