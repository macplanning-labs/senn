//! 社内 SSO(Cloudflare Access + Keycloak / Google Workspace)でのログイン。
//!
//! Cloudflare Access がログイン済みの人のリクエストに付ける JWT(`Cf-Access-Jwt-Assertion`)を
//! **署名まで検証**し、そのメールアドレスと一致する有効な SENN アカウントにトークンを発行する。
//! ヘッダーを検証せずに信じると、Tunnel を通らない経路(社内 LAN から直接など)で
//! 誰にでもなりすませるため、必ず `cf_access_jwt::Verifier` を通す。
//!
//! - SSO が無効(`SENN_SSO_ENABLED` が true でない)なら 404。従来のログインだけが使える
//! - 本人確認は Keycloak(パスキー)/ Google(2 段階認証必須)で済んでいるため、SENN の TOTP は求めない
//! - 同じメールのアカウントが複数ある人(例: 社長としての管理者と、社員としての担当者)には、
//!   どれで入るかを選んでもらう。選択は毎回 JWT を検証し直したうえで、候補の中からだけ受け付ける
//! - SSO は「誰か」を決めるだけ。何をしてよいかは、これまでどおり SENN 側の権限で判定する
//! - JWT はヘッダーを優先し、無ければ Access のログイン Cookie(`CF_Authorization`)から読む。
//!   本番は `/api/` を Access の Bypass にしており(機械の利用経路のため)、Bypass ではヘッダーが付かない。
//!   Cookie もヘッダーと同じ JWT なので、同じ検証(署名・iss・aud・期限)を通す。
//!   Cookie はブラウザが自動で送るため、Cookie を使うときは Origin が SENN 自身であることも確かめる

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::domain::models::user::User;
use crate::domain::services::jwt_service;
use crate::infrastructure::repositories::user_repo;
use crate::presentation::state::AppState;

#[derive(Debug, Default, Deserialize)]
pub struct SsoLoginRequest {
    /// 複数のアカウントから選ぶときだけ指定する
    #[serde(rename = "userId", default)]
    pub user_id: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct SsoAccount {
    pub id: i32,
    pub username: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "isSystemAdmin")]
    pub is_system_admin: bool,
}

#[derive(Debug, Serialize)]
pub struct SsoChooseResponse {
    #[serde(rename = "chooseAccount")]
    pub choose_account: bool,
    pub accounts: Vec<SsoAccount>,
}

#[derive(Debug, Serialize)]
pub struct SsoTokenResponse {
    pub access: String,
    pub refresh: String,
}

/// アカウントの決め方(DB・JWT に依存しない純粋な判定。テスト対象)
#[derive(Debug, PartialEq, Eq)]
pub enum Selection {
    /// このアカウントでログインする
    Login(i32),
    /// 選んでもらう(候補の id)
    Choose(Vec<i32>),
    /// 一致するアカウントが無い
    NoAccount,
    /// 指定された id が候補に無い
    InvalidChoice,
}

/// `candidates` は「メールが一致する有効なアカウント」。`ai_user` はログインさせない機械用アカウント名。
pub fn select_account(candidates: &[User], requested: Option<i32>, ai_user: &str) -> Selection {
    let ids: Vec<i32> = candidates
        .iter()
        .filter(|u| u.is_active && u.username != ai_user)
        .map(|u| u.id)
        .collect();
    match (ids.as_slice(), requested) {
        ([], _) => Selection::NoAccount,
        (_, Some(id)) if ids.contains(&id) => Selection::Login(id),
        (_, Some(_)) => Selection::InvalidChoice,
        ([only], None) => Selection::Login(*only),
        (_, None) => Selection::Choose(ids),
    }
}

/// Access のログイン Cookie の名前
const ACCESS_COOKIE: &str = "CF_Authorization";

fn allowed_origins(state: &AppState) -> Vec<String> {
    std::iter::once(&state.config.base_url)
        .chain(state.config.additional_allowed_origins.iter())
        .map(|o| o.trim_end_matches('/').to_string())
        .collect()
}

