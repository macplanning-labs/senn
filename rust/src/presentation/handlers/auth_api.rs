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
use crate::infrastructure::repositories::{user_repo, jwt_blacklist_repo, notification_preference_repo};
use crate::domain::services::auth_service;
use crate::domain::services::jwt_service;
use crate::domain::models::user::User;
use crate::domain::models::notification::NotificationCategory;

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
    pub alias: Option<String>,
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
    pub alias: Option<String>,
    #[serde(rename = "isStaff")]
    pub is_staff: bool,
    #[serde(rename = "emailNotificationsEnabled")]
    pub email_notifications_enabled: bool,
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
    pub alias: Option<String>,
}

#[derive(Deserialize)]
pub struct ListUsersQuery {
    pub project: Option<i32>,
}

#[derive(Deserialize)]
pub struct SelfProfileUpdateIn {
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub alias: Option<String>,
    pub email: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub email_notifications_enabled: Option<bool>,
}

#[derive(Deserialize)]
pub struct DeactivateMyAccountIn {
    pub current_password: String,
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
    pub alias: Option<String>,
    #[serde(rename = "isStaff")]
    pub is_staff: bool,
    #[serde(rename = "emailNotificationsEnabled")]
    pub email_notifications_enabled: bool,
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
        let mfa_token = match jwt_service::issue_mfa_token(
            user.id,
            &state.config.jwt_secret,
            state.config.mfa_token_lifetime_seconds,
        ) {
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
        let token_pair = match jwt_service::issue_token_pair(
        user.id,
        &state.config.jwt_secret,
        state.config.access_token_lifetime_minutes,
        state.config.refresh_token_lifetime_days,
    ) {
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

    let mfa_user_id = match mfa_claims.user_id() {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"detail": "MFAトークンが無効です"})),
            ).into_response();
        }
    };

    // ユーザー確認
    let user = match user_repo::find_by_id(&state.pool, mfa_user_id).await {
        Ok(Some(u)) => u,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"detail": "MFAトークンが無効です"})),
            ).into_response();
        }
    };

    // TOTPコード検証（auth_service::verify_totp_for_user で DB から秘密鍵を取得・検証）
    let totp_valid = match auth_service::verify_totp_for_user(&state.pool, &state.config.jwt_secret, user.id, &body.totp_code).await {
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
    let token_pair = match jwt_service::issue_token_pair(
        user.id,
        &state.config.jwt_secret,
        state.config.access_token_lifetime_minutes,
        state.config.refresh_token_lifetime_days,
    ) {
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
    if claims.token_type != jwt_service::TokenType::Refresh {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"detail": "トークンが無効です"})),
        ).into_response();
    }

    let (user_id, jti) = match (claims.user_id(), claims.jti.as_deref()) {
        (Ok(user_id), Some(jti)) => (user_id, jti.to_string()),
        _ => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"detail": "トークンが無効です"})),
            ).into_response();
        }
    };

    // ブラックリスト確認
    let is_blacklisted = match jwt_blacklist_repo::is_blacklisted(&state.pool, &jti).await {
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
    if let Err(e) = jwt_blacklist_repo::blacklist(&state.pool, &jti, expires_at).await {
        tracing::error!("failed to blacklist refresh token: {:?}", e);
    }

    // 新しいトークンペア発行
    let token_pair = match jwt_service::issue_token_pair(
        user_id,
        &state.config.jwt_secret,
        state.config.access_token_lifetime_minutes,
        state.config.refresh_token_lifetime_days,
    ) {
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
            alias: user.alias,
            is_staff: user.is_staff,
            email_notifications_enabled: user.email_notifications_enabled,
        }),
    ).into_response()
}

