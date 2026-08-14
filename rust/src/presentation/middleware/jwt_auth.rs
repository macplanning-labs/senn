/// presentation/middleware/jwt_auth.rs — JWT Bearer トークン認証ミドルウェア
///
/// JSON APIルート用のBearer トークン認証。
/// Authorization: Bearer <token> ヘッダーから access トークンを取得して検証し、
/// リクエスト拡張に AuthUser を注入する。

use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
    http::StatusCode,
    Json,
};
use crate::presentation::state::AppState;
// Step 2: jwt_service は auth-core クレート（django_compat、クレーム形状は同一）へ移行。
use auth_core::domain::jwt::django_compat as jwt_service;

#[derive(Clone, Copy, Debug)]
pub struct AuthUser {
    pub user_id: i32,
}

pub async fn jwt_auth(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    // Authorization ヘッダーを取得
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"detail": "認証情報が提供されていません"})),
            )
        })?;

    // "Bearer " プレフィックスを確認
    if !auth_header.starts_with("Bearer ") {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"detail": "認証情報が正しくありません"})),
        ));
    }

    // トークン部分を抽出
    let token = &auth_header[7..];

    // トークンをデコード
    let claims = jwt_service::decode_token(token, &state.config.jwt_secret).map_err(|_| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"detail": "認証情報が正しくありません"})),
        )
    })?;

    // token_type が "access" であることを確認
    if claims.token_type != "access" {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"detail": "認証情報が正しくありません"})),
        ));
    }

    // AuthUser を拡張に挿入
    let mut req = req;
    req.extensions_mut().insert(AuthUser {
        user_id: claims.user_id,
    });

    Ok(next.run(req).await)
}
