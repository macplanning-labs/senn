/// presentation/handlers/security_api.rs — TOTP エンロールメントAPI
///
/// TOTP多要素認証の設定・確認・無効化。

use axum::{
    extract::{State, Path},
    response::IntoResponse,
    http::StatusCode,
    Json,
    Extension,
};
use serde::{Deserialize, Serialize};
use crate::presentation::state::AppState;
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::infrastructure::repositories::user_repo;
use crate::domain::services::auth_service;
// Step 2: totp_service / webauthn_service / jwt_service は auth-core クレートへ移行済み。
// クレーム形状・関数シグネチャがほぼ同一のため、モジュールエイリアスとして差し替える。
// ただし totp::generate_qr_base64 / verify_code は issuer 引数が増え、
// webauthn::create_webauthn_from_headers は rp_name 引数が増え、
// webauthn::start_registration の user_id は i32 ではなく Uuid を要求するため、
// 該当の呼び出し箇所のみ引数を追加している。
use auth_core::domain::totp as totp_service;
use auth_core::domain::webauthn as webauthn_service;
use crate::domain::services::jwt_service;
use webauthn_rs::prelude::*;

/// WebAuthnのRP名 / TOTPのissuer名。移植元のWIP実装が固定していた値
/// （旧 webauthn_service.rs の rp_name、旧 totp_service.rs の issuer="WIP"）を踏襲する。
pub(crate) const WIP_RP_NAME: &str = "WIP — プロジェクト管理ツール";
const WIP_TOTP_ISSUER: &str = "WIP";

// =============================================================================
// リクエスト・レスポンス構造体
// =============================================================================

#[derive(Serialize)]
pub struct TotpBeginResponse {
    pub secret_b64: String,
    pub secret_base32: String,
    pub qr_base64: String,
}

#[derive(Deserialize)]
pub struct TotpConfirmRequest {
    pub secret_b64: String,
    pub code: String,
}

#[derive(Serialize)]
pub struct TotpConfirmResponse {
    pub ok: bool,
}

#[derive(Deserialize)]
pub struct TotpDisableRequest {
    pub password: Option<String>,
    pub totp_code: Option<String>,
}

#[derive(Serialize)]
pub struct TotpDisableResponse {
    pub ok: bool,
}

// =============================================================================
// パスキー（WebAuthn）リクエスト・レスポンス
// =============================================================================

#[derive(Serialize)]
pub struct PasskeyRegisterBeginResponse {
    /// CreationChallengeResponse（navigator.credentials.create() に渡す）
    pub creation_challenge: CreationChallengeResponse,
    /// PasskeyRegistration state をクライアントが echo-back するための JSON
    pub registration_state_json: String,
}

#[derive(Deserialize)]
pub struct PasskeyRegisterCompleteRequest {
    /// クライアントが echo-back する registration state（JSON 文字列）
    pub registration_state_json: String,
    /// ブラウザが返した RegisterPublicKeyCredential（JSON）
    pub credential: RegisterPublicKeyCredential,
}

#[derive(Serialize)]
pub struct PasskeyRegisterCompleteResponse {
    pub ok: bool,
    pub message: String,
}

#[derive(Serialize)]
pub struct PasskeySummary {
    pub id: i32,
    pub name: String,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct PasskeyListResponse {
    pub passkeys: Vec<PasskeySummary>,
}

// =============================================================================
// ハンドラー実装
// =============================================================================

/// POST /api/v1/settings/security/totp/begin
/// 新規 TOTP 秘密鍵を生成して QR コード + Base32 を返す。
/// 秘密鍵はまだ DB に保存しない（stateless）。
pub async fn totp_begin(
    Extension(auth_user): Extension<AuthUser>,
) -> impl IntoResponse {
    let secret_bytes = totp_service::generate_secret();

    let qr_base64 = match totp_service::generate_qr_base64(&secret_bytes, WIP_TOTP_ISSUER, &auth_user.user_id.to_string()) {
        Ok(qr) => qr,
        Err(e) => {
            tracing::error!("QR code generation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"detail": "QRコード生成エラー"})),
            ).into_response();
        }
    };

    let secret_b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        &secret_bytes,
    );
    let secret_base32 = totp_service::secret_to_base32(&secret_bytes);

