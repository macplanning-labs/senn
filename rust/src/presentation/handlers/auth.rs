/// presentation/handlers/auth.rs — 認証ハンドラ
///
/// ログイン/ログアウト/パスワード変更/MFA 設定を処理する。
/// Step2②：Cookie + JWT 方式に移行。tower_sessions 依存を廃止。

use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
    Extension, Form, Json,
};
use axum_extra::extract::cookie::CookieJar;
use crate::domain::services::jwt_service;
use chrono::DateTime;
use serde::Deserialize;

use crate::domain::services::auth_service;
use crate::infrastructure::repositories::{jwt_blacklist_repo, user_repo};
use crate::presentation::handlers::security_api::{PasskeyLoginCompleteRequest, WIP_RP_NAME};
use crate::presentation::middleware::auth::{
    build_cookie, resolve_session_user, ResolveOutcome, SessionUser, ACCESS_COOKIE, MFA_COOKIE,
    PROJECT_COOKIE, REFRESH_COOKIE,
};
use crate::presentation::state::AppState;
use auth_core::domain::webauthn as webauthn_service;
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
    let jar = build_login_cookie_jar(state, user_id).await;
    (jar, Redirect::to(to)).into_response()
}

/// ログイン成功時に発行するaccess/refresh Cookie（+ MFA Cookieのクリア）を組み立てる。
/// レスポンス種別（Redirect / JSON）を問わず共通で使う。
async fn build_login_cookie_jar(state: &AppState, user_id: i32) -> CookieJar {
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
    CookieJar::new()
        .add(access_cookie)
        .add(refresh_cookie)
        .add(clear_mfa)
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
            if let Some(jti) = claims.jti.as_deref() {
                let expires_at =
                    DateTime::<chrono::Utc>::from_timestamp(claims.exp, 0).unwrap_or_else(chrono::Utc::now);
                let _ = jwt_blacklist_repo::blacklist(&state.pool, jti, expires_at).await;
            }
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
            match auth_service::hash_password(&form.new_password) {
                Ok(new_hash) => match user_repo::update_password(&state.pool, user.user_id, &new_hash).await {
                    Ok(()) => {
                        // セッション更新は不要（次リクエストの require_auth で DBから最新値を取得）
                        Redirect::to("/").into_response()
                    }
                    Err(e) => {
                        tracing::error!("[認証/パスワード変更] 処理=パスワード更新 結果=失敗 影響=新パスワードが保存されていない | {}", e);
                        Html("<script>alert('パスワードの更新に失敗しました');history.back();</script>").into_response()
                    }
                },
                Err(e) => {
                    tracing::error!("[認証/パスワード変更] 処理=パスワードハッシュ 結果=失敗 影響=パスワード変更不能 | {}", e);
                    Html("<script>alert('パスワードの更新に失敗しました');history.back();</script>").into_response()
                }
            }
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

pub async fn totp_page(jar: CookieJar) -> impl IntoResponse {
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

    let Ok(user_id) = mfa_claims.user_id() else {
        return Redirect::to("/auth/login").into_response();
    };

    match auth_service::verify_totp_for_user(&state.pool, &state.config.jwt_secret, user_id, &form.code)
        .await
    {
        Ok(true) => issue_login_cookies_and_redirect(&state, user_id, "/").await,
        _ => Html("<script>alert('認証コードが正しくありません');history.back();</script>").into_response(),
    }
}

// ======== WebAuthn ========

pub async fn webauthn_page() -> Html<&'static str> {
    Html(include_str!("../../../templates/auth/webauthn.html"))
}

/// POST /auth/webauthn/login/complete — Web UI向けパスキーログイン完了。
/// `security_api::passkey_login_complete`（JSON API、アクセス/リフレッシュトークンをJSONで返す）とは異なり、
/// Web UIのセッションモデル（Step2②のhttpOnly Cookie配布）に合わせてCookieを発行する。
/// チャレンジ発行（begin）は`security_api::passkey_login_begin`をそのまま再利用している
/// （ユーザー未識別の段階でCookie発行が絡まないため差し替え不要。routes.rsのルーティング参照）。
pub async fn webauthn_login_complete(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<PasskeyLoginCompleteRequest>,
) -> impl IntoResponse {
    let webauthn = match webauthn_service::create_webauthn_from_headers(&headers, WIP_RP_NAME) {
        Ok(w) => w,
        Err(e) => {
            tracing::error!("WebAuthn初期化エラー: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"status": "error", "error": "WebAuthn初期化エラー"})),
            )
                .into_response();
        }
    };

    let auth_state =
        match webauthn_service::authentication_state_from_json_string(&body.auth_state_json) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("認証状態デシリアライズエラー: {:?}", e);
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"status": "error", "error": "不正な認証セッション"})),
                )
                    .into_response();
            }
        };

    // 検証前に、レスポンスからユーザーを識別する(全ユーザー分を先読みしない)
    let (user_uuid, _cred_id_hint) = match webauthn_service::identify_authentication(
        &webauthn,
        &body.credential,
    ) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("パスキー識別エラー: {:?}", e);
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"status": "error", "error": "パスキー認証に失敗しました"})),
            )
                .into_response();
        }
    };
    let user_id = user_uuid.as_u128() as i32;

    let passkeys = match user_repo::find_passkeys_by_user(&state.pool, user_id).await {
        Ok(p) if !p.is_empty() => p,
        Ok(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"status": "error", "error": "パスキー認証に失敗しました"})),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    serde_json::json!({"status": "error", "error": "サーバーエラーが発生しました"}),
                ),
            )
                .into_response();
        }
    };

    let auth_result = match webauthn_service::finish_authentication(
        &webauthn,
        auth_state,
        &body.credential,
        &passkeys,
    ) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("パスキー認証完了エラー: {:?}", e);
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"status": "error", "error": "パスキー認証に失敗しました"})),
            )
                .into_response();
        }
    };

    let user = match user_repo::find_by_id(&state.pool, user_id).await {
        Ok(Some(u)) => u,
        _ => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"status": "error", "error": "ユーザーが見つかりません"})),
            )
                .into_response();
        }
    };
    if !user.is_active {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"status": "error", "error": "アカウントが無効化されています"})),
        )
            .into_response();
    }

    // sign_count更新(リプレイ攻撃防止)。iCloudキーチェーン等の同期パスキーでは
    // カウントが変化しない/ズレる場合があるため、失敗してもログイン自体は継続する
    // (ベストエフォート、tracing::warn!のみ)。security_api::passkey_login_completeと同様の扱い。
    let cred_id_bytes: Vec<u8> = auth_result.cred_id().as_ref().to_vec();
    if let Some(matching) = passkeys
        .iter()
        .find(|pk| pk.cred_id().as_ref() == cred_id_bytes.as_slice())
    {
        let mut updated = matching.clone();
        updated.update_credential(&auth_result);
        if let Ok(updated_json) = webauthn_service::passkey_to_json_string(&updated) {
            if let Err(e) =
                user_repo::update_webauthn_passkey_json(&state.pool, &cred_id_bytes, &updated_json)
                    .await
            {
                tracing::warn!("sign_count更新に失敗(ログインは継続): {:?}", e);
            }
        }
    }

    let jar = build_login_cookie_jar(&state, user.id).await;
    (jar, Json(serde_json::json!({"status": "ok"}))).into_response()
}
