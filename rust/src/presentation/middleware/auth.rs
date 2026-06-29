/// presentation/middleware/auth.rs — 認証ミドルウェア
///
/// セッションから user_id を取得し、リクエスト拡張に User を注入する。
/// 未ログインなら /auth/login にリダイレクト。
/// must_change_password なら /auth/password に強制リダイレクト。

use axum::{
    extract::Request,
    middleware::Next,
    response::{Redirect, Response},
};
use tower_sessions::Session;

/// 認証済みユーザーのセッションデータ
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SessionUser {
    pub user_id: i32,
    pub username: String,
    pub display_name: String,
    pub is_staff: bool,
    pub must_change_password: bool,
    pub current_project_id: Option<i32>,
    pub current_project_name: Option<String>,
}

/// 認証チェックミドルウェア
pub async fn require_auth(
    session: Session,
    mut req: Request,
    next: Next,
) -> Result<Response, Redirect> {
    let user: Option<SessionUser> = session
        .get("user")
        .await
        .unwrap_or(None);

    match user {
        Some(u) => {
            let path = req.uri().path().to_string();

            // パスワード変更必須ユーザーの強制リダイレクト
            if u.must_change_password && path != "/auth/password" && !path.starts_with("/static") {
                return Err(Redirect::to("/auth/password"));
            }

            // MFA pending チェック
            let mfa_pending: bool = session
                .get("mfa_pending")
                .await
                .unwrap_or(None)
                .unwrap_or(false);
            if mfa_pending && !path.starts_with("/auth/totp") && !path.starts_with("/auth/webauthn") && !path.starts_with("/static") {
                // MFA設定済みだがまだ認証していない → TOTP/WebAuthnページへ
                let mfa_type: String = session
                    .get("mfa_type")
                    .await
                    .unwrap_or(None)
                    .unwrap_or_else(|| "totp".to_string());
                return Err(Redirect::to(&format!("/auth/{}", mfa_type)));
            }

            req.extensions_mut().insert(u);
            Ok(next.run(req).await)
        }
        None => Err(Redirect::to("/auth/login")),
    }
}