/// 検証する JWT を取り出す(検証はまだしない)。ヘッダーを優先し、無ければ Cookie。
/// Cookie を使うのは、Origin が許可したオリジンと完全に一致するときだけ(別サイトからの送信を拒む)。
pub fn access_token<'a>(
    headers: &'a HeaderMap,
    allowed: &[String],
) -> Result<&'a str, &'static str> {
    if let Some(v) = headers
        .get(cf_access_jwt::HEADER_NAME)
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.trim().is_empty())
    {
        return Ok(v);
    }
    let cookie = headers
        .get_all(axum::http::header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == ACCESS_COOKIE)
        .map(|(_, v)| v.trim())
        .filter(|v| !v.is_empty());
    let Some(cookie) = cookie else {
        return Err("JWT がありません(ヘッダー・Cookie とも)");
    };
    let origin = headers
        .get(axum::http::header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .map(|o| o.trim_end_matches('/'));
    match origin {
        Some(o) if allowed.iter().any(|a| a.trim_end_matches('/') == o) => Ok(cookie),
        _ => Err("Cookie の JWT は SENN 自身のページからの送信だけ受け付けます"),
    }
}

fn detail(status: StatusCode, msg: &str) -> Response {
    (status, Json(serde_json::json!({ "detail": msg }))).into_response()
}

