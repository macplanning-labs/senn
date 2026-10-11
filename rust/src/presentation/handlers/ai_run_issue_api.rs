/// presentation/handlers/ai_run_issue_api.rs — チケット詳細での AI 実行準備
///
/// ユーザーが「AI 用プロンプトをコピー」を押したとき、`t_ai_run` 行を事前に作って
/// `run_uuid` を発行し、SENN への作業記録手順ブロックを返す API。
/// フロントがこの API を呼び、コピーするプロンプトの末尾に手順ブロックを付ける。
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    domain::access::{Action, Viewer},
    infrastructure::repositories::ai_run_repo,
    presentation::{extractors::authorize, state::AppState},
};

/// リクエストボディ。tool を指定可能(省略時 claude_code)。
#[derive(Deserialize)]
pub struct IssueRunRequest {
    tool: Option<String>,
}

/// レスポンス。run_uuid・手順ブロック・再利用フラグを返す。
#[derive(Serialize)]
pub struct IssueRunResponse {
    #[serde(rename = "runUuid")]
    run_uuid: String,
    #[serde(rename = "instructionBlock")]
    instruction_block: String,
    reused: bool,
}

/// POST /api/v1/tickets/{ticket_key}/ai-run/ ハンドラ。
pub async fn issue_ai_run(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(ticket_key): Path<String>,
    body: Option<Json<IssueRunRequest>>,
) -> impl IntoResponse {
    // 本文は任意(省略時は claude_code)
    let body = body
        .map(|Json(b)| b)
        .unwrap_or(IssueRunRequest { tool: None });
    // JWT 認証
    let auth_user = match viewer.require_user_id() {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    // tool パラメータ検証と取得(デフォルト claude_code)
    let tool = {
        let t = body.tool.as_deref().unwrap_or("claude_code");
        match t {
            "claude_code" | "cursor" => t.to_string(),
            _ => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"detail": "tool は claude_code または cursor のみ"})),
                )
                    .into_response();
            }
        }
    };

    // ticket_key から ticket_id を解決
    let ticket_id: i64 =
        match sqlx::query_scalar("SELECT id::int8 FROM tickets_ticket WHERE ticket_key = $1")
            .bind(&ticket_key)
            .fetch_optional(&state.pool)
            .await
        {
            Ok(Some(id)) => id,
            Ok(None) => {
                tracing::warn!("Ticket not found: {}", ticket_key);
                return (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({"detail": "見つかりません"})),
                )
                    .into_response();
            }
            Err(e) => {
                tracing::error!("ticket lookup failed: {:?}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"detail": "データベースエラー"})),
                )
                    .into_response();
            }
        };

    // Write 権限チェック
    if let Err(resp) = authorize::authorize_ticket(
        &state,
        &viewer,
        ticket_id as i32,
        Action::Write,
        "POST /api/v1/tickets/{ticket_key}/ai-run/",
    )
    .await
    {
        return resp;
    }

    // 再利用可能な未開始実行を検索(120 分以内)
    let reusable =
        match ai_run_repo::find_reusable_unstarted(&state.pool, ticket_id, auth_user as i64, 120)
            .await
        {
            Ok(run) => run,
            Err(e) => {
                tracing::error!("find_reusable_unstarted failed: {:?}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"detail": "データベースエラー"})),
                )
                    .into_response();
            }
        };

    // 再利用するか新規作成するか判定
    let (run_uuid, reused) = match reusable {
        Some(run) => {
            // tool を更新する必要がある場合は UPDATE する
            if run.tool != tool {
                if let Err(e) = sqlx::query("UPDATE t_ai_run SET tool = $1 WHERE id = $2")
                    .bind(&tool)
                    .bind(run.id)
                    .execute(&state.pool)
                    .await
                {
                    tracing::error!("Failed to update tool: {:?}", e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"detail": "データベースエラー"})),
                    )
                        .into_response();
                }
            }
            (run.run_uuid, true)
        }
        None => {
            // 新規作成
            let new_uuid = Uuid::new_v4();
            let input = ai_run_repo::InsertInput {
                run_uuid: new_uuid,
                ticket_id,
                trigger_type: Some("manual_copy".to_string()),
                tool: Some(tool.clone()),
                model: None,
                triggered_by_id: Some(auth_user as i64),
            };

            match ai_run_repo::insert(&state.pool, &input).await {
                Ok(_) => (new_uuid, false),
                Err(e) => {
                    tracing::error!("ai_run_repo::insert failed: {:?}", e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"detail": "データベースエラー"})),
                    )
                        .into_response();
                }
            }
        }
    };

    // 手順ブロック生成
    let instruction_block = crate::domain::services::ai_run_service::build_instruction_block(
        &ticket_key,
        &run_uuid,
        &tool,
    );

    (
        StatusCode::OK,
        Json(IssueRunResponse {
            run_uuid: run_uuid.to_string(),
            instruction_block,
            reused,
        }),
    )
        .into_response()
}

