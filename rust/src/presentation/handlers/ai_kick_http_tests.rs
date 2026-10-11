//! 「AIに対応させる」の HTTP テスト(認証・権限・ルーティング・入力検証)。
//! 上限・緊急停止・キューは全体で共有なので、サービスのテストと同じロックで直列にし、毎回まっさらにする。
#![allow(clippy::field_reassign_with_default)]
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use crate::domain::services::ai_kick_policy::Limits;
use crate::domain::services::ai_kick_service::{self, TEST_LOCK};
use crate::infrastructure::encrypted_settings;
use crate::infrastructure::repositories::ai_kick_repo;
use crate::test_support::{
    add_test_team_member, create_test_project, create_test_ticket, create_test_user, test_config,
    test_pool,
};

struct Hx {
    pool: PgPool,
    app: Router,
    member: i32,
    other_member: i32,
    outsider: i32,
    guest: i32,
    admin: i32,
    project: i32,
    team: i32,
    _guard: tokio::sync::MutexGuard<'static, ()>,
}

fn bearer(user_id: i32) -> String {
    let token = crate::domain::services::jwt_service::issue_access_token(
        user_id,
        &test_config().jwt_secret,
        60,
    )
    .expect("JWT の発行");
    format!("Bearer {token}")
}

async fn hx() -> Option<Hx> {
    // 失敗したときに原因が分かるよう、警告以上のログをテストの出力に出す
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .try_init();
    let pool = test_pool().await?;
    let guard = TEST_LOCK.lock().await;
    sqlx::query("DELETE FROM t_ai_run WHERE queued_at IS NOT NULL")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM system_settings WHERE key LIKE 'sys.ai.kick.%'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE t_ai_runner SET revoked_at = NOW() WHERE revoked_at IS NULL")
        .execute(&pool)
        .await
        .unwrap();
    let member = create_test_user(&pool, "kh-member").await;
    let other_member = create_test_user(&pool, "kh-member2").await;
    let outsider = create_test_user(&pool, "kh-outsider").await;
    let guest = create_test_user(&pool, "kh-guest").await;
    let admin = create_test_user(&pool, "kh-admin").await;
    sqlx::query("UPDATE accounts_user SET is_guest = true WHERE id = $1")
        .bind(guest)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE accounts_user SET is_system_admin = true WHERE id = $1")
        .bind(admin)
        .execute(&pool)
        .await
        .unwrap();
    let project = create_test_project(&pool, "KHT", member).await;
    let team: i32 =
        sqlx::query_scalar("SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1")
            .bind(project)
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query("UPDATE m_team SET visibility = 'private' WHERE id = $1::int8")
        .bind(team)
        .execute(&pool)
        .await
        .unwrap();
    add_test_team_member(&pool, team, member, "member").await;
    add_test_team_member(&pool, team, other_member, "member").await;
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
        .bind(slug)
        .bind(cat)
        .bind(pos)
        .bind(i64::from(project))
        .execute(&pool)
        .await
        .unwrap();
    }
    ai_kick_repo::set_project_kick(&pool, project, true, "org/repo", "develop")
        .await
        .unwrap();
    ai_kick_repo::set_allowed_repos(&pool, &["org/repo".to_string()])
        .await
        .unwrap();
    ai_kick_repo::set_limits(&pool, &Limits::default())
        .await
        .unwrap();
    ai_kick_repo::set_enabled(&pool, true).await.unwrap();
    let state = crate::presentation::state::AppState::new(pool.clone(), test_config(), None)
        .await
        .expect("AppState");
    let app = crate::routes::create_router(state);
    Some(Hx {
        pool,
        app,
        member,
        other_member,
        outsider,
        guest,
        admin,
        project,
        team,
        _guard: guard,
    })
}

