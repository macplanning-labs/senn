/// presentation/handlers/github_app_setup.rs — GitHub App の作成・インストール(セットアップ)
///
/// 1. `POST /manifest/`        システム管理者が「GitHub と連携をはじめる」を押す
/// 2. `GET  /manifest/callback` GitHub が App 作成後に戻す(code を認証情報に交換して保存)
/// 3. `POST /install/`         リポジトリを選ぶ画面(GitHub 側)への URL を返す
/// 4. `GET  /install/callback`  GitHub がインストール後に戻す
///
/// 設計: docs/design/詳細設計_GitHub連携かんたん接続.md §3.2
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::domain::access::Viewer;
use crate::infrastructure::github_app::{
    self, client, manifest,
    store::{self, PURPOSE_INSTALL, PURPOSE_MANIFEST},
};
use crate::presentation::state::AppState;

fn reply(status: StatusCode, detail: &str) -> Response {
    (status, Json(json!({ "detail": detail }))).into_response()
}

fn server_error() -> Response {
    reply(
        StatusCode::INTERNAL_SERVER_ERROR,
        "サーバーエラーが発生しました",
    )
}

/// 人のログインユーザー(キー経由は不可)の ID
fn human_user_id(viewer: &Viewer) -> Result<i32, Response> {
    if viewer.principal.is_key() {
        return Err(reply(
            StatusCode::FORBIDDEN,
            "この操作はログインして行ってください",
        ));
    }
    viewer.user_id().ok_or_else(|| {
        reply(
            StatusCode::FORBIDDEN,
            "この操作はログインして行ってください",
        )
    })
}

#[derive(Deserialize)]
pub struct ManifestIn {
    pub org: Option<String>,
    #[serde(default, rename = "appName", alias = "app_name")]
    pub app_name: Option<String>,
    /// 完了後に戻る画面(省略時はトップ)
    #[serde(default, rename = "returnPath", alias = "return_path")]
    pub return_path: Option<String>,
}

/// POST /api/v1/github-app/manifest/(システム管理者)
pub async fn start_manifest(
    State(state): State<AppState>,
    viewer: Viewer,
    Json(body): Json<ManifestIn>,
) -> Response {
    let user_id = match human_user_id(&viewer) {
        Ok(id) => id,
        Err(r) => return r,
    };
    if !viewer.is_system_admin() {
        return reply(StatusCode::FORBIDDEN, "システム管理者のみ実行できます");
    }
    match store::app_exists(&state.pool).await {
        Ok(true) => return reply(StatusCode::CONFLICT, "GitHub連携は既に作成されています"),
        Ok(false) => {}
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return server_error();
        }
    }

    // この環境のアドレスがインターネットから届かないと、GitHub が作成画面の前に英語のエラーで断る。
    // その前に、分かる言葉で理由を返す(画面の案内と同じ文言)。
    if let Some(reason) = manifest::unreachable_webhook_reason(
        &manifest::webhook_url(&state.config.base_url),
        &client::web_base(),
    ) {
        return reply(StatusCode::BAD_REQUEST, &reason);
    }

    let org = body.org.as_deref().map(str::trim).filter(|s| !s.is_empty());
    if let Some(org) = org {
        if !github_app::is_valid_github_name(org) {
            return reply(StatusCode::BAD_REQUEST, "組織名が正しくありません");
        }
    }
    let app_name = match body
        .app_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(n) if manifest::is_valid_app_name(n) => n.to_string(),
        Some(_) => {
            return reply(
                StatusCode::BAD_REQUEST,
                "App名は半角の英数字・ハイフン・空白で34文字以内にしてください",
            )
        }
        None => manifest::default_app_name(
            &state.config.base_url,
            std::env::var("GITHUB_APP_NAME").ok().as_deref(),
        ),
    };

    let return_path = match body.return_path.as_deref() {
        Some(p) if github_app::is_safe_return_path(p) => p.to_string(),
        Some(_) => return reply(StatusCode::BAD_REQUEST, "戻り先が正しくありません"),
        None => "/".to_string(),
    };
    let state_token = match store::create_state(
        &state.pool,
        PURPOSE_MANIFEST,
        user_id as i64,
        &return_path,
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return server_error();
        }
    };

    let base = state.config.base_url.as_str();
    let manifest_json = manifest::build_manifest(base, &manifest::webhook_url(base), &app_name);
    let web = client::web_base();
    let post_url = match org {
        Some(org) => format!("{web}/organizations/{org}/settings/apps/new?state={state_token}"),
        None => format!("{web}/settings/apps/new?state={state_token}"),
    };
    (
        StatusCode::OK,
        Json(json!({ "postUrl": post_url, "manifest": manifest_json })),
    )
        .into_response()
}