#[cfg(test)]
mod http_tests {
    //! ルーターを通した HTTP テスト(認証・権限・ルーティング・再利用)。JWT(Bearer)で認証する。
    use crate::test_support::{
        add_test_team_member, create_test_project, create_test_ticket, create_test_user,
        test_config, test_pool,
    };
    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use axum::Router;
    use serde_json::{json, Value};
    use sqlx::PgPool;
    use tower::ServiceExt;
    use uuid::Uuid;

    async fn app(pool: &PgPool) -> Router {
        let state = crate::presentation::state::AppState::new(pool.clone(), test_config(), None)
            .await
            .expect("AppState");
        crate::routes::create_router(state)
    }

    /// そのユーザーの JWT(アプリの設定と同じ署名鍵で発行する)。
    fn bearer(user_id: i32) -> String {
        let token = crate::domain::services::jwt_service::issue_access_token(
            user_id,
            &test_config().jwt_secret,
            60,
        )
        .expect("JWT の発行");
        format!("Bearer {token}")
    }

    async fn call(
        app: &Router,
        ticket_key: &str,
        auth: Option<String>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/tickets/{ticket_key}/ai-run/"));
        if let Some(a) = auth {
            req = req.header("Authorization", a);
        }
        let req = match body {
            Some(b) => req
                .header("content-type", "application/json")
                .body(Body::from(b.to_string())),
            None => req.body(Body::empty()),
        };
        let mut req = req.unwrap();
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

    /// 非公開のチームとプロジェクト・チケットを作り、`member` をそのチームの member にする。
    /// 戻り値: (チケットキー, ticket_id)
    async fn private_ticket(pool: &PgPool, member: i32) -> (String, i32) {
        let project = create_test_project(pool, "AIRH", member).await;
        let team: i32 = sqlx::query_scalar(
            "SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1",
        )
        .bind(project)
        .fetch_one(pool)
        .await
        .unwrap();
        sqlx::query("UPDATE m_team SET visibility = 'private' WHERE id = $1::int8")
            .bind(team)
            .execute(pool)
            .await
            .unwrap();
        add_test_team_member(pool, team, member, "member").await;
        let id = create_test_ticket(pool, project, "AIRH", member).await;
        let key: String = sqlx::query_scalar("SELECT ticket_key FROM tickets_ticket WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap();
        (key, id)
    }

    async fn run_rows(pool: &PgPool, ticket_id: i32) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM t_ai_run WHERE ticket_id = $1")
            .bind(ticket_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn requires_authentication() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let app = app(&pool).await;
        let (s, _) = call(&app, "X-1", None, Some(json!({}))).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "認証なしは拒否");
        let (s, _) = call(
            &app,
            "X-1",
            Some("Bearer not-a-token".into()),
            Some(json!({})),
        )
        .await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "不正なトークンは拒否");
    }

    /// 前提の確認: このテストの JWT が、本当に認証を通ること(権限のテストが「認証エラー」で
    /// 偶然通らないように、成功する呼び出しを必ず 1 つ持つ)。
    #[tokio::test]
    async fn issues_run_and_instruction_block_then_reuses_it() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = create_test_user(&pool, "ai-run-http").await;
        let (key, tid) = private_ticket(&pool, user).await;
        let app = app(&pool).await;

