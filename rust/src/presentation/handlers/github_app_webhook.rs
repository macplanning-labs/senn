/// presentation/handlers/github_app_webhook.rs — GitHub App の Webhook 受信
///
/// `POST /api/v1/webhooks/github-app/`(認証なし。App 共通のシークレットで HMAC 署名を検証)。
/// 設計: docs/design/詳細設計_GitHub連携かんたん接続.md §3.4
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde_json::Value;

use crate::domain::services::git_webhook_service;
use crate::domain::services::git_webhook_service::GitTrigger;
use crate::infrastructure::github_app::{client::InstallationInfo, store};
use crate::infrastructure::repositories::integration_repo;
use crate::presentation::handlers::integration_api::{
    delivery_outcome, dispatch_git_event, fire_pr_trigger,
};
use crate::presentation::state::AppState;

fn internal() -> axum::response::Response {
    (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
}

/// POST /api/v1/webhooks/github-app/
pub async fn github_app_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let event_type = headers
        .get("X-GitHub-Event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let signature = headers
        .get("X-Hub-Signature-256")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if event_type.is_empty() {
        return (StatusCode::BAD_REQUEST, "Missing X-GitHub-Event").into_response();
    }

    let secret = match store::get_webhook_secret(&state.pool, &state.config.jwt_secret).await {
        Ok(Some(s)) => s,
        Ok(None) => return (StatusCode::NOT_FOUND, "GitHub App is not configured").into_response(),
        Err(e) => {
            tracing::error!("github app secret load failed: {:?}", e);
            return internal();
        }
    };
    if !git_webhook_service::verify_github_signature(&body, signature, &secret) {
        tracing::warn!("github app webhook: invalid signature (event={event_type})");
        return (StatusCode::FORBIDDEN, "Invalid signature").into_response();
    }

    let payload: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid JSON").into_response(),
    };

    match event_type {
        "ping" => (StatusCode::OK, "pong").into_response(),
        "installation" => handle_installation(&state, &payload).await,
        "installation_repositories" => handle_installation_repositories(&state, &payload).await,
        "push" | "pull_request" => handle_git_event(&state, event_type, &payload).await,
        "pull_request_review" => handle_review(&state, &payload).await,
        "check_run" => handle_check_run(&state, &payload).await,
        // pull_request_review / check_run など。Phase 2 で扱う(権限・イベントは登録済み)
        _ => record_ignored(&state, event_type, &payload).await,
    }
}

async fn handle_git_event(
    state: &AppState,
    event_type: &str,
    payload: &Value,
) -> axum::response::Response {
    let repo_url = payload
        .get("repository")
        .and_then(|r| r.get("html_url"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if repo_url.is_empty() {
        return (StatusCode::BAD_REQUEST, "Missing repository URL").into_response();
    }

    let integration_id = match store::find_app_integration_id(&state.pool, repo_url).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            // App は入っているが、SENN でこのリポジトリを選んでいない
            tracing::info!("github app webhook: no integration for {repo_url}");
            return (StatusCode::OK, "No matching integration").into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return internal();
        }
    };
    let integration = match integration_repo::find_by_id(&state.pool, integration_id).await {
        Ok(Some(i)) => i,
        Ok(None) => return (StatusCode::OK, "No matching integration").into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return internal();
        }
    };

    let created = dispatch_git_event(state, &integration, event_type, payload).await;
    let (result, note) = delivery_outcome(state, event_type, payload, created).await;
    store::record_delivery(&state.pool, integration_id, event_type, result, &note).await;

    (StatusCode::OK, format!("OK: {created} events linked")).into_response()
}

/// イベントのリポジトリに対応する、App 経由の連携を読み込む。無ければ None(App は入っているが、SENN で未選択)。
async fn load_integration(
    state: &AppState,
    payload: &Value,
) -> Result<
    Option<crate::domain::models::integration_api::GitIntegrationOut>,
    axum::response::Response,
> {
    let repo_url = payload
        .get("repository")
        .and_then(|r| r.get("html_url"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if repo_url.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Missing repository URL").into_response());
    }
    let id = match store::find_app_integration_id(&state.pool, repo_url).await {
        Ok(Some(id)) => id,
        Ok(None) => return Ok(None),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return Err(internal());
        }
    };
    match integration_repo::find_by_id(&state.pool, id).await {
        Ok(i) => Ok(i),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            Err(internal())
        }
    }
}