/// 5. ログアウト
///
/// ボディ省略(Content-Type無し)のリクエストは`Json<LogoutRequest>`だと415で弾かれてしまうため、
/// `Option<Json<_>>`で受けボディが無ければリフレッシュトークンの無効化をスキップする(WIPAPPDEV-000046)。
pub async fn logout(
    State(state): State<AppState>,
    Extension(_auth_user): Extension<AuthUser>,
    body: Option<Json<LogoutRequest>>,
) -> impl IntoResponse {
    // リフレッシュトークンがあればブラックリスト登録
    if let Some(refresh_token) = body.and_then(|Json(b)| b.refresh) {
        if let Ok(claims) = jwt_service::decode_token(&refresh_token, &state.config.jwt_secret) {
            if let Some(jti) = claims.jti.as_deref() {
                let expires_at = DateTime::<chrono::Utc>::from_timestamp(claims.exp, 0)
                    .unwrap_or_else(|| chrono::Utc::now());
                if let Err(e) = jwt_blacklist_repo::blacklist(&state.pool, jti, expires_at).await {
                    tracing::error!("failed to blacklist refresh token on logout: {:?}", e);
                }
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

    let mut user = match user_repo::create_user(
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

    if let Some(alias) = body.alias {
        if !alias.trim().is_empty() {
            match user_repo::update_alias(&state.pool, user.id, Some(&alias)).await {
                Ok(updated) => user = updated,
                Err(e) => {
                    tracing::error!("DB operation failed: {:?}", e);
                }
            }
        }
    }

    // トークンペア発行
    let token_pair = match jwt_service::issue_token_pair(
        user.id,
        &state.config.jwt_secret,
        state.config.access_token_lifetime_minutes,
        state.config.refresh_token_lifetime_days,
    ) {
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
                alias: user.alias,
                is_staff: user.is_staff,
                email_notifications_enabled: user.email_notifications_enabled,
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
            alias: u.alias,
        })
        .collect();

    (StatusCode::OK, Json(result)).into_response()
}

#[derive(serde::Deserialize)]
pub struct SetActiveIn {
    pub is_active: bool,
}

/// ユーザーの有効/無効を切り替える(Django Admin代替)。
/// 呼び出し元がis_staffであること、自分自身を無効化しないことを必須とする。
pub async fn set_user_active(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    axum::extract::Path(target_id): axum::extract::Path<i32>,
    Json(body): Json<SetActiveIn>,
) -> impl IntoResponse {
    let caller = match user_repo::find_by_id(&state.pool, auth_user.user_id).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"detail": "ユーザーが見つかりません"}))).into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"detail": "サーバーエラーが発生しました"}))).into_response();
        }
    };

    if !caller.is_staff {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"detail": "権限がありません"}))).into_response();
    }

    if target_id == auth_user.user_id && !body.is_active {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"detail": "自分自身を無効化することはできません"})),
        ).into_response();
    }

    match user_repo::find_by_id(&state.pool, target_id).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            return (StatusCode::NOT_FOUND, Json(serde_json::json!({"detail": "ユーザーが見つかりません"}))).into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"detail": "サーバーエラーが発生しました"}))).into_response();
        }
    }

    if let Err(e) = user_repo::set_active(&state.pool, target_id, body.is_active).await {
        tracing::error!("DB operation failed: {:?}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"detail": "サーバーエラーが発生しました"}))).into_response();
    }

    (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
}

#[derive(serde::Deserialize)]
pub struct UpdateProfileIn {
    pub email: String,
    pub username: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub alias: Option<String>,
}

/// ユーザーのプロフィール(メールアドレス・氏名)を更新する(Django Admin代替)。
/// 呼び出し元がis_staffであることを必須とする。
pub async fn update_user_profile(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    axum::extract::Path(target_id): axum::extract::Path<i32>,
    Json(body): Json<UpdateProfileIn>,
) -> impl IntoResponse {
    let caller = match user_repo::find_by_id(&state.pool, auth_user.user_id).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"detail": "ユーザーが見つかりません"}))).into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"detail": "サーバーエラーが発生しました"}))).into_response();
        }
    };

    if !caller.is_staff {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"detail": "権限がありません"}))).into_response();
    }

    if body.email.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"detail": "メールアドレスは必須です"}))).into_response();
    }

    let target = match user_repo::find_by_id(&state.pool, target_id).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, Json(serde_json::json!({"detail": "ユーザーが見つかりません"}))).into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"detail": "サーバーエラーが発生しました"}))).into_response();
        }
    };

    let first_name = body.first_name.unwrap_or(target.first_name);
    let last_name = body.last_name.unwrap_or(target.last_name);

    let mut updated = match user_repo::update_profile(&state.pool, target_id, &body.email, &first_name, &last_name).await {
        Ok(u) => u,
        Err(e) => {
            let err_str = e.to_string();
            if err_str.contains("duplicate key") || err_str.contains("unique") || err_str.contains("23505") {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"detail": "このメールアドレスは既に使用されています"})),
                ).into_response();
            }
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"detail": "サーバーエラーが発生しました"}))).into_response();
        }
    };

    if let Some(new_alias) = body.alias {
        updated = match user_repo::update_alias(&state.pool, target_id, Some(&new_alias)).await {
            Ok(u) => u,
            Err(e) => {
                tracing::error!("DB operation failed: {:?}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"detail": "サーバーエラーが発生しました"}))).into_response();
            }
        };
    }

    if let Some(new_username) = body.username {
        let trimmed_username = new_username.trim();
        if !trimmed_username.is_empty() && trimmed_username != updated.username {
            updated = match user_repo::update_username(&state.pool, target_id, trimmed_username).await {
                Ok(u) => u,
                Err(e) => {
                    let err_str = e.to_string();
                    if err_str.contains("duplicate key") || err_str.contains("unique") || err_str.contains("23505") {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(serde_json::json!({"detail": "このログイン名は既に使用されています"})),
                        ).into_response();
                    }
                    tracing::error!("DB operation failed: {:?}", e);
                    return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"detail": "サーバーエラーが発生しました"}))).into_response();
                }
            };
        } else if trimmed_username.is_empty() {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"detail": "ログイン名は必須です"}))).into_response();
        }
    }

    (
        StatusCode::OK,
        Json(UserResponse {
            id: updated.id,
            username: updated.username,
            email: updated.email,
            first_name: updated.first_name,
            last_name: updated.last_name,
            display_name: updated.display_name,
            alias: updated.alias,
            is_staff: updated.is_staff,
            email_notifications_enabled: updated.email_notifications_enabled,
        }),
    ).into_response()
}