    (
        StatusCode::OK,
        Json(TotpBeginResponse {
            secret_b64,
            secret_base32,
            qr_base64,
        }),
    ).into_response()
}

/// POST /api/v1/settings/security/totp/confirm
/// クライアントが秘密鍵と確認コードを送り返す → 検証 → 暗号化して保存。
pub async fn totp_confirm(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Json(body): Json<TotpConfirmRequest>,
) -> impl IntoResponse {
    // Base64 デコード
    let secret_bytes = match base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        &body.secret_b64,
    ) {
        Ok(b) => b,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"detail": "不正な秘密鍵です"})),
            ).into_response();
        }
    };

    // TOTP コード検証
    match totp_service::verify_code(&secret_bytes, WIP_TOTP_ISSUER, &auth_user.user_id.to_string(), &body.code) {
        Ok(true) => {}
        Ok(false) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"detail": "確認コードが正しくありません"})),
            ).into_response();
        }
        Err(e) => {
            tracing::error!("TOTP verification failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"detail": "TOTP検証エラー"})),
            ).into_response();
        }
    }

    // 秘密鍵を暗号化
    let encrypted = match totp_service::encrypt_secret(&secret_bytes, &state.config.jwt_secret) {
        Ok(e) => e,
        Err(e) => {
            tracing::error!("Encryption failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"detail": "暗号化エラー"})),
            ).into_response();
        }
    };

    // DB に保存
    if let Err(e) = user_repo::save_totp(&state.pool, auth_user.user_id, &encrypted.blob_b64).await {
        tracing::error!("Failed to save TOTP: {:?}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"detail": "TOTP設定の保存に失敗しました"})),
        ).into_response();
    }

    // confirmed フラグを true にする
    if let Err(e) = user_repo::confirm_totp(&state.pool, auth_user.user_id).await {
        tracing::error!("Failed to confirm TOTP: {:?}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"detail": "TOTP確認に失敗しました"})),
        ).into_response();
    }

    (StatusCode::OK, Json(TotpConfirmResponse { ok: true })).into_response()
}

/// POST /api/v1/settings/security/totp/disable
/// TOTP 多要素認証を無効化する。
pub async fn totp_disable(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Json(body): Json<TotpDisableRequest>,
) -> impl IntoResponse {
    // 無効化はセッション乗っ取りだけでは実行できないよう、現在のパスワード
    // またはTOTPコードのいずれかによる再確認を必須とする。
    let mut verified = false;

    if let Some(password) = body.password.as_deref().filter(|p| !p.is_empty()) {
        match user_repo::find_by_id(&state.pool, auth_user.user_id).await {
            Ok(Some(user)) => {
                match auth_service::verify_password(&state.pool, user.id, password, &user.password_hash).await {
                    Ok(true) => verified = true,
                    Ok(false) => {}
                    Err(e) => {
                        tracing::error!("Password verification failed: {:?}", e);
                        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"detail": "サーバーエラーが発生しました"}))).into_response();
                    }
                }
            }
            Ok(None) => {
                return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"detail": "ユーザーが見つかりません"}))).into_response();
            }
            Err(e) => {
                tracing::error!("DB operation failed: {:?}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"detail": "サーバーエラーが発生しました"}))).into_response();
            }
        }
    }

    if !verified {
        if let Some(code) = body.totp_code.as_deref().filter(|c| !c.is_empty()) {
            match user_repo::find_totp(&state.pool, auth_user.user_id).await {
                Ok(Some(device)) if device.confirmed => {
                    match totp_service::decrypt_secret(&device.secret, &state.config.jwt_secret) {
                        Ok(secret_bytes) => {
                            match totp_service::verify_code(&secret_bytes, WIP_TOTP_ISSUER, &auth_user.user_id.to_string(), code) {
                                Ok(true) => verified = true,
                                Ok(false) => {}
                                Err(e) => {
                                    tracing::error!("TOTP verification failed: {:?}", e);
                                    return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"detail": "サーバーエラーが発生しました"}))).into_response();
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("TOTP secret decryption failed: {:?}", e);
                            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"detail": "サーバーエラーが発生しました"}))).into_response();
                        }
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::error!("DB operation failed: {:?}", e);
                    return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"detail": "サーバーエラーが発生しました"}))).into_response();
                }
            }
        }
    }

    if !verified {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"detail": "現在のパスワードまたは確認コードを入力してください"})),
        ).into_response();
    }

    if let Err(e) = user_repo::delete_totp(&state.pool, auth_user.user_id).await {
        tracing::error!("Failed to delete TOTP: {:?}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"detail": "TOTP削除に失敗しました"})),
        ).into_response();
    }

    (StatusCode::OK, Json(TotpDisableResponse { ok: true })).into_response()
}