/// pull_request_review: 承認・変更依頼を、PR のレビュー結果として保存し、変わったときだけトリガーを動かす。
async fn handle_review(state: &AppState, payload: &Value) -> axum::response::Response {
    let integration = match load_integration(state, payload).await {
        Ok(Some(i)) => i,
        Ok(None) => return (StatusCode::OK, "No matching integration").into_response(),
        Err(r) => return r,
    };
    let action = payload.get("action").and_then(|v| v.as_str()).unwrap_or("");
    let review_state = payload
        .get("review")
        .and_then(|r| r.get("state"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_lowercase();
    let pr_number = payload
        .get("pull_request")
        .and_then(|p| p.get("number"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as i32;

    // (新しい状態, 動かすトリガー)。コメントだけのレビューは、結果を変えない
    let (new_state, trigger) = match (action, review_state.as_str()) {
        ("submitted", "approved") => ("approved", Some(GitTrigger::PrReviewApproved)),
        ("submitted", "changes_requested") => {
            ("changes_requested", Some(GitTrigger::PrChangesRequested))
        }
        ("dismissed", _) => ("none", None),
        _ => {
            store::record_delivery(
                &state.pool,
                integration.id,
                "pull_request_review",
                "ignored",
                "コメントだけのレビューは、結果に反映しません",
            )
            .await;
            return (StatusCode::OK, "ignored").into_response();
        }
    };

    let linked = integration_repo::pr_event_tickets(&state.pool, integration.id, pr_number)
        .await
        .map(|t| !t.is_empty())
        .unwrap_or(false);
    if !linked {
        store::record_delivery(
            &state.pool,
            integration.id,
            "pull_request_review",
            "ignored",
            &format!("PR #{pr_number} に紐付いたチケットがありません"),
        )
        .await;
        return (StatusCode::OK, "ignored").into_response();
    }

    match integration_repo::set_pr_review_state(&state.pool, integration.id, pr_number, new_state)
        .await
    {
        Ok(Some(_old)) => {
            if let Some(t) = trigger {
                fire_pr_trigger(state, &integration, pr_number, t).await;
            }
            store::record_delivery(
                &state.pool,
                integration.id,
                "pull_request_review",
                "ok",
                &format!("PR #{pr_number} のレビュー結果: {new_state}"),
            )
            .await;
            (StatusCode::OK, "OK").into_response()
        }
        Ok(None) => {
            // 同じ状態の再送。何も動かさない
            store::record_delivery(
                &state.pool,
                integration.id,
                "pull_request_review",
                "ok",
                &format!("PR #{pr_number} のレビュー結果は変わりません"),
            )
            .await;
            (StatusCode::OK, "OK").into_response()
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            internal()
        }
    }
}

/// check_run: チェックごとの最新の結果を保存し、PR の CI の状態を決め直す。失敗に変わったときだけトリガーを動かす。
async fn handle_check_run(state: &AppState, payload: &Value) -> axum::response::Response {
    let integration = match load_integration(state, payload).await {
        Ok(Some(i)) => i,
        Ok(None) => return (StatusCode::OK, "No matching integration").into_response(),
        Err(r) => return r,
    };
    let check = payload.get("check_run");
    let name = check
        .and_then(|c| c.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let status = check
        .and_then(|c| c.get("status"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let conclusion = check
        .and_then(|c| c.get("conclusion"))
        .and_then(|v| v.as_str());
    let pr_numbers: Vec<i32> = check
        .and_then(|c| c.get("pull_requests"))
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|p| p.get("number").and_then(|v| v.as_i64()))
                .map(|n| n as i32)
                .collect()
        })
        .unwrap_or_default();

    if name.is_empty() || status.is_empty() || pr_numbers.is_empty() {
        store::record_delivery(
            &state.pool,
            integration.id,
            "check_run",
            "ignored",
            "PR に結び付かないチェックです",
        )
        .await;
        return (StatusCode::OK, "ignored").into_response();
    }

    let mut handled = 0;
    for pr_number in pr_numbers {
        let linked = integration_repo::pr_event_tickets(&state.pool, integration.id, pr_number)
            .await
            .map(|t| !t.is_empty())
            .unwrap_or(false);
        if !linked {
            continue;
        }
        if let Err(e) = integration_repo::upsert_check_run(
            &state.pool,
            integration.id,
            pr_number,
            name,
            status,
            conclusion,
        )
        .await
        {
            tracing::error!("DB operation failed: {:?}", e);
            return internal();
        }
        match integration_repo::recompute_ci_state(&state.pool, integration.id, pr_number).await {
            Ok(Some((_old, new_state))) => {
                if new_state == "failure" {
                    fire_pr_trigger(state, &integration, pr_number, GitTrigger::PrCiFailed).await;
                }
            }
            Ok(None) => {}
            Err(e) => {
                tracing::error!("DB operation failed: {:?}", e);
                return internal();
            }
        }
        handled += 1;
    }

    let (result, note) = if handled > 0 {
        (
            "ok",
            format!("チェック {name}: {}", conclusion.unwrap_or(status)),
        )
    } else {
        (
            "ignored",
            "紐付いたチケットがある PR のチェックではありません".to_string(),
        )
    };
    store::record_delivery(&state.pool, integration.id, "check_run", result, &note).await;
    (StatusCode::OK, "OK").into_response()
}

/// 対象外のイベント。連携が分かれば、診断に「対象外」と残す(受信できていることは分かる)。
async fn record_ignored(
    state: &AppState,
    event_type: &str,
    payload: &Value,
) -> axum::response::Response {
    if let Some(url) = payload
        .get("repository")
        .and_then(|r| r.get("html_url"))
        .and_then(|v| v.as_str())
    {
        if let Ok(Some(id)) = store::find_app_integration_id(&state.pool, url).await {
            store::record_delivery(
                &state.pool,
                id,
                event_type,
                "ignored",
                &format!("{event_type} は現在の連携の対象外です"),
            )
            .await;
        }
    }
    (StatusCode::OK, "ignored").into_response()
}

async fn handle_installation(state: &AppState, payload: &Value) -> axum::response::Response {
    let action = payload.get("action").and_then(|v| v.as_str()).unwrap_or("");
    let Some(inst) = payload.get("installation") else {
        return (StatusCode::BAD_REQUEST, "Missing installation").into_response();
    };
    let Some(installation_id) = inst.get("id").and_then(|v| v.as_i64()) else {
        return (StatusCode::BAD_REQUEST, "Missing installation id").into_response();
    };

    let result = match action {
        "deleted" => store::delete_installation(&state.pool, installation_id).await,
        "suspend" => store::set_installation_suspended(&state.pool, installation_id, true).await,
        "unsuspend" => store::set_installation_suspended(&state.pool, installation_id, false).await,
        "created" | "new_permissions_accepted" => {
            match serde_json::from_value::<InstallationInfo>(inst.clone()) {
                Ok(info) => store::upsert_installation(&state.pool, &info).await,
                Err(_) => return (StatusCode::BAD_REQUEST, "Invalid installation").into_response(),
            }
        }
        _ => Ok(()),
    };
    match result {
        Ok(()) => (StatusCode::OK, "OK").into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            internal()
        }
    }
}

/// リポジトリが外されたら、そのリポジトリの連携を無効にする。
async fn handle_installation_repositories(
    state: &AppState,
    payload: &Value,
) -> axum::response::Response {
    let installation_id = payload
        .get("installation")
        .and_then(|i| i.get("id"))
        .and_then(|v| v.as_i64());
    let Some(installation_id) = installation_id else {
        return (StatusCode::BAD_REQUEST, "Missing installation id").into_response();
    };
    let urls: Vec<String> = payload
        .get("repositories_removed")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|r| r.get("full_name").and_then(|v| v.as_str()))
                .map(|n| format!("https://github.com/{n}"))
                .collect()
        })
        .unwrap_or_default();
    match store::deactivate_repo_integrations(&state.pool, installation_id, &urls).await {
        Ok(n) => (StatusCode::OK, format!("OK: {n} deactivated")).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            internal()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::github_app::client::ManifestConversion;
    use crate::test_support::{
        create_test_project, create_test_ticket, create_test_user, test_pool, test_state,
    };
    use axum::body::Bytes;
    use axum::response::IntoResponse;
    use hmac::{Hmac, Mac};
    use serde_json::json;
    use sha2::Sha256;

    const SECRET: &str = "test-app-webhook-secret";

    fn sign(body: &[u8], secret: &str) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let hex: String = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        format!("sha256={hex}")
    }

    fn headers(event: &str, sig: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("X-GitHub-Event", event.parse().unwrap());
        h.insert("X-Hub-Signature-256", sig.parse().unwrap());
        h
    }

    async fn call(state: &AppState, event: &str, body: &Value, secret: &str) -> StatusCode {
        let bytes = serde_json::to_vec(body).unwrap();
        let sig = sign(&bytes, secret);
        github_app_webhook(
            State(state.clone()),
            headers(event, &sig),
            Bytes::from(bytes),
        )
        .await
        .into_response()
        .status()
    }

    async fn delivery(pool: &sqlx::PgPool, id: i32) -> (Option<String>, Option<String>, bool) {
        sqlx::query_as::<_, (Option<String>, Option<String>, bool)>(
            "SELECT last_delivery_result, last_delivery_note, is_active
             FROM t_git_integration WHERE id = $1",
        )
        .bind(id as i64)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    /// t_github_app は環境に 1 行だけなので、シナリオを 1 つのテストにまとめて直列に流す。
    #[tokio::test]
    async fn app_webhook_end_to_end() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let _guard = store::APP_TABLE_LOCK.lock().await;
        let state = test_state(&pool).await;

        // 準備: App(シークレットだけ本物。他はダミー)、チケット、App 連携
        sqlx::query("DELETE FROM t_github_app")
            .execute(&pool)
            .await
            .unwrap();
        let conv = ManifestConversion {
            id: 4242,
            slug: "senn-test".into(),
            name: "SENN-test".into(),
            html_url: "https://github.com/apps/senn-test".into(),
            client_id: "cid".into(),
            client_secret: "csecret".into(),
            webhook_secret: SECRET.into(),
            pem: "dummy-pem".into(),
            owner: None,
        };
        let user = create_test_user(&pool, "ghapp").await;
        assert!(
            store::save_app(&pool, &conv, user as i64, &state.config.jwt_secret)
                .await
                .unwrap()
        );
        // 暗号化して保存され、平文では入っていない
        let raw: String =
            sqlx::query_scalar("SELECT webhook_secret_enc FROM t_github_app WHERE id = 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(!raw.contains(SECRET));
        // 2 つ目は作れない(最初の 1 つを守る)
        assert!(
            !store::save_app(&pool, &conv, user as i64, &state.config.jwt_secret)
                .await
                .unwrap()
        );

        let project = create_test_project(&pool, "GHA", user).await;
        let ticket = create_test_ticket(&pool, project, "GHA", user).await;
        let n: u32 = 100_000 + rand::random::<u32>() % 900_000;
        let key = format!("GHT-{n}");
        sqlx::query("UPDATE tickets_ticket SET ticket_key = $1 WHERE id = $2")
            .bind(&key)
            .bind(ticket)
            .execute(&pool)
            .await
            .unwrap();

        let repo = format!("acme/ghapp-{}", rand::random::<u32>());
        let repo_url = format!("https://github.com/{repo}");
        let installation_id: i64 = 9_000_000 + (rand::random::<u32>() % 1_000_000) as i64;
        let integration: i32 = sqlx::query_scalar(
            "INSERT INTO t_git_integration
               (project_id, provider, repository_url, webhook_secret, is_active,
                auto_status_transition, created_by_id, created_at, installation_id)
             VALUES ($1, 'github', $2, '', true, false, $3, NOW(), $4)
             RETURNING id::int4",
        )
        .bind(project as i64)
        .bind(&repo_url)
        .bind(user as i64)
        .bind(installation_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO t_github_installation
               (installation_id, account_login, account_type, repository_selection)
             VALUES ($1, 'acme', 'Organization', 'selected')",
        )
        .bind(installation_id)
        .execute(&pool)
        .await
        .unwrap();

        // 1. 署名が違えば 403(何も記録しない)
        let push_ok = json!({
            "ref": "refs/heads/main",
            "repository": {"html_url": repo_url},
            "commits": [{
                "id": "a".repeat(40), "message": format!("Fix thing [{key}]"),
                "url": format!("{repo_url}/commit/aaa"), "author": {"name": "Tester"}
            }]
        });
        assert_eq!(
            call(&state, "push", &push_ok, "wrong-secret").await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(delivery(&pool, integration).await.0, None);

        // 2. ping
        assert_eq!(
            call(&state, "ping", &json!({}), SECRET).await,
            StatusCode::OK
        );

        // 3. push(チケット番号あり)→ 紐付き、診断は ok
        assert_eq!(call(&state, "push", &push_ok, SECRET).await, StatusCode::OK);
        let linked: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM t_git_event WHERE ticket_id = $1 AND integration_id = $2",
        )
        .bind(ticket as i64)
        .bind(integration as i64)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(linked, 1);
        assert_eq!(delivery(&pool, integration).await.0.as_deref(), Some("ok"));

        // 4. 同じ push をもう一度 → 二重に記録しない
        assert_eq!(call(&state, "push", &push_ok, SECRET).await, StatusCode::OK);
        let linked: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM t_git_event WHERE ticket_id = $1 AND integration_id = $2",
        )
        .bind(ticket as i64)
        .bind(integration as i64)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(linked, 1);

        // 5. チケット番号なし(作業ブランチ) → no_ticket と理由
        let push_none = json!({
            "ref": "refs/heads/fix/no-key-here",
            "repository": {"html_url": repo_url},
            "commits": [{"id": "b".repeat(40), "message": "no key here",
                         "url": "", "author": {"name": "Tester"}}]
        });
        assert_eq!(
            call(&state, "push", &push_none, SECRET).await,
            StatusCode::OK
        );
        let (result, note, _) = delivery(&pool, integration).await;
        assert_eq!(result.as_deref(), Some("no_ticket"));
        assert!(note.unwrap_or_default().contains("チケット番号"));

        // 5b. 番号が無いのが正常なもの(main への push・タグの push)は警告にしない(DEMO-000183)
        for r in [
            "refs/heads/main",
            "refs/tags/deploy/production/20261007-140612",
        ] {
            let push_exempt = json!({
                "ref": r,
                "repository": {"html_url": repo_url},
                "commits": [{"id": "c".repeat(40), "message": "Merge pull request #1",
                             "url": "", "author": {"name": "Tester"}}]
            });
            assert_eq!(
                call(&state, "push", &push_exempt, SECRET).await,
                StatusCode::OK
            );
            let (result, note, _) = delivery(&pool, integration).await;
            assert_eq!(result.as_deref(), Some("ignored"), "{r}");
            assert!(note.unwrap_or_default().contains("警告しません"), "{r}");
        }

        // 6. 対象外のイベントは ignored(受信できていることは分かる)
        let review = json!({"repository": {"html_url": repo_url}, "action": "submitted"});
        assert_eq!(
            call(&state, "pull_request_review", &review, SECRET).await,
            StatusCode::OK
        );
        assert_eq!(
            delivery(&pool, integration).await.0.as_deref(),
            Some("ignored")
        );

        // 7. 未選択のリポジトリは何もしない(200)
        let other = json!({
            "ref": "refs/heads/main",
            "repository": {"html_url": "https://github.com/acme/not-linked-zzz"},
            "commits": []
        });
        assert_eq!(call(&state, "push", &other, SECRET).await, StatusCode::OK);

        // 8. リポジトリが外されたら連携を無効にする
        let removed = json!({
            "action": "removed",
            "installation": {"id": installation_id},
            "repositories_removed": [{"full_name": repo}]
        });
        assert_eq!(
            call(&state, "installation_repositories", &removed, SECRET).await,
            StatusCode::OK
        );
        assert!(!delivery(&pool, integration).await.2);

        // 9. インストールが消えたら、行も消える
        let deleted = json!({"action": "deleted", "installation": {"id": installation_id}});
        assert_eq!(
            call(&state, "installation", &deleted, SECRET).await,
            StatusCode::OK
        );
        assert!(!store::installation_exists(&pool, installation_id)
            .await
            .unwrap());

        sqlx::query("DELETE FROM t_github_app")
            .execute(&pool)
            .await
            .unwrap();
        // 10. App が無ければ 404
        assert_eq!(
            call(&state, "ping", &json!({}), SECRET).await,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn manual_webhook_never_accepts_empty_secret_signature() {
        // App 連携の行は webhook_secret=''。その行に対して、空の鍵で作った署名は通してはならない
        let body = b"{}";
        let sig = sign(body, "");
        assert!(!git_webhook_service::verify_github_signature(
            body, &sig, ""
        ));
    }

    async fn status_of(pool: &sqlx::PgPool, ticket: i32) -> String {
        sqlx::query_scalar("SELECT status FROM tickets_ticket WHERE id = $1")
            .bind(ticket)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    fn pr_payload(
        repo_url: &str,
        number: i32,
        action: &str,
        title: &str,
        body: &str,
        merged: bool,
    ) -> Value {
        json!({
            "action": action,
            "pull_request": {
                "number": number, "title": title, "body": body,
                "html_url": format!("{repo_url}/pull/{number}"),
                "state": if action == "closed" { "closed" } else { "open" },
                "merged": merged, "draft": false,
                "head": {"ref": "feat/x"},
                "user": {"login": "tester", "avatar_url": ""}
            },
            "repository": {"html_url": repo_url}
        })
    }

    /// 規則(標準・プロジェクトの規則)と、Closes/Refs の書き分けが、Webhook から通しで効くこと。
    #[tokio::test]
    async fn app_webhook_applies_rules_and_respects_refs() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let _guard = store::APP_TABLE_LOCK.lock().await;
        let state = test_state(&pool).await;

        sqlx::query("DELETE FROM t_github_app")
            .execute(&pool)
            .await
            .unwrap();
        let conv = ManifestConversion {
            id: 4243,
            slug: "senn-test2".into(),
            name: "SENN-test2".into(),
            html_url: "https://github.com/apps/senn-test2".into(),
            client_id: "cid".into(),
            client_secret: "csecret".into(),
            webhook_secret: SECRET.into(),
            pem: "dummy-pem".into(),
            owner: None,
        };
        let user = create_test_user(&pool, "ghrule").await;
        store::save_app(&pool, &conv, user as i64, &state.config.jwt_secret)
            .await
            .unwrap();

        let project = create_test_project(&pool, "GHR", user).await;
        for (slug, cat, pos) in [
            ("todo", "unstarted", 0),
            ("doing", "started", 10),
            ("done", "completed", 30),
        ] {
            sqlx::query(
                "INSERT INTO t_workflow_status (project_id, slug, category, name, color, position, is_default)
                 VALUES ($1, $2, $3, $2, '#888888', $4, false)",
            )
            .bind(project)
            .bind(slug)
            .bind(cat)
            .bind(pos)
            .execute(&pool)
            .await
            .unwrap();
        }
        // 3 つのチケット(キーは GHT-<6 桁>)。t1 は Refs、t2 は Closes、t3 は規則で「何もしない」を選ぶ
        let mut tickets = Vec::new();
        for _ in 0..3 {
            let id = create_test_ticket(&pool, project, "GHR", user).await;
            let n: u32 = 100_000 + rand::random::<u32>() % 900_000;
            let key = format!("GHT-{n}");
            sqlx::query("UPDATE tickets_ticket SET ticket_key = $1, status = 'todo' WHERE id = $2")
                .bind(&key)
                .bind(id)
                .execute(&pool)
                .await
                .unwrap();
            tickets.push((id, key));
        }
        let ((t1, k1), (t2, k2), (t3, k3)) =
            (tickets[0].clone(), tickets[1].clone(), tickets[2].clone());

        let repo_url = format!("https://github.com/acme/ghrule-{}", rand::random::<u32>());
        let installation_id: i64 = 8_000_000 + (rand::random::<u32>() % 1_000_000) as i64;
        let integration: i32 = sqlx::query_scalar(
            "INSERT INTO t_git_integration
               (project_id, provider, repository_url, webhook_secret, is_active,
                auto_status_transition, created_by_id, created_at, installation_id)
             VALUES ($1, 'github', $2, '', true, true, $3, NOW(), $4)
             RETURNING id::int4",
        )
        .bind(project as i64)
        .bind(&repo_url)
        .bind(user as i64)
        .bind(installation_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let _ = integration;

        // 1. push: t1 は Refs(紐付けのみ)、t2 は Closes。Refs のキーは動かない・Closes のキーは「着手中」へ
        let push = json!({
            "ref": "refs/heads/main",
            "repository": {"html_url": repo_url},
            "commits": [{
                "id": "c".repeat(40),
                "message": format!("Update things\n\nRefs {k1}\nCloses {k2}"),
                "url": "", "author": {"name": "Tester"}
            }]
        });
        assert_eq!(call(&state, "push", &push, SECRET).await, StatusCode::OK);
        assert_eq!(
            status_of(&pool, t1).await,
            "todo",
            "Refs のキーは遷移しない"
        );
        assert_eq!(
            status_of(&pool, t2).await,
            "doing",
            "Closes のキーは着手中へ(標準)"
        );

        // 2. PR マージ(標準): t2 は完了。タイトルが Refs の t1 は、紐付けのみで動かない
        let merged = pr_payload(
            &repo_url,
            11,
            "closed",
            &format!("Resolve {k2}"),
            &format!("Refs {k1}"),
            true,
        );
        assert_eq!(
            call(&state, "pull_request", &merged, SECRET).await,
            StatusCode::OK
        );
        assert_eq!(status_of(&pool, t2).await, "done");
        assert_eq!(status_of(&pool, t1).await, "todo");

        // 3. プロジェクトの規則で「マージしても何もしない」を選ぶと、t3 は動かない
        sqlx::query(
            "INSERT INTO t_git_status_rule (project_id, trigger, action) VALUES ($1, 'pr_merged', 'none')",
        )
        .bind(project as i64)
        .execute(&pool)
        .await
        .unwrap();
        let merged3 = pr_payload(&repo_url, 12, "closed", &format!("Fix {k3}"), "", true);
        assert_eq!(
            call(&state, "pull_request", &merged3, SECRET).await,
            StatusCode::OK
        );
        assert_eq!(
            status_of(&pool, t3).await,
            "todo",
            "規則で『何もしない』を選んだら動かない"
        );

        // 4. 規則で「閉じたら着手中へ」を選ぶと、マージされずに閉じた PR で動く
        sqlx::query(
            "INSERT INTO t_git_status_rule (project_id, trigger, action, target_slug)
             VALUES ($1, 'pr_closed', 'set', 'doing')",
        )
        .bind(project as i64)
        .execute(&pool)
        .await
        .unwrap();
        let closed3 = pr_payload(&repo_url, 13, "closed", &format!("Try {k3}"), "", false);
        assert_eq!(
            call(&state, "pull_request", &closed3, SECRET).await,
            StatusCode::OK
        );
        assert_eq!(status_of(&pool, t3).await, "doing");

        sqlx::query("DELETE FROM t_git_status_rule WHERE project_id = $1")
            .bind(project as i64)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM t_github_app")
            .execute(&pool)
            .await
            .unwrap();
    }

    fn review_payload(repo_url: &str, number: i32, action: &str, state: &str) -> Value {
        json!({
            "action": action,
            "review": {"state": state, "user": {"login": "reviewer"}},
            "pull_request": {"number": number},
            "repository": {"html_url": repo_url}
        })
    }

    fn check_payload(
        repo_url: &str,
        prs: &[i32],
        name: &str,
        status: &str,
        conclusion: Option<&str>,
    ) -> Value {
        json!({
            "action": if status == "completed" { "completed" } else { "created" },
            "check_run": {
                "name": name, "status": status, "conclusion": conclusion,
                "pull_requests": prs.iter().map(|n| json!({"number": n})).collect::<Vec<_>>()
            },
            "repository": {"html_url": repo_url}
        })
    }

    /// PR の詳細(Draft・レビュアー・変更量)、レビュー結果、CI が保存され、変わったときだけトリガーが動くこと。
    #[tokio::test]
    async fn app_webhook_pr_details_review_and_ci() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let _guard = store::APP_TABLE_LOCK.lock().await;
        let state = test_state(&pool).await;

        sqlx::query("DELETE FROM t_github_app")
            .execute(&pool)
            .await
            .unwrap();
        let conv = ManifestConversion {
            id: 4244,
            slug: "senn-test3".into(),
            name: "SENN-test3".into(),
            html_url: "https://github.com/apps/senn-test3".into(),
            client_id: "cid".into(),
            client_secret: "csecret".into(),
            webhook_secret: SECRET.into(),
            pem: "dummy-pem".into(),
            owner: None,
        };
        let user = create_test_user(&pool, "ghpr").await;
        store::save_app(&pool, &conv, user as i64, &state.config.jwt_secret)
            .await
            .unwrap();

        let project = create_test_project(&pool, "GHP", user).await;
        for (slug, cat, pos) in [
            ("todo", "unstarted", 0),
            ("doing", "started", 10),
            ("done", "completed", 30),
        ] {
            sqlx::query(
                "INSERT INTO t_workflow_status (project_id, slug, category, name, color, position, is_default)
                 VALUES ($1, $2, $3, $2, '#888888', $4, false)",
            )
            .bind(project)
            .bind(slug)
            .bind(cat)
            .bind(pos)
            .execute(&pool)
            .await
            .unwrap();
        }
        // t1: PR タイトルに Fix(Closing)、t2: PR 本文に Refs(紐付けのみ)
        let mut tickets = Vec::new();
        for _ in 0..2 {
            let id = create_test_ticket(&pool, project, "GHP", user).await;
            let n: u32 = 100_000 + rand::random::<u32>() % 900_000;
            let key = format!("GHT-{n}");
            sqlx::query("UPDATE tickets_ticket SET ticket_key = $1, status = 'todo' WHERE id = $2")
                .bind(&key)
                .bind(id)
                .execute(&pool)
                .await
                .unwrap();
            tickets.push((id, key));
        }
        let ((t1, k1), (t2, k2)) = (tickets[0].clone(), tickets[1].clone());
        // 規則: 承認されたら「着手中」へ、CI が失敗したら「Todo」へ
        for (trigger, slug) in [("pr_review_approved", "doing"), ("pr_ci_failed", "todo")] {
            sqlx::query(
                "INSERT INTO t_git_status_rule (project_id, trigger, action, target_slug)
                 VALUES ($1, $2, 'set', $3)",
            )
            .bind(project as i64)
            .bind(trigger)
            .bind(slug)
            .execute(&pool)
            .await
            .unwrap();
        }

        let repo_url = format!("https://github.com/acme/ghpr-{}", rand::random::<u32>());
        let installation_id: i64 = 7_000_000 + (rand::random::<u32>() % 1_000_000) as i64;
        let integration: i32 = sqlx::query_scalar(
            "INSERT INTO t_git_integration
               (project_id, provider, repository_url, webhook_secret, is_active,
                auto_status_transition, created_by_id, created_at, installation_id)
             VALUES ($1, 'github', $2, '', true, true, $3, NOW(), $4)
             RETURNING id::int4",
        )
        .bind(project as i64)
        .bind(&repo_url)
        .bind(user as i64)
        .bind(installation_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        // 1. PR を開く(詳細つき)。t1 は Closing、t2 は Reference として保存される
        let mut opened = pr_payload(
            &repo_url,
            21,
            "opened",
            &format!("Fix {k1}"),
            &format!("Refs {k2}"),
            false,
        );
        opened["pull_request"]["requested_reviewers"] =
            json!([{"login": "alice"}, {"login": "bob"}]);
        opened["pull_request"]["additions"] = json!(10);
        opened["pull_request"]["deletions"] = json!(2);
        opened["pull_request"]["changed_files"] = json!(3);
        opened["pull_request"]["base"] = json!({"ref": "main"});
        assert_eq!(
            call(&state, "pull_request", &opened, SECRET).await,
            StatusCode::OK
        );
        let events =
            crate::infrastructure::repositories::integration_repo::find_events_by_ticket(&pool, t1)
                .await
                .unwrap();
        assert_eq!(events.len(), 1);
        let e = &events[0];
        assert_eq!(
            (e.is_draft, e.additions, e.deletions, e.changed_files),
            (false, Some(10), Some(2), Some(3))
        );
        assert_eq!(e.reviewers, vec!["alice".to_string(), "bob".to_string()]);
        assert_eq!(
            (
                e.review_state.as_str(),
                e.ci_state.as_str(),
                e.base_branch.as_str()
            ),
            ("none", "none", "main")
        );
        let intents: Vec<(i32, String)> = sqlx::query_as(
            "SELECT ticket_id::int4, key_intent FROM t_git_event WHERE integration_id = $1 AND pr_number = 21 ORDER BY ticket_id",
        )
        .bind(integration as i64)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert!(intents.contains(&(t1, "closing".to_string())));
        assert!(intents.contains(&(t2, "reference".to_string())));

        // 2. 承認: 結果が approved になり、規則で t1 は「着手中」へ。Refs の t2 は動かない
        assert_eq!(
            call(
                &state,
                "pull_request_review",
                &review_payload(&repo_url, 21, "submitted", "approved"),
                SECRET
            )
            .await,
            StatusCode::OK
        );
        let e = &crate::infrastructure::repositories::integration_repo::find_events_by_ticket(
            &pool, t1,
        )
        .await
        .unwrap()[0];
        assert_eq!(e.review_state, "approved");
        assert_eq!(status_of(&pool, t1).await, "doing");
        assert_eq!(
            status_of(&pool, t2).await,
            "todo",
            "Refs のチケットは遷移しない"
        );

        // 3. 同じ承認の再送では、トリガーを動かさない(状態が変わったときだけ)
        sqlx::query("UPDATE tickets_ticket SET status = 'todo' WHERE id = $1")
            .bind(t1)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            call(
                &state,
                "pull_request_review",
                &review_payload(&repo_url, 21, "submitted", "approved"),
                SECRET
            )
            .await,
            StatusCode::OK
        );
        assert_eq!(
            status_of(&pool, t1).await,
            "todo",
            "同じ状態の再送では動かさない"
        );

        // 4. コメントだけのレビューは対象外。取り消し(dismissed)で none に戻る
        assert_eq!(
            call(
                &state,
                "pull_request_review",
                &review_payload(&repo_url, 21, "submitted", "commented"),
                SECRET
            )
            .await,
            StatusCode::OK
        );
        assert_eq!(
            delivery(&pool, integration).await.0.as_deref(),
            Some("ignored")
        );
        assert_eq!(
            call(
                &state,
                "pull_request_review",
                &review_payload(&repo_url, 21, "dismissed", "approved"),
                SECRET
            )
            .await,
            StatusCode::OK
        );
        let e = &crate::infrastructure::repositories::integration_repo::find_events_by_ticket(
            &pool, t1,
        )
        .await
        .unwrap()[0];
        assert_eq!(e.review_state, "none");

        // 5. CI: 実行中 → pending。失敗に変わったときだけ、規則で t1 を「Todo」へ
        sqlx::query("UPDATE tickets_ticket SET status = 'doing' WHERE id = $1")
            .bind(t1)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            call(
                &state,
                "check_run",
                &check_payload(&repo_url, &[21], "build", "in_progress", None),
                SECRET
            )
            .await,
            StatusCode::OK
        );
        let e = &crate::infrastructure::repositories::integration_repo::find_events_by_ticket(
            &pool, t1,
        )
        .await
        .unwrap()[0];
        assert_eq!(e.ci_state, "pending");
        assert_eq!(status_of(&pool, t1).await, "doing");
        assert_eq!(
            call(
                &state,
                "check_run",
                &check_payload(&repo_url, &[21], "build", "completed", Some("failure")),
                SECRET
            )
            .await,
            StatusCode::OK
        );
        let e = &crate::infrastructure::repositories::integration_repo::find_events_by_ticket(
            &pool, t1,
        )
        .await
        .unwrap()[0];
        assert_eq!(e.ci_state, "failure");
        assert_eq!(status_of(&pool, t1).await, "todo", "CI の失敗で規則が動く");
        // 失敗の再送・別のチェックの失敗では、もう動かさない(すでに failure)
        sqlx::query("UPDATE tickets_ticket SET status = 'doing' WHERE id = $1")
            .bind(t1)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            call(
                &state,
                "check_run",
                &check_payload(&repo_url, &[21], "lint", "completed", Some("failure")),
                SECRET
            )
            .await,
            StatusCode::OK
        );
        assert_eq!(
            status_of(&pool, t1).await,
            "doing",
            "すでに failure のときは、動かさない"
        );
        assert_eq!(status_of(&pool, t2).await, "todo");
        // すべて成功に変わったら success
        for name in ["build", "lint"] {
            assert_eq!(
                call(
                    &state,
                    "check_run",
                    &check_payload(&repo_url, &[21], name, "completed", Some("success")),
                    SECRET
                )
                .await,
                StatusCode::OK
            );
        }
        let e = &crate::infrastructure::repositories::integration_repo::find_events_by_ticket(
            &pool, t1,
        )
        .await
        .unwrap()[0];
        assert_eq!(e.ci_state, "success");

        // 6. PR に結び付かないチェックは対象外(保存しない)
        assert_eq!(
            call(
                &state,
                "check_run",
                &check_payload(&repo_url, &[], "build", "completed", Some("failure")),
                SECRET
            )
            .await,
            StatusCode::OK
        );
        assert_eq!(
            delivery(&pool, integration).await.0.as_deref(),
            Some("ignored")
        );
        // 紐付いていない PR のチェックも、保存しない
        assert_eq!(
            call(
                &state,
                "check_run",
                &check_payload(&repo_url, &[999], "build", "completed", Some("failure")),
                SECRET
            )
            .await,
            StatusCode::OK
        );
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM t_git_check_run WHERE integration_id = $1 AND pr_number = 999",
        )
        .bind(integration as i64)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(n, 0);

        sqlx::query("DELETE FROM t_git_status_rule WHERE project_id = $1")
            .bind(project as i64)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM t_git_check_run WHERE integration_id = $1")
            .bind(integration as i64)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM t_github_app")
            .execute(&pool)
            .await
            .unwrap();
    }
}
