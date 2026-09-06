/// presentation/handlers/password_reset_api.rs — パスワードリセット（セルフサービス）
///
/// メール（またはユーザー名）でリセットリンクを送付し、ワンタイムトークンで
/// 新パスワードを設定する。アカウント列挙を防ぐため、リクエスト時は常に
/// 同じ成功レスポンスを返す。

use axum::{extract::State, http::HeaderMap, http::StatusCode, response::IntoResponse, Json};
use auth_core::domain::one_time_token::{OneTimeToken, OneTimeTokenStore};
use chrono::Duration;
use serde::{Deserialize, Serialize};

use crate::domain::models::user::User;
use crate::domain::services::auth_service;
use crate::infrastructure::repositories::{password_reset_repo, user_repo};
use crate::presentation::state::AppState;

const RESET_TOKEN_TTL: Duration = Duration::hours(24);

#[derive(Deserialize)]
pub struct PasswordResetRequestBody {
    /// メールアドレスまたはユーザー名
    pub identifier: String,
}

#[derive(Serialize)]
pub struct PasswordResetRequestResponse {
    pub ok: bool,
    pub message: String,
    /// SMTP未設定の非本番環境のみ。メールの代わりに画面でリンクを表示する
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_url: Option<String>,
}

#[derive(Deserialize)]
pub struct PasswordResetConfirmBody {
    pub token: String,
    pub new_password: String,
}

#[derive(Serialize)]
pub struct PasswordResetConfirmResponse {
    pub ok: bool,
    pub message: String,
}

/// POST /api/v1/auth/password-reset/request/
pub async fn password_reset_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<PasswordResetRequestBody>,
) -> impl IntoResponse {
    let identifier = body.identifier.trim();
    let mut response = PasswordResetRequestResponse {
        ok: true,
        message: "登録されている場合、パスワードリセット用のメールを送信しました。".to_string(),
        reset_url: None,
    };

    if identifier.is_empty() {
        return (StatusCode::OK, Json(response)).into_response();
    }

    let user = match resolve_user_for_reset(&state.pool, identifier).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            tracing::info!(
                "パスワードリセット要求: 対象ユーザーなし identifier_len={}",
                identifier.len()
            );
            return (StatusCode::OK, Json(response)).into_response();
        }
        Err(e) => {
            tracing::error!("パスワードリセット要求: ユーザー検索エラー: {:?}", e);
            return (StatusCode::OK, Json(response)).into_response();
        }
    };

    if user.email.trim().is_empty() {
        tracing::warn!(
            "パスワードリセット要求: メール未設定 user_id={}",
            user.id
        );
        return (StatusCode::OK, Json(response)).into_response();
    }

    let token = OneTimeToken::issue(RESET_TOKEN_TTL);
    let store = password_reset_repo::PasswordResetTokenStore::new(state.pool.clone());

    if let Err(e) = store.replace_for_user(user.id, &token).await {
        tracing::error!("パスワードリセット要求: トークン保存失敗 user_id={}: {:?}", user.id, e);
        return (StatusCode::OK, Json(response)).into_response();
    }

    let reset_path = format!("/reset-password?token={}", token.token);
    let reset_url = format!(
        "{}{}",
        public_base_url(&headers, &state.config.base_url),
        reset_path
    );

    let display = user.display();
    let subject = "WIP パスワードリセット";
    let body_text = format!(
        "{display} 様\n\n\
         WIP のパスワードリセットを受け付けました。\n\
         以下のリンクから24時間以内に新しいパスワードを設定してください。\n\n\
         {reset_url}\n\n\
         心当たりがない場合は、このメールを無視してください。\n"
    );

    if let Some(sender) = &state.mail_sender {
        if sender.is_configured() {
            if let Err(e) = sender.send(&user.email, subject, &body_text).await {
                tracing::error!(
                    "パスワードリセット要求: メール送信失敗 user_id={}: {:?}",
                    user.id,
                    e
                );
            }
        } else {
            tracing::info!(
                "パスワードリセット [DRY-RUN] to={} url={}",
                user.email,
                reset_url
            );
            expose_reset_url_for_non_production(&mut response, &reset_path);
        }
    } else {
        tracing::info!(
            "パスワードリセット [DRY-RUN] to={} url={}",
            user.email,
            reset_url
        );
        expose_reset_url_for_non_production(&mut response, &reset_path);
    }

    (StatusCode::OK, Json(response)).into_response()
}

