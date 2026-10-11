//! presentation/handlers/kb_api.rs — KB エンジン API
//!
//! クローズ判定ロジック呼び出しと結果返却

use axum::{
    extract::Path, extract::State, http::StatusCode, response::IntoResponse, response::Response,
    Json,
};
use chrono::Utc;
use serde_json::json;

use crate::domain::access::Viewer;
use crate::domain::services::kb::{self, config::shared_runner, runner::Runner};
use crate::infrastructure::repositories::{kb_repo, ticket_repo};
use crate::presentation::state::AppState;

/// ブロッカー情報を JSON レスポンスの形式に変換
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct BlockerResponse {
    code: String,
    arg: String,
    message: String,
}

/// ブロッカーの code に対する日本語メッセージを生成
fn message_for(code: &str, arg: &str) -> String {
    match code {
        "open_subtask" => format!("未完了のサブチケットがあります: {}", arg),
        "missing_section" => format!("必須セクション「{}」が未記入または短すぎます", arg),
        "no_assignee" => "担当者が未設定です".to_string(),
        "mixed_type" => {
            "機能追加と障害のラベルが混在しています(別チケットに分けてください)".to_string()
        }
        "unknown_ticket" => "チケットが見つかりません".to_string(),
        _ => format!("{}: {}", code, arg),
    }
}

/// GET /api/v1/tickets/{ticket_key}/close-check/
pub async fn close_check(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(ticket_key): Path<String>,
) -> impl IntoResponse {
    close_check_core(&state, &viewer, &ticket_key, shared_runner(&state.config)).await
}

/// 本体。実行用のランナーを引数で受けるのは、テストで別々の実行ファイルを試せるようにするため
/// (共有ランナーはプロセス全体で1つ)。
async fn close_check_core(
    state: &AppState,
    viewer: &Viewer,
    ticket_key: &str,
    runner: &Runner,
) -> Response {
    // ticket_key の検証
    if !kb::atom::valid_key(ticket_key) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid_key"})),
        )
            .into_response();
    }

    let auth_user_id = match viewer.require_user_id() {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    // チケットの存在確認と閲覧権限チェック
    let ticket = match ticket_repo::api_find_by_key(
        &state.pool,
        ticket_key,
        Some(auth_user_id),
        &state.config.wip_ai_api_user,
    )
    .await
    {
        Ok(Some(t)) => t,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, Json(json!({"error": "not_found"}))).into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "db_error"})),
            )
                .into_response();
        }
    };

    // 閲覧権限チェック
    match crate::presentation::extractors::authorize::authorize_ticket(
        state,
        viewer,
        ticket.base.id,
        crate::domain::access::Action::Read,
        "GET /api/v1/tickets/{ticket_key}/close-check/",
    )
    .await
    {
        Ok(_) => {}
        Err(resp) => return resp,
    }

    // KB エンジンが off の場合
    if state.config.kb_engine == kb::config::KbEngine::Off {
        return (
            StatusCode::OK,
            Json(json!({
                "engine": "off",
                "canClose": serde_json::Value::Null,
                "checkedAt": Utc::now().to_rfc3339(),
                "blockers": []
            })),
        )
            .into_response();
    }

    // KB エンジンが Rules の場合
    // チェック入力を読み込む
    let input = match kb_repo::load_close_check_input(&state.pool, ticket_key).await {
        Ok(Some(i)) => i,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, Json(json!({"error": "not_found"}))).into_response();
        }
        Err(e) => {
            if e.to_string().contains("too many descendants") {
                return (
                    StatusCode::PAYLOAD_TOO_LARGE,
                    Json(json!({"error": "too_many_descendants"})),
                )
                    .into_response();
            }
            tracing::error!("Failed to load close check input: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "db_error"})),
            )
                .into_response();
        }
    };

    // クローズ判定を実行
    let result = match kb::close_check::run_close_check(runner, &input).await {
        Ok(r) => r,
        Err(kb::KbError::Busy) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error": "kb_busy"})),
            )
                .into_response();
        }
        Err(kb::KbError::Timeout) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error": "kb_unavailable"})),
            )
                .into_response();
        }
        Err(kb::KbError::TooLarge) => {
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(json!({"error": "too_large"})),
            )
                .into_response();
        }
        Err(e) => {
            tracing::warn!("KB error: {:?}", e);
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error": "kb_unavailable"})),
            )
                .into_response();
        }
    };

    // ブロッカーをレスポンス形式に変換
    let blockers: Vec<BlockerResponse> = result
        .blockers
        .iter()
        .map(|b| BlockerResponse {
            code: b.code.clone(),
            arg: b.arg.clone(),
            message: message_for(&b.code, &b.arg),
        })
        .collect();

    (
        StatusCode::OK,
        Json(json!({
            "engine": "rules",
            "canClose": result.can_close,
            "checkedAt": Utc::now().to_rfc3339(),
            "blockers": blockers
        })),
    )
        .into_response()
}

