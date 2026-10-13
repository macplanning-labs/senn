//! 管理者のパスワードログインの次の段階(DEMO-000153 A-4b)。
//!
//! 管理者がパスワードでログインしたとき(`auth_api::login`)、パスキーを登録済みなら「パスキーの確認」、
//! 強制の期間で未登録なら「パスキーの新規登録」に進む。この 2 つの段階の API をここに置く。
//!
//! - 確認(`step_begin` / `step_complete`): そのユーザーの鍵だけを対象にした認証。TOTP の代わりの第 2 要素
//! - 新規登録(`enroll_begin` / `enroll_complete`): **別の管理者(または復旧コマンド)が、その人のために窓を
//!   開けたときだけ**。パスワードだけで鍵を登録できると、パスワードを知った攻撃者が自分の鍵を登録して管理者になれる
//! - WebAuthn の状態は、**サーバーが署名したもの**だけを受け付ける(`jwt_service::issue_webauthn_state_token`)。
//!   クライアントが作った状態に、攻撃者の公開鍵が入っていても通らないようにするため
//! - 確認の失敗は、確認コード(TOTP)と同じ、ユーザー単位の数え方(15 分に 10 回。A-1)で数える
//! - 失敗の理由は、外には細かく出さない(同じ文言)

use axum::{extract::State, http::HeaderMap, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;
use webauthn_rs::prelude::*;

use crate::domain::services::{auth_service, jwt_service};
use crate::infrastructure::repositories::user_repo;
use crate::presentation::handlers::auth_api::LoginResponse;
use crate::presentation::handlers::security_api::WIP_RP_NAME;
use crate::presentation::state::AppState;
use auth_core::domain::webauthn as webauthn_service;

const PURPOSE_AUTH: &str = "passkey_step_auth";
const PURPOSE_ENROLL: &str = "passkey_enroll_reg";
/// 状態トークンの有効期間(秒)。ブラウザで鍵を選ぶ時間を見込む
const STATE_TTL_SECONDS: i64 = 300;

const MSG_FAILED: &str = "パスキーの確認に失敗しました";

fn fail(status: StatusCode, msg: &str) -> axum::response::Response {
    (status, Json(json!({ "detail": msg }))).into_response()
}

fn server_error() -> axum::response::Response {
    fail(
        StatusCode::INTERNAL_SERVER_ERROR,
        "サーバーエラーが発生しました",
    )
}

#[derive(Deserialize)]
pub struct StepBeginRequest {
    pub passkey_token: String,
}

#[derive(Serialize)]
pub struct StepBeginResponse {
    pub request_challenge: RequestChallengeResponse,
    /// サーバーが署名した認証の状態。`step_complete` にそのまま送り返す
    pub auth_token: String,
}

/// POST /api/v1/auth/login/passkey/begin/ — 「パスキーの確認」の開始(公開ルート。段階のトークンで認証)
pub async fn step_begin(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<StepBeginRequest>,
) -> impl IntoResponse {
    let secret = &state.config.jwt_secret;
    let Ok(claims) = jwt_service::decode_passkey_step_token(&body.passkey_token, secret) else {
        return fail(StatusCode::UNAUTHORIZED, MSG_FAILED);
    };
    let Ok(user_id) = claims.user_id() else {
        return fail(StatusCode::UNAUTHORIZED, MSG_FAILED);
    };

    let passkeys = match user_repo::find_passkeys_by_user(&state.pool, user_id).await {
        Ok(p) if !p.is_empty() => p,
        Ok(_) => return fail(StatusCode::UNAUTHORIZED, MSG_FAILED),
        Err(e) => {
            tracing::error!(error = ?e, "パスキーの取得に失敗しました");
            return server_error();
        }
    };
    let Ok(webauthn) = webauthn_service::create_webauthn_from_headers(&headers, WIP_RP_NAME) else {
        return server_error();
    };
    let (rcr, auth_state) =
        match webauthn_service::start_authentication_with_credentials(&webauthn, &passkeys) {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(error = ?e, "パスキーの確認の開始に失敗しました");
                return server_error();
            }
        };
    let Ok(state_json) = serde_json::to_string(&auth_state) else {
        return server_error();
    };
    let Ok(auth_token) = jwt_service::issue_webauthn_state_token(
        user_id,
        PURPOSE_AUTH,
        &state_json,
        secret,
        STATE_TTL_SECONDS,
    ) else {
        return server_error();
    };
    (
        StatusCode::OK,
        Json(StepBeginResponse {
            request_challenge: rcr,
            auth_token,
        }),
    )
        .into_response()
}