// =============================================================================
// パスキー（WebAuthn）ハンドラー実装
// =============================================================================

/// POST /api/v1/settings/security/passkey/register/begin
/// パスキー登録を開始する。
/// Webauthn instance をリクエストヘッダーから建てて、
/// CreationChallengeResponse と PasskeyRegistration state を返す。
/// state はクライアントが echo-back するための JSON として返される（stateless パターン）。
pub async fn passkey_register_begin(
    Extension(auth_user): Extension<AuthUser>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    // リクエストヘッダーから WebAuthn インスタンスを構築
    let webauthn = match webauthn_service::create_webauthn_from_headers(&headers, WIP_RP_NAME) {
        Ok(w) => w,
        Err(e) => {
            tracing::error!("WebAuthn初期化エラー: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"detail": "WebAuthn初期化エラー"})),
            ).into_response();
        }
    };

    // 既存パスキーを取得（exclude list 構築用）
    // NOTE: WIP の DB schema では credential_id と public_key をバイナリで保存するため、
    // 完全な Passkey を復元することはできない。exclude list は省略（同じ端末から複数登録可能）。
    let existing_credentials = None;

    // パスキー登録を開始
    // auth-core の start_registration は user_id: Uuid を要求する(アプリのユーザーID表現を
    // 知らない汎用設計のため)。WIP側のi32 user_idは、ログイン検証(identify_authentication)側で
    // 従来から使っているUuid::from_u128(user_id as u128)と対になる変換で埋め込む。
    match webauthn_service::start_registration(
        &webauthn,
        Uuid::from_u128(auth_user.user_id as u128),
        &auth_user.user_id.to_string(),
        &format!("パスキー_{}", auth_user.user_id),
        existing_credentials,
    ) {
        Ok((challenge_response, reg_state)) => {
            // PasskeyRegistration state を JSON 文字列にシリアライズ
            let reg_state_json = match webauthn_service::registration_state_to_json_string(&reg_state) {
                Ok(json) => json,
                Err(e) => {
                    tracing::error!("Registration state JSON化エラー: {:?}", e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"detail": "Registration state の保存に失敗"})),
                    ).into_response();
                }
            };

            (
                StatusCode::OK,
                Json(PasskeyRegisterBeginResponse {
                    creation_challenge: challenge_response,
                    registration_state_json: reg_state_json,
                }),
            ).into_response()
        }
        Err(e) => {
            tracing::error!("パスキー登録開始エラー: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"detail": format!("{}", e)})),
            ).into_response()
        }
    }
}

/// POST /api/v1/settings/security/passkey/register/complete
/// パスキー登録を完了する。
/// クライアントが echo-back した registration state とブラウザのクレデンシャルレスポンスを受け取り、
/// 登録を確認して DB に保存。
pub async fn passkey_register_complete(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    headers: axum::http::HeaderMap,
    Json(body): Json<PasskeyRegisterCompleteRequest>,
) -> impl IntoResponse {
    // リクエストヘッダーから WebAuthn インスタンスを構築
    let webauthn = match webauthn_service::create_webauthn_from_headers(&headers, WIP_RP_NAME) {
        Ok(w) => w,
        Err(e) => {
            tracing::error!("WebAuthn初期化エラー: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"detail": "WebAuthn初期化エラー"})),
            ).into_response();
        }
    };

    // クライアントが echo-back した registration state を復元
    let reg_state = match webauthn_service::registration_state_from_json_string(&body.registration_state_json) {
        Ok(state) => state,
        Err(e) => {
            tracing::error!("Registration state 復元エラー: {:?}", e);
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"detail": "不正な登録セッション"})),
            ).into_response();
        }
    };

    // 登録完了（WebAuthn の暗号検証が実行される）
    let passkey = match webauthn_service::finish_registration(&webauthn, &reg_state, &body.credential) {
        Ok(pk) => pk,
        Err(e) => {
            tracing::error!("パスキー登録完了エラー: {:?}", e);
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"detail": "パスキーの検証に失敗しました"})),
            ).into_response();
        }
    };

    // credential_id をバイナリで抽出
    let credential_id = webauthn_service::credential_id_from_passkey(&passkey);

    // Passkey 全体を JSON 文字列にシリアライズ
    let passkey_json_str = match webauthn_service::passkey_to_json_string(&passkey) {
        Ok(json) => json,
        Err(e) => {
            tracing::error!("Passkey JSON シリアライズエラー: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"detail": "クレデンシャル処理エラー"})),
            ).into_response();
        }
    };

    // credential_id のバイナリ表現（WIP schema では public_key として保存、実際の public key は JSON に含まれる）
    let public_key_bin = credential_id.clone();

    // DB に保存
    if let Err(e) = user_repo::save_webauthn_credential_with_json(
        &state.pool,
        auth_user.user_id,
        &credential_id,
        &public_key_bin,
        &passkey_json_str,
        "パスキー",
    ).await {
        tracing::error!("WebAuthnCredential 保存エラー: {:?}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"detail": "パスキーの保存に失敗しました"})),
        ).into_response();
    }

    (
        StatusCode::OK,
        Json(PasskeyRegisterCompleteResponse {
            ok: true,
            message: "パスキーを登録しました".to_string(),
        }),
    ).into_response()
}