// ───────────── 類似障害検索(3B) ─────────────

/// 入力の上限(巨大な入力で、検索の負荷を増やさない)
const SIMILAR_TITLE_MAX_CHARS: usize = 500;
const SIMILAR_DESCRIPTION_MAX_CHARS: usize = 20_000;

fn bad_request(code: &str) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({"error": code}))).into_response()
}

/// 検索の本体。エンジンが off なら何もせず `engine: "off"`、rules なら過去の類似障害を返す。
/// 権限(プロジェクトの閲覧)は、呼び出し側が確認したあとに呼ぶ。
async fn similar_incidents_response(
    state: &AppState,
    project_id: i32,
    title: &str,
    description: &str,
) -> Response {
    if state.config.kb_engine == kb::config::KbEngine::Off {
        return (
            StatusCode::OK,
            Json(json!({"engine": "off", "incidents": []})),
        )
            .into_response();
    }
    match kb::similar::search_similar_incidents(&state.pool, project_id, title, description).await {
        Ok(incidents) => (
            StatusCode::OK,
            Json(json!({"engine": "rules", "incidents": incidents})),
        )
            .into_response(),
        Err(e) => {
            // 付加情報の検索なので、失敗の詳細は返さない(ログにだけ残す)
            tracing::error!("similar incidents search failed: {e}");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error": "kb_unavailable"})),
            )
                .into_response()
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimilarIncidentsBody {
    pub project_prefix: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
}

/// POST /api/v1/ai/similar-incidents/(新規バグ起票画面の「過去の類似障害」)
pub async fn similar_incidents(
    State(state): State<AppState>,
    viewer: Viewer,
    Json(body): Json<SimilarIncidentsBody>,
) -> Response {
    if viewer.require_user_id().is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "unauthorized"})),
        )
            .into_response();
    }
    if !kb::atom::valid_key(&body.project_prefix)
        || body.title.trim().is_empty()
        || body.title.chars().count() > SIMILAR_TITLE_MAX_CHARS
        || body.description.chars().count() > SIMILAR_DESCRIPTION_MAX_CHARS
    {
        return bad_request("invalid_input");
    }
    let project_id =
        match ticket_repo::resolve_project_id_by_prefix(&state.pool, &body.project_prefix).await {
            Ok(Some(id)) => id,
            Ok(None) => {
                return (StatusCode::NOT_FOUND, Json(json!({"error": "not_found"}))).into_response()
            }
            Err(e) => {
                tracing::error!("DB operation failed: {:?}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "db_error"})),
                )
                    .into_response();
            }
        };
    // プロジェクトの閲覧権限(閲覧できないプロジェクトの過去の障害は返さない)
    if let Err(resp) = crate::presentation::extractors::authorize::authorize_project_open_today(
        &state,
        &viewer,
        project_id,
        crate::domain::access::Action::Read,
        "POST /api/v1/ai/similar-incidents/",
    )
    .await
    {
        return resp;
    }
    similar_incidents_response(&state, project_id, &body.title, &body.description).await
}

#[derive(serde::Deserialize)]
pub struct AgentSimilarQuery {
    pub project_prefix: Option<String>,
    pub q: Option<String>,
}