#[derive(Deserialize)]
pub struct StepCompleteRequest {
    pub passkey_token: String,
    pub auth_token: String,
    pub credential: PublicKeyCredential,
}

/// POST /api/v1/auth/login/passkey/verify/ — 「パスキーの確認」の完了。成功したらログインする
pub async fn step_complete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<StepCompleteRequest>,
) -> impl IntoResponse {
    let secret = &state.config.jwt_secret;
    let Ok(claims) = jwt_service::decode_passkey_step_token(&body.passkey_token, secret) else {
        return fail(StatusCode::UNAUTHORIZED, MSG_FAILED);
    };
    let Ok(user_id) = claims.user_id() else {
        return fail(StatusCode::UNAUTHORIZED, MSG_FAILED);
    };
    // 署名・期限・用途・ユーザーを確かめた状態だけを使う(クライアントが作った状態は通らない)
    let Ok(state_json) =
        jwt_service::decode_webauthn_state_token(&body.auth_token, PURPOSE_AUTH, user_id, secret)
    else {
        return fail(StatusCode::UNAUTHORIZED, MSG_FAILED);
    };
    let Ok(auth_state) = serde_json::from_str::<PasskeyAuthentication>(&state_json) else {
        return fail(StatusCode::UNAUTHORIZED, MSG_FAILED);
    };

    // 検証する前に、試行を 1 回予約する(確認コードと同じ、ユーザー単位の数え方)
    match user_repo::totp_reserve_attempt(
        &state.pool,
        user_id,
        auth_service::TOTP_MAX_ATTEMPTS,
        chrono::Duration::minutes(auth_service::TOTP_LOCK_WINDOW_MINUTES),
    )
    .await
    {
        Ok(user_repo::TotpAttemptReservation::Allowed) => {}
        Ok(user_repo::TotpAttemptReservation::Locked) => {
            tracing::warn!(
                user_id,
                "パスキーの確認の試行が上限に達したため、一時的に止めています"
            );
            return fail(StatusCode::UNAUTHORIZED, MSG_FAILED);
        }
        Err(e) => {
            tracing::error!(error = ?e, "試行の予約に失敗しました");
            return server_error();
        }
    }

    let Ok(webauthn) = webauthn_service::create_webauthn_from_headers(&headers, WIP_RP_NAME) else {
        return server_error();
    };
    let auth_result = match webauthn_service::finish_authentication_with_credentials(
        &webauthn,
        &auth_state,
        &body.credential,
    ) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = ?e, user_id, "パスキーの確認に失敗しました");
            return fail(StatusCode::UNAUTHORIZED, MSG_FAILED);
        }
    };

    let user = match user_repo::find_by_id(&state.pool, user_id).await {
        Ok(Some(u)) if u.is_active => u,
        _ => return fail(StatusCode::UNAUTHORIZED, MSG_FAILED),
    };

    // sign_count の更新(ベストエフォート。同期パスキーではずれることがあるので、失敗してもログインは続ける)
    if let Ok(passkeys) = user_repo::find_passkeys_by_user(&state.pool, user_id).await {
        let cred_id: Vec<u8> = auth_result.cred_id().as_ref().to_vec();
        if let Some(matching) = passkeys
            .iter()
            .find(|pk| pk.cred_id().as_ref() == cred_id.as_slice())
        {
            let mut updated = matching.clone();
            updated.update_credential(&auth_result);
            if let Ok(json) = webauthn_service::passkey_to_json_string(&updated) {
                if let Err(e) =
                    user_repo::update_webauthn_passkey_json(&state.pool, &cred_id, &json).await
                {
                    tracing::warn!(error = ?e, "sign_count の更新に失敗(ログインは継続)");
                }
            }
        }
    }

    // 成功したので、数を消してログインする
    if let Err(e) = user_repo::totp_reset_attempts(&state.pool, user.id).await {
        tracing::warn!(error = ?e, "試行の数の削除に失敗(ログインは継続)");
    }
    issue_login(&state, user.id)
}

