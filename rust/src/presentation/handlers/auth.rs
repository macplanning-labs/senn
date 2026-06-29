/// presentation/handlers/auth.rs — 認証ハンドラ
///
/// ログイン/ログアウト/パスワード変更/MFA 設定を処理する。

use axum::{
    extract::State,
    response::{Html, Redirect, IntoResponse},
    Form,
};
use serde::Deserialize;
use tower_sessions::Session;
use crate::presentation::state::AppState;
use crate::presentation::middleware::auth::SessionUser;
use crate::infrastructure::repositories::{user_repo};
use crate::domain::services::auth_service;

// ======== ログイン ========

pub async fn login_page(session: Session) -> impl IntoResponse {
    // 既にログイン済みならダッシュボードへ
    let user: Option<SessionUser> = session.get("user").await.unwrap_or(None);
    if user.is_some() {
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
    session: Session,
    Form(form): Form<LoginForm>,
) -> impl IntoResponse {
    let user = user_repo::find_by_username(&state.pool, &form.username).await;

    match user {
        Ok(Some(u)) => {
            // パスワード検証
            match auth_service::verify_password(&state.pool, u.id, &form.password, &u.password_hash).await {
                Ok(true) => {
                    // セッションにユーザー情報保存
                    let session_user = SessionUser {
                        user_id: u.id,
                        username: u.username.clone(),
                        display_name: u.display_name.clone(),
                        is_staff: u.is_staff,
                        must_change_password: u.must_change_password,
                        current_project_id: None,
                        current_project_name: None,
                    };
                    let _ = session.insert("user", session_user).await;

                    // TODO: MFA チェック（TOTP / WebAuthn）
                    // MFA 設定済みなら mfa_pending = true にして
                    // TOTP/WebAuthn ページにリダイレクト

                    Redirect::to("/").into_response()
                }
                _ => {
                    // 認証失敗
                    Html("<script>alert('ユーザー名またはパスワードが正しくありません');history.back();</script>").into_response()
                }
            }
        }
        _ => {
            Html("<script>alert('ユーザー名またはパスワードが正しくありません');history.back();</script>").into_response()
        }
    }
}

// ======== ログアウト ========

pub async fn logout(session: Session) -> Redirect {
    let _ = session.delete().await;
    Redirect::to("/auth/login")
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
    session: Session,
    Form(form): Form<PasswordForm>,
) -> impl IntoResponse {
    let user: Option<SessionUser> = session.get("user").await.unwrap_or(None);
    let user = match user {
        Some(u) => u,
        None => return Redirect::to("/auth/login").into_response(),
    };

    if form.new_password != form.confirm_password {
        return Html("<script>alert('新しいパスワードが一致しません');history.back();</script>").into_response();
    }

    // 現在のパスワード検証 → 新パスワードで更新
    let db_user = user_repo::find_by_id(&state.pool, user.user_id).await;
    match db_user {
        Ok(Some(u)) => {
            match auth_service::verify_password(&state.pool, u.id, &form.current_password, &u.password_hash).await {
                Ok(true) => {
                    let new_hash = auth_service::hash_password(&form.new_password)
                        .unwrap_or_default();
                    let _ = user_repo::update_password(&state.pool, user.user_id, &new_hash).await;

                    // セッション更新
                    let mut session_user = user.clone();
                    session_user.must_change_password = false;
                    let _ = session.insert("user", session_user).await;

                    Redirect::to("/").into_response()
                }
                _ => {
                    Html("<script>alert('現在のパスワードが正しくありません');history.back();</script>").into_response()
                }
            }
        }
        _ => Redirect::to("/auth/login").into_response(),
    }
}

// ======== MFA 設定 ========

pub async fn mfa_page() -> Html<&'static str> {
    Html(include_str!("../../../templates/auth/mfa.html"))
}

// ======== TOTP ========

pub async fn totp_page() -> Html<&'static str> {
    Html(include_str!("../../../templates/auth/totp.html"))
}

pub async fn totp_verify() -> impl IntoResponse {
    // TODO: Phase D 詳細実装
    Redirect::to("/")
}

// ======== WebAuthn ========

pub async fn webauthn_page() -> Html<&'static str> {
    Html(include_str!("../../../templates/auth/webauthn.html"))
}
