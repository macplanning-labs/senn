/// presentation/handlers/auth.rs — 認証ハンドラ
///
/// ログイン/ログアウト/パスワード変更/MFA 設定を処理する。
/// Step2②：Cookie + JWT 方式に移行。tower_sessions 依存を廃止。

use axum::{
    extract::State,
    response::{Html, IntoResponse, Redirect, Response},
    Extension, Form,
};
use axum_extra::extract::cookie::CookieJar;
use auth_core::domain::jwt::django_compat as jwt_service;
use chrono::DateTime;
use serde::Deserialize;

use crate::domain::services::auth_service;
use crate::infrastructure::repositories::{jwt_blacklist_repo, user_repo};
use crate::presentation::middleware::auth::{
    build_cookie, resolve_session_user, ResolveOutcome, SessionUser, ACCESS_COOKIE, MFA_COOKIE,
    PROJECT_COOKIE, REFRESH_COOKIE,
};
use crate::presentation::state::AppState;
use axum_extra::extract::cookie::SameSite;

// ======== ログイン ========

pub async fn login_page(State(state): State<AppState>, jar: CookieJar) -> impl IntoResponse {
    // 既にログイン済みならダッシュボードへ
    if matches!(
        resolve_session_user(&state, &jar).await,
        ResolveOutcome::Authenticated { .. }
    ) {
        return Redirect::to("/").into_response();
    }
    Html(include_str!("../../../templates/auth/login.html")).into_response()
}

#[derive(Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
}

pub async fn login_submit(
    State(state): State<AppState>,
    Form(form): Form<LoginForm>,
) -> impl IntoResponse {
    let user = match user_repo::find_by_username(&state.pool, &form.username).await {
        Ok(Some(u)) if u.is_active => u,
        _ => return login_failed_response(),
    };

    match auth_service::verify_password(&state.pool, user.id, &form.password, &user.password_hash).await {
        Ok(true) => {}
        _ => return login_failed_response(),
    }

    // MFA判定（auth_api::login と同じロジック）
    let totp_confirmed = matches!(
        user_repo::find_totp(&state.pool, user.id).await,
        Ok(Some(d)) if d.confirmed
    );

    if totp_confirmed {
        let mfa_token = jwt_service::issue_mfa_token(user.id, &state.config.jwt_secret).unwrap();
        let cookie = build_cookie(
            MFA_COOKIE,
            mfa_token,
            "/auth",
            300,
            SameSite::Strict,
            state.config.cookie_secure,
        );
        (CookieJar::new().add(cookie), Redirect::to("/auth/totp")).into_response()
    } else {
        issue_login_cookies_and_redirect(&state, user.id, "/").await
    }
}

/// ログイン成功時（MFA不要 or TOTP検証成功後）共通: access/refresh Cookieを発行して指定URLへ。
async fn issue_login_cookies_and_redirect(state: &AppState, user_id: i32, to: &str) -> Response {
    let pair = jwt_service::issue_token_pair(user_id, &state.config.jwt_secret).unwrap();
    let access_cookie = build_cookie(
        ACCESS_COOKIE,
        pair.access,
        "/",
        30 * 60,
        SameSite::Lax,
        state.config.cookie_secure,
    );
    let refresh_cookie = build_cookie(
        REFRESH_COOKIE,
        pair.refresh,
        "/",
        7 * 24 * 3600,
        SameSite::Lax,
        state.config.cookie_secure,
    );
    // MFA Cookieが残っていれば掃除
    let clear_mfa = build_cookie(
        MFA_COOKIE,
        String::new(),
        "/auth",
        0,
        SameSite::Strict,
        state.config.cookie_secure,
    );
    (
        CookieJar::new()
            .add(access_cookie)
            .add(refresh_cookie)
            .add(clear_mfa),
        Redirect::to(to),
    )
        .into_response()
}

fn login_failed_response() -> Response {
    Html("<script>alert('ユーザー名またはパスワードが正しくありません');history.back();</script>")
        .into_response()
}

// ======== ログアウト ========

