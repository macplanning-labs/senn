/// presentation/handlers/auth_api.rs — JSON認証API
///
/// Djangoの /api/v1/auth/* と挙動を一致させるハンドラー。
/// Phase 1: access/refresh トークン、MFA (TOTP), ユーザー登録。

use axum::{
    extract::{State, Query},
    response::IntoResponse,
    http::StatusCode,
    Json,
    Extension,
};
use serde::{Deserialize, Serialize};
use chrono::DateTime;
use crate::presentation::state::AppState;
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::infrastructure::repositories::{user_repo, jwt_blacklist_repo};
use crate::domain::services::{jwt_service, auth_service, totp_service};

// =============================================================================
// リクエスト・レスポンス構造体
// =============================================================================

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub access: String,
    pub refresh: String,
}

#[derive(Serialize)]
pub struct LoginMfaResponse {
    pub mfa_required: bool,
    pub mfa_token: String,
}

#[derive(Deserialize)]
pub struct LoginVerifyRequest {
    pub mfa_token: String,
    pub totp_code: String,
}

#[derive(Deserialize)]
pub struct TokenRefreshRequest {
    pub refresh: String,
}

#[derive(Deserialize)]
pub struct LogoutRequest {
    pub refresh: Option<String>,
}

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
}

#[derive(Serialize)]
pub struct UserResponse {
    pub id: i32,
    pub username: String,
    pub email: String,
    #[serde(rename = "firstName")]
    pub first_name: String,
    #[serde(rename = "lastName")]
    pub last_name: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "isStaff")]
    pub is_staff: bool,
}

#[derive(Serialize)]
pub struct RegisterResponse {
    pub user: UserResponse,
    pub tokens: LoginResponse,
}

#[derive(Serialize)]
pub struct UserListResponse {
    pub id: i32,
    pub username: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
}

#[derive(Deserialize)]
pub struct ListUsersQuery {
    pub project: Option<i32>,
}

#[derive(Serialize)]
pub struct MeResponse {
    pub id: i32,
    pub username: String,
    pub email: String,
    #[serde(rename = "firstName")]
    pub first_name: String,
    #[serde(rename = "lastName")]
    pub last_name: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "isStaff")]
    pub is_staff: bool,
}

// =============================================================================
// ハンドラー実装
// =============================================================================

/// 1. ログイン
pub async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> impl IntoResponse {
    // ユーザー検索
    let user = match user_repo::find_by_username(&state.pool, &body.username).await {
        Ok(Some(u)) => u,
        _ => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"detail": "No active account found with the given credentials"})),
            ).into_response();
        }
    };

    // is_active チェック
    if !user.is_active {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"detail": "No active account found with the given credentials"})),
        ).into_response();
    }

    // パスワード検証
    let password_valid = match auth_service::verify_password(
        &state.pool,
        user.id,
        &body.password,
        &user.password_hash,
    ).await {
        Ok(valid) => valid,
        Err(_) => false,
    };

    if !password_valid {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"detail": "No active account found with the given credentials"})),
        ).into_response();
    }

    // MFA確認
    let totp_device = match user_repo::find_totp(&state.pool, user.id).await {
        Ok(Some(d)) if d.confirmed => Some(d),
        _ => None,
    };

    if let Some(_device) = totp_device {
        // MFA必須
        let mfa_token = match jwt_service::issue_mfa_token(user.id, &state.config.jwt_secret) {
            Ok(token) => token,
            Err(e) => {
                tracing::error!("DB operation failed: {:?}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"detail": "トークン発行エラー"})),
                ).into_response();
            }
        };
        (
            StatusCode::OK,
            Json(LoginMfaResponse {
                mfa_required: true,
                mfa_token,
            }),
        ).into_response()
    } else {
        // MFA不要
        let token_pair = match jwt_service::issue_token_pair(user.id, &state.config.jwt_secret) {
            Ok(pair) => pair,
            Err(e) => {
                tracing::error!("DB operation failed: {:?}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"detail": "トークン発行エラー"})),
                ).into_response();
            }
        };
        (
            StatusCode::OK,
            Json(LoginResponse {
                access: token_pair.access,
                refresh: token_pair.refresh,
            }),
        ).into_response()
    }
}

