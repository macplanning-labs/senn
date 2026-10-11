//! presentation/middleware/login_lock.rs — ログインの試行回数ロック(DEMO-000153)
//!
//! IP ごとの率制限(1分20回)だけでは、IP を変えながらの総当たりを止められないため、
//! auth-core の試行ロック(`attempt_lock_middleware_with_scope`)を、次のルートにだけかける。
//!
//! | ルート | 数える単位 | ロックの条件 |
//! |:--|:--|:--|
//! | `POST /api/v1/auth/login/` | IP＋ユーザー名 | 15分に5回失敗 → 15分ロック |
//! | `POST /api/v1/auth/login/` | ユーザー名だけ(IP を問わない) | 15分に30回失敗 → 15分ロック |
//! | `POST /api/v1/auth/login/verify/` | MFA トークン(IP を問わない) | 5回失敗 → そのトークンは15分使えない |
//! | `POST /auth/login`(ログイン画面のフォーム) | API のログインと同じ2段・同じストア | 同上(回数は API と合算) |
//! | `POST /auth/totp`(ログイン画面の二要素認証) | MFA の Cookie(API と同じストア) | 同上(回数は API と合算) |
//!
//! - ユーザー名だけのロックは、第三者でも本人を締め出せてしまうため、回数を高めにしてある。
//!   IP ごとの率制限(1分20回)があるので、30回に届くには複数の IP が要る。
//! - 二要素認証は、トークンを得るのにパスワードが要るので、トークン単位なら締め出しに悪用されない。
//!   IP を問わずに数えるので、IP を変えても1つのトークンで試せるのは5回まで。
//! - ロック中は 429 + `Retry-After` + `{"error":"account_locked","retry_after_seconds":N}` を返す。
//!   存在しないユーザー名も同じ 401 → 同じように数えられるので、応答からアカウントの有無は分からない。
//! - 数は、用途ごとに別のメモリ内ストア(`InMemoryAttemptStore`)で持つ(キーが混ざらないように)。
//!   プロセスごとの数なので、再起動で消え、複数台なら台ごとに数える。

use std::sync::Arc;

use auth_core::domain::attempt_lock::{
    AttemptLockConfig, AttemptOutcome, AttemptOutcomeKind, AttemptStore, DefaultAttemptOutcome,
    LockScope,
};
use auth_core::infrastructure::attempt_store::InMemoryAttemptStore;
use auth_core::infrastructure::rate_limit::attempt_lock_middleware_with_scope;
use axum::http::StatusCode;
use axum::middleware as axum_middleware;
use axum::routing::MethodRouter;

use auth_core::domain::attempt_lock::build_scoped_lock_key;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::cookie::CookieJar;

use crate::presentation::handlers::auth::LoginForm;
use crate::presentation::handlers::auth_api::{LoginRequest, LoginVerifyRequest};
use crate::presentation::middleware::auth::MFA_COOKIE;

/// ログインの試行ロック用のストア一式。ルーターを作るときに1回だけ作る。
#[derive(Clone)]
pub struct LoginAttemptStores {
    /// ログイン: IP＋ユーザー名
    login_ip_account: Arc<dyn AttemptStore>,
    /// ログイン: ユーザー名だけ
    login_account: Arc<dyn AttemptStore>,
    /// 二要素認証: MFA トークン
    verify_token: Arc<dyn AttemptStore>,
}

impl LoginAttemptStores {
    pub fn new() -> Self {
        Self {
            login_ip_account: Arc::new(InMemoryAttemptStore::new()),
            login_account: Arc::new(InMemoryAttemptStore::new()),
            verify_token: Arc::new(InMemoryAttemptStore::new()),
        }
    }
}

impl Default for LoginAttemptStores {
    fn default() -> Self {
        Self::new()
    }
}

/// 二要素認証の判定。`login_verify` は、認証コードの誤りを 400 で返す(トークンの不正も 400)。
/// 既定の判定(401/422 だけを失敗とする)では数えられないため、400 も失敗として数える。
/// 2xx/3xx は成功(数を消す)。
pub struct MfaVerifyAttemptOutcome;