        let (s, b) = call(
            &app,
            &key,
            Some(bearer(user)),
            Some(json!({"tool": "claude_code"})),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        assert_eq!(b["reused"], false);
        let uuid = b["runUuid"].as_str().unwrap().to_string();
        let block = b["instructionBlock"].as_str().unwrap();
        assert!(
            block.contains(&uuid) && block.contains(&key),
            "手順に実行IDとチケットキーが入る"
        );
        assert!(block.contains("senn_ai_run_start") && block.contains("senn_ai_run_finish"));

        // 行ができている: 人が起動(manual_copy)・起動した人が記録される・未着手
        let (trig, by, started): (String, Option<i64>, bool) = sqlx::query_as(
            "SELECT trigger_type, triggered_by_id, started_at IS NOT NULL FROM t_ai_run WHERE run_uuid = $1::uuid",
        )
        .bind(&uuid)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(trig, "manual_copy");
        assert_eq!(by, Some(i64::from(user)));
        assert!(!started);

        // すぐもう一度: 同じ実行を再利用する(コピーのたびに実行が増えない)
        let (s, b2) = call(&app, &key, Some(bearer(user)), None).await; // 本文なしでも良い
        assert_eq!(s, StatusCode::OK, "{b2}");
        assert_eq!(b2["runUuid"], uuid.as_str());
        assert_eq!(b2["reused"], true);
        assert_eq!(run_rows(&pool, tid).await, 1);

        // ツールを変えると同じ実行のツールが更新される
        let (s, b3) = call(
            &app,
            &key,
            Some(bearer(user)),
            Some(json!({"tool": "cursor"})),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(b3["runUuid"], uuid.as_str());
        assert!(b3["instructionBlock"].as_str().unwrap().contains("cursor"));
        let tool: String =
            sqlx::query_scalar("SELECT tool FROM t_ai_run WHERE run_uuid = $1::uuid")
                .bind(&uuid)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(tool, "cursor");

        // AI が start したあとは、再利用せず新しい実行を作る
        crate::domain::services::ai_run_service::start_run(
            &pool,
            user,
            Uuid::parse_str(&uuid).unwrap(),
            crate::domain::services::ai_run_service::StartInput {
                ticket_key: key.clone(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let (s, b4) = call(&app, &key, Some(bearer(user)), None).await;
        assert_eq!(s, StatusCode::OK);
        assert_ne!(b4["runUuid"], uuid.as_str());
        assert_eq!(b4["reused"], false);
        assert_eq!(run_rows(&pool, tid).await, 2);

        // AI の start でも、プロンプトを作った人(triggered_by)は保たれる
        let by: Option<i64> =
            sqlx::query_scalar("SELECT triggered_by_id FROM t_ai_run WHERE run_uuid = $1::uuid")
                .bind(&uuid)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(by, Some(i64::from(user)));
    }

    #[tokio::test]
    async fn another_users_reuse_window_is_separate() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let a = create_test_user(&pool, "ai-run-http-a").await;
        let (key, tid) = private_ticket(&pool, a).await;
        let b = create_test_user(&pool, "ai-run-http-b").await;
        let team: i32 =
            sqlx::query_scalar("SELECT t.team_id::int4 FROM tickets_ticket t WHERE t.id = $1")
                .bind(tid)
                .fetch_one(&pool)
                .await
                .unwrap();
        add_test_team_member(&pool, team, b, "member").await;
        let app = app(&pool).await;

        let (_, ra) = call(&app, &key, Some(bearer(a)), None).await;
        let (s, rb) = call(&app, &key, Some(bearer(b)), None).await;
        assert_eq!(s, StatusCode::OK);
        assert_ne!(
            ra["runUuid"], rb["runUuid"],
            "別の人のコピーは別の実行(再利用しない)"
        );
        assert_eq!(rb["reused"], false);
    }

    #[tokio::test]
    async fn rejects_invalid_tool_and_unknown_ticket() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = create_test_user(&pool, "ai-run-http-v").await;
        let (key, tid) = private_ticket(&pool, user).await;
        let app = app(&pool).await;
        let (s, _) = call(&app, &key, Some(bearer(user)), Some(json!({"tool": "vim"}))).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert_eq!(run_rows(&pool, tid).await, 0, "不正な入力では行を作らない");
        let (s, _) = call(&app, "NOPE-999999", Some(bearer(user)), None).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }

    /// 見えない・書けないチケットには、実行を作れない(権限で拒否され、行も作られない)。
    #[tokio::test]
    async fn denies_users_without_access_and_creates_no_row() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let owner = create_test_user(&pool, "ai-run-http-o").await;
        let (key, tid) = private_ticket(&pool, owner).await;
        let outsider = create_test_user(&pool, "ai-run-http-x").await;
        let app = app(&pool).await;

        // 同じ JWT の作り方で、所属する人は成功する(認証エラーで偶然通っていないことの確認)
        let (s, _) = call(&app, &key, Some(bearer(owner)), None).await;
        assert_eq!(s, StatusCode::OK);
        let rows_before = run_rows(&pool, tid).await;

        let (s, _) = call(&app, &key, Some(bearer(outsider)), None).await;
        assert!(
            s == StatusCode::FORBIDDEN || s == StatusCode::NOT_FOUND,
            "所属しない人は 403 か 404(401 ではない): {s}"
        );
        assert_ne!(s, StatusCode::UNAUTHORIZED);
        assert_eq!(
            run_rows(&pool, tid).await,
            rows_before,
            "拒否されたときは行を作らない"
        );
    }
}