/// GET /api/v1/ai-agent/similar-incidents/?project_prefix=XXX&q=…(障害自動化パイプラインのコンテキスト注入)
pub async fn agent_similar_incidents(
    State(state): State<AppState>,
    caller: crate::presentation::handlers::ai_agent_api::AiCaller,
    axum::extract::Query(params): axum::extract::Query<AgentSimilarQuery>,
) -> Response {
    let prefix = params.project_prefix.unwrap_or_default();
    let q = params.q.unwrap_or_default();
    if !kb::atom::valid_key(&prefix)
        || q.trim().is_empty()
        || q.chars().count() > SIMILAR_DESCRIPTION_MAX_CHARS
    {
        return bad_request("invalid_input");
    }
    if let Err(resp) = crate::presentation::handlers::ai_agent_api::ai_project_gate(
        &state,
        &caller,
        &prefix,
        crate::domain::access::Action::Read,
        "GET /api/v1/ai-agent/similar-incidents/",
    )
    .await
    {
        return resp;
    }
    let project_id = match ticket_repo::resolve_project_id_by_prefix(&state.pool, &prefix).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return crate::presentation::handlers::ai_agent_api::project_not_found(&state, &prefix)
                .await
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "db_error"})),
            )
                .into_response();
        }
    };
    // q は 1 つの文章。先頭の 1 行をタイトル、残りを本文として扱う(検索語は全体から取る)
    similar_incidents_response(&state, project_id, &q, "").await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_for_open_subtask() {
        assert_eq!(
            message_for("open_subtask", "DEMO-001"),
            "未完了のサブチケットがあります: DEMO-001"
        );
    }

    #[test]
    fn test_message_for_missing_section() {
        assert_eq!(
            message_for("missing_section", "実施した内容"),
            "必須セクション「実施した内容」が未記入または短すぎます"
        );
    }

    #[test]
    fn test_message_for_no_assignee() {
        assert_eq!(message_for("no_assignee", ""), "担当者が未設定です");
    }

    #[test]
    fn test_message_for_mixed_type() {
        assert_eq!(
            message_for("mixed_type", ""),
            "機能追加と障害のラベルが混在しています(別チケットに分けてください)"
        );
    }

    #[test]
    fn test_message_for_unknown_ticket() {
        assert_eq!(
            message_for("unknown_ticket", ""),
            "チケットが見つかりません"
        );
    }

    #[test]
    fn test_message_for_unknown_code() {
        assert_eq!(
            message_for("custom_error", "some arg"),
            "custom_error: some arg"
        );
    }

    // ───────────── API テスト(実 DB・実 Scryer) ─────────────
    // DB が無い環境ではスキップ(REQUIRE_TEST_DB=1 なら失敗)。Scryer が無いときも同様(REQUIRE_SCRYER=1)。

    use crate::config::AppConfig;
    use crate::domain::services::kb::config::KbEngine;
    use crate::test_support::{
        add_test_team_member, create_test_project, create_test_ticket, create_test_user,
        test_config, test_pool, test_viewer,
    };
    use std::time::Duration;

    fn scryer_bin() -> Option<String> {
        let bin = std::env::var("SENN_KB_SCRYER_BIN").unwrap_or_else(|_| "scryer-prolog".into());
        let ok = std::process::Command::new(&bin)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !ok {
            assert!(
                std::env::var("REQUIRE_SCRYER").as_deref() != Ok("1"),
                "REQUIRE_SCRYER=1 but scryer-prolog is not runnable: {bin}"
            );
            eprintln!("skip: scryer-prolog is not available");
            return None;
        }
        Some(bin)
    }

    async fn state_with(pool: &sqlx::PgPool, engine: KbEngine) -> AppState {
        let cfg: AppConfig = AppConfig {
            kb_engine: engine,
            ..test_config()
        };
        AppState::new(pool.clone(), cfg, None).await.expect("state")
    }

    async fn call(
        state: &AppState,
        viewer: &Viewer,
        key: &str,
        runner: &Runner,
    ) -> (StatusCode, serde_json::Value) {
        let resp = close_check_core(state, viewer, key, runner).await;
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
            .await
            .expect("body");
        let v = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, v)
    }

    async fn key_of(pool: &sqlx::PgPool, id: i32) -> String {
        sqlx::query_scalar("SELECT ticket_key FROM tickets_ticket WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .expect("key")
    }

    /// 親 + 子1件のチケットを作り、(viewer, 親キー, 子キー, 子id) を返す
    async fn fixture(pool: &sqlx::PgPool) -> (Viewer, String, String, i32) {
        let user = create_test_user(pool, "kbapi").await;
        let project = create_test_project(pool, "KBA", user).await;
        // 閲覧権限はチームの所属で決まるので、作成者をプロジェクトのチームに入れる
        let team: i32 = sqlx::query_scalar(
            "SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 ORDER BY team_id LIMIT 1",
        )
        .bind(project)
        .fetch_one(pool)
        .await
        .expect("team");
        add_test_team_member(pool, team, user, "member").await;
        let parent = create_test_ticket(pool, project, "KBA", user).await;
        let child = create_test_ticket(pool, project, "KBA", user).await;
        sqlx::query(
            "UPDATE tickets_ticket SET parent_id = $2, status = 'in_progress' WHERE id = $1",
        )
        .bind(child)
        .bind(parent)
        .execute(pool)
        .await
        .expect("link child");
        sqlx::query(
            "INSERT INTO tickets_ticket_assignees (ticketmodel_id, user_id) VALUES ($1, $2)",
        )
        .bind(parent)
        .bind(user)
        .execute(pool)
        .await
        .expect("assignee");
        let viewer = test_viewer(pool, user).await;
        (
            viewer,
            key_of(pool, parent).await,
            key_of(pool, child).await,
            child,
        )
    }

    #[tokio::test]
    async fn api_engine_off_returns_off_without_running_anything() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let (viewer, parent_key, _, _) = fixture(&pool).await;
        let state = state_with(&pool, KbEngine::Off).await;
        // 存在しない実行ファイルでも影響しない(起動されない)
        let runner = Runner::new("/nonexistent/scryer", Duration::from_secs(1), 1);
        let (status, v) = call(&state, &viewer, &parent_key, &runner).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(v["engine"], "off");
        assert!(v["canClose"].is_null());
        assert_eq!(v["blockers"].as_array().map(|a| a.len()), Some(0));
    }

    #[tokio::test]
    async fn api_invalid_key_is_400_and_unknown_is_404() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let (viewer, _, _, _) = fixture(&pool).await;
        let state = state_with(&pool, KbEngine::Rules).await;
        let runner = Runner::new("/nonexistent/scryer", Duration::from_secs(1), 1);
        for bad in ["a b", "x'; drop", "日本語", &"A".repeat(40)] {
            let (status, _) = call(&state, &viewer, bad, &runner).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{bad}");
        }
        let (status, _) = call(&state, &viewer, "NOPE-999999", &runner).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn api_other_user_cannot_read_the_check() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let (_, parent_key, _, _) = fixture(&pool).await;
        let stranger = create_test_user(&pool, "kbstranger").await;
        let viewer = test_viewer(&pool, stranger).await;
        let state = state_with(&pool, KbEngine::Rules).await;
        let runner = Runner::new("/nonexistent/scryer", Duration::from_secs(1), 1);
        let (status, v) = call(&state, &viewer, &parent_key, &runner).await;
        assert_ne!(
            status,
            StatusCode::OK,
            "権限の無い人に判定結果を返してはいけない"
        );
        assert!(v.get("blockers").is_none());
    }

    #[tokio::test]
    async fn api_open_child_blocks_and_done_child_allows() {
        let Some(bin) = scryer_bin() else { return };
        let Some(pool) = test_pool().await else {
            return;
        };
        let (viewer, parent_key, child_key, child_id) = fixture(&pool).await;
        let state = state_with(&pool, KbEngine::Rules).await;
        let runner = Runner::new(bin, Duration::from_secs(10), 2);

        let (status, v) = call(&state, &viewer, &parent_key, &runner).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["engine"], "rules");
        assert_eq!(v["canClose"], false);
        let blockers = v["blockers"].as_array().expect("array");
        assert_eq!(blockers.len(), 1, "{v}");
        assert_eq!(blockers[0]["code"], "open_subtask");
        assert_eq!(blockers[0]["arg"], child_key.as_str());
        assert!(blockers[0]["message"]
            .as_str()
            .is_some_and(|m| m.contains(&child_key)));

        // 子を完了にすると可になる
        sqlx::query("UPDATE tickets_ticket SET status = 'closed' WHERE id = $1")
            .bind(child_id)
            .execute(&pool)
            .await
            .expect("close child");
        let (status, v) = call(&state, &viewer, &parent_key, &runner).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["canClose"], true, "{v}");
        assert_eq!(v["blockers"].as_array().map(|a| a.len()), Some(0));
    }

    #[tokio::test]
    async fn api_scryer_failure_is_503_and_never_says_closable() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let (viewer, parent_key, _, _) = fixture(&pool).await;
        let state = state_with(&pool, KbEngine::Rules).await;
        let runner = Runner::new("/nonexistent/scryer-prolog", Duration::from_secs(1), 1);
        let (status, v) = call(&state, &viewer, &parent_key, &runner).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(v["error"], "kb_unavailable");
        assert!(v.get("canClose").is_none());
    }

    // ───────────── 類似障害検索(3B)のテスト ─────────────

    async fn prefix_of(pool: &sqlx::PgPool, project: i32) -> String {
        sqlx::query_scalar("SELECT prefix FROM tickets_project WHERE id = $1")
            .bind(project)
            .fetch_one(pool)
            .await
            .expect("prefix")
    }

    /// 完了済みの障害を作る(症状・調査・対策つき)
    /// 無関係な過去の障害を n 件作る(点数の重みは、候補が少ないと偏るので、ある程度の数を用意する)
    async fn filler_bugs(pool: &sqlx::PgPool, project: i32, user: i32, n: usize) {
        for i in 0..n {
            let id = create_test_ticket(pool, project, "KBA", user).await;
            sqlx::query("UPDATE tickets_ticket SET title = $2, description = '別の話題の説明', status = 'closed', ticket_type = 'bug' WHERE id = $1")
                .bind(id)
                .bind(format!("冷蔵庫の温度警告が鳴る その{i}"))
                .execute(pool)
                .await
                .expect("filler");
        }
    }

    async fn past_bug(pool: &sqlx::PgPool, project: i32, user: i32, title: &str) -> String {
        let id = create_test_ticket(pool, project, "KBA", user).await;
        sqlx::query("UPDATE tickets_ticket SET title = $2, description = $3, status = 'closed', ticket_type = 'bug' WHERE id = $1")
            .bind(id)
            .bind(title)
            .bind("## 症状\n請求書 一括出力 が失敗する。\n## 調査\n待ち時間が短い rust/src/billing.rs\n## 対策\n待ち時間を延ばした\n")
            .execute(pool)
            .await
            .expect("bug");
        key_of(pool, id).await
    }

    async fn post_similar(
        state: &AppState,
        viewer: &Viewer,
        prefix: &str,
        title: &str,
        description: &str,
    ) -> (StatusCode, serde_json::Value) {
        let resp = similar_incidents(
            State(state.clone()),
            viewer.clone(),
            Json(SimilarIncidentsBody {
                project_prefix: prefix.to_string(),
                title: title.to_string(),
                description: description.to_string(),
            }),
        )
        .await;
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
            .await
            .expect("body");
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
        )
    }

    #[tokio::test]
    async fn similar_incidents_off_returns_empty_without_searching() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let (viewer, _, _, _) = fixture(&pool).await;
        let user = viewer.user_id().expect("user");
        let project = create_test_project(&pool, "KBS", user).await;
        past_bug(&pool, project, user, "請求書の出力が失敗する").await;
        past_bug(&pool, project, user, "請求書の出力が止まる").await;
        let prefix = prefix_of(&pool, project).await;
        let state = state_with(&pool, KbEngine::Off).await;
        let (status, v) =
            post_similar(&state, &viewer, &prefix, "請求書 一括出力 が落ちる", "").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(v["engine"], "off");
        assert_eq!(v["incidents"], json!([]));
    }

    #[tokio::test]
    async fn similar_incidents_rules_returns_past_incidents_with_reasons_and_excerpts() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let (viewer, _, _, _) = fixture(&pool).await;
        let user = viewer.user_id().expect("user");
        let project = create_test_project(&pool, "KBS", user).await;
        filler_bugs(&pool, project, user, 12).await;
        let k1 = past_bug(&pool, project, user, "請求書の出力が失敗する").await;
        let k2 = past_bug(&pool, project, user, "請求書の出力が止まる").await;
        // 機能追加(障害でない)は出ない
        let feat = create_test_ticket(&pool, project, "KBA", user).await;
        sqlx::query("UPDATE tickets_ticket SET title = '請求書の一括出力', description = '請求書 一括出力', status = 'closed', ticket_type = 'task' WHERE id = $1")
            .bind(feat)
            .execute(&pool)
            .await
            .expect("feat");
        let feat_key = key_of(&pool, feat).await;
        let prefix = prefix_of(&pool, project).await;
        let state = state_with(&pool, KbEngine::Rules).await;
        let (status, v) = post_similar(
            &state,
            &viewer,
            &prefix,
            "請求書 一括出力 が落ちる",
            "rust/src/billing.rs を確認",
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["engine"], "rules");
        let list = v["incidents"].as_array().expect("array");
        let keys: Vec<&str> = list
            .iter()
            .filter_map(|x| x["ticketKey"].as_str())
            .collect();
        assert!(
            keys.contains(&k1.as_str()) && keys.contains(&k2.as_str()),
            "{keys:?}"
        );
        assert!(
            !keys.contains(&feat_key.as_str()),
            "機能追加は出ない: {keys:?}"
        );
        let first = &list[0];
        assert!(first["score"].as_u64().unwrap_or(0) >= 25, "{first}");
        assert_eq!(first["symptom"], "請求書 一括出力 が失敗する。");
        assert_eq!(first["cause"], "待ち時間が短い rust/src/billing.rs");
        assert_eq!(first["solution"], "待ち時間を延ばした");
        let reasons: Vec<&str> = first["reasons"]
            .as_array()
            .expect("reasons")
            .iter()
            .filter_map(|r| r.as_str())
            .collect();
        assert!(
            reasons.iter().any(|r| r.starts_with("共通する語: ")),
            "{reasons:?}"
        );
        assert!(
            reasons.iter().any(|r| r.contains("rust/src/billing.rs")),
            "同じファイル: {reasons:?}"
        );
    }

    #[tokio::test]
    async fn similar_incidents_match_even_when_wording_differs() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let (viewer, _, _, _) = fixture(&pool).await;
        let user = viewer.user_id().expect("user");
        let project = create_test_project(&pool, "KBS", user).await;
        filler_bugs(&pool, project, user, 12).await;
        let k = past_bug(&pool, project, user, "ログインできない").await;
        let prefix = prefix_of(&pool, project).await;
        let state = state_with(&pool, KbEngine::Rules).await;
        let (status, v) = post_similar(&state, &viewer, &prefix, "ログインできない", "").await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let keys: Vec<&str> = v["incidents"]
            .as_array()
            .expect("array")
            .iter()
            .filter_map(|x| x["ticketKey"].as_str())
            .collect();
        assert!(
            keys.contains(&k.as_str()),
            "1 語でも長い複合語なら出る: {keys:?}"
        );
        // 言い回しが違っても、重なりが多ければ出る(「ログインできない」→「ログイン」だけ)
        let (_, v2) = post_similar(&state, &viewer, &prefix, "ログインが出来ない", "").await;
        let keys2: Vec<&str> = v2["incidents"]
            .as_array()
            .expect("array")
            .iter()
            .filter_map(|x| x["ticketKey"].as_str())
            .collect();
        assert!(
            keys2.contains(&k.as_str()),
            "言い回しが違っても出る: {keys2:?}"
        );
        // 関係の無い入力では出ない
        let (_, v3) = post_similar(&state, &viewer, &prefix, "猫の写真が表示されない", "").await;
        assert!(
            v3["incidents"].as_array().expect("array").is_empty(),
            "{v3}"
        );
    }

    #[tokio::test]
    async fn similar_incidents_never_returns_other_projects_and_validates_input() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let (viewer, _, _, _) = fixture(&pool).await;
        let user = viewer.user_id().expect("user");
        let mine = create_test_project(&pool, "KBS", user).await;
        let other = create_test_project(&pool, "KBT", user).await;
        let secret = past_bug(&pool, other, user, "請求書の出力が失敗する(別)").await;
        past_bug(&pool, other, user, "請求書の出力が止まる(別)").await;
        let prefix = prefix_of(&pool, mine).await;
        let state = state_with(&pool, KbEngine::Rules).await;
        let (status, v) =
            post_similar(&state, &viewer, &prefix, "請求書 一括出力 が落ちる", "").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(v["incidents"], json!([]), "別プロジェクトは返さない: {v}");
        assert!(!v.to_string().contains(&secret));

        // 入力の検証
        for (p, t, d, want) in [
            (prefix.as_str(), "", "", StatusCode::BAD_REQUEST),
            (prefix.as_str(), "   ", "", StatusCode::BAD_REQUEST),
            ("bad prefix!", "x", "", StatusCode::BAD_REQUEST),
            ("NOPE-NONE", "請求書 一括出力", "", StatusCode::NOT_FOUND),
        ] {
            let (status, _) = post_similar(&state, &viewer, p, t, d).await;
            assert_eq!(status, want, "{p:?}/{t:?}");
        }
        let long = "あ".repeat(SIMILAR_TITLE_MAX_CHARS + 1);
        let (status, _) = post_similar(&state, &viewer, &prefix, &long, "").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let long = "あ".repeat(SIMILAR_DESCRIPTION_MAX_CHARS + 1);
        let (status, _) = post_similar(&state, &viewer, &prefix, "題名", &long).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn agent_similar_incidents_returns_results_and_validates_input() {
        use crate::presentation::handlers::ai_agent_api::{AiAuthResult, AiCaller};
        let Some(pool) = test_pool().await else {
            return;
        };
        let (viewer, _, _, _) = fixture(&pool).await;
        let user = viewer.user_id().expect("user");
        let project = create_test_project(&pool, "KBS", user).await;
        filler_bugs(&pool, project, user, 12).await;
        let k1 = past_bug(&pool, project, user, "請求書の出力が失敗する").await;
        past_bug(&pool, project, user, "請求書の出力が止まる").await;
        let prefix = prefix_of(&pool, project).await;
        let state = state_with(&pool, KbEngine::Rules).await;
        let caller = || AiCaller {
            legacy: AiAuthResult {
                user_id: user,
                acting_user_id: None,
                key_id: None,
            },
            viewer: None,
        };
        let call = |p: Option<&str>, q: Option<&str>| {
            let st = state.clone();
            let c = caller();
            let (p, q) = (p.map(str::to_string), q.map(str::to_string));
            async move {
                let resp = agent_similar_incidents(
                    State(st),
                    c,
                    axum::extract::Query(AgentSimilarQuery {
                        project_prefix: p,
                        q,
                    }),
                )
                .await;
                let status = resp.status();
                let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
                    .await
                    .expect("body");
                (
                    status,
                    serde_json::from_slice::<serde_json::Value>(&bytes)
                        .unwrap_or(serde_json::Value::Null),
                )
            }
        };
        let (status, v) = call(Some(&prefix), Some("請求書 一括出力 が落ちる")).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let keys: Vec<String> = v["incidents"]
            .as_array()
            .expect("array")
            .iter()
            .filter_map(|x| x["ticketKey"].as_str().map(str::to_string))
            .collect();
        assert!(keys.contains(&k1), "{keys:?}");
        // 入力の検証
        assert_eq!(call(None, Some("x")).await.0, StatusCode::BAD_REQUEST);
        assert_eq!(call(Some(&prefix), None).await.0, StatusCode::BAD_REQUEST);
        assert_eq!(
            call(Some(&prefix), Some("  ")).await.0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            call(Some("NOPE-NONE"), Some("請求書 一括出力")).await.0,
            StatusCode::NOT_FOUND
        );
    }
}