#[derive(Deserialize)]
pub struct ManifestCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
}

/// state を発行した本人の閲覧者を読み込む。
///
/// コールバックは GitHub から戻ってくるブラウザの画面遷移で、この画面のログイン情報
/// (Authorization ヘッダ)は付かない。そのため認証済みルートではなく公開ルートに置き、
/// 本人確認は state(推測不能・使い捨て・10 分・発行時にログインしていた本人に紐付く)で行う。
async fn viewer_of_state(state: &AppState, user_id: i64) -> Option<Viewer> {
    crate::infrastructure::access::viewer_repo::load(
        &state.pool,
        crate::domain::access::Principal::Human {
            user_id: user_id as i32,
        },
        crate::infrastructure::access::viewer_repo::today_utc(),
    )
    .await
    .ok()
    .flatten()
}

/// GitHub からの戻り(ブラウザの画面遷移)に、JSON ではなく案内つきの画面を返す。
/// 文言は固定の文字列だけを渡す(利用者の入力を埋め込まない)。
fn guidance_page(status: StatusCode, title: &str, message: &str) -> Response {
    let html = format!(
        "<!doctype html><html lang=\"ja\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
<title>{title}</title>\
<style>body{{font-family:-apple-system,BlinkMacSystemFont,sans-serif;background:#0d0d0d;color:#eee;\
display:flex;align-items:center;justify-content:center;min-height:100vh;margin:0;padding:16px}}\
main{{max-width:480px}}h1{{font-size:20px}}p{{line-height:1.7;color:#bbb}}\
a{{display:inline-block;margin-top:12px;padding:10px 16px;border-radius:8px;background:#6366f1;color:#fff;text-decoration:none}}</style>\
</head><body><main><h1>{title}</h1><p>{message}</p><a href=\"/\">SENN に戻る</a></main></body></html>"
    );
    (status, axum::response::Html(html)).into_response()
}

fn invalid_state() -> Response {
    guidance_page(
        StatusCode::BAD_REQUEST,
        "この手順は期限が切れています",
        "GitHub との連携の手順には、10 分の制限があります。SENN に戻り、もう一度「GitHub と連携をはじめる」から行ってください。",
    )
}

fn forbidden_page() -> Response {
    guidance_page(
        StatusCode::FORBIDDEN,
        "この手順を完了する権限がありません",
        "この手順を始めた人の権限が変わった可能性があります。SENN に戻り、権限のあるアカウントでもう一度行ってください。",
    )
}

/// 処理に失敗したときは、JSON ではなく元の画面へ戻して理由を画面で知らせる。
fn back_with_error(return_path: &str) -> Response {
    Redirect::to(&github_app::with_result_param(return_path, "error")).into_response()
}

/// GET /api/v1/github-app/manifest/callback?code=&state=(公開ルート。state で本人確認)
pub async fn manifest_callback(
    State(state): State<AppState>,
    Query(q): Query<ManifestCallbackQuery>,
) -> Response {
    let (Some(code), Some(state_token)) = (q.code.as_deref(), q.state.as_deref()) else {
        return reply(StatusCode::BAD_REQUEST, "code と state が必要です");
    };

    let rec = match store::consume_state(&state.pool, state_token, PURPOSE_MANIFEST).await {
        Ok(Some(r)) => r,
        Ok(None) => return invalid_state(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return server_error();
        }
    };
    // state を発行した人が、今もシステム管理者であること
    let is_admin = match viewer_of_state(&state, rec.user_id).await {
        Some(v) => v.is_system_admin() && !v.principal.is_key(),
        None => false,
    };
    if !is_admin {
        return forbidden_page();
    }

    let conv = match client::exchange_manifest_code(code).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("github manifest exchange failed: {e}");
            return back_with_error(&rec.return_path);
        }
    };

    match store::save_app(&state.pool, &conv, rec.user_id, &state.config.jwt_secret).await {
        Ok(true) => {
            tracing::info!("github app created: slug={}", conv.slug);
            Redirect::to(&github_app::with_result_param(&rec.return_path, "created"))
                .into_response()
        }
        Ok(false) => back_with_error(&rec.return_path),
        Err(e) => {
            tracing::error!("github app save failed: {:?}", e);
            back_with_error(&rec.return_path)
        }
    }
}

#[derive(Deserialize)]
pub struct InstallIn {
    #[serde(default, rename = "returnPath", alias = "return_path")]
    pub return_path: Option<String>,
}

/// POST /api/v1/github-app/install/(ログイン済みの人。実際のリポジトリ連携は別途、管理権限が要る)
pub async fn start_install(
    State(state): State<AppState>,
    viewer: Viewer,
    Json(body): Json<InstallIn>,
) -> Response {
    let user_id = match human_user_id(&viewer) {
        Ok(id) => id,
        Err(r) => return r,
    };
    if !viewer.is_full_member() {
        return reply(StatusCode::FORBIDDEN, "ゲストは実行できません");
    }
    let return_path = match body.return_path.as_deref() {
        Some(p) if github_app::is_safe_return_path(p) => p.to_string(),
        Some(_) => return reply(StatusCode::BAD_REQUEST, "戻り先が正しくありません"),
        None => "/".to_string(),
    };
    let app = match store::get_app_summary(&state.pool).await {
        Ok(Some(a)) => a,
        Ok(None) => return reply(StatusCode::CONFLICT, "GitHub連携がまだ有効になっていません"),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return server_error();
        }
    };
    if !github_app::is_valid_github_name(&app.slug) {
        return server_error();
    }
    let state_token =
        match store::create_state(&state.pool, PURPOSE_INSTALL, user_id as i64, &return_path).await
        {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("DB operation failed: {:?}", e);
                return server_error();
            }
        };
    let url = format!(
        "{}/apps/{}/installations/new?state={}",
        client::web_base(),
        app.slug,
        state_token
    );
    (StatusCode::OK, Json(json!({ "url": url }))).into_response()
}