/// POST /api/v1/auth/password-reset/confirm/
pub async fn password_reset_confirm(
    State(state): State<AppState>,
    Json(body): Json<PasswordResetConfirmBody>,
) -> impl IntoResponse {
    if body.new_password.len() < 8 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"detail": "パスワードは8文字以上で入力してください"})),
        )
            .into_response();
    }

    let token = body.token.trim();
    if token.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"detail": "トークンが無効です"})),
        )
            .into_response();
    }

    let store = password_reset_repo::PasswordResetTokenStore::new(state.pool.clone());
    let user_id_str = match store.consume(token).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"detail": "トークンが無効または期限切れです"})),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("パスワードリセット確認: トークン消費エラー: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"detail": "サーバーエラーが発生しました"})),
            )
                .into_response();
        }
    };

    let user_id: i32 = match user_id_str.parse() {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"detail": "サーバーエラーが発生しました"})),
            )
                .into_response();
        }
    };

    let user = match user_repo::find_by_id(&state.pool, user_id).await {
        Ok(Some(u)) if u.is_active => u,
        Ok(Some(_)) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"detail": "アカウントが無効化されています"})),
            )
                .into_response();
        }
        Ok(None) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"detail": "ユーザーが見つかりません"})),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("パスワードリセット確認: ユーザー取得エラー: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"detail": "サーバーエラーが発生しました"})),
            )
                .into_response();
        }
    };

    let password_hash = match auth_service::hash_password(&body.new_password) {
        Ok(hash) => hash,
        Err(e) => {
            tracing::error!("パスワードリセット確認: ハッシュエラー: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"detail": "パスワードの更新に失敗しました"})),
            )
                .into_response();
        }
    };

    if let Err(e) = user_repo::update_password(&state.pool, user.id, &password_hash).await {
        tracing::error!(
            "パスワードリセット確認: DB更新失敗 user_id={}: {:?}",
            user.id,
            e
        );
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"detail": "パスワードの更新に失敗しました"})),
        )
            .into_response();
    }

    tracing::info!("パスワードリセット完了 user_id={}", user.id);

    (
        StatusCode::OK,
        Json(PasswordResetConfirmResponse {
            ok: true,
            message: "パスワードを更新しました。新しいパスワードでログインしてください。".to_string(),
        }),
    )
        .into_response()
}

/// 本番以外で SMTP 未設定のとき、メールの代わりにリセット URL を API レスポンスへ含める。
fn expose_reset_url_for_non_production(
    response: &mut PasswordResetRequestResponse,
    reset_url: &str,
) {
    let is_production = std::env::var("ENV_NAME").as_deref() == Ok("production");
    if is_production {
        return;
    }
    response.reset_url = Some(reset_url.to_string());
    response.message =
        "SMTP未設定のためメールは送信されませんでした。以下のリンクからパスワードを再設定してください。"
            .to_string();
}

/// パスワードリセットメール内リンク用の公開ベース URL。
/// Origin → Referer → X-Forwarded-Host → Host の順で解決し、最後に BASE_URL。
fn public_base_url(headers: &HeaderMap, configured_base_url: &str) -> String {
    if let Some(origin) = headers
        .get("origin")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
    {
        return origin.trim_end_matches('/').to_string();
    }

    if let Some(referer) = headers.get("referer").and_then(|v| v.to_str().ok()) {
        if let Ok(parsed) = url::Url::parse(referer) {
            let origin = parsed.origin().ascii_serialization();
            if !origin.is_empty() && origin != "null" {
                return origin;
            }
        }
    }

    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get(axum::http::header::HOST))
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty());
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty());

    if let Some(host) = host {
        let scheme = scheme.unwrap_or("http");
        return format!("{scheme}://{host}");
    }

    configured_base_url.trim_end_matches('/').to_string()
}

async fn resolve_user_for_reset(
    pool: &sqlx::PgPool,
    identifier: &str,
) -> anyhow::Result<Option<User>> {
    if identifier.contains('@') {
        let users = user_repo::find_active_by_email(pool, identifier).await?;
        match users.len() {
            0 => Ok(None),
            1 => Ok(users.into_iter().next()),
            _ => {
                tracing::warn!(
                    "パスワードリセット要求: メールアドレスが複数ユーザーに紐づいているためスキップ"
                );
                Ok(None)
            }
        }
    } else {
        if let Some(user) = user_repo::find_by_username(pool, identifier).await? {
            if user.is_active {
                return Ok(Some(user));
            }
        }

        let users = user_repo::find_active_by_display_name(pool, identifier).await?;
        match users.len() {
            0 => Ok(None),
            1 => Ok(users.into_iter().next()),
            _ => {
                tracing::warn!(
                    "パスワードリセット要求: 表示名が複数ユーザーに紐づいているためスキップ"
                );
                Ok(None)
            }
        }
    }
}