impl AttemptOutcome for MfaVerifyAttemptOutcome {
    fn classify(&self, status: StatusCode) -> Option<AttemptOutcomeKind> {
        if status == StatusCode::BAD_REQUEST {
            Some(AttemptOutcomeKind::Failure)
        } else {
            DefaultAttemptOutcome.classify(status)
        }
    }
}

/// IP＋ユーザー名のロック条件: 15分に5回失敗 → 15分ロック
pub fn login_ip_account_config() -> AttemptLockConfig {
    AttemptLockConfig::default()
}

/// ユーザー名だけのロック条件: 15分に30回失敗 → 15分ロック
pub fn login_account_config() -> AttemptLockConfig {
    AttemptLockConfig::account_wide_default()
}

/// 二要素認証のロック条件: 1つの MFA トークンにつき5回失敗 → 15分ロック
pub fn mfa_verify_config() -> AttemptLockConfig {
    AttemptLockConfig::default()
}

/// `POST /api/v1/auth/login/` に、2段のロックをかける。
/// 外側がユーザー名だけ、内側が IP＋ユーザー名(内側でロックされた 429 は、外側では数えない)。
pub fn with_login_lock<S>(route: MethodRouter<S>, stores: &LoginAttemptStores) -> MethodRouter<S>
where
    S: Clone + Send + Sync + 'static,
{
    let outcome: Arc<dyn AttemptOutcome> = Arc::new(DefaultAttemptOutcome);
    route
        .route_layer(axum_middleware::from_fn(
            attempt_lock_middleware_with_scope::<LoginRequest>(
                stores.login_ip_account.clone(),
                outcome.clone(),
                login_ip_account_config(),
                LockScope::IpAndIdentifier,
            ),
        ))
        .route_layer(axum_middleware::from_fn(
            attempt_lock_middleware_with_scope::<LoginRequest>(
                stores.login_account.clone(),
                outcome,
                login_account_config(),
                LockScope::IdentifierOnly,
            ),
        ))
}

/// `POST /api/v1/auth/login/verify/` に、MFA トークン単位のロックをかける。
pub fn with_login_verify_lock<S>(
    route: MethodRouter<S>,
    stores: &LoginAttemptStores,
) -> MethodRouter<S>
where
    S: Clone + Send + Sync + 'static,
{
    route.route_layer(axum_middleware::from_fn(
        attempt_lock_middleware_with_scope::<LoginVerifyRequest>(
            stores.verify_token.clone(),
            Arc::new(MfaVerifyAttemptOutcome),
            mfa_verify_config(),
            LockScope::IdentifierOnly,
        ),
    ))
}

/// `POST /auth/login`(ログイン画面のフォーム)に、API のログインと同じ2段のロックをかける。
/// ストアも同じものを使うので、画面と API を行き来しても回数は合算される(入口を変えて回数を稼げない)。
pub fn with_cookie_login_lock<S>(
    route: MethodRouter<S>,
    stores: &LoginAttemptStores,
) -> MethodRouter<S>
where
    S: Clone + Send + Sync + 'static,
{
    let outcome: Arc<dyn AttemptOutcome> = Arc::new(DefaultAttemptOutcome);
    route
        .route_layer(axum_middleware::from_fn(
            attempt_lock_middleware_with_scope::<LoginForm>(
                stores.login_ip_account.clone(),
                outcome.clone(),
                login_ip_account_config(),
                LockScope::IpAndIdentifier,
            ),
        ))
        .route_layer(axum_middleware::from_fn(
            attempt_lock_middleware_with_scope::<LoginForm>(
                stores.login_account.clone(),
                outcome,
                login_account_config(),
                LockScope::IdentifierOnly,
            ),
        ))
}

/// `POST /auth/totp`(ログイン画面の二要素認証)に、MFA トークン単位のロックをかける。
/// トークンは本文ではなく Cookie にあるので、auth-core のミドルウェアではなくここで数える。
/// キーとストアは API の二要素認証と同じ(同じトークンなら、画面と API の回数は合算される)。
pub fn with_cookie_totp_lock<S>(
    route: MethodRouter<S>,
    stores: &LoginAttemptStores,
) -> MethodRouter<S>
where
    S: Clone + Send + Sync + 'static,
{
    let store = stores.verify_token.clone();
    route.route_layer(axum_middleware::from_fn(move |req: Request, next: Next| {
        let store = store.clone();
        async move { run_cookie_totp_lock(req, next, store).await }
    }))
}