fn issue_login(state: &AppState, user_id: i32) -> axum::response::Response {
    match jwt_service::issue_token_pair(
        user_id,
        &state.config.jwt_secret,
        state.config.access_token_lifetime_minutes,
        state.config.refresh_token_lifetime_days,
    ) {
        Ok(pair) => (
            StatusCode::OK,
            Json(LoginResponse {
                access: pair.access,
                refresh: pair.refresh,
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = ?e, "トークン発行エラー");
            fail(StatusCode::INTERNAL_SERVER_ERROR, "トークン発行エラー")
        }
    }
}

// ======== 新規登録 ========

#[derive(Deserialize)]
pub struct EnrollBeginRequest {
    pub enroll_token: String,
}

#[derive(Serialize)]
pub struct EnrollBeginResponse {
    pub creation_challenge: CreationChallengeResponse,
    /// サーバーが署名した登録の状態。`enroll_complete` にそのまま送り返す
    pub registration_token: String,
}

/// 登録の段階のトークンと、窓が開いていることを確かめて、ユーザー ID を返す
async fn check_enroll(
    state: &AppState,
    enroll_token: &str,
) -> Result<i32, axum::response::Response> {
    let claims = jwt_service::decode_enroll_token(enroll_token, &state.config.jwt_secret)
        .map_err(|_| fail(StatusCode::UNAUTHORIZED, MSG_FAILED))?;
    let user_id = claims
        .user_id()
        .map_err(|_| fail(StatusCode::UNAUTHORIZED, MSG_FAILED))?;
    match user_repo::passkey_enroll_window_open(&state.pool, user_id).await {
        Ok(true) => Ok(user_id),
        Ok(false) => Err(fail(
            StatusCode::FORBIDDEN,
            "パスキーを新しく登録できる状態ではありません。別のシステム管理者に、パスキーのリセットを依頼してください",
        )),
        Err(e) => {
            tracing::error!(error = ?e, "登録の窓の確認に失敗しました");
            Err(server_error())
        }
    }
}

/// POST /api/v1/auth/login/passkey-enroll/begin/ — パスキーの新規登録の開始
pub async fn enroll_begin(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<EnrollBeginRequest>,
) -> impl IntoResponse {
    let user_id = match check_enroll(&state, &body.enroll_token).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    let Ok(webauthn) = webauthn_service::create_webauthn_from_headers(&headers, WIP_RP_NAME) else {
        return server_error();
    };
    // 既存の登録(security_api::passkey_register_begin)と同じ、ユーザー ID の埋め込み方
    let (ccr, reg_state) = match webauthn_service::start_registration(
        &webauthn,
        Uuid::from_u128(user_id as u128),
        &user_id.to_string(),
        &format!("パスキー_{}", user_id),
        None,
    ) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = ?e, "パスキーの登録の開始に失敗しました");
            return server_error();
        }
    };
    let Ok(state_json) = webauthn_service::registration_state_to_json_string(&reg_state) else {
        return server_error();
    };
    let Ok(registration_token) = jwt_service::issue_webauthn_state_token(
        user_id,
        PURPOSE_ENROLL,
        &state_json,
        &state.config.jwt_secret,
        STATE_TTL_SECONDS,
    ) else {
        return server_error();
    };
    (
        StatusCode::OK,
        Json(EnrollBeginResponse {
            creation_challenge: ccr,
            registration_token,
        }),
    )
        .into_response()
}

#[derive(Deserialize)]
pub struct EnrollCompleteRequest {
    pub enroll_token: String,
    pub registration_token: String,
    pub credential: RegisterPublicKeyCredential,
}