impl Hx {
    async fn call(
        &self,
        method: &str,
        uri: &str,
        headers: &[(&str, String)],
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder().method(method).uri(uri);
        for (k, v) in headers {
            req = req.header(*k, v);
        }
        let req = match body {
            Some(b) => req
                .header("content-type", "application/json")
                .body(Body::from(b.to_string())),
            None => req.body(Body::empty()),
        };
        let mut req = req.unwrap();
        req.extensions_mut().insert(axum::extract::ConnectInfo(
            "127.0.0.1:40000".parse::<std::net::SocketAddr>().unwrap(),
        ));
        let resp = self.app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn as_user(
        &self,
        user: i32,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        self.call(method, uri, &[("Authorization", bearer(user))], body)
            .await
    }

    /// 優先度つきのチケット(要約済みのプロンプトも入れておく。生成(Ollama)を呼ばせないため)。
    async fn ticket(&self, priority: &str) -> (i32, String) {
        let id = create_test_ticket(&self.pool, self.project, "KHT", self.member).await;
        sqlx::query("UPDATE tickets_ticket SET priority = $2, ai_prompt = '要約済みのチケットの内容' WHERE id = $1")
            .bind(id)
            .bind(priority)
            .execute(&self.pool)
            .await
            .unwrap();
        let key: String = sqlx::query_scalar("SELECT ticket_key FROM tickets_ticket WHERE id = $1")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .unwrap();
        (id, key)
    }

    /// プレビューして、そのハッシュで依頼する(確認つき/なし)。
    async fn preview_then_kick(
        &self,
        user: i32,
        key: &str,
        confirmed: bool,
    ) -> (StatusCode, Value) {
        let (s, pv) = self
            .as_user(
                user,
                "POST",
                &format!("/api/v1/tickets/{key}/ai-kick/preview/"),
                None,
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{pv}");
        let hash = pv["promptHash"].as_str().unwrap().to_string();
        self.as_user(
            user,
            "POST",
            &format!("/api/v1/tickets/{key}/ai-kick/"),
            Some(json!({ "promptHash": hash, "confirmed": confirmed })),
        )
        .await
    }

    async fn runner(&self, name: &str) -> (i64, String, String) {
        let token = format!("srun_{}", URL_SAFE_NO_PAD.encode(rand_bytes()));
        let secret = URL_SAFE_NO_PAD.encode(rand_bytes());
        let enc = encrypted_settings::encrypt_value(&secret, &test_config().jwt_secret).unwrap();
        let r = ai_kick_repo::create_runner(
            &self.pool,
            &format!("{name}-{}", Uuid::new_v4().simple()),
            &ai_kick_repo::hash_token(&token),
            &token[..12],
            &enc,
            i64::from(self.admin),
        )
        .await
        .unwrap();
        (r.id, token, secret)
    }

    async fn runner_call(
        &self,
        token: &str,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        self.call(
            "POST",
            uri,
            &[("X-Runner-Token", token.to_string())],
            Some(body.unwrap_or(json!({}))),
        )
        .await
    }
}

fn rand_bytes() -> [u8; 32] {
    use rand::RngCore;
    let mut b = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut b);
    b
}

// ================= 人が使う側 =================

#[tokio::test]
async fn kick_endpoints_require_login_and_reject_ai_keys_and_runner_tokens() {
    let Some(h) = hx().await else { return };
    let (_, key) = h.ticket("medium").await;
    let uuid = Uuid::new_v4();
    for (method, uri, body) in [
        (
            "POST",
            format!("/api/v1/tickets/{key}/ai-kick/preview/"),
            Some(json!({})),
        ),
        (
            "POST",
            format!("/api/v1/tickets/{key}/ai-kick/"),
            Some(json!({"promptHash": "x"})),
        ),
        (
            "POST",
            format!("/api/v1/tickets/{key}/ai-kick/{uuid}/cancel/"),
            Some(json!({})),
        ),
        ("GET", format!("/api/v1/tickets/{key}/ai-runs/"), None),
        (
            "GET",
            format!("/api/v1/projects/{}/ai-kick/", h.project),
            None,
        ),
        (
            "PUT",
            format!("/api/v1/projects/{}/ai-kick/", h.project),
            Some(json!({"enabled": false})),
        ),
    ] {
        let (s, _) = h.call(method, &uri, &[], body.clone()).await;
        assert_eq!(
            s,
            StatusCode::UNAUTHORIZED,
            "ログインなしは拒否: {method} {uri}"
        );
        // AI のキーや、ランナーのトークンだけでは、人が使う入口は通らない(AI が AI を呼ぶ連鎖を作れない)
        let (s, _) = h
            .call(
                method,
                &uri,
                &[
                    ("X-AI-Api-Key", "senn_ai_anything".into()),
                    ("X-Runner-Token", "srun_x".into()),
                ],
                body,
            )
            .await;
        assert_eq!(
            s,
            StatusCode::UNAUTHORIZED,
            "キー・トークンだけでは拒否: {method} {uri}"
        );
    }
}

#[tokio::test]
async fn member_can_preview_and_kick_and_the_run_is_queued_for_the_runner() {
    let Some(h) = hx().await else { return };
    let (tid, key) = h.ticket("medium").await;
    let (s, pv) = h
        .as_user(
            h.member,
            "POST",
            &format!("/api/v1/tickets/{key}/ai-kick/preview/"),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{pv}");
    assert_eq!(pv["verdict"], "allow");
    assert_eq!(pv["priorityClass"], "normal");
    assert_eq!(pv["repo"], "org/repo");
    assert_eq!(pv["caps"]["maxWeightedTokens"], 30_000_000);
    let prompt = pv["prompt"].as_str().unwrap();
    assert!(
        prompt.contains("要約済みのチケットの内容") && prompt.contains("<untrusted-ticket-data>")
    );
    // プレビューでは、依頼は積まれない
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM t_ai_run WHERE ticket_id = $1 AND queued_at IS NOT NULL",
    )
    .bind(tid)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(n, 0);

    let (s, r) = h.preview_then_kick(h.member, &key, false).await;
    assert_eq!(s, StatusCode::ACCEPTED, "{r}");
    assert_eq!(r["status"], "queued");
    let (trig, by, hash): (String, Option<i64>, Option<String>) = sqlx::query_as(
        "SELECT trigger_type, triggered_by_id, kick_prompt_hash FROM t_ai_run WHERE run_uuid = $1::uuid",
    )
    .bind(r["runUuid"].as_str().unwrap())
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!((trig.as_str(), by), ("webhook", Some(i64::from(h.member))));
    assert_eq!(
        hash.unwrap(),
        pv["promptHash"].as_str().unwrap(),
        "確認した内容のハッシュで依頼されている"
    );
    // 同じチケットは、もう起動できない(二重起動の防止)
    let (s, r) = h.preview_then_kick(h.member, &key, false).await;
    assert_eq!(s, StatusCode::CONFLICT, "{r}");
    assert_eq!(r["code"], "ticket_busy");
}

#[tokio::test]
async fn kick_is_refused_if_the_ticket_changed_after_the_preview() {
    let Some(h) = hx().await else { return };
    let (tid, key) = h.ticket("medium").await;
    let (_, pv) = h
        .as_user(
            h.member,
            "POST",
            &format!("/api/v1/tickets/{key}/ai-kick/preview/"),
            None,
        )
        .await;
    let hash = pv["promptHash"].as_str().unwrap();
    sqlx::query(
        "UPDATE tickets_ticket SET ai_prompt = '確認したあとに書き換えられた内容' WHERE id = $1",
    )
    .bind(tid)
    .execute(&h.pool)
    .await
    .unwrap();
    let (s, r) = h
        .as_user(
            h.member,
            "POST",
            &format!("/api/v1/tickets/{key}/ai-kick/"),
            Some(json!({ "promptHash": hash, "confirmed": true })),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{r}");
    assert_eq!(r["code"], "prompt_changed");
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM t_ai_run WHERE ticket_id = $1 AND queued_at IS NOT NULL",
    )
    .bind(tid)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(n, 0, "確認していない内容は、送られない");
    // ハッシュが無い・空の要求も、起動できない
    let (s, _) = h
        .as_user(
            h.member,
            "POST",
            &format!("/api/v1/tickets/{key}/ai-kick/"),
            Some(json!({"confirmed": true})),
        )
        .await;
    assert!(s.is_client_error() && s != StatusCode::ACCEPTED);
    let (s, _) = h
        .as_user(
            h.member,
            "POST",
            &format!("/api/v1/tickets/{key}/ai-kick/"),
            Some(json!({"promptHash": "", "confirmed": true})),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
}

#[tokio::test]
async fn emergency_requires_confirmation_and_normal_priority_does_not() {
    let Some(h) = hx().await else { return };
    let (_, urgent) = h.ticket("urgent").await;
    let (_, pv) = h
        .as_user(
            h.member,
            "POST",
            &format!("/api/v1/tickets/{urgent}/ai-kick/preview/"),
            None,
        )
        .await;
    assert_eq!(pv["verdict"], "needs_confirm");
    assert_eq!(pv["priorityClass"], "emergency");
    assert_eq!(pv["priorityLabel"], "緊急");
    assert_eq!(
        pv["caps"]["maxWeightedTokens"], 60_000_000,
        "緊急は上限が 2 倍"
    );
    let (s, r) = h.preview_then_kick(h.member, &urgent, false).await;
    assert_eq!(s, StatusCode::PRECONDITION_REQUIRED, "{r}");
    assert_eq!(r["code"], "needs_confirm");
    assert!(r["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|w| w["code"] == "emergency"));
    let (s, r) = h.preview_then_kick(h.member, &urgent, true).await;
    assert_eq!(s, StatusCode::ACCEPTED, "{r}");
    assert_eq!(r["priorityClass"], "emergency");
}

#[tokio::test]
async fn denials_are_explained_with_a_code() {
    let Some(h) = hx().await else { return };
    let (_, key) = h.ticket("medium").await;
    ai_kick_repo::set_killed(&h.pool, true).await.unwrap();
    let (s, r) = h.preview_then_kick(h.member, &key, true).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert_eq!(r["code"], "killed");
    assert!(r["detail"].as_str().unwrap().contains("緊急停止"));
    ai_kick_repo::set_killed(&h.pool, false).await.unwrap();
    ai_kick_repo::set_project_kick(&h.pool, h.project, false, "org/repo", "develop")
        .await
        .unwrap();
    let (s, r) = h.preview_then_kick(h.member, &key, true).await;
    assert_eq!(
        (s, r["code"].as_str()),
        (StatusCode::FORBIDDEN, Some("project_disabled"))
    );
    // プレビューも、拒否の理由を返す(押す前に分かる)
    let (s, pv) = h
        .as_user(
            h.member,
            "POST",
            &format!("/api/v1/tickets/{key}/ai-kick/preview/"),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(pv["verdict"], "deny");
    assert_eq!(pv["denied"]["code"], "project_disabled");
}

#[tokio::test]
async fn guests_outsiders_and_ai_account_cannot_kick() {
    let Some(h) = hx().await else { return };
    let (tid, key) = h.ticket("medium").await;
    for (user, label) in [(h.guest, "ゲスト"), (h.outsider, "所属しない人")] {
        let (s, r) = h
            .as_user(
                user,
                "POST",
                &format!("/api/v1/tickets/{key}/ai-kick/preview/"),
                None,
            )
            .await;
        assert!(
            s == StatusCode::FORBIDDEN || s == StatusCode::NOT_FOUND,
            "{label}は拒否(401 ではない): {s} {r}"
        );
        assert_ne!(s, StatusCode::UNAUTHORIZED, "{label}: 認証は通っている");
        let (s, _) = h
            .as_user(
                user,
                "POST",
                &format!("/api/v1/tickets/{key}/ai-kick/"),
                Some(json!({"promptHash": "x", "confirmed": true})),
            )
            .await;
        assert!(
            s == StatusCode::FORBIDDEN || s == StatusCode::NOT_FOUND || s == StatusCode::CONFLICT,
            "{label}: {s}"
        );
        assert_ne!(s, StatusCode::ACCEPTED);
    }
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM t_ai_run WHERE ticket_id = $1 AND queued_at IS NOT NULL",
    )
    .bind(tid)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(n, 0, "拒否されたときは、依頼を積まない");
    // 共有の AI アカウント自身が(仮に JWT を持っても)起動できない
    let ai_user = crate::presentation::handlers::ai_agent_api::resolve_shared_ai_user(
        &crate::presentation::state::AppState::new(h.pool.clone(), test_config(), None)
            .await
            .unwrap(),
    )
    .await
    .unwrap();
    add_test_team_member(&h.pool, h.team, ai_user, "member").await;
    let (s, r) = h
        .as_user(
            ai_user,
            "POST",
            &format!("/api/v1/tickets/{key}/ai-kick/preview/"),
            None,
        )
        .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "AI のアカウントからは起動できない: {r}"
    );
}

#[tokio::test]
async fn only_the_triggerer_or_an_admin_can_cancel() {
    let Some(h) = hx().await else { return };
    let (_, key) = h.ticket("medium").await;
    let (s, r) = h.preview_then_kick(h.member, &key, false).await;
    assert_eq!(s, StatusCode::ACCEPTED);
    let uuid = r["runUuid"].as_str().unwrap().to_string();
    let uri = format!("/api/v1/tickets/{key}/ai-kick/{uuid}/cancel/");
    // 同じチームの別のメンバーは、取り消せない
    let (s, _) = h
        .as_user(h.other_member, "POST", &uri, Some(json!({})))
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let result: Option<String> =
        sqlx::query_scalar("SELECT result FROM t_ai_run WHERE run_uuid = $1::uuid")
            .bind(&uuid)
            .fetch_one(&h.pool)
            .await
            .unwrap();
    assert_eq!(result, None, "他の人が取り消しても、実行は止まらない");
    // 起動した人は取り消せる(取得前なので、そのまま終わる)
    let (s, r) = h.as_user(h.member, "POST", &uri, Some(json!({}))).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    let result: Option<String> =
        sqlx::query_scalar("SELECT result FROM t_ai_run WHERE run_uuid = $1::uuid")
            .bind(&uuid)
            .fetch_one(&h.pool)
            .await
            .unwrap();
    assert_eq!(result.as_deref(), Some("aborted"));
}

#[tokio::test]
async fn runs_list_shows_status_and_hides_unused_prepared_runs() {
    let Some(h) = hx().await else { return };
    let (tid, key) = h.ticket("medium").await;
    // 手動コピー用に準備されただけの実行(使われていない)は出さない
    sqlx::query("INSERT INTO t_ai_run (run_uuid, ticket_id, trigger_type) VALUES (gen_random_uuid(), $1, 'manual_copy')")
        .bind(i64::from(tid))
        .execute(&h.pool)
        .await
        .unwrap();
    let (s, _) = h.preview_then_kick(h.member, &key, false).await;
    assert_eq!(s, StatusCode::ACCEPTED);
    let (s, body) = h
        .as_user(
            h.other_member,
            "GET",
            &format!("/api/v1/tickets/{key}/ai-runs/"),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    let runs = body["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["status"], "queued");
    assert_eq!(runs[0]["priorityClass"], "normal");
    // 所属しない人には見えない
    let (s, _) = h
        .as_user(
            h.outsider,
            "GET",
            &format!("/api/v1/tickets/{key}/ai-runs/"),
            None,
        )
        .await;
    assert!(s == StatusCode::FORBIDDEN || s == StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn project_settings_can_be_changed_only_by_owner_or_admin_and_are_validated() {
    let Some(h) = hx().await else { return };
    let uri = format!("/api/v1/projects/{}/ai-kick/", h.project);
    let (s, r) = h.as_user(h.member, "GET", &uri, None).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["enabled"], true);
    assert_eq!(r["canEdit"], false, "一般のメンバーは変更できない");
    assert_eq!(r["systemEnabled"], true);
    // 一般のメンバーは変更できない
    let (s, _) = h
        .as_user(
            h.member,
            "PUT",
            &uri,
            Some(json!({"enabled": false, "repo": "org/repo"})),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let p = ai_kick_repo::get_project_kick(&h.pool, h.project)
        .await
        .unwrap()
        .unwrap();
    assert!(p.ai_kick_enabled, "変更されていない");
    // 管理者は変更できる(ただし入力は検証される)
    for bad in [
        json!({"enabled": true, "repo": "not-a-repo"}),
        json!({"enabled": true, "repo": "a/b/c"}),
        json!({"enabled": true, "repo": "org/repo", "baseBranch": "a..b"}),
        json!({"enabled": true, "repo": "org/repo", "baseBranch": "-x"}),
        json!({"enabled": true, "repo": ""}),
    ] {
        let (s, r) = h.as_user(h.admin, "PUT", &uri, Some(bad.clone())).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad} → {r}");
    }
    // 管理者が許可していないリポジトリは選べない(ほかのチームのリポジトリを指定させない)
    let (s, r) = h
        .as_user(
            h.admin,
            "PUT",
            &uri,
            Some(json!({"enabled": true, "repo": "my-org/my-repo", "baseBranch": "release/1", "dataSharingAcknowledged": true})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{r}");
    assert!(r["detail"].as_str().unwrap_or("").contains("許可"), "{r}");
    // 許可されたリポジトリでも、Anthropic に送られることの確認が無ければ、有効にできない
    let (s, r) = h
        .as_user(
            h.admin,
            "PUT",
            &uri,
            Some(json!({"enabled": true, "repo": "org/repo", "baseBranch": "release/1"})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{r}");
    assert!(
        r["detail"].as_str().unwrap_or("").contains("Anthropic"),
        "{r}"
    );
    // 許可されたリポジトリで、確認があれば有効にできる
    let (s, r) = h
        .as_user(
            h.admin,
            "PUT",
            &uri,
            Some(json!({"enabled": true, "repo": "org/repo", "baseBranch": "release/1", "dataSharingAcknowledged": true})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    let (_, r) = h.as_user(h.member, "GET", &uri, None).await;
    assert_eq!(r["allowedRepos"], json!(["org/repo"]), "{r}");
    // 無効にするときは、確認は要らない
    let (s, r) = h
        .as_user(
            h.admin,
            "PUT",
            &uri,
            Some(json!({"enabled": false, "repo": "org/repo"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    ai_kick_repo::set_project_kick(&h.pool, h.project, true, "org/repo", "develop")
        .await
        .unwrap();
}

#[tokio::test]
async fn admin_sets_allowed_repos_with_validation() {
    let Some(h) = hx().await else { return };
    let uri = "/api/v1/system-admin/ai-kick/";
    let (s, _) = h
        .as_user(
            h.member,
            "PUT",
            uri,
            Some(json!({"allowedRepos": ["org/repo"]})),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "一般のメンバーは変更できない");
    let (s, r) = h
        .as_user(
            h.admin,
            "PUT",
            uri,
            Some(json!({"allowedRepos": ["org/repo", "bad"]})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{r}");
    let (s, r) = h
        .as_user(
            h.admin,
            "PUT",
            uri,
            Some(json!({"allowedRepos": [" org/repo ", "ORG/REPO", "", "team-b/app"]})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(
        r["allowedRepos"],
        json!(["org/repo", "team-b/app"]),
        "空白・重複・空を除く"
    );
    ai_kick_repo::set_allowed_repos(&h.pool, &["org/repo".to_string()])
        .await
        .unwrap();
}

// ================= ランナー側 =================

#[tokio::test]
async fn runner_endpoints_reject_missing_malformed_unknown_and_revoked_tokens() {
    let Some(h) = hx().await else { return };
    let uuid = Uuid::new_v4();
    let uris = [
        "/api/v1/ai-agent/runner/claim/".to_string(),
        format!("/api/v1/ai-agent/runner/runs/{uuid}/start/"),
        format!("/api/v1/ai-agent/runner/runs/{uuid}/heartbeat/"),
        format!("/api/v1/ai-agent/runner/runs/{uuid}/finish/"),
        format!("/api/v1/ai-agent/runner/runs/{uuid}/error/"),
    ];
    let (rid, good, _) = h.runner("auth").await;
    for uri in &uris {
        let (s, _) = h.call("POST", uri, &[], Some(json!({}))).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "トークンなし: {uri}");
        for bad in [
            "",
            "wrong",
            "srun_short",
            "Bearer srun_x",
            &format!("{good}x"),
            "senn_ai_key",
        ] {
            let (s, _) = h
                .call(
                    "POST",
                    uri,
                    &[("X-Runner-Token", bad.to_string())],
                    Some(json!({})),
                )
                .await;
            assert_eq!(s, StatusCode::UNAUTHORIZED, "不正なトークン {bad:?}: {uri}");
        }
        // 人の JWT・AI キーでは、ランナーの入口は通らない
        let (s, _) = h
            .call(
                "POST",
                uri,
                &[("Authorization", bearer(h.admin))],
                Some(json!({})),
            )
            .await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "JWT では通らない: {uri}");
    }
    // 有効なトークンは通る(存在しない実行は 404、claim は 200)
    let (s, _) = h.runner_call(&good, &uris[0], None).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = h.runner_call(&good, &uris[2], None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // 失効させると、通らなくなる
    ai_kick_repo::revoke_runner(&h.pool, rid).await.unwrap();
    let (s, _) = h.runner_call(&good, &uris[0], None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "失効したトークンは拒否");
}

#[tokio::test]
async fn runner_full_flow_claim_start_heartbeat_finish_and_isolation() {
    let Some(h) = hx().await else { return };
    let (tid, key) = h.ticket("medium").await;
    let (s, r) = h.preview_then_kick(h.member, &key, false).await;
    assert_eq!(s, StatusCode::ACCEPTED, "{r}");
    let uuid = r["runUuid"].as_str().unwrap().to_string();
    let (_, token, secret) = h.runner("flow").await;
    let (_, other_token, _) = h.runner("other").await;

    // 待機中(accepting=false)は、依頼を取らない
    let (s, body) = h
        .runner_call(
            &token,
            "/api/v1/ai-agent/runner/claim/",
            Some(json!({"accepting": false})),
        )
        .await;
    assert_eq!((s, body["job"].is_null()), (StatusCode::OK, true));
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM t_ai_run WHERE run_uuid = $1::uuid AND claimed_at IS NOT NULL",
    )
    .bind(&uuid)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(n, 0);

    // 取得
    let (s, body) = h
        .runner_call(&token, "/api/v1/ai-agent/runner/claim/", None)
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    let job = &body["job"];
    assert_eq!(job["runUuid"], uuid.as_str());
    assert_eq!(job["ticketKey"], key.as_str());
    assert_eq!(job["repo"], "org/repo");
    assert!(job["prompt"]
        .as_str()
        .unwrap()
        .contains("<untrusted-ticket-data>"));
    // 署名が検証できる(ランナーの秘密で)。他の鍵では検証できない
    let caps: crate::domain::services::ai_kick_policy::Caps =
        serde_json::from_value(job["caps"].clone()).unwrap();
    let msg = ai_kick_service::signing_string(
        &Uuid::parse_str(&uuid).unwrap(),
        &key,
        job["promptHash"].as_str().unwrap(),
        "org/repo",
        "develop",
        &caps,
    );
    assert_eq!(
        job["signature"],
        ai_kick_service::sign(&secret, &msg).as_str()
    );
    assert_ne!(
        job["signature"],
        ai_kick_service::sign("wrong", &msg).as_str()
    );

    // 他のランナーは、この実行に触れない(存在しないのと同じ 404)
    for tail in ["start", "heartbeat", "finish", "error"] {
        let (s, _) = h
            .runner_call(
                &other_token,
                &format!("/api/v1/ai-agent/runner/runs/{uuid}/{tail}/"),
                Some(json!({"result": "resolved", "detail": "x"})),
            )
            .await;
        assert_eq!(
            s,
            StatusCode::NOT_FOUND,
            "{tail}: 他のランナーの実行は見えない"
        );
    }
    // 別のランナーが claim しても、取得済みのものは渡されない
    let (_, body) = h
        .runner_call(&other_token, "/api/v1/ai-agent/runner/claim/", None)
        .await;
    assert!(body["job"].is_null());

    // 着手 → チケットが進行中
    let (s, b) = h
        .runner_call(
            &token,
            &format!("/api/v1/ai-agent/runner/runs/{uuid}/start/"),
            Some(json!({"model": "claude-sonnet"})),
        )
        .await;
    assert_eq!(
        (s, b["stop"].clone()),
        (StatusCode::OK, json!(false)),
        "{b}"
    );
    let status: String = sqlx::query_scalar("SELECT status FROM tickets_ticket WHERE id = $1")
        .bind(tid)
        .fetch_one(&h.pool)
        .await
        .unwrap();
    assert_eq!(status, "in_progress");

    // ハートビート: 続けてよい
    let hb = format!("/api/v1/ai-agent/runner/runs/{uuid}/heartbeat/");
    let (s, b) = h
        .runner_call(&token, &hb, Some(json!({"inputTokens": 1000, "outputTokens": 500, "cacheCreationTokens": 0, "cacheReadTokens": 10000})))
        .await;
    assert_eq!(
        (s, b["stop"].clone()),
        (StatusCode::OK, json!(false)),
        "{b}"
    );
    // 異常な値(負・巨大)でも、安全に扱われる(使用量は減らない・上限で丸める)
    let (s, _) = h
        .runner_call(
            &token,
            &hb,
            Some(json!({"inputTokens": -5, "outputTokens": -5})),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let weighted: i64 =
        sqlx::query_scalar("SELECT weighted_tokens FROM t_ai_run WHERE run_uuid = $1::uuid")
            .bind(&uuid)
            .fetch_one(&h.pool)
            .await
            .unwrap();
    assert_eq!(weighted, 2500, "1000+500+10000/10。負の値で減らない");
    // 1 実行の上限を超えたら、止める指示が返る
    let (s, b) = h
        .runner_call(
            &token,
            &hb,
            Some(json!({"inputTokens": 999_999_999_999_i64})),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["stop"], true);
    assert_eq!(b["reason"], "per_run_tokens");

    // 完了(止められたあとの報告): 記録される。AI は resolved まで
    let (s, b) = h
        .runner_call(
            &token,
            &format!("/api/v1/ai-agent/runner/runs/{uuid}/finish/"),
            Some(json!({"result": "aborted", "summary": "上限で止まりました", "inputTokens": 3000, "outputTokens": 100})),
        )
        .await;
    assert_eq!(
        (s, b["recorded"].clone()),
        (StatusCode::OK, json!(true)),
        "{b}"
    );
    let (result, w): (Option<String>, i64) =
        sqlx::query_as("SELECT result, weighted_tokens FROM t_ai_run WHERE run_uuid = $1::uuid")
            .bind(&uuid)
            .fetch_one(&h.pool)
            .await
            .unwrap();
    assert_eq!(result.as_deref(), Some("aborted"));
    // 使用量は減らない: ハートビートで記録した最大値(上限で丸めた 10,000,000,000)を、完了の報告が下回っても保つ
    assert_eq!(w, 10_000_000_000);
    // 終わった実行へのハートビートは、止める指示を返す
    let (_, b) = h.runner_call(&token, &hb, Some(json!({}))).await;
    assert_eq!(b["stop"], true);
    // 入力の検証: 長すぎる要約・空のエラーは 400
    let (s, _) = h
        .runner_call(
            &token,
            &format!("/api/v1/ai-agent/runner/runs/{uuid}/finish/"),
            Some(json!({"result": "resolved", "summary": "あ".repeat(2001)})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = h
        .runner_call(
            &token,
            &format!("/api/v1/ai-agent/runner/runs/{uuid}/error/"),
            Some(json!({"detail": ""})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

// ================= 管理者側 =================

#[tokio::test]
async fn admin_endpoints_are_for_admins_only() {
    let Some(h) = hx().await else { return };
    for (method, uri, body) in [
        ("GET", "/api/v1/system-admin/ai-kick/".to_string(), None),
        (
            "PUT",
            "/api/v1/system-admin/ai-kick/".to_string(),
            Some(json!({"killed": false})),
        ),
        (
            "POST",
            "/api/v1/system-admin/ai-kick/runners/".to_string(),
            Some(json!({"name": "x"})),
        ),
        (
            "DELETE",
            "/api/v1/system-admin/ai-kick/runners/1/".to_string(),
            None,
        ),
    ] {
        let (s, _) = h.call(method, &uri, &[], body.clone()).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "ログインなし: {method} {uri}");
        for user in [h.member, h.guest, h.outsider] {
            let (s, _) = h.as_user(user, method, &uri, body.clone()).await;
            assert_eq!(s, StatusCode::FORBIDDEN, "管理者以外は拒否: {method} {uri}");
        }
    }
}

#[tokio::test]
async fn admin_cannot_enable_without_saved_limits_and_invalid_limits_are_rejected() {
    let Some(h) = hx().await else { return };
    sqlx::query("DELETE FROM system_settings WHERE key LIKE 'sys.ai.kick.%'")
        .execute(&h.pool)
        .await
        .unwrap();
    let uri = "/api/v1/system-admin/ai-kick/";
    // 上限が無いと、有効にできない(無制限では有効にできない)
    let (s, r) = h
        .as_user(h.admin, "PUT", uri, Some(json!({"enabled": true})))
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{r}");
    assert!(!ai_kick_repo::get_settings(&h.pool).await.unwrap().enabled);
    // 0(無制限)・実質無制限・矛盾した値は拒否
    let mut zero = Limits::default();
    zero.max_weighted_tokens = 0;
    let mut huge = Limits::default();
    huge.max_weighted_tokens = u64::MAX;
    let mut inconsistent = Limits::default();
    inconsistent.daily_budget_project = inconsistent.daily_budget_system + 1;
    for bad in [zero, huge, inconsistent] {
        let (s, r) = h
            .as_user(h.admin, "PUT", uri, Some(json!({"limits": bad})))
            .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{r}");
    }
    assert!(
        ai_kick_repo::get_settings(&h.pool)
            .await
            .unwrap()
            .limits
            .is_none(),
        "不正な値は保存されない"
    );
    // 正しい上限を保存して、有効にできる
    let (s, r) = h
        .as_user(
            h.admin,
            "PUT",
            uri,
            Some(json!({"limits": Limits::default(), "enabled": true})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["enabled"], true);
    assert_eq!(r["limits"]["maxWeightedTokens"], 30_000_000);
    assert_eq!(r["defaultLimits"]["weeklyBudgetSystem"], 150_000_000);
    assert!(r["usage"]["weekBudget"].is_number());
    // 緊急停止の作動と解除
    let (_, r) = h
        .as_user(h.admin, "PUT", uri, Some(json!({"killed": true})))
        .await;
    assert_eq!(r["killed"], true);
    let (_, r) = h
        .as_user(h.admin, "PUT", uri, Some(json!({"killed": false})))
        .await;
    assert_eq!(r["killed"], false);
    // 監査ログに残る(秘密は含まない)
    let actions: Vec<String> = sqlx::query_scalar(
        "SELECT action FROM access_audit_log WHERE action LIKE 'ai_kick.%' AND actor_user_id = $1 ORDER BY id DESC LIMIT 10",
    )
    .bind(i64::from(h.admin))
    .fetch_all(&h.pool)
    .await
    .unwrap();
    for a in [
        "ai_kick.limits_changed",
        "ai_kick.enabled",
        "ai_kick.killed",
        "ai_kick.unkilled",
    ] {
        assert!(
            actions.iter().any(|x| x == a),
            "監査ログに {a} がある: {actions:?}"
        );
    }
}

#[tokio::test]
async fn runner_registration_shows_the_token_once_and_revocation_stops_it() {
    let Some(h) = hx().await else { return };
    let base = "/api/v1/system-admin/ai-kick/runners/";
    // 名前の検証
    for bad in ["", "a b", "名前", "a/b"] {
        let (s, _) = h
            .as_user(h.admin, "POST", base, Some(json!({"name": bad})))
            .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad:?}");
    }
    let name = format!("mac-mini-{}", Uuid::new_v4().simple());
    let (s, r) = h
        .as_user(h.admin, "POST", base, Some(json!({"name": name})))
        .await;
    assert_eq!(s, StatusCode::CREATED, "{r}");
    let token = r["token"].as_str().unwrap().to_string();
    let signing = r["signingSecret"].as_str().unwrap().to_string();
    let id = r["id"].as_i64().unwrap();
    assert!(token.starts_with("srun_") && token.len() >= 40);
    assert!(signing.len() >= 40);
    // 同じ名前は登録できない
    let (s, _) = h
        .as_user(h.admin, "POST", base, Some(json!({"name": name})))
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    // 一覧には、トークン・ハッシュ・署名の秘密が出ない(先頭の識別用の文字だけ)
    let (_, list) = h
        .as_user(h.admin, "GET", "/api/v1/system-admin/ai-kick/", None)
        .await;
    let text = list.to_string();
    assert!(
        !text.contains(&token) && !text.contains(&signing),
        "秘密は、登録時の 1 回しか出ない"
    );
    assert!(
        !text.contains(&ai_kick_repo::hash_token(&token)),
        "ハッシュも出さない"
    );
    let mine = list["runners"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] == id)
        .unwrap()
        .clone();
    assert_eq!(mine["tokenPrefix"], token[..12].to_string());
    // DB には、生のトークンも署名の生の秘密も保存されない
    let (hash, enc): (String, String) =
        sqlx::query_as("SELECT token_hash, signing_secret_enc FROM t_ai_runner WHERE id = $1")
            .bind(id)
            .fetch_one(&h.pool)
            .await
            .unwrap();
    assert_eq!(hash, ai_kick_repo::hash_token(&token));
    assert!(!enc.contains(&signing) && !hash.contains(&token));
    assert_eq!(
        encrypted_settings::decrypt_value(&enc, &test_config().jwt_secret).unwrap(),
        signing
    );
    // 登録したトークンで、ランナーの入口に入れる
    let (s, _) = h
        .runner_call(&token, "/api/v1/ai-agent/runner/claim/", None)
        .await;
    assert_eq!(s, StatusCode::OK);
    // 失効させると、入れない。実行中の依頼には停止が求められる
    let (_, key) = h.ticket("medium").await;
    let (s, kr) = h.preview_then_kick(h.member, &key, false).await;
    assert_eq!(s, StatusCode::ACCEPTED);
    let uuid = kr["runUuid"].as_str().unwrap();
    let (_, b) = h
        .runner_call(&token, "/api/v1/ai-agent/runner/claim/", None)
        .await;
    assert_eq!(b["job"]["runUuid"], uuid);
    let (s, _) = h
        .as_user(h.admin, "DELETE", &format!("{base}{id}/"), None)
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = h
        .runner_call(&token, "/api/v1/ai-agent/runner/claim/", None)
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (reason,): (Option<String>,) =
        sqlx::query_as("SELECT stop_reason FROM t_ai_run WHERE run_uuid = $1::uuid")
            .bind(uuid)
            .fetch_one(&h.pool)
            .await
            .unwrap();
    assert_eq!(
        reason.as_deref(),
        Some("runner_revoked"),
        "失効したランナーの実行には、停止が求められる"
    );
    // 存在しない・失効済みの失効は 404
    let (s, _) = h
        .as_user(h.admin, "DELETE", &format!("{base}{id}/"), None)
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}