/// GET /api/v1/settings/security/passkeys/
/// ユーザーの登録済みパスキー一覧を返す。
pub async fn list_passkeys(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
) -> impl IntoResponse {
    match user_repo::find_webauthn_credentials(&state.pool, auth_user.user_id).await {
        Ok(creds) => {
            let passkeys = creds.into_iter().map(|cred| {
                PasskeySummary {
                    id: cred.id,
                    name: if cred.name.is_empty() { "パスキー".to_string() } else { cred.name },
                    // タイムゾーン情報を保持したRFC3339形式で返す(フロント側でユーザーの
                    // ローカルタイムゾーンに変換して表示する)。以前は"%Y-%m-%d %H:%M"で
                    // UTC時刻をそのままフォーマットしており、日本時間のユーザーには
                    // 9時間ずれた時刻が表示されていた。
                    created_at: cred.created_at.to_rfc3339(),
                }
            }).collect();

            (
                StatusCode::OK,
                Json(PasskeyListResponse { passkeys }),
            ).into_response()
        }
        Err(e) => {
            tracing::error!("パスキー一覧取得エラー: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"detail": "パスキー一覧の取得に失敗しました"})),
            ).into_response()
        }
    }
}

/// POST /api/v1/settings/security/passkey/{id}/delete
/// パスキーを削除する（本人のものであることを確認してから削除）。
pub async fn delete_passkey(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Path(passkey_id): Path<i32>,
) -> impl IntoResponse {
    // パスキーが本人のものであることを確認
    match user_repo::find_webauthn_credentials(&state.pool, auth_user.user_id).await {
        Ok(creds) => {
            if !creds.iter().any(|c| c.id == passkey_id) {
                return (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({"detail": "このパスキーを削除する権限がありません"})),
                ).into_response();
            }
        }
        Err(e) => {
            tracing::error!("パスキー確認エラー: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"detail": "パスキーの確認に失敗しました"})),
            ).into_response();
        }
    }

    // 削除
    if let Err(e) = user_repo::delete_webauthn_credential(&state.pool, passkey_id).await {
        tracing::error!("パスキー削除エラー: {:?}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"detail": "パスキーの削除に失敗しました"})),
        ).into_response();
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({"ok": true, "message": "パスキーを削除しました"})),
    ).into_response()
}

// =============================================================================
// パスキー（WebAuthn）ログイン
// =============================================================================

#[derive(Serialize)]
pub struct PasskeyLoginBeginResponse {
    pub request_challenge: RequestChallengeResponse,
    pub auth_state_json: String,
}

