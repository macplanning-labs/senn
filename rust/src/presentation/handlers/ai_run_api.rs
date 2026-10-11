/// presentation/handlers/ai_run_api.rs — AI実行ライフサイクルのHTTPハンドラ
use crate::domain::services::ai_run_service;
use crate::presentation::handlers::ai_agent_api::{ai_ticket_gate, AiCaller};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::domain::access::Action;
use crate::presentation::state::AppState;

fn err(detail: impl Into<String>) -> serde_json::Value {
    json!({"error": detail.into()})
}

// ============================================================================
// リクエスト/レスポンス型
// ============================================================================

#[derive(Deserialize)]
pub struct StartRunIn {
    pub ticket_key: String,
    pub tool: Option<String>,
    pub model: Option<String>,
    pub trigger_type: Option<String>,
}

#[derive(Serialize)]
pub struct StartRunOut {
    pub run_uuid: Uuid,
    pub ticket_key: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub created: bool,
    pub ticket_status: String,
}

#[derive(Deserialize)]
pub struct FinishRunIn {
    pub result: String,
    pub summary: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_read_tokens: Option<i64>,
    pub cache_creation_tokens: Option<i64>,
    pub usd_equivalent: Option<f64>,
}

#[derive(Serialize)]
pub struct FinishRunOut {
    pub run_uuid: Uuid,
    pub result: String,
    pub ended_at: chrono::DateTime<chrono::Utc>,
    pub ticket_status: String,
}

#[derive(Deserialize)]
pub struct ErrorRunIn {
    pub detail: String,
    pub ticket_key: Option<String>,
}

#[derive(Serialize)]
pub struct ErrorRunOut {
    pub run_uuid: Uuid,
    pub recorded: bool,
}

// ============================================================================
// ハンドラ
// ============================================================================