async fn run_cookie_totp_lock(req: Request, next: Next, store: Arc<dyn AttemptStore>) -> Response {
    let cfg = mfa_verify_config();
    let key = CookieJar::from_headers(req.headers())
        .get(MFA_COOKIE)
        .map(|c| build_scoped_lock_key(LockScope::IdentifierOnly, "", c.value()));
    let Some(key) = key else {
        // Cookie が無ければ、ハンドラがログイン画面へ戻す(数えない)
        return next.run(req).await;
    };
    match store.locked_retry_after_seconds(&key).await {
        Ok(Some(retry_after)) => return cookie_totp_locked_response(retry_after),
        Ok(None) => {}
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
    let response = next.run(req).await;
    match MfaVerifyAttemptOutcome.classify(response.status()) {
        Some(AttemptOutcomeKind::Failure) => {
            let _ = store.record_failure(&key, &cfg).await;
        }
        Some(AttemptOutcomeKind::Success) => {
            let _ = store.reset(&key).await;
        }
        None => {}
    }
    response
}

/// ログイン画面向けのロック中の応答(画面のほかの失敗と同じく、アラートで知らせて戻る)
fn cookie_totp_locked_response(retry_after_seconds: u64) -> Response {
    let minutes = retry_after_seconds.div_ceil(60).max(1);
    let html = format!(
        "<script>alert('認証コードの試行が多すぎます。{minutes}分ほど待ってから、もう一度ログインしてください。');location.href='/auth/login';</script>"
    );
    let mut res = (StatusCode::TOO_MANY_REQUESTS, axum::response::Html(html)).into_response();
    if let Ok(v) = axum::http::HeaderValue::from_str(&retry_after_seconds.to_string()) {
        res.headers_mut().insert(axum::http::header::RETRY_AFTER, v);
    }
    res
}

#[cfg(test)]
mod tests {
    //! DB を使わないテスト。本物のハンドラの代わりに、パスワード(認証コード)が "ok" なら 200、
    //! それ以外は本物と同じ失敗のステータス(ログイン 401 / 二要素認証 400)を返すだけのハンドラを置く。
    use super::*;
    use auth_core::infrastructure::rate_limit::{ip_rate_limit_layer, IpRateLimitConfig};
    use axum::body::Body;
    use axum::http::{header, Request, Response};
    use axum::routing::post;
    use axum::{Json, Router};
    use std::net::SocketAddr;
    use tower::ServiceExt;

    async fn stub_login(Json(v): Json<serde_json::Value>) -> StatusCode {
        if v["password"] == "ok" {
            StatusCode::OK
        } else {
            StatusCode::UNAUTHORIZED
        }
    }

    async fn stub_verify(Json(v): Json<serde_json::Value>) -> StatusCode {
        if v["totp_code"] == "123456" {
            StatusCode::OK
        } else {
            StatusCode::BAD_REQUEST
        }
    }

    fn app() -> Router {
        let stores = LoginAttemptStores::new();
        Router::new()
            .route(
                "/api/v1/auth/login/",
                with_login_lock(post(stub_login), &stores),
            )
            .route(
                "/api/v1/auth/login/verify/",
                with_login_verify_lock(post(stub_verify), &stores),
            )
            // 本番と同じく、IP の率制限はロックの外側(ここではテストのため十分に広くする)
            .layer(ip_rate_limit_layer(IpRateLimitConfig::new(
                std::time::Duration::from_millis(1),
                10_000,
            )))
    }

    /// 信用する中継(nginx)経由で、`client` から来た JSON の POST
    fn post_json(uri: &str, client: &str, body: serde_json::Value) -> Request<Body> {
        let mut req = Request::builder()
            .method("POST")
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-real-ip", client)
            .body(Body::from(body.to_string()))
            .unwrap();
        req.extensions_mut()
            .insert(axum::extract::ConnectInfo(SocketAddr::from((
                [172, 18, 0, 5],
                40000,
            ))));
        req
    }

    async fn login(app: &Router, client: &str, username: &str, password: &str) -> Response<Body> {
        app.clone()
            .oneshot(post_json(
                "/api/v1/auth/login/",
                client,
                serde_json::json!({ "username": username, "password": password }),
            ))
            .await
            .unwrap()
    }

    async fn body_json(res: Response<Body>) -> serde_json::Value {
        let bytes = axum::body::to_bytes(res.into_body(), 64 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn five_failures_from_same_ip_and_account_lock_the_sixth() {
        let app = app();
        for _ in 0..5 {
            let res = login(&app, "198.51.100.1", "alice", "bad").await;
            assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        }
        // 6回目は、正しいパスワードでも 429
        let res = login(&app, "198.51.100.1", "Alice ", "ok").await;
        assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
        let retry: u64 = res
            .headers()
            .get(header::RETRY_AFTER)
            .expect("Retry-After")
            .to_str()
            .unwrap()
            .parse()
            .unwrap();
        assert!((1..=15 * 60).contains(&retry), "Retry-After: {retry}");
        let body = body_json(res).await;
        assert_eq!(body["error"], "account_locked");
        assert_eq!(body["retry_after_seconds"], retry);
        // ユーザー名などの情報は返さない
        assert_eq!(body.as_object().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn another_account_from_the_same_ip_is_not_locked() {
        let app = app();
        for _ in 0..5 {
            login(&app, "198.51.100.1", "alice", "bad").await;
        }
        let res = login(&app, "198.51.100.1", "bob", "ok").await;
        assert_eq!(res.status(), StatusCode::OK);
        // 同じアカウントでも、別の IP の本人は締め出されない
        let res = login(&app, "198.51.100.2", "alice", "ok").await;
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn success_resets_the_count() {
        let app = app();
        for _ in 0..4 {
            login(&app, "198.51.100.1", "carol", "bad").await;
        }
        assert_eq!(
            login(&app, "198.51.100.1", "carol", "ok").await.status(),
            StatusCode::OK
        );
        for _ in 0..4 {
            assert_eq!(
                login(&app, "198.51.100.1", "carol", "bad").await.status(),
                StatusCode::UNAUTHORIZED
            );
        }
        assert_eq!(
            login(&app, "198.51.100.1", "carol", "ok").await.status(),
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn unknown_and_existing_accounts_lock_the_same_way() {
        // ハンドラは、存在しない名前にも同じ 401 を返す → 同じ回数で、同じ形の 429 になる
        let app = app();
        for _ in 0..5 {
            login(&app, "198.51.100.1", "no-such-user", "bad").await;
            login(&app, "198.51.100.1", "dave", "bad").await;
        }
        let a = body_json(login(&app, "198.51.100.1", "no-such-user", "ok").await).await;
        let b = body_json(login(&app, "198.51.100.1", "dave", "ok").await).await;
        assert_eq!(a["error"], "account_locked");
        assert_eq!(
            a.as_object().unwrap().keys().collect::<Vec<_>>(),
            b.as_object().unwrap().keys().collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn account_wide_lock_works_across_rotating_ips() {
        let app = app();
        let threshold = login_account_config().max_attempts;
        // 毎回違う IP から(IP＋アカウントの5回には届かない)
        for i in 0..threshold {
            let ip = format!("203.0.113.{}", i % 250 + 1);
            let res = login(&app, &ip, "erin", "bad").await;
            assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "attempt {i}");
        }
        let res = login(&app, "192.0.2.77", "ERIN", "ok").await;
        assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(body_json(res).await["error"], "account_locked");
        // 別のアカウントには影響しない
        assert_eq!(
            login(&app, "192.0.2.77", "frank", "ok").await.status(),
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn account_wide_threshold_is_not_reached_by_one_ip_lock() {
        // 1つの IP から5回失敗してロックされても(内側の 429)、外側のユーザー名単位の数は増えない
        let app = app();
        for _ in 0..20 {
            login(&app, "198.51.100.9", "grace", "bad").await;
        }
        let res = login(&app, "198.51.100.10", "grace", "ok").await;
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn mfa_verify_locks_a_token_after_five_wrong_codes_even_across_ips() {
        let app = app();
        let verify = |ip: String, code: &'static str| {
            let app = app.clone();
            async move {
                app.oneshot(post_json(
                    "/api/v1/auth/login/verify/",
                    &ip,
                    serde_json::json!({ "mfa_token": "token-A", "totp_code": code }),
                ))
                .await
                .unwrap()
                .status()
            }
        };
        for i in 0..5 {
            assert_eq!(
                verify(format!("198.51.100.{i}"), "000000").await,
                StatusCode::BAD_REQUEST
            );
        }
        assert_eq!(
            verify("198.51.100.200".into(), "123456").await,
            StatusCode::TOO_MANY_REQUESTS
        );
        // 別のトークン(=もう一度パスワードを通した本人)は使える
        let res = app
            .clone()
            .oneshot(post_json(
                "/api/v1/auth/login/verify/",
                "198.51.100.1",
                serde_json::json!({ "mfa_token": "token-B", "totp_code": "123456" }),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[test]
    fn mfa_outcome_counts_bad_request_as_failure() {
        let o = MfaVerifyAttemptOutcome;
        assert_eq!(
            o.classify(StatusCode::BAD_REQUEST),
            Some(AttemptOutcomeKind::Failure)
        );
        assert_eq!(
            o.classify(StatusCode::UNAUTHORIZED),
            Some(AttemptOutcomeKind::Failure)
        );
        assert_eq!(
            o.classify(StatusCode::OK),
            Some(AttemptOutcomeKind::Success)
        );
        assert_eq!(o.classify(StatusCode::TOO_MANY_REQUESTS), None);
        assert_eq!(o.classify(StatusCode::INTERNAL_SERVER_ERROR), None);
    }

    #[test]
    fn thresholds() {
        assert_eq!(login_ip_account_config().max_attempts, 5);
        assert_eq!(login_account_config().max_attempts, 30);
        assert_eq!(mfa_verify_config().max_attempts, 5);
        for c in [
            login_ip_account_config(),
            login_account_config(),
            mfa_verify_config(),
        ] {
            assert_eq!(c.window.as_secs(), 15 * 60);
            assert_eq!(c.lock_duration.as_secs(), 15 * 60);
        }
    }

    // --- 新規登録・トークン更新の IP 率制限(DEMO-000153) ---

    async fn burst_statuses(cfg: IpRateLimitConfig, n: usize) -> Vec<StatusCode> {
        let app = Router::new()
            .route("/x", post(|| async { StatusCode::OK }))
            .layer(ip_rate_limit_layer(cfg));
        let mut out = Vec::new();
        for _ in 0..n {
            out.push(
                app.clone()
                    .oneshot(post_json("/x", "198.51.100.1", serde_json::json!({})))
                    .await
                    .unwrap()
                    .status(),
            );
        }
        out
    }

    #[tokio::test]
    async fn register_limit_returns_429_after_burst() {
        let s = burst_statuses(IpRateLimitConfig::register_preset(), 11).await;
        assert!(s[..10].iter().all(|c| *c == StatusCode::OK), "{s:?}");
        assert_eq!(s[10], StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn token_refresh_limit_returns_429_after_burst() {
        let s = burst_statuses(IpRateLimitConfig::token_refresh_preset(), 31).await;
        assert!(s[..30].iter().all(|c| *c == StatusCode::OK), "{s:?}");
        assert_eq!(s[30], StatusCode::TOO_MANY_REQUESTS);
    }

    // ---- ログイン画面(Cookie 方式) ----

    async fn stub_form_login(
        axum::Form(f): axum::Form<std::collections::HashMap<String, String>>,
    ) -> StatusCode {
        if f.get("password").map(String::as_str) == Some("ok") {
            StatusCode::SEE_OTHER
        } else {
            StatusCode::UNAUTHORIZED
        }
    }

    async fn stub_form_totp(
        axum::Form(f): axum::Form<std::collections::HashMap<String, String>>,
    ) -> StatusCode {
        if f.get("code").map(String::as_str) == Some("123456") {
            StatusCode::SEE_OTHER
        } else {
            StatusCode::UNAUTHORIZED
        }
    }

    /// API とログイン画面で、同じストアを使う(本番の routes.rs と同じ組み方)
    fn app_with_cookie_routes() -> Router {
        let stores = LoginAttemptStores::new();
        Router::new()
            .route(
                "/api/v1/auth/login/",
                with_login_lock(post(stub_login), &stores),
            )
            .route(
                "/api/v1/auth/login/verify/",
                with_login_verify_lock(post(stub_verify), &stores),
            )
            .route(
                "/auth/login",
                with_cookie_login_lock(post(stub_form_login), &stores),
            )
            .route(
                "/auth/totp",
                with_cookie_totp_lock(post(stub_form_totp), &stores),
            )
    }

    fn post_form(uri: &str, client: &str, body: &str, cookie: Option<&str>) -> Request<Body> {
        let mut b = Request::builder()
            .method("POST")
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header("x-real-ip", client);
        if let Some(c) = cookie {
            b = b.header(header::COOKIE, c);
        }
        let mut req = b.body(Body::from(body.to_string())).unwrap();
        req.extensions_mut()
            .insert(axum::extract::ConnectInfo(SocketAddr::from((
                [172, 18, 0, 5],
                40000,
            ))));
        req
    }

    async fn form_login(app: &Router, client: &str, username: &str, password: &str) -> StatusCode {
        app.clone()
            .oneshot(post_form(
                "/auth/login",
                client,
                &format!("username={username}&password={password}"),
                None,
            ))
            .await
            .unwrap()
            .status()
    }

    #[tokio::test]
    async fn login_form_is_locked_after_five_failures() {
        let app = app_with_cookie_routes();
        for _ in 0..5 {
            assert_eq!(
                form_login(&app, "198.51.100.7", "carol", "bad").await,
                StatusCode::UNAUTHORIZED
            );
        }
        assert_eq!(
            form_login(&app, "198.51.100.7", "carol", "ok").await,
            StatusCode::TOO_MANY_REQUESTS
        );
    }

    /// 入口を変えても回数は合算される(API で3回 + 画面で2回 → 6回目はどちらも 429)
    #[tokio::test]
    async fn api_and_login_form_share_the_same_count() {
        let app = app_with_cookie_routes();
        for _ in 0..3 {
            assert_eq!(
                login(&app, "198.51.100.8", "dave", "bad").await.status(),
                StatusCode::UNAUTHORIZED
            );
        }
        for _ in 0..2 {
            assert_eq!(
                form_login(&app, "198.51.100.8", "Dave", "bad").await,
                StatusCode::UNAUTHORIZED
            );
        }
        assert_eq!(
            login(&app, "198.51.100.8", "dave", "ok").await.status(),
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(
            form_login(&app, "198.51.100.8", "dave", "ok").await,
            StatusCode::TOO_MANY_REQUESTS
        );
    }

    #[tokio::test]
    async fn totp_form_is_locked_per_mfa_cookie_and_shared_with_api() {
        let app = app_with_cookie_routes();
        let cookie = format!("{MFA_COOKIE}=mfa-token-A");
        for _ in 0..3 {
            let res = app
                .clone()
                .oneshot(post_form(
                    "/auth/totp",
                    "198.51.100.9",
                    "code=000000",
                    Some(&cookie),
                ))
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        }
        // 同じトークンで API から2回失敗 → 合計5回
        for _ in 0..2 {
            let res = app
                .clone()
                .oneshot(post_json(
                    "/api/v1/auth/login/verify/",
                    "203.0.113.50",
                    serde_json::json!({ "mfa_token": "mfa-token-A", "totp_code": "000000" }),
                ))
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        }
        // 6回目は、正しいコードでも、別の IP からでも 429(Retry-After 付き)
        let res = app
            .clone()
            .oneshot(post_form(
                "/auth/totp",
                "198.51.100.10",
                "code=123456",
                Some(&cookie),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(res.headers().get(header::RETRY_AFTER).is_some());
        // 別のトークンには影響しない
        let res = app
            .clone()
            .oneshot(post_form(
                "/auth/totp",
                "198.51.100.9",
                "code=123456",
                Some(&format!("{MFA_COOKIE}=mfa-token-B")),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
    }
}