/// 2. MFA検証
pub async fn login_verify(
    State(state): State<AppState>,
    Json(body): Json<LoginVerifyRequest>,
) -> impl IntoResponse {
    // MFAトークンをデコード
    let mfa_claims = match jwt_service::decode_mfa_token(&body.mfa_token, &state.config.jwt_secret) {
        Ok(claims) => claims,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"detail": "MFAトークンが無効です"})),
            ).into_response();
        }
    };

    // ユーザー確認
    let user = match user_repo::find_by_id(&state.pool, mfa_claims.user_id).await {
        Ok(Some(u)) => u,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"detail": "MFAトークンが無効です"})),
            ).into_response();
        }
    };

    // TOTPデバイス確認
    let totp_device = match user_repo::find_totp(&state.pool, user.id).await {
        Ok(Some(d)) if d.confirmed => d,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"detail": "MFAトークンが無効です"})),
            ).into_response();
        }
    };

    // TOTPコード検証（秘密鍵が暗号化されている場合は復号）
    let secret_to_verify = if totp_device.secret.len() > 32 && !totp_device.secret.contains("$") {
        // 暗号化された秘密鍵の可能性（Base64 blob、Base32 secretよりも長い）
        match totp_service::decrypt_secret(&totp_device.secret, &state.config.jwt_secret) {
            Ok(secret_bytes) => {
                // 復号化した raw bytes を Base32 に変換
                totp_service::secret_to_base32(&secret_bytes)
            }
            Err(_) => {
                // 復号失敗 → 平文の Base32 秘密鍵として扱う
                totp_device.secret.clone()
            }
        }
    } else {
        // 平文の Base32 秘密鍵（pyotpやDjango由来）
        totp_device.secret.clone()
    };

    let totp_valid = match auth_service::verify_totp(&secret_to_verify, &body.totp_code) {
        Ok(valid) => valid,
        Err(_) => false,
    };

    if !totp_valid {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"detail": "認証コードが正しくありません"})),
        ).into_response();
    }

    // トークンペア発行
    let token_pair = match jwt_service::issue_token_pair(user.id, &state.config.jwt_secret) {
        Ok(pair) => pair,
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"detail": "トークン発行エラー"})),
            ).into_response();
        }
    };

    (
        StatusCode::OK,
        Json(LoginResponse {
            access: token_pair.access,
            refresh: token_pair.refresh,
        }),
    ).into_response()
}

/// 3. トークンリフレッシュ
pub async fn token_refresh(
    State(state): State<AppState>,
    Json(body): Json<TokenRefreshRequest>,
) -> impl IntoResponse {
    // リフレッシュトークンをデコード
    let claims = match jwt_service::decode_token(&body.refresh, &state.config.jwt_secret) {
        Ok(c) => c,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"detail": "トークンが無効です"})),
            ).into_response();
        }
    };

    // token_type チェック
    if claims.token_type != "refresh" {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"detail": "トークンが無効です"})),
        ).into_response();
    }

    // ブラックリスト確認
    let is_blacklisted = match jwt_blacklist_repo::is_blacklisted(&state.pool, &claims.jti).await {
        Ok(blacklisted) => blacklisted,
        Err(_) => false,
    };

    if is_blacklisted {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"detail": "トークンが無効です"})),
        ).into_response();
    }

    // 古いトークンをブラックリスト登録
    let expires_at = DateTime::<chrono::Utc>::from_timestamp(claims.exp, 0)
        .unwrap_or_else(|| chrono::Utc::now());
    if let Err(e) = jwt_blacklist_repo::blacklist(&state.pool, &claims.jti, expires_at).await {
        tracing::error!("failed to blacklist refresh token: {:?}", e);
    }

    // 新しいトークンペア発行
    let token_pair = match jwt_service::issue_token_pair(claims.user_id, &state.config.jwt_secret) {
        Ok(pair) => pair,
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"detail": "トークン発行エラー"})),
            ).into_response();
        }
    };

    (
        StatusCode::OK,
        Json(LoginResponse {
            access: token_pair.access,
            refresh: token_pair.refresh,
        }),
    ).into_response()
}