/// 本人プロフィール編集（セルフサービス）
/// 呼び出し元（auth.user_id）の情報のみを更新。Pathでid受け取り不要。
pub async fn update_my_profile(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<SelfProfileUpdateIn>,
) -> impl IntoResponse {
    // 現在のユーザー情報を取得
    let current = match user_repo::find_by_id(&state.pool, auth.user_id).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"detail": "ユーザーが見つかりません"}))).into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"detail": "サーバーエラーが発生しました"}))).into_response();
        }
    };

    let mut updated = current.clone();

    // email/first_name/last_nameのいずれかが Some の場合、update_profileを呼ぶ
    if body.email.is_some() || body.first_name.is_some() || body.last_name.is_some() {
        let new_email = body.email.as_deref().unwrap_or(&current.email);
        let new_first_name = body.first_name.as_deref().unwrap_or(&current.first_name);
        let new_last_name = body.last_name.as_deref().unwrap_or(&current.last_name);

        updated = match user_repo::update_profile(&state.pool, auth.user_id, new_email, new_first_name, new_last_name).await {
            Ok(u) => u,
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("duplicate key") || err_str.contains("unique") || err_str.contains("23505") {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({"detail": "このメールアドレスは既に使用されています"})),
                    ).into_response();
                }
                tracing::error!("DB operation failed: {:?}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"detail": "サーバーエラーが発生しました"}))).into_response();
            }
        };
    }

    // display_name が Some の場合は更新
    if let Some(new_display_name) = body.display_name {
        let trimmed = new_display_name.trim();
        if !trimmed.is_empty() && trimmed != updated.display_name {
            let update_result: anyhow::Result<User> = sqlx::query_as(
                "UPDATE accounts_user SET display_name=$2 WHERE id=$1
                 RETURNING id::int4 AS id, username, password AS password_hash, display_name, email,
                        first_name, last_name,
                        is_active, is_staff, must_change_password, email_notifications_enabled"
            )
            .bind(auth.user_id)
            .bind(trimmed)
            .fetch_one(&state.pool)
            .await
            .map_err(|e| anyhow::anyhow!(e));

            updated = match update_result {
                Ok(u) => u,
                Err(e) => {
                    tracing::error!("DB operation failed: {:?}", e);
                    return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"detail": "サーバーエラーが発生しました"}))).into_response();
                }
            };
        }
    }

    // alias（ニックネーム）が Some の場合、update_alias を呼ぶ（空文字はNULL扱い）
    if let Some(new_alias) = body.alias {
        updated = match user_repo::update_alias(&state.pool, auth.user_id, Some(&new_alias)).await {
            Ok(u) => u,
            Err(e) => {
                tracing::error!("DB operation failed: {:?}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"detail": "サーバーエラーが発生しました"}))).into_response();
            }
        };
    }

    // username が Some の場合、update_username を呼ぶ
    if let Some(new_username) = body.username {
        let trimmed_username = new_username.trim();
        if !trimmed_username.is_empty() && trimmed_username != updated.username {
            updated = match user_repo::update_username(&state.pool, auth.user_id, trimmed_username).await {
                Ok(u) => u,
                Err(e) => {
                    let err_str = e.to_string();
                    if err_str.contains("duplicate key") || err_str.contains("unique") || err_str.contains("23505") {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(serde_json::json!({"detail": "このログイン名は既に使用されています"})),
                        ).into_response();
                    }
                    tracing::error!("DB operation failed: {:?}", e);
                    return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"detail": "サーバーエラーが発生しました"}))).into_response();
                }
            };
        } else if trimmed_username.is_empty() {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"detail": "ログイン名は必須です"}))).into_response();
        }
    }

    // email_notifications_enabled が Some の場合、マスターON/OFFを更新
    if let Some(enabled) = body.email_notifications_enabled {
        updated = match user_repo::update_email_notifications_enabled(&state.pool, auth.user_id, enabled).await {
            Ok(u) => u,
            Err(e) => {
                tracing::error!("DB operation failed: {:?}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"detail": "サーバーエラーが発生しました"}))).into_response();
            }
        };
    }

    (
        StatusCode::OK,
        Json(UserResponse {
            id: updated.id,
            username: updated.username,
            email: updated.email,
            first_name: updated.first_name,
            last_name: updated.last_name,
            display_name: updated.display_name,
            alias: updated.alias,
            is_staff: updated.is_staff,
            email_notifications_enabled: updated.email_notifications_enabled,
        }),
    ).into_response()
}

