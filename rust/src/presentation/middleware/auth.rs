/// presentation/middleware/auth.rs — 認証ミドルウェア
///
/// Cookie（wip_access_token / wip_refresh_token）からJWTを取得し、
/// DBから最新のユーザー情報を引いてリクエスト拡張に SessionUser を注入する。
/// 未ログインなら /auth/login にリダイレクト。
/// must_change_password なら /auth/password に強制リダイレクト。
///
/// Step2②（Cookie→JWT統一）でtower_sessions::Session依存を廃止した。
/// SessionUser型・フィールド構成はダウンストリーム10ファイル以上への影響を
/// ゼロにするため変更していない。

use axum::{
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use crate::domain::services::jwt_service;
use crate::infrastructure::repositories::{jwt_blacklist_repo, project_repo, user_repo};
use crate::presentation::state::AppState;

pub const ACCESS_COOKIE: &str = "wip_access_token";
pub const REFRESH_COOKIE: &str = "wip_refresh_token";
pub const MFA_COOKIE: &str = "wip_mfa_token";
pub const PROJECT_COOKIE: &str = "wip_current_project_id";

/// 認証済みユーザーのセッションデータ
#[derive(Clone, Debug)]
pub struct SessionUser {
    pub user_id: i32,
    pub username: String,
    pub display_name: String,
    pub is_staff: bool,
    pub must_change_password: bool,
    pub current_project_id: Option<i32>,
    pub current_project_name: Option<String>,
}

/// Cookie設定の共通ヘルパー（Secure/SameSite/Pathを一元管理）
pub fn build_cookie(
    name: &str,
    value: String,
    path: &str,
    max_age_secs: i64,
    same_site: SameSite,
    secure: bool,
) -> Cookie<'static> {
    Cookie::build((name.to_string(), value))
        .http_only(true)
        .secure(secure)
        .same_site(same_site)
        .path(path.to_string())
        .max_age(time::Duration::seconds(max_age_secs))
        .build()
}

/// access→refreshの順でCookieを解決し、SessionUserを構築する。
/// アクセストークンが失効していてリフレッシュトークンが有効なら
/// サイレントリフレッシュを行い、新しいaccess Cookieを`refreshed`として返す。
pub enum ResolveOutcome {
    Authenticated {
        user: SessionUser,
        refreshed_access_cookie: Option<Cookie<'static>>,
    },
    Unauthenticated,
}

pub async fn resolve_session_user(state: &AppState, jar: &CookieJar) -> ResolveOutcome {
    // 1. access token を試す
    if let Some(access_cookie) = jar.get(ACCESS_COOKIE) {
        if let Ok(claims) = jwt_service::decode_token(access_cookie.value(), &state.config.jwt_secret) {
            if claims.token_type == jwt_service::TokenType::Access {
                if let Ok(user_id) = claims.user_id() {
                    if let Some(user) = build_session_user(state, user_id, jar).await {
                        return ResolveOutcome::Authenticated {
                            user,
                            refreshed_access_cookie: None,
                        };
                    }
                }
            }
        }
    }

    // 2. access が無い/失効 → refresh を試す（サイレントリフレッシュ）
    if let Some(refresh_cookie) = jar.get(REFRESH_COOKIE) {
        if let Ok(claims) = jwt_service::decode_token(refresh_cookie.value(), &state.config.jwt_secret) {
            if claims.token_type == jwt_service::TokenType::Refresh {
                if let (Ok(user_id), Some(jti)) = (claims.user_id(), claims.jti.as_deref()) {
                    let blacklisted = match jwt_blacklist_repo::is_blacklisted(&state.pool, jti).await {
                        Ok(v) => v,
                        Err(e) => {
                            tracing::error!("[認証/JWT] 処理=ブラックリスト照会 結果=失敗 影響=安全側で再ログイン要求 | {}", e);
                            return ResolveOutcome::Unauthenticated;
                        }
                    };
                    if !blacklisted {
                        if let Ok(new_access) = jwt_service::issue_access_token(
                            user_id,
                            &state.config.jwt_secret,
                            state.config.access_token_lifetime_minutes,
                        ) {
                            if let Some(user) = build_session_user(state, user_id, jar).await {
                                let cookie = build_cookie(
                                    ACCESS_COOKIE,
                                    new_access,
                                    "/",
                                    state.config.access_token_lifetime_minutes * 60,
                                    SameSite::Lax,
                                    state.config.cookie_secure,
                                );
                                return ResolveOutcome::Authenticated {
                                    user,
                                    refreshed_access_cookie: Some(cookie),
                                };
                            }
                        }
                    }
                }
            }
        }
    }

    ResolveOutcome::Unauthenticated
}

/// user_id からDBの最新情報を引いてSessionUserを組み立てる（must_change_password等はここで常に最新値）。
/// current_project_id はJWTではなく別Cookieから読み、DBで名前解決する。
async fn build_session_user(state: &AppState, user_id: i32, jar: &CookieJar) -> Option<SessionUser> {
    let user = match user_repo::find_by_id(&state.pool, user_id).await {
        Ok(Some(u)) => u,
        Ok(None) => return None,
        Err(e) => {
            tracing::error!("[認証/セッション] 処理=ユーザー取得 結果=失敗 影響=ログイン状態を確認できない | {}", e);
            return None;
        }
    };
    if !user.is_active {
        return None;
    }
    let (current_project_id, current_project_name) = match jar
        .get(PROJECT_COOKIE)
        .and_then(|c| c.value().parse::<i32>().ok())
    {
        Some(pid) => match project_repo::find_by_id(&state.pool, pid).await {
            Ok(Some(p)) => (Some(p.id), Some(p.name)),
            _ => (None, None),
        },
        None => (None, None),
    };
    Some(SessionUser {
        user_id: user.id,
        username: user.username,
        display_name: user.display_name,
        is_staff: user.is_staff,
        must_change_password: user.must_change_password,
        current_project_id,
        current_project_name,
    })
}

/// 認証チェックミドルウェア
///
/// 旧`require_auth`は`mfa_pending`のチェックを持っていたが、新設計ではこの
/// チェックは不要になる。有効なアクセストークンを持っている時点でMFA
/// （必要な場合）は既にログイン時に完了しているため。MFA未完了のユーザーは
/// `wip_access_token`をそもそも持たず、`resolve_session_user`が
/// `Unauthenticated`を返して`/auth/login`へリダイレクトされる。
pub async fn require_auth(
    State(state): State<AppState>,
    jar: CookieJar,
    mut req: Request,
    next: Next,
) -> Response {
    match resolve_session_user(&state, &jar).await {
        ResolveOutcome::Unauthenticated => Redirect::to("/auth/login").into_response(),
        ResolveOutcome::Authenticated {
            user,
            refreshed_access_cookie,
        } => {
            let path = req.uri().path().to_string();
            if user.must_change_password && path != "/auth/password" && !path.starts_with("/static") {
                return Redirect::to("/auth/password").into_response();
            }
            req.extensions_mut().insert(user);
            let mut response = next.run(req).await;
            if let Some(cookie) = refreshed_access_cookie {
                let jar = CookieJar::new().add(cookie);
                for header_value in jar.into_response().headers().get_all(axum::http::header::SET_COOKIE) {
                    response.headers_mut().append(axum::http::header::SET_COOKIE, header_value.clone());
                }
            }
            response
        }
    }
}