pub async fn logout(State(state): State<AppState>, jar: CookieJar) -> impl IntoResponse {
    // リフレッシュトークンがあればブラックリスト登録
    if let Some(refresh_cookie) = jar.get(REFRESH_COOKIE) {
        if let Ok(claims) = jwt_service::decode_token(refresh_cookie.value(), &state.config.jwt_secret) {
            let expires_at =
                DateTime::<chrono::Utc>::from_timestamp(claims.exp, 0).unwrap_or_else(chrono::Utc::now);
            let _ = jwt_blacklist_repo::blacklist(&state.pool, &claims.jti, expires_at).await;
        }
    }

    let clear = |name: &str, path: &str| {
        build_cookie(name, String::new(), path, 0, SameSite::Lax, state.config.cookie_secure)
    };
    let jar = CookieJar::new()
        .add(clear(ACCESS_COOKIE, "/"))
        .add(clear(REFRESH_COOKIE, "/"))
        .add(clear(PROJECT_COOKIE, "/"));
    (jar, Redirect::to("/auth/login")).into_response()
}

// ======== パスワード変更 ========

pub async fn password_page() -> Html<&'static str> {
    Html(include_str!("../../../templates/auth/password.html"))
}

#[derive(Deserialize)]
pub struct PasswordForm {
    pub current_password: String,
    pub new_password: String,
    pub confirm_password: String,
}

pub async fn password_change(
    State(state): State<AppState>,
    Extension(user): Extension<SessionUser>,
    Form(form): Form<PasswordForm>,
) -> impl IntoResponse {
    if form.new_password != form.confirm_password {
        return Html("<script>alert('新しいパスワードが一致しません');history.back();</script>")
            .into_response();
    }

    // 現在のパスワード検証 → 新パスワードで更新
    let db_user = match user_repo::find_by_id(&state.pool, user.user_id).await {
        Ok(Some(u)) => u,
        _ => return Redirect::to("/auth/login").into_response(),
    };

    match auth_service::verify_password(&state.pool, db_user.id, &form.current_password, &db_user.password_hash)
        .await
    {
        Ok(true) => {
            let new_hash = auth_service::hash_password(&form.new_password).unwrap_or_default();
            let _ = user_repo::update_password(&state.pool, user.user_id, &new_hash).await;

            // セッション更新は不要（次リクエストの require_auth で DBから最新値を取得）
            Redirect::to("/").into_response()
        }
        _ => Html("<script>alert('現在のパスワードが正しくありません');history.back();</script>")
            .into_response(),
    }
}

// ======== MFA 設定 ========

pub async fn mfa_page() -> Html<&'static str> {
    Html(include_str!("../../../templates/auth/mfa.html"))
}

// ======== TOTP ========

pub async fn totp_page(State(state): State<AppState>, jar: CookieJar) -> impl IntoResponse {
    // wip_mfa_token が無ければ /auth/login へリダイレクト
    if jar.get(MFA_COOKIE).is_none() {
        return Redirect::to("/auth/login").into_response();
    }
    Html(include_str!("../../../templates/auth/totp.html")).into_response()
}

#[derive(Deserialize)]
pub struct TotpVerifyForm {
    pub code: String,
}

pub async fn totp_verify(
    State(state): State<AppState>,
    jar: CookieJar,
    Form(form): Form<TotpVerifyForm>,
) -> impl IntoResponse {
    let Some(mfa_cookie) = jar.get(MFA_COOKIE) else {
        return Redirect::to("/auth/login").into_response();
    };

    let Ok(mfa_claims) = jwt_service::decode_mfa_token(mfa_cookie.value(), &state.config.jwt_secret) else {
        return Redirect::to("/auth/login").into_response();
    };

    match auth_service::verify_totp_for_user(&state.pool, &state.config.jwt_secret, mfa_claims.user_id, &form.code)
        .await
    {
        Ok(true) => issue_login_cookies_and_redirect(&state, mfa_claims.user_id, "/").await,
        _ => Html("<script>alert('認証コードが正しくありません');history.back();</script>").into_response(),
    }
}

// ======== WebAuthn ========

pub async fn webauthn_page() -> Html<&'static str> {
    Html(include_str!("../../../templates/auth/webauthn.html"))
}