#[derive(Serialize)]
pub struct NotificationPreferenceOut {
    pub category: String,
    #[serde(rename = "emailEnabled")]
    pub email_enabled: bool,
}

#[derive(Deserialize)]
pub struct NotificationPreferenceUpdateIn {
    pub category: String,
    pub email_enabled: bool,
}

async fn build_preference_list_response(state: &AppState, user_id: i32) -> impl IntoResponse {
    match notification_preference_repo::list_by_user(&state.pool, user_id).await {
        Ok(rows) => {
            let out: Vec<NotificationPreferenceOut> = rows
                .into_iter()
                .map(|r| NotificationPreferenceOut { category: r.category, email_enabled: r.email_enabled })
                .collect();
            (StatusCode::OK, Json(out)).into_response()
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"detail": "サーバーエラーが発生しました"}))).into_response()
        }
    }
}

/// GET /api/v1/auth/me/notification-preferences/ — イベント別メール通知設定一覧
pub async fn list_notification_preferences(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
) -> impl IntoResponse {
    build_preference_list_response(&state, auth.user_id).await.into_response()
}

/// PATCH /api/v1/auth/me/notification-preferences/ — 1カテゴリ分のメール通知ON/OFFを更新
pub async fn update_notification_preference(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<NotificationPreferenceUpdateIn>,
) -> impl IntoResponse {
    if !NotificationCategory::EMAIL_CAPABLE.iter().any(|c| c.as_db_str() == body.category) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"detail": "不正なカテゴリです"})),
        ).into_response();
    }

    if let Err(e) = notification_preference_repo::upsert(&state.pool, auth.user_id, &body.category, body.email_enabled).await {
        tracing::error!("DB operation failed: {:?}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"detail": "サーバーエラーが発生しました"}))).into_response();
    }

    build_preference_list_response(&state, auth.user_id).await.into_response()
}

/// 本人がアカウントを無効化する（論理削除）
/// 現在のパスワードの検証が必須。成功後はJWTをブラックリストに追加して即座に無効化する。
pub async fn deactivate_my_account(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    headers: axum::http::HeaderMap,
    Json(body): Json<DeactivateMyAccountIn>,
) -> impl IntoResponse {
    // 現在のユーザー情報を取得（パスワードハッシュが必要）
    let current = match user_repo::find_by_id(&state.pool, auth.user_id).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"detail": "ユーザーが見つかりません"}))).into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"detail": "サーバーエラーが発生しました"}))).into_response();
        }
    };

    // パスワード検証
    match auth_service::verify_password(&state.pool, auth.user_id, &body.current_password, &current.password_hash).await {
        Ok(true) => {
            // パスワード正しい、続行
        }
        Ok(false) => {
            // パスワード誤り
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"detail": "パスワードが正しくありません"}))).into_response();
        }
        Err(e) => {
            tracing::error!("Password verification failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"detail": "サーバーエラーが発生しました"}))).into_response();
        }
    };

    // ユーザーを無効化
    if let Err(e) = user_repo::deactivate_self(&state.pool, auth.user_id).await {
        tracing::error!("Failed to deactivate user: {:?}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"detail": "サーバーエラーが発生しました"}))).into_response();
    }

    // 現在のアクセストークンをブラックリストに追加
    if let Some(auth_header) = headers.get("Authorization")
        .and_then(|v| v.to_str().ok())
    {
        if auth_header.starts_with("Bearer ") {
            let token = &auth_header[7..];
            if let Ok(claims) = jwt_service::decode_token(token, &state.config.jwt_secret) {
                if let Some(jti) = claims.jti.as_deref() {
                    let expires_at = chrono::DateTime::<chrono::Utc>::from_timestamp(claims.exp, 0)
                        .unwrap_or_else(|| chrono::Utc::now());
                    if let Err(e) = jwt_blacklist_repo::blacklist(&state.pool, jti, expires_at).await {
                        tracing::error!("failed to blacklist access token on deactivate: {:?}", e);
                    }
                }
            }
        }
    }

    (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
}