/// POST /api/v1/ai-agent/runs/{run_uuid}/start/
pub async fn start_run(
    State(state): State<AppState>,
    caller: AiCaller,
    Path(run_uuid): Path<Uuid>,
    Json(body): Json<StartRunIn>,
) -> impl IntoResponse {
    // チケット権限チェック
    if let Err(resp) = ai_ticket_gate(
        &state,
        &caller,
        &body.ticket_key,
        Action::Write,
        "POST /api/v1/ai-agent/runs/{run_uuid}/start/",
    )
    .await
    {
        return resp;
    }

    let ai_user_id = caller.legacy.user_id;

    match ai_run_service::start_run(
        &state.pool,
        ai_user_id,
        run_uuid,
        ai_run_service::StartInput {
            ticket_key: body.ticket_key.clone(),
            tool: body.tool,
            model: body.model,
            trigger_type: body.trigger_type,
            triggered_by_id: caller.legacy.acting_user_id,
        },
    )
    .await
    {
        Ok(outcome) => (
            StatusCode::OK,
            Json(StartRunOut {
                run_uuid,
                ticket_key: outcome.ticket_key,
                started_at: outcome.started_at,
                created: outcome.created,
                ticket_status: outcome.ticket_status,
            }),
        )
            .into_response(),
        Err(ai_run_service::AiRunError::NotFound) => {
            (StatusCode::NOT_FOUND, Json(err("チケットが見つかりません"))).into_response()
        }
        Err(ai_run_service::AiRunError::Conflict(msg)) => {
            (StatusCode::CONFLICT, Json(err(msg))).into_response()
        }
        Err(ai_run_service::AiRunError::BadRequest(msg)) => {
            (StatusCode::BAD_REQUEST, Json(err(msg))).into_response()
        }
        Err(ai_run_service::AiRunError::Internal(e)) => {
            tracing::error!("start_run failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
    }
}

/// POST /api/v1/ai-agent/runs/{run_uuid}/finish/
pub async fn finish_run(
    State(state): State<AppState>,
    caller: AiCaller,
    Path(run_uuid): Path<Uuid>,
    Json(body): Json<FinishRunIn>,
) -> impl IntoResponse {
    let ai_user_id = caller.legacy.user_id;

    // run を取得してチケットを引く(権限チェック用)
    let run = match crate::infrastructure::repositories::ai_run_repo::find_by_uuid(
        &state.pool,
        &run_uuid,
    )
    .await
    {
        Ok(Some(r)) => r,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, Json(err("実行が見つかりません"))).into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
    };

    // ticket_key を引く
    let ticket_key = match crate::infrastructure::repositories::ticket_repo::find_by_id(
        &state.pool,
        run.ticket_id as i32,
    )
    .await
    {
        Ok(Some(t)) => t.ticket_key,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, Json(err("チケットが見つかりません"))).into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
    };

    // チケット権限チェック
    if let Err(resp) = ai_ticket_gate(
        &state,
        &caller,
        &ticket_key,
        Action::Write,
        "POST /api/v1/ai-agent/runs/{run_uuid}/finish/",
    )
    .await
    {
        return resp;
    }

    let summary = body.summary.unwrap_or_default();
    if summary.chars().count() > 2000 {
        return (
            StatusCode::BAD_REQUEST,
            Json(err("summary は2000字以内で指定してください")),
        )
            .into_response();
    }

    match ai_run_service::finish_run(
        &state.pool,
        ai_user_id,
        run_uuid,
        ai_run_service::FinishInput {
            result: &body.result,
            summary: &summary,
            usage: crate::infrastructure::repositories::ai_run_repo::TokenUsage {
                input_tokens: body.input_tokens,
                output_tokens: body.output_tokens,
                cache_read_tokens: body.cache_read_tokens,
                cache_creation_tokens: body.cache_creation_tokens,
                usd_equivalent: body.usd_equivalent,
            },
        },
    )
    .await
    {
        Ok(outcome) => (
            StatusCode::OK,
            Json(FinishRunOut {
                run_uuid,
                result: outcome.result,
                ended_at: outcome.ended_at,
                ticket_status: outcome.ticket_status,
            }),
        )
            .into_response(),
        Err(ai_run_service::AiRunError::NotFound) => {
            (StatusCode::NOT_FOUND, Json(err("実行が見つかりません"))).into_response()
        }
        Err(ai_run_service::AiRunError::BadRequest(msg)) => {
            (StatusCode::BAD_REQUEST, Json(err(msg))).into_response()
        }
        Err(ai_run_service::AiRunError::Internal(e)) => {
            tracing::error!("finish_run failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
        Err(ai_run_service::AiRunError::Conflict(msg)) => {
            (StatusCode::CONFLICT, Json(err(msg))).into_response()
        }
    }
}

/// POST /api/v1/ai-agent/runs/{run_uuid}/error/
pub async fn error_run(
    State(state): State<AppState>,
    caller: AiCaller,
    Path(run_uuid): Path<Uuid>,
    Json(body): Json<ErrorRunIn>,
) -> impl IntoResponse {
    let detail_len = body.detail.chars().count();
    if detail_len == 0 || detail_len > 2000 {
        return (
            StatusCode::BAD_REQUEST,
            Json(err("detail は1〜2000字で指定してください")),
        )
            .into_response();
    }

    let ai_user_id = caller.legacy.user_id;

    // 権限は必ずチケットで確認する。run が既にあれば run のチケット、無ければ body の ticket_key(必須)。
    let existing = match crate::infrastructure::repositories::ai_run_repo::find_by_uuid(
        &state.pool,
        &run_uuid,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
    };
    let gate_key: String = match existing {
        Some(run) => {
            let run_ticket_key = match crate::infrastructure::repositories::ticket_repo::find_by_id(
                &state.pool,
                run.ticket_id as i32,
            )
            .await
            {
                Ok(Some(t)) => t.ticket_key,
                _ => {
                    return (StatusCode::NOT_FOUND, Json(err("チケットが見つかりません")))
                        .into_response();
                }
            };
            if let Some(k) = body.ticket_key.as_deref() {
                if k != run_ticket_key {
                    return (
                        StatusCode::CONFLICT,
                        Json(err("この実行は別のチケットのものです")),
                    )
                        .into_response();
                }
            }
            run_ticket_key
        }
        None => match body.ticket_key.clone() {
            Some(k) => k,
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(err(
                        "実行が見つかりません(ticket_key を指定すると新規に記録します)",
                    )),
                )
                    .into_response();
            }
        },
    };
    if let Err(resp) = ai_ticket_gate(
        &state,
        &caller,
        &gate_key,
        Action::Write,
        "POST /api/v1/ai-agent/runs/{run_uuid}/error/",
    )
    .await
    {
        return resp;
    }

    match ai_run_service::report_error(
        &state.pool,
        ai_user_id,
        run_uuid,
        body.ticket_key.as_deref(),
        &body.detail,
    )
    .await
    {
        Ok(_) => (
            StatusCode::OK,
            Json(ErrorRunOut {
                run_uuid,
                recorded: true,
            }),
        )
            .into_response(),
        Err(ai_run_service::AiRunError::NotFound) => (
            StatusCode::NOT_FOUND,
            Json(err("実行またはチケットが見つかりません")),
        )
            .into_response(),
        Err(ai_run_service::AiRunError::Internal(e)) => {
            tracing::error!("error_run failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(err("サーバーエラーが発生しました")),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod http_tests {
    //! ルーターを通した HTTP テスト(認証・権限・ルーティング・入力検証)。
    use crate::test_support::{
        create_test_project, create_test_ticket, create_test_user, test_config, test_pool,
    };
    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use serde_json::{json, Value};
    use sqlx::PgPool;
    use tower::ServiceExt;
    use uuid::Uuid;

    const KEY: &str = "test-ai-run-key-0123456789";

    async fn app(pool: &PgPool) -> axum::Router {
        let mut cfg = test_config();
        cfg.wip_ai_api_key = Some(KEY.to_string());
        let state = crate::presentation::state::AppState::new(pool.clone(), cfg, None)
            .await
            .expect("AppState");
        crate::routes::create_router(state)
    }

    async fn call(
        app: &axum::Router,
        uri: &str,
        key: Option<&str>,
        body: Value,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json");
        if let Some(k) = key {
            req = req.header("X-AI-Api-Key", k);
        }
        let mut req = req.body(Body::from(body.to_string())).unwrap();
        // レート制限の層が接続元 IP を見るので、実サーバーと同じく ConnectInfo を渡す
        req.extensions_mut().insert(axum::extract::ConnectInfo(
            "127.0.0.1:40000".parse::<std::net::SocketAddr>().unwrap(),
        ));
        let resp = app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn ticket(pool: &PgPool) -> String {
        let user = create_test_user(pool, "ai-http").await;
        let project = create_test_project(pool, "AIH", user).await;
        for (slug, cat, pos) in [
            ("open", "unstarted", 1),
            ("in_progress", "started", 2),
            ("resolved", "completed", 3),
        ] {
            sqlx::query(
                "INSERT INTO t_workflow_status (slug, name, category, color, position, is_default, project_id)
                 VALUES ($1, $1, $2, '#000000', $3, false, $4)
                 ON CONFLICT (project_id, slug) WHERE project_id IS NOT NULL DO NOTHING",
            )
            .bind(slug).bind(cat).bind(pos).bind(i64::from(project))
            .execute(pool).await.unwrap();
        }
        let id = create_test_ticket(pool, project, "AIH", user).await;
        sqlx::query_scalar("SELECT ticket_key FROM tickets_ticket WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    /// PATCH(AI Agent API でチケットの状態を直接進める)。
    async fn patch(
        app: &axum::Router,
        key: &str,
        ticket: &str,
        body: Value,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method("PATCH")
            .uri(format!("/api/v1/ai-agent/tickets/{ticket}/"))
            .header("content-type", "application/json")
            .header("X-AI-Api-Key", key)
            .body(Body::from(body.to_string()))
            .unwrap();
        req.extensions_mut().insert(axum::extract::ConnectInfo(
            "127.0.0.1:40000".parse::<std::net::SocketAddr>().unwrap(),
        ));
        let resp = app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn start_date_of(pool: &PgPool, key: &str) -> Option<chrono::NaiveDate> {
        sqlx::query_scalar("SELECT start_date FROM tickets_ticket WHERE ticket_key = $1")
            .bind(key)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    /// AI が API で状態を「着手」に進めたとき、開始日が空なら入れる。人が入れた開始日・着手以外への変更では、触らない。
    #[tokio::test]
    async fn patching_status_to_started_sets_a_missing_start_date_only() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let app = app(&pool).await;

        // 着手に進める → 開始日が入る
        let a = ticket(&pool).await;
        assert_eq!(start_date_of(&pool, &a).await, None);
        let (st, _) = patch(&app, KEY, &a, json!({"status": "in_progress"})).await;
        assert_eq!(st, StatusCode::OK);
        let jst_today: chrono::NaiveDate =
            sqlx::query_scalar("SELECT (now() AT TIME ZONE 'Asia/Tokyo')::date")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(start_date_of(&pool, &a).await, Some(jst_today));

        // 人が入れた開始日は、変えない
        let b = ticket(&pool).await;
        sqlx::query("UPDATE tickets_ticket SET start_date = '2026-01-15' WHERE ticket_key = $1")
            .bind(&b)
            .execute(&pool)
            .await
            .unwrap();
        let (st, _) = patch(&app, KEY, &b, json!({"status": "in_progress"})).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(
            start_date_of(&pool, &b).await,
            chrono::NaiveDate::from_ymd_opt(2026, 1, 15)
        );

        // 着手以外(完了・タイトル変更)では、入れない
        let c = ticket(&pool).await;
        let (st, _) = patch(&app, KEY, &c, json!({"status": "resolved"})).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(
            start_date_of(&pool, &c).await,
            None,
            "完了への変更では入れない"
        );
        let d = ticket(&pool).await;
        let (st, _) = patch(&app, KEY, &d, json!({"title": "題名だけ変える"})).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(
            start_date_of(&pool, &d).await,
            None,
            "状態を変えないときは入れない"
        );
    }

    #[tokio::test]
    async fn requires_api_key() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let app = app(&pool).await;
        let u = Uuid::new_v4();
        let uri = format!("/api/v1/ai-agent/runs/{u}/start/");
        let (s, _) = call(&app, &uri, None, json!({"ticket_key": "X-1"})).await;
        assert!(
            s == StatusCode::UNAUTHORIZED || s == StatusCode::FORBIDDEN,
            "鍵なしは拒否: {s}"
        );
        let (s, _) = call(&app, &uri, Some("wrong"), json!({"ticket_key": "X-1"})).await;
        assert!(
            s == StatusCode::UNAUTHORIZED || s == StatusCode::FORBIDDEN,
            "誤った鍵は拒否: {s}"
        );
        for tail in ["finish", "error"] {
            let (s, _) = call(
                &app,
                &format!("/api/v1/ai-agent/runs/{u}/{tail}/"),
                None,
                json!({}),
            )
            .await;
            assert!(
                s == StatusCode::UNAUTHORIZED || s == StatusCode::FORBIDDEN,
                "{tail} も鍵なしは拒否: {s}"
            );
        }
    }

    #[tokio::test]
    async fn start_finish_flow_over_http() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let app = app(&pool).await;
        let key = ticket(&pool).await;
        let u = Uuid::new_v4();

        let (s, b) = call(
            &app,
            &format!("/api/v1/ai-agent/runs/{u}/start/"),
            Some(KEY),
            json!({"ticket_key": key, "tool": "claude_code", "trigger_type": "webhook"}),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        assert_eq!(b["created"], true);
        assert_eq!(b["ticket_status"], "in_progress");

        // 日本語の長い要約(2000字ちょうど)は通る(バイト数ではなく文字数で数える)
        let summary: String = "あ".repeat(2000);
        let (s, b) = call(
            &app,
            &format!("/api/v1/ai-agent/runs/{u}/finish/"),
            Some(KEY),
            json!({"result": "resolved", "summary": summary}),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        assert_eq!(b["ticket_status"], "resolved");

        // 2001字は 400
        let (s, _) = call(
            &app,
            &format!("/api/v1/ai-agent/runs/{u}/finish/"),
            Some(KEY),
            json!({"result": "resolved", "summary": "あ".repeat(2001)}),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);

        // 不正な result は 400、不正な UUID は 400 系
        let (s, _) = call(
            &app,
            &format!("/api/v1/ai-agent/runs/{u}/finish/"),
            Some(KEY),
            json!({"result": "closed"}),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        let (s, _) = call(
            &app,
            "/api/v1/ai-agent/runs/not-a-uuid/start/",
            Some(KEY),
            json!({"ticket_key": key}),
        )
        .await;
        assert!(s.is_client_error());
    }

    #[tokio::test]
    async fn error_endpoint_checks_ticket_access_and_mismatch() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let app = app(&pool).await;
        let key1 = ticket(&pool).await;
        let key2 = ticket(&pool).await;
        let u = Uuid::new_v4();
        let (s, _) = call(
            &app,
            &format!("/api/v1/ai-agent/runs/{u}/start/"),
            Some(KEY),
            json!({"ticket_key": key1}),
        )
        .await;
        assert_eq!(s, StatusCode::OK);

        // 既存の run を、別のチケットとして報告しようとすると 409
        let (s, _) = call(
            &app,
            &format!("/api/v1/ai-agent/runs/{u}/error/"),
            Some(KEY),
            json!({"detail": "x", "ticket_key": key2}),
        )
        .await;
        assert_eq!(s, StatusCode::CONFLICT);

        // 存在しない run を ticket_key 無しで報告すると 404
        let (s, _) = call(
            &app,
            &format!("/api/v1/ai-agent/runs/{}/error/", Uuid::new_v4()),
            Some(KEY),
            json!({"detail": "x"}),
        )
        .await;
        assert_eq!(s, StatusCode::NOT_FOUND);

        // 日本語 2000 字ちょうどの detail は通る。空・2001 字は 400
        let (s, b) = call(
            &app,
            &format!("/api/v1/ai-agent/runs/{u}/error/"),
            Some(KEY),
            json!({"detail": "あ".repeat(2000)}),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        let (s, _) = call(
            &app,
            &format!("/api/v1/ai-agent/runs/{u}/error/"),
            Some(KEY),
            json!({"detail": ""}),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        let (s, _) = call(
            &app,
            &format!("/api/v1/ai-agent/runs/{u}/error/"),
            Some(KEY),
            json!({"detail": "あ".repeat(2001)}),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
    }
}