/// 4. 現在のユーザー情報
pub async fn me(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
) -> impl IntoResponse {
    let user = match user_repo::find_by_id(&state.pool, auth_user.user_id).await {
        Ok(Some(u)) => u,
        _ => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"detail": "ユーザーが見つかりません"})),
            ).into_response();
        }
    };

    (
        StatusCode::OK,
        Json(MeResponse {
            id: user.id,
            username: user.username,
            email: user.email,
            first_name: user.first_name,
            last_name: user.last_name,
            display_name: user.display_name,
            is_staff: user.is_staff,
        }),
    ).into_response()
}

/// 5. ログアウト
pub async fn logout(
    State(state): State<AppState>,
    Extension(_auth_user): Extension<AuthUser>,
    Json(body): Json<LogoutRequest>,
) -> impl IntoResponse {
    // リフレッシュトークンがあればブラックリスト登録
    if let Some(refresh_token) = body.refresh {
        if let Ok(claims) = jwt_service::decode_token(&refresh_token, &state.config.jwt_secret) {
            let expires_at = DateTime::<chrono::Utc>::from_timestamp(claims.exp, 0)
                .unwrap_or_else(|| chrono::Utc::now());
            if let Err(e) = jwt_blacklist_repo::blacklist(&state.pool, &claims.jti, expires_at).await {
                tracing::error!("failed to blacklist refresh token on logout: {:?}", e);
            }
        }
    }

    (StatusCode::RESET_CONTENT, ()).into_response()
}

/// 6. ユーザー登録
pub async fn register(
    State(state): State<AppState>,
    Json(body): Json<RegisterRequest>,
) -> impl IntoResponse {
    // パスワード長チェック
    if body.password.len() < 8 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"detail": "パスワードは8文字以上で入力してください"})),
        ).into_response();
    }

    // パスワードハッシュ化
    let password_hash = match auth_service::hash_password(&body.password) {
        Ok(hash) => hash,
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"detail": "パスワードハッシュエラー"})),
            ).into_response();
        }
    };

    // ユーザー作成
    let first_name = body.first_name.unwrap_or_default();
    let last_name = body.last_name.unwrap_or_default();

    let user = match user_repo::create_user(
        &state.pool,
        &body.username,
        &body.email,
        &password_hash,
        &first_name,
        &last_name,
    ).await {
        Ok(u) => u,
        Err(e) => {
            // ユーザー名重複チェック
            let err_str = e.to_string();
            if err_str.contains("duplicate key") || err_str.contains("unique")
                || err_str.contains("23505") {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"detail": "このユーザー名は既に使用されています"})),
                ).into_response();
            } else {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"detail": "ユーザー作成エラー"})),
                ).into_response();
            }
        }
    };

    // トークンペア発行
    let token_pair = match jwt_service::issue_token_pair(user.id, &state.config.jwt_secret) {
        Ok(pair) => pair,
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"detail": "トークン発行エラー"})),
            ).into_response();
        }
    };

    (
        StatusCode::CREATED,
        Json(RegisterResponse {
            user: UserResponse {
                id: user.id,
                username: user.username,
                email: user.email,
                first_name: user.first_name,
                last_name: user.last_name,
                display_name: user.display_name,
                is_staff: user.is_staff,
            },
            tokens: LoginResponse {
                access: token_pair.access,
                refresh: token_pair.refresh,
            },
        }),
    ).into_response()
}

/// 7. ユーザー一覧
pub async fn list_users(
    State(state): State<AppState>,
    Extension(_auth_user): Extension<AuthUser>,
    Query(params): Query<ListUsersQuery>,
) -> impl IntoResponse {
    let users = match params.project {
        Some(project_id) => {
            // プロジェクトメンバーのみを返す
            match user_repo::find_project_members(&state.pool, project_id).await {
                Ok(users) => users,
                Err(e) => {
                    tracing::error!("DB operation failed: {:?}", e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"detail": "ユーザー一覧取得エラー"})),
                    ).into_response();
                }
            }
        }
        None => {
            // プロジェクト指定なしなら全is_active=trueユーザーを返す
            match user_repo::find_all(&state.pool).await {
                Ok(users) => users,
                Err(e) => {
                    tracing::error!("DB operation failed: {:?}", e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"detail": "ユーザー一覧取得エラー"})),
                    ).into_response();
                }
            }
        }
    };

    let result: Vec<UserListResponse> = users
        .into_iter()
        .map(|u| UserListResponse {
            id: u.id,
            username: u.username,
            display_name: u.display_name,
        })
        .collect();

    (StatusCode::OK, Json(result)).into_response()
}