/// POST /api/v1/auth/passkey/login/begin/ — パスキーログイン開始(認証不要、publicルート)
/// DBアクセスなし(discoverable credential方式のため)。
pub async fn passkey_login_begin(
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let webauthn = match webauthn_service::create_webauthn_from_headers(&headers, WIP_RP_NAME) {
        Ok(w) => w,
        Err(e) => {
            tracing::error!("WebAuthn初期化エラー: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"detail": "WebAuthn初期化エラー"}))).into_response();
        }
    };

    match webauthn_service::start_authentication(&webauthn) {
        Ok((rcr, auth_state)) => {
            let auth_state_json = match webauthn_service::authentication_state_to_json_string(&auth_state) {
                Ok(j) => j,
                Err(e) => {
                    tracing::error!("認証状態シリアライズエラー: {:?}", e);
                    return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"detail": "サーバーエラーが発生しました"}))).into_response();
                }
            };
            (StatusCode::OK, Json(PasskeyLoginBeginResponse { request_challenge: rcr, auth_state_json })).into_response()
        }
        Err(e) => {
            tracing::error!("パスキー認証開始エラー: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"detail": "パスキー認証を開始できません"}))).into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct PasskeyLoginCompleteRequest {
    pub auth_state_json: String,
    pub credential: PublicKeyCredential,
}

/// POST /api/v1/auth/passkey/login/complete/ — パスキーログイン完了(認証不要、publicルート)
pub async fn passkey_login_complete(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<PasskeyLoginCompleteRequest>,
) -> impl IntoResponse {
    let webauthn = match webauthn_service::create_webauthn_from_headers(&headers, WIP_RP_NAME) {
        Ok(w) => w,
        Err(e) => {
            tracing::error!("WebAuthn初期化エラー: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"detail": "WebAuthn初期化エラー"}))).into_response();
        }
    };

    let auth_state = match webauthn_service::authentication_state_from_json_string(&body.auth_state_json) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("認証状態デシリアライズエラー: {:?}", e);
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"detail": "不正な認証セッション"}))).into_response();
        }
    };

    // 検証前に、レスポンスからユーザーを識別する(全ユーザー分を先読みしない)
    let (user_uuid, _cred_id_hint) = match webauthn_service::identify_authentication(&webauthn, &body.credential) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("パスキー識別エラー: {:?}", e);
            return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"detail": "パスキー認証に失敗しました"}))).into_response();
        }
    };
    let user_id = user_uuid.as_u128() as i32;

    let passkeys = match user_repo::find_passkeys_by_user(&state.pool, user_id).await {
        Ok(p) if !p.is_empty() => p,
        Ok(_) => {
            return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"detail": "パスキー認証に失敗しました"}))).into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"detail": "サーバーエラーが発生しました"}))).into_response();
        }
    };

    let auth_result = match webauthn_service::finish_authentication(&webauthn, auth_state, &body.credential, &passkeys) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("パスキー認証完了エラー: {:?}", e);
            return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"detail": "パスキー認証に失敗しました"}))).into_response();
        }
    };

    let user = match user_repo::find_by_id(&state.pool, user_id).await {
        Ok(Some(u)) => u,
        _ => {
            return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"detail": "ユーザーが見つかりません"}))).into_response();
        }
    };
    if !user.is_active {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"detail": "アカウントが無効化されています"}))).into_response();
    }

    // sign_count更新(リプレイ攻撃防止)。iCloudキーチェーン等の同期パスキーでは
    // カウントが変化しない/ズレる場合があるため、失敗してもログイン自体は継続する
    // (ベストエフォート、tracing::warn!のみ)。
    let cred_id_bytes: Vec<u8> = auth_result.cred_id().as_ref().to_vec();
    if let Some(matching) = passkeys.iter().find(|pk| pk.cred_id().as_ref() == cred_id_bytes.as_slice()) {
        let mut updated = matching.clone();
        updated.update_credential(&auth_result);
        if let Ok(updated_json) = webauthn_service::passkey_to_json_string(&updated) {
            if let Err(e) = user_repo::update_webauthn_passkey_json(&state.pool, &cred_id_bytes, &updated_json).await {
                tracing::warn!("sign_count更新に失敗(ログインは継続): {:?}", e);
            }
        }
    }

    let token_pair = match jwt_service::issue_token_pair(user.id, &state.config.jwt_secret) {
        Ok(pair) => pair,
        Err(e) => {
            tracing::error!("トークン発行エラー: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"detail": "トークン発行エラー"}))).into_response();
        }
    };

    (StatusCode::OK, Json(crate::presentation::handlers::auth_api::LoginResponse {
        access: token_pair.access,
        refresh: token_pair.refresh,
    })).into_response()
}