#[derive(Deserialize)]
pub struct InstallCallbackQuery {
    pub installation_id: Option<i64>,
    pub state: Option<String>,
}

/// GET /api/v1/github-app/install/callback?installation_id=&setup_action=&state=(公開ルート。state で本人確認)
pub async fn install_callback(
    State(state): State<AppState>,
    Query(q): Query<InstallCallbackQuery>,
) -> Response {
    let (Some(installation_id), Some(state_token)) = (q.installation_id, q.state.as_deref()) else {
        return reply(
            StatusCode::BAD_REQUEST,
            "installation_id と state が必要です",
        );
    };

    let rec = match store::consume_state(&state.pool, state_token, PURPOSE_INSTALL).await {
        Ok(Some(r)) => r,
        Ok(None) => return invalid_state(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return server_error();
        }
    };
    // state を発行した人が、今もゲストでない人であること
    let allowed = match viewer_of_state(&state, rec.user_id).await {
        Some(v) => v.is_full_member() && !v.principal.is_key(),
        None => false,
    };
    if !allowed {
        return forbidden_page();
    }

    let creds = match store::get_app_credentials(&state.pool, &state.config.jwt_secret).await {
        Ok(Some(c)) => c,
        Ok(None) => return back_with_error(&rec.return_path),
        Err(e) => {
            tracing::error!("github app credentials load failed: {:?}", e);
            return back_with_error(&rec.return_path);
        }
    };
    let info = match client::app_jwt(creds.0, &creds.1) {
        Ok(jwt) => client::get_installation(&jwt, installation_id).await,
        Err(e) => Err(e),
    };
    let info = match info {
        Ok(i) => i,
        Err(e) => {
            tracing::warn!("github installation lookup failed: {e}");
            return back_with_error(&rec.return_path);
        }
    };
    if let Err(e) = store::upsert_installation(&state.pool, &info).await {
        tracing::error!("DB operation failed: {:?}", e);
        return back_with_error(&rec.return_path);
    }
    Redirect::to(&github_app::with_result_param(
        &rec.return_path,
        "installed",
    ))
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expired_state_gets_a_guidance_page_not_json() {
        let r = invalid_state();
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
        let ct = r.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.starts_with("text/html"), "{ct}");
    }

    #[tokio::test]
    async fn guidance_pages_say_what_to_do_and_link_back() {
        for (resp, must_have) in [(invalid_state(), "10 分"), (forbidden_page(), "権限")] {
            let status = resp.status();
            let body = axum::body::to_bytes(resp.into_body(), 1 << 20)
                .await
                .unwrap();
            let html = String::from_utf8(body.to_vec()).unwrap();
            assert!(html.contains(must_have), "{status}: {html}");
            assert!(html.contains("href=\"/\""), "戻るリンクがない");
            assert!(!html.contains("\"detail\""), "JSON が混ざっている");
        }
    }
    use crate::domain::models::integration_api::GitIntegrationOut;
    use crate::presentation::handlers::github_app_link_api;
    use crate::test_support::{
        create_test_project, create_test_user, test_pool, test_state, test_viewer,
    };
    use axum::{routing::get, routing::post, Router};

    /// 擬似 GitHub。Manifest の code 交換・インストール取得・トークン・リポジトリ一覧だけ返す。
    async fn start_mock_github(pem: String) -> String {
        let app = Router::new()
            .route(
                "/app-manifests/{code}/conversions",
                post(move || {
                    let pem = pem.clone();
                    async move {
                        Json(json!({
                            "id": 777, "slug": "senn-mock", "name": "SENN-mock",
                            "html_url": "https://github.com/apps/senn-mock",
                            "client_id": "cid", "client_secret": "csec",
                            "webhook_secret": "whsec", "pem": pem,
                            "owner": {"login": "acme"}
                        }))
                    }
                }),
            )
            .route(
                "/app/installations/{id}",
                get(|axum::extract::Path(id): axum::extract::Path<i64>| async move {
                    Json(json!({
                        "id": id, "account": {"login": "acme", "type": "Organization"},
                        "repository_selection": "selected", "suspended_at": null
                    }))
                }),
            )
            .route(
                "/app/installations/{id}/access_tokens",
                post(|| async { Json(json!({"token": "tok"})) }),
            )
            .route(
                "/installation/repositories",
                get(|| async {
                    Json(json!({"repositories": [
                        {"full_name": "acme/alpha", "html_url": "https://github.com/acme/alpha", "private": true},
                        {"full_name": "acme/beta", "html_url": "https://github.com/acme/beta", "private": false}
                    ]}))
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }

    fn generate_pem() -> Option<String> {
        let dir = std::env::temp_dir().join(format!("senn-setup-key-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).ok()?;
        let path = dir.join("k.pem");
        let ok = std::process::Command::new("openssl")
            .args(["genrsa", "-out"])
            .arg(&path)
            .arg("2048")
            .output()
            .ok()?
            .status
            .success();
        let pem = if ok {
            std::fs::read_to_string(&path).ok()
        } else {
            None
        };
        let _ = std::fs::remove_dir_all(&dir);
        pem
    }

    fn location(resp: &Response) -> String {
        resp.headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string()
    }

    async fn body_json(resp: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap_or(json!(null))
    }

    fn state_from(url: &str) -> String {
        url.split("state=")
            .nth(1)
            .unwrap()
            .split('&')
            .next()
            .unwrap()
            .to_string()
    }

    /// App 作成 → インストール → リポジトリ選択 までを、擬似 GitHub に対して通す。
    #[tokio::test]
    async fn start_manifest_refuses_when_webhook_host_is_unreachable_from_github() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let _guard = store::APP_TABLE_LOCK.lock().await;
        // 本物の GitHub(既定)向け。擬似 GitHub の環境変数は外す
        std::env::remove_var("GITHUB_API_BASE");
        std::env::remove_var("GITHUB_WEB_BASE");
        std::env::remove_var("GITHUB_APP_WEBHOOK_URL");
        sqlx::query("DELETE FROM t_github_app")
            .execute(&pool)
            .await
            .unwrap();
        let admin = create_test_user(&pool, "ghadmin_unreach").await;
        sqlx::query("UPDATE accounts_user SET is_system_admin = true WHERE id = $1")
            .bind(admin as i64)
            .execute(&pool)
            .await
            .unwrap();
        let input = || ManifestIn {
            org: None,
            app_name: None,
            return_path: None,
        };

        // localhost の環境: GitHub に断られる前に、分かる言葉で断る(400)。App も state も作らない
        let mut state = test_state(&pool).await;
        state.config.base_url = "http://localhost:8150".to_string();
        let r = start_manifest(
            State(state.clone()),
            test_viewer(&pool, admin).await,
            Json(input()),
        )
        .await;
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(r.into_body(), 1 << 20).await.unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains("GitHub App を作れません"), "{text}");
        assert!(text.contains("localhost"), "{text}");

        // 社内アドレス(staging の社内向けアドレス)も同じ
        state.config.base_url = "http://10.0.0.5:8160".to_string();
        let r = start_manifest(
            State(state.clone()),
            test_viewer(&pool, admin).await,
            Json(input()),
        )
        .await;
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);

        // インターネットから届くアドレスなら、先へ進む(Manifest を返す)
        state.config.base_url = "https://senn.example.com".to_string();
        let r = start_manifest(
            State(state.clone()),
            test_viewer(&pool, admin).await,
            Json(input()),
        )
        .await;
        assert_eq!(r.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn setup_flow_with_mock_github() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let Some(pem) = generate_pem() else {
            eprintln!("openssl が無いためスキップ");
            return;
        };
        let _guard = store::APP_TABLE_LOCK.lock().await;
        let mock = start_mock_github(pem).await;
        std::env::set_var("GITHUB_API_BASE", &mock);
        std::env::set_var("GITHUB_WEB_BASE", "https://github.test");

        let state = test_state(&pool).await;
        sqlx::query("DELETE FROM t_github_app")
            .execute(&pool)
            .await
            .unwrap();
        let admin = create_test_user(&pool, "ghadmin").await;
        sqlx::query("UPDATE accounts_user SET is_system_admin = true WHERE id = $1")
            .bind(admin as i64)
            .execute(&pool)
            .await
            .unwrap();
        let normal = create_test_user(&pool, "ghnormal").await;
        let admin_v = test_viewer(&pool, admin).await;
        let normal_v = test_viewer(&pool, normal).await;

        // 1. システム管理者以外は App を作れない
        let r = start_manifest(
            State(state.clone()),
            normal_v,
            Json(ManifestIn {
                org: None,
                app_name: None,
                return_path: None,
            }),
        )
        .await;
        assert_eq!(r.status(), StatusCode::FORBIDDEN);

        // 2. 不正な組織名・戻り先は拒否
        for (org, rp) in [(Some("a/b"), None), (None, Some("https://evil.example"))] {
            let r = start_manifest(
                State(state.clone()),
                test_viewer(&pool, admin).await,
                Json(ManifestIn {
                    org: org.map(String::from),
                    app_name: None,
                    return_path: rp.map(String::from),
                }),
            )
            .await;
            assert_eq!(r.status(), StatusCode::BAD_REQUEST);
        }

        // 3. 管理者は Manifest と state を受け取る(組織指定で組織用の URL になる)
        let r = start_manifest(
            State(state.clone()),
            admin_v,
            Json(ManifestIn {
                org: Some("acme".into()),
                app_name: None,
                return_path: Some("/projects/1/settings".into()),
            }),
        )
        .await;
        assert_eq!(r.status(), StatusCode::OK);
        let v = body_json(r).await;
        let post_url = v["postUrl"].as_str().unwrap().to_string();
        assert!(
            post_url.starts_with("https://github.test/organizations/acme/settings/apps/new?state=")
        );
        assert!(v["manifest"]["redirect_url"]
            .as_str()
            .unwrap()
            .ends_with("/api/v1/github-app/manifest/callback"));
        let st = state_from(&post_url);

        // 4. 知らない state は拒否。管理者でない人の state では App を作れない
        let cb = |code: &str, st: &str| ManifestCallbackQuery {
            code: Some(code.to_string()),
            state: Some(st.to_string()),
        };
        let r = manifest_callback(State(state.clone()), Query(cb("c0de", "nope"))).await;
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
        let other_users_state = store::create_state(&pool, PURPOSE_MANIFEST, normal as i64, "/")
            .await
            .unwrap();
        let r =
            manifest_callback(State(state.clone()), Query(cb("c0de", &other_users_state))).await;
        assert_eq!(r.status(), StatusCode::FORBIDDEN);

        // 5. 正しい state で App が保存され、戻り先へ(created)。state は 1 回しか使えない
        let r = manifest_callback(State(state.clone()), Query(cb("c0de", &st))).await;
        assert!(r.status().is_redirection(), "{:?}", r.status());
        assert_eq!(location(&r), "/projects/1/settings?githubApp=created");
        let again = manifest_callback(State(state.clone()), Query(cb("c0de", &st))).await;
        assert_eq!(again.status(), StatusCode::BAD_REQUEST);
        assert!(store::app_exists(&pool).await.unwrap());
        // 2 つ目の App は作れない
        let r = start_manifest(
            State(state.clone()),
            test_viewer(&pool, admin).await,
            Json(ManifestIn {
                org: None,
                app_name: None,
                return_path: None,
            }),
        )
        .await;
        assert_eq!(r.status(), StatusCode::CONFLICT);

        // 6. インストール: URL を受け取り、コールバックで保存
        let r = start_install(
            State(state.clone()),
            test_viewer(&pool, normal).await,
            Json(InstallIn {
                return_path: Some("/teams/x/settings".into()),
            }),
        )
        .await;
        assert_eq!(r.status(), StatusCode::OK);
        let url = body_json(r).await["url"].as_str().unwrap().to_string();
        assert!(url.starts_with("https://github.test/apps/senn-mock/installations/new?state="));
        let ist = state_from(&url);
        let installation_id: i64 = 5_000_000 + (rand::random::<u32>() % 1_000_000) as i64;
        let r = install_callback(
            State(state.clone()),
            Query(InstallCallbackQuery {
                installation_id: Some(installation_id),
                state: Some(ist),
            }),
        )
        .await;
        assert_eq!(location(&r), "/teams/x/settings?githubApp=installed");
        assert!(store::installation_exists(&pool, installation_id)
            .await
            .unwrap());

        // 7. リポジトリ一覧と連携(管理者がプロジェクトに連携する)
        let project = create_test_project(&pool, "GHS", admin).await;
        // プロジェクトの設定を管理できる人(チームの Owner)にする
        let team: i32 = sqlx::query_scalar(
            "SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1",
        )
        .bind(project)
        .fetch_one(&pool)
        .await
        .unwrap();
        crate::test_support::add_test_team_member(&pool, team, admin, "admin").await;
        let r = github_app_link_api::repositories(
            State(state.clone()),
            test_viewer(&pool, admin).await,
            Query(github_app_link_api::RepositoriesQuery {
                project: Some(project),
                team: None,
            }),
        )
        .await
        .into_response();
        assert_eq!(r.status(), StatusCode::OK);
        let repos = body_json(r).await;
        assert!(repos
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["fullName"] == "acme/alpha" && x["linked"] == false));

        let link = |repos: Vec<&str>| github_app_link_api::CreateIntegrationsIn {
            project: Some(project),
            team: None,
            installation_id,
            repositories: repos.into_iter().map(String::from).collect(),
        };
        let r = github_app_link_api::create_integrations(
            State(state.clone()),
            test_viewer(&pool, admin).await,
            Json(link(vec!["acme/alpha", "acme/not-in-list", "../etc"])),
        )
        .await
        .into_response();
        assert_eq!(r.status(), StatusCode::CREATED);
        let v = body_json(r).await;
        assert_eq!(v["created"], json!(["acme/alpha"]));
        assert_eq!(v["rejected"], json!(["acme/not-in-list", "../etc"]));

        // 作られた連携は「App 経由」で、シークレットは空
        let row: GitIntegrationOut = {
            let id: i32 = sqlx::query_scalar(
                "SELECT id::int4 FROM t_git_integration
                 WHERE repository_url = 'https://github.com/acme/alpha' AND project_id = $1",
            )
            .bind(project as i64)
            .fetch_one(&pool)
            .await
            .unwrap();
            crate::infrastructure::repositories::integration_repo::find_by_id(&pool, id)
                .await
                .unwrap()
                .unwrap()
        };
        assert_eq!(row.installation_id, Some(installation_id));
        assert_eq!(row.webhook_secret, "");

        // 同じものをもう一度 → skipped(二重に作らない)。連携済みは linked=true で見える
        let r = github_app_link_api::create_integrations(
            State(state.clone()),
            test_viewer(&pool, admin).await,
            Json(link(vec!["acme/alpha"])),
        )
        .await
        .into_response();
        assert_eq!(body_json(r).await["skipped"], json!(["acme/alpha"]));

        // 8. プロジェクト管理者でない人は連携できない
        let r = github_app_link_api::create_integrations(
            State(state.clone()),
            test_viewer(&pool, normal).await,
            Json(link(vec!["acme/beta"])),
        )
        .await
        .into_response();
        assert_eq!(r.status(), StatusCode::FORBIDDEN);

        // 後片付け(他のテストの前提を壊さない)
        sqlx::query("DELETE FROM t_git_integration WHERE installation_id = $1")
            .bind(installation_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM t_github_installation WHERE installation_id = $1")
            .bind(installation_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM t_github_app")
            .execute(&pool)
            .await
            .unwrap();
        std::env::remove_var("GITHUB_API_BASE");
        std::env::remove_var("GITHUB_WEB_BASE");
    }
}