/// POST /api/v1/auth/sso/login/
pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Option<Json<SsoLoginRequest>>,
) -> Response {
    let Some(verifier) = state.sso.as_ref() else {
        return detail(StatusCode::NOT_FOUND, "SSO は有効になっていません");
    };
    let token = match access_token(&headers, &allowed_origins(&state)) {
        Ok(t) => t,
        Err(reason) => {
            tracing::warn!(
                reason,
                "SSO ログイン: Cloudflare Access の JWT を受け取れません"
            );
            return detail(StatusCode::UNAUTHORIZED, "SSO の認証情報を確認できません");
        }
    };
    let email = match verifier.verify(token).await {
        Ok(cf_access_jwt::Principal::User { email, .. }) => email,
        Ok(cf_access_jwt::Principal::ServiceToken { .. }) => {
            return detail(StatusCode::UNAUTHORIZED, "SSO の認証情報を確認できません");
        }
        Err(e) => {
            // 理由はログにだけ残す(トークンそのものは出さない)
            tracing::warn!(reason = %e, "SSO ログイン: Cloudflare Access の JWT を検証できません");
            return detail(StatusCode::UNAUTHORIZED, "SSO の認証情報を確認できません");
        }
    };

    let candidates = match user_repo::find_active_by_email(&state.pool, &email).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("SSO ログイン: 利用者の検索に失敗: {:?}", e);
            return detail(StatusCode::INTERNAL_SERVER_ERROR, "ログインに失敗しました");
        }
    };
    let requested = body.and_then(|Json(b)| b.user_id);
    match select_account(&candidates, requested, &state.config.wip_ai_api_user) {
        Selection::NoAccount => {
            tracing::warn!("SSO ログイン: 一致する有効なアカウントがありません");
            detail(
                StatusCode::FORBIDDEN,
                "このメールアドレスの SENN アカウントがありません。管理者に連絡してください",
            )
        }
        Selection::InvalidChoice => detail(StatusCode::FORBIDDEN, "選んだアカウントでは入れません"),
        Selection::Choose(ids) => {
            let accounts = candidates
                .iter()
                .filter(|u| ids.contains(&u.id))
                .map(|u| SsoAccount {
                    id: u.id,
                    username: u.username.clone(),
                    display_name: u.display().to_string(),
                    is_system_admin: u.is_system_admin,
                })
                .collect();
            (
                StatusCode::OK,
                Json(SsoChooseResponse {
                    choose_account: true,
                    accounts,
                }),
            )
                .into_response()
        }
        Selection::Login(user_id) => match jwt_service::issue_token_pair(
            user_id,
            &state.config.jwt_secret,
            state.config.access_token_lifetime_minutes,
            state.config.refresh_token_lifetime_days,
        ) {
            Ok(pair) => {
                tracing::info!(user_id, "SSO ログイン");
                (
                    StatusCode::OK,
                    Json(SsoTokenResponse {
                        access: pair.access,
                        refresh: pair.refresh,
                    }),
                )
                    .into_response()
            }
            Err(e) => {
                tracing::error!("SSO ログイン: トークン発行に失敗: {:?}", e);
                detail(StatusCode::INTERNAL_SERVER_ERROR, "ログインに失敗しました")
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn hdrs(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.append(*k, HeaderValue::from_str(v).unwrap());
        }
        h
    }
    const SELF: &str = "https://senn.example.com";

    #[test]
    fn ヘッダーを優先する() {
        let h = hdrs(&[
            ("cf-access-jwt-assertion", "H"),
            ("cookie", "CF_Authorization=C"),
        ]);
        assert_eq!(access_token(&h, &[SELF.into()]), Ok("H"));
    }

    #[test]
    fn ヘッダーが無ければ_自分のページからの_cookie_を使う() {
        let h = hdrs(&[("cookie", "a=1; CF_Authorization=C; b=2"), ("origin", SELF)]);
        assert_eq!(access_token(&h, &[format!("{SELF}/")]), Ok("C"));
    }

    #[test]
    fn 別サイトや_origin_なしの_cookie_は使わない() {
        let other = hdrs(&[
            ("cookie", "CF_Authorization=C"),
            ("origin", "https://evil.example"),
        ]);
        assert!(access_token(&other, &[SELF.into()]).is_err());
        let none = hdrs(&[("cookie", "CF_Authorization=C")]);
        assert!(access_token(&none, &[SELF.into()]).is_err());
        let prefix = hdrs(&[
            ("cookie", "CF_Authorization=C"),
            ("origin", "https://senn.example.com.evil.example"),
        ]);
        assert!(access_token(&prefix, &[SELF.into()]).is_err());
    }

    #[test]
    fn どちらも無ければ_失敗() {
        assert!(access_token(&HeaderMap::new(), &[SELF.into()]).is_err());
        let similar = hdrs(&[("cookie", "CF_AuthorizationX=C"), ("origin", SELF)]);
        assert!(access_token(&similar, &[SELF.into()]).is_err());
    }

    fn user(id: i32, username: &str, active: bool) -> User {
        User {
            id,
            username: username.into(),
            password_hash: String::new(),
            display_name: String::new(),
            alias: None,
            email: "a@example.com".into(),
            first_name: String::new(),
            last_name: String::new(),
            is_active: active,
            is_staff: false,
            is_system_admin: false,
            is_guest: false,
            must_change_password: false,
            email_notifications_enabled: true,
        }
    }

    #[test]
    fn 一人だけならそのまま入る() {
        assert_eq!(
            select_account(&[user(1, "a", true)], None, "ai_agent"),
            Selection::Login(1)
        );
    }

    #[test]
    fn 一致なしは入れない() {
        assert_eq!(select_account(&[], None, "ai_agent"), Selection::NoAccount);
        assert_eq!(
            select_account(&[user(1, "a", false)], None, "ai_agent"),
            Selection::NoAccount
        );
    }

    #[test]
    fn 複数なら選ばせ_候補の中からだけ受け付ける() {
        let us = [user(1, "boss", true), user(2, "staff", true)];
        assert_eq!(
            select_account(&us, None, "ai_agent"),
            Selection::Choose(vec![1, 2])
        );
        assert_eq!(
            select_account(&us, Some(2), "ai_agent"),
            Selection::Login(2)
        );
        assert_eq!(
            select_account(&us, Some(3), "ai_agent"),
            Selection::InvalidChoice
        );
    }

    #[test]
    fn 一人のときも候補外の指定は拒否する() {
        assert_eq!(
            select_account(&[user(1, "a", true)], Some(99), "ai_agent"),
            Selection::InvalidChoice
        );
    }

    #[test]
    fn 機械用アカウントには入らせない() {
        assert_eq!(
            select_account(&[user(9, "ai_agent", true)], None, "ai_agent"),
            Selection::NoAccount
        );
        assert_eq!(
            select_account(
                &[user(9, "ai_agent", true), user(1, "a", true)],
                Some(9),
                "ai_agent"
            ),
            Selection::InvalidChoice
        );
    }
}