/// POST /api/v1/auth/login/passkey-enroll/complete/ — パスキーの新規登録の完了。成功したらログインする
pub async fn enroll_complete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<EnrollCompleteRequest>,
) -> impl IntoResponse {
    let user_id = match check_enroll(&state, &body.enroll_token).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    let Ok(state_json) = jwt_service::decode_webauthn_state_token(
        &body.registration_token,
        PURPOSE_ENROLL,
        user_id,
        &state.config.jwt_secret,
    ) else {
        return fail(StatusCode::UNAUTHORIZED, MSG_FAILED);
    };
    let Ok(reg_state) = webauthn_service::registration_state_from_json_string(&state_json) else {
        return fail(StatusCode::UNAUTHORIZED, MSG_FAILED);
    };
    let Ok(webauthn) = webauthn_service::create_webauthn_from_headers(&headers, WIP_RP_NAME) else {
        return server_error();
    };
    let passkey =
        match webauthn_service::finish_registration(&webauthn, &reg_state, &body.credential) {
            Ok(pk) => pk,
            Err(e) => {
                tracing::warn!(error = ?e, user_id, "パスキーの登録の検証に失敗しました");
                return fail(StatusCode::BAD_REQUEST, "パスキーの検証に失敗しました");
            }
        };
    let credential_id = webauthn_service::credential_id_from_passkey(&passkey);
    let Ok(passkey_json) = webauthn_service::passkey_to_json_string(&passkey) else {
        return server_error();
    };
    if let Err(e) = user_repo::save_webauthn_credential_with_json(
        &state.pool,
        user_id,
        &credential_id,
        &credential_id,
        &passkey_json,
        "パスキー",
    )
    .await
    {
        tracing::error!(error = ?e, "パスキーの保存に失敗しました");
        return server_error();
    }
    // 登録できたので、窓を閉じる(同じ窓で、もう一度登録されないように)
    if let Err(e) = user_repo::close_passkey_enroll_window(&state.pool, user_id).await {
        tracing::error!(error = ?e, "登録の窓を閉じられませんでした");
    }
    tracing::warn!(
        event = "admin_passkey_enrolled",
        user_id,
        "強制の期間に、管理者がパスキーを新規登録しました"
    );
    issue_login(&state, user_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::auth_service::{
        admin_login_policy, AdminLoginPolicy, AdminPasskeyStatus, ADMIN_PASSKEY_ENFORCE_AFTER_KEY,
    };
    use crate::infrastructure::repositories::system_settings_repo;
    use crate::presentation::handlers::auth::{login_submit, LoginForm};
    use crate::presentation::handlers::auth_api::{login, LoginRequest};
    use crate::test_support::{
        add_test_team_member, create_test_team, create_test_user, test_pool, test_state,
        unique_suffix,
    };
    use axum::body::to_bytes;
    use axum::extract::Form;
    use chrono::{Duration, Utc};

    const PASSWORD: &str = "test-pass-for-a4b";

    fn st(
        is_admin: bool,
        has_passkey: bool,
        enforce_after: Option<chrono::DateTime<Utc>>,
    ) -> AdminPasskeyStatus {
        AdminPasskeyStatus {
            is_admin,
            has_passkey,
            enforce_after,
        }
    }

    #[test]
    fn policy_table() {
        let now = Utc::now();
        let past = Some(now - Duration::days(1));
        let future = Some(now + Duration::days(1));
        // 管理者でない: いつでも今までどおり
        assert_eq!(
            admin_login_policy(&st(false, false, past), now),
            AdminLoginPolicy::Normal
        );
        assert_eq!(
            admin_login_policy(&st(false, true, past), now),
            AdminLoginPolicy::Normal
        );
        // 管理者で登録済み: 強制の前でも後でも、パスキーの確認(強制の開始日を待たずに、登録した人から守る)
        assert_eq!(
            admin_login_policy(&st(true, true, None), now),
            AdminLoginPolicy::RequirePasskey
        );
        assert_eq!(
            admin_login_policy(&st(true, true, future), now),
            AdminLoginPolicy::RequirePasskey
        );
        assert_eq!(
            admin_login_policy(&st(true, true, past), now),
            AdminLoginPolicy::RequirePasskey
        );
        // 管理者で未登録: 猶予の間(未設定・開始前)は今までどおり。強制の期間は、新規登録へ
        assert_eq!(
            admin_login_policy(&st(true, false, None), now),
            AdminLoginPolicy::Normal
        );
        assert_eq!(
            admin_login_policy(&st(true, false, future), now),
            AdminLoginPolicy::Normal
        );
        assert_eq!(
            admin_login_policy(&st(true, false, past), now),
            AdminLoginPolicy::EnrollRequired
        );
    }

    #[test]
    fn step_tokens_are_not_interchangeable() {
        let secret = "test-secret";
        let step = jwt_service::issue_passkey_step_token(7, secret, 60).unwrap();
        let enroll = jwt_service::issue_enroll_token(7, secret, 60).unwrap();
        let mfa = jwt_service::issue_mfa_token(7, secret, 60).unwrap();
        let pair = jwt_service::issue_token_pair(7, secret, 5, 1).unwrap();
        // それぞれ、自分の用途でだけ読める
        assert!(jwt_service::decode_passkey_step_token(&step, secret).is_ok());
        assert!(jwt_service::decode_enroll_token(&enroll, secret).is_ok());
        assert!(jwt_service::decode_passkey_step_token(&enroll, secret).is_err());
        assert!(jwt_service::decode_enroll_token(&step, secret).is_err());
        assert!(jwt_service::decode_passkey_step_token(&mfa, secret).is_err());
        assert!(jwt_service::decode_passkey_step_token(&pair.access, secret).is_err());
        assert!(jwt_service::decode_enroll_token(&pair.access, secret).is_err());
        // 段階のトークンは、API の認証に使えない(Access ではない)
        assert_ne!(
            jwt_service::decode_token(&step, secret).unwrap().token_type,
            jwt_service::TokenType::Access
        );
        assert_ne!(
            jwt_service::decode_token(&enroll, secret)
                .unwrap()
                .token_type,
            jwt_service::TokenType::Access
        );
        // MFA 用のトークンでは、パスキーの段階に進めない
        assert!(jwt_service::decode_mfa_token(&step, secret).is_err());
    }

    #[test]
    fn webauthn_state_token_is_bound_to_purpose_user_and_signature() {
        let secret = "test-secret";
        let t = jwt_service::issue_webauthn_state_token(7, PURPOSE_AUTH, "{\"k\":1}", secret, 60)
            .unwrap();
        assert_eq!(
            jwt_service::decode_webauthn_state_token(&t, PURPOSE_AUTH, 7, secret).unwrap(),
            "{\"k\":1}"
        );
        // 用途・ユーザーが違う / 署名の鍵が違う / 期限切れ は通らない
        assert!(jwt_service::decode_webauthn_state_token(&t, PURPOSE_ENROLL, 7, secret).is_err());
        assert!(jwt_service::decode_webauthn_state_token(&t, PURPOSE_AUTH, 8, secret).is_err());
        assert!(
            jwt_service::decode_webauthn_state_token(&t, PURPOSE_AUTH, 7, "other-secret").is_err()
        );
        let expired =
            jwt_service::issue_webauthn_state_token(7, PURPOSE_AUTH, "{}", secret, -120).unwrap();
        assert!(
            jwt_service::decode_webauthn_state_token(&expired, PURPOSE_AUTH, 7, secret).is_err()
        );
        // クライアントが作った(署名の無い)状態は通らない
        assert!(jwt_service::decode_webauthn_state_token(
            "{\"state\":\"x\"}",
            PURPOSE_AUTH,
            7,
            secret
        )
        .is_err());
    }

    async fn user_with_password(pool: &sqlx::PgPool, prefix: &str) -> (i32, String) {
        let id = create_test_user(pool, prefix).await;
        let hash = auth_service::hash_password(PASSWORD).unwrap();
        sqlx::query("UPDATE accounts_user SET password = $2 WHERE id = $1")
            .bind(id as i64)
            .bind(hash)
            .execute(pool)
            .await
            .unwrap();
        let username: String =
            sqlx::query_scalar("SELECT username FROM accounts_user WHERE id = $1")
                .bind(id as i64)
                .fetch_one(pool)
                .await
                .unwrap();
        (id, username)
    }

    async fn make_system_admin(pool: &sqlx::PgPool, id: i32) {
        sqlx::query(
            "UPDATE accounts_user SET is_staff = true, is_system_admin = true WHERE id = $1",
        )
        .bind(id as i64)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn add_passkey(pool: &sqlx::PgPool, user_id: i32) {
        // 確認に使える行(passkey_json あり)。中身は、ログインの分岐の試験には使わない
        sqlx::query(
            "INSERT INTO mfa_webauthn_credential (credential_id, public_key, passkey_json, sign_count, name, created_at, user_id)
             VALUES ($1, $2, '{}', 0, 'test', now(), $3)",
        )
        .bind(format!("cred-{}", unique_suffix()).into_bytes())
        .bind(vec![1u8])
        .bind(user_id as i64)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn set_enforce_after(pool: &sqlx::PgPool, v: Option<chrono::DateTime<Utc>>) {
        match v {
            Some(t) => {
                system_settings_repo::set(pool, ADMIN_PASSKEY_ENFORCE_AFTER_KEY, &t.to_rfc3339())
                    .await
                    .unwrap()
            }
            None => system_settings_repo::delete(pool, ADMIN_PASSKEY_ENFORCE_AFTER_KEY)
                .await
                .unwrap(),
        }
    }

    async fn do_login(
        state: &AppState,
        username: &str,
        password: &str,
    ) -> (StatusCode, serde_json::Value) {
        let resp = login(
            State(state.clone()),
            Json(LoginRequest {
                username: username.to_string(),
                password: password.to_string(),
            }),
        )
        .await
        .into_response();
        let status = resp.status();
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or_default())
    }

    /// 設定(システム全体の 1 つの値)を触る試験は、互いに干渉しないよう、まとめて 1 つの試験で順に確かめる
    #[tokio::test]
    async fn password_login_flow_for_admins_and_non_admins() {
        let Some(pool) = test_pool().await else {
            return;
        };
        // 強制の開始日(システム全体の 1 つの設定)を書き換えるので、同じ設定を触るほかの試験と重ならないようにする
        let _guard = crate::test_support::ADMIN_PASSKEY_SETTING_LOCK.lock().await;
        let state = test_state(&pool).await;
        let now = Utc::now();

        let (plain, plain_name) = user_with_password(&pool, "a4b-plain").await;
        let (admin, admin_name) = user_with_password(&pool, "a4b-admin").await;
        make_system_admin(&pool, admin).await;
        let (owner, owner_name) = user_with_password(&pool, "a4b-owner").await;
        let team = create_test_team(&pool, "a4b-team").await;
        add_test_team_member(&pool, team, owner, "admin").await;

        // 強制の開始日が未設定: 未登録の管理者は、今までどおり(パスワードだけで入れる)
        set_enforce_after(&pool, None).await;
        let (st_, body) = do_login(&state, &admin_name, PASSWORD).await;
        assert_eq!(st_, StatusCode::OK);
        assert!(body.get("access").is_some() && body.get("passkey_required").is_none());

        // 管理者でない人は、パスキーを持っていても、今までどおり
        add_passkey(&pool, plain).await;
        let (st_, body) = do_login(&state, &plain_name, PASSWORD).await;
        assert_eq!(st_, StatusCode::OK);
        assert!(body.get("access").is_some() && body.get("passkey_required").is_none());

        // 管理者でパスキーを登録済み: 強制の開始日を待たずに、パスキーの確認に進む(アクセストークンは出ない)
        add_passkey(&pool, admin).await;
        let (st_, body) = do_login(&state, &admin_name, PASSWORD).await;
        assert_eq!(st_, StatusCode::OK);
        assert_eq!(body["passkey_required"], true);
        assert!(body.get("access").is_none() && body.get("refresh").is_none());
        let step = body["passkey_token"].as_str().unwrap();
        assert!(jwt_service::decode_passkey_step_token(step, &state.config.jwt_secret).is_ok());

        // TOTP を登録していても、パスキーの確認が先(TOTP の段階には進まない)
        sqlx::query(
            "INSERT INTO mfa_totp_device (secret, confirmed, created_at, user_id) VALUES ('JBSWY3DPEHPK3PXP', true, now(), $1)",
        )
        .bind(admin as i64)
        .execute(&pool)
        .await
        .unwrap();
        let (_, body) = do_login(&state, &admin_name, PASSWORD).await;
        assert_eq!(body["passkey_required"], true);
        assert!(body.get("mfa_required").is_none());

        // パスワードが違えば、今までどおり 401
        let (st_, _) = do_login(&state, &admin_name, "wrong-password").await;
        assert_eq!(st_, StatusCode::UNAUTHORIZED);

        // チーム Owner も管理者として扱う。強制の期間で未登録なら、窓が無い限り入れない(403)
        set_enforce_after(&pool, Some(now - Duration::days(1))).await;
        let (st_, body) = do_login(&state, &owner_name, PASSWORD).await;
        assert_eq!(st_, StatusCode::FORBIDDEN);
        assert_eq!(body["code"], "passkey_enroll_not_allowed");
        assert!(body.get("access").is_none() && body.get("enroll_token").is_none());

        // 別の管理者が窓を開けると、新規登録に進める(登録の段階のトークンが出る)
        user_repo::open_passkey_enroll_window(&pool, owner, 24)
            .await
            .unwrap();
        let (st_, body) = do_login(&state, &owner_name, PASSWORD).await;
        assert_eq!(st_, StatusCode::OK);
        assert_eq!(body["passkey_enroll_required"], true);
        let enroll = body["enroll_token"].as_str().unwrap().to_string();
        assert!(jwt_service::decode_enroll_token(&enroll, &state.config.jwt_secret).is_ok());

        // 窓を閉じると、登録の段階の API も通らない(トークンを持っていても)
        user_repo::close_passkey_enroll_window(&pool, owner)
            .await
            .unwrap();
        let resp = enroll_begin(
            State(state.clone()),
            HeaderMap::new(),
            Json(EnrollBeginRequest {
                enroll_token: enroll.clone(),
            }),
        )
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        // 窓が期限切れなら、同じ
        sqlx::query("UPDATE accounts_user SET passkey_enroll_until = now() - interval '1 second' WHERE id = $1")
            .bind(owner as i64)
            .execute(&pool)
            .await
            .unwrap();
        assert!(!user_repo::passkey_enroll_window_open(&pool, owner)
            .await
            .unwrap());

        // 強制の期間でも、パスキー登録済みの管理者は、パスキーの確認に進む(登録の流れではない)
        let (_, body) = do_login(&state, &admin_name, PASSWORD).await;
        assert_eq!(body["passkey_required"], true);

        // Cookie 方式のログイン画面は、管理者のパスワードだけの入口にしない(パスキーの確認の段階が無いので断る)
        let resp = login_submit(
            State(state.clone()),
            Form(LoginForm {
                username: admin_name.clone(),
                password: PASSWORD.to_string(),
            }),
        )
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        // 管理者でない人は、今までどおり入れる(リダイレクト)
        let resp = login_submit(
            State(state.clone()),
            Form(LoginForm {
                username: plain_name.clone(),
                password: PASSWORD.to_string(),
            }),
        )
        .await
        .into_response();
        assert!(resp.status().is_redirection(), "status = {}", resp.status());

        set_enforce_after(&pool, None).await;
    }

    #[tokio::test]
    async fn step_endpoints_reject_wrong_or_forged_tokens() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let state = test_state(&pool).await;
        let secret = state.config.jwt_secret.clone();
        let (user, _) = user_with_password(&pool, "a4b-tok").await;

        // 段階のトークンではないもの(アクセストークン・MFA トークン・登録のトークン)では、確認を始められない
        let access = jwt_service::issue_token_pair(user, &secret, 5, 1)
            .unwrap()
            .access;
        let mfa = jwt_service::issue_mfa_token(user, &secret, 60).unwrap();
        let enroll = jwt_service::issue_enroll_token(user, &secret, 60).unwrap();
        for bad in [access, mfa, enroll, "garbage".to_string()] {
            let resp = step_begin(
                State(state.clone()),
                HeaderMap::new(),
                Json(StepBeginRequest { passkey_token: bad }),
            )
            .await
            .into_response();
            assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        }
        // パスキーを持たない人は、正しい段階のトークンがあっても始められない
        let step = jwt_service::issue_passkey_step_token(user, &secret, 60).unwrap();
        let resp = step_begin(
            State(state.clone()),
            HeaderMap::new(),
            Json(StepBeginRequest {
                passkey_token: step,
            }),
        )
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        // 段階のトークンではないもの(アクセストークン)では、登録も始められない
        let access = jwt_service::issue_token_pair(user, &secret, 5, 1)
            .unwrap()
            .access;
        let resp = enroll_begin(
            State(state.clone()),
            HeaderMap::new(),
            Json(EnrollBeginRequest {
                enroll_token: access,
            }),
        )
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
