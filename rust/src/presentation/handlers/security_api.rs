/// presentation/handlers/security_api.rs — TOTP エンロールメントAPI
///
/// TOTP多要素認証の設定・確認・無効化。

use axum::{
    extract::State,
    response::IntoResponse,
    http::StatusCode,
    Json,
    Extension,
};
use serde::{Deserialize, Serialize};
use crate::presentation::state::AppState;
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::infrastructure::repositories::user_repo;
use crate::domain::services::{totp_service, auth_service};

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
// ハンドラー実装
// =============================================================================

/// POST /api/v1/settings/security/totp/begin
/// 新規 TOTP 秘密鍵を生成して QR コード + Base32 を返す。
/// 秘密鍵はまだ DB に保存しない（stateless）。
pub async fn totp_begin(
    Extension(auth_user): Extension<AuthUser>,
) -> impl IntoResponse {
    let secret_bytes = totp_service::generate_secret();

    let qr_base64 = match totp_service::generate_qr_base64(&secret_bytes, &auth_user.user_id.to_string()) {
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
    match totp_service::verify_code(&secret_bytes, &auth_user.user_id.to_string(), &body.code) {
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
                            match totp_service::verify_code(&secret_bytes, &auth_user.user_id.to_string(), code) {
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
