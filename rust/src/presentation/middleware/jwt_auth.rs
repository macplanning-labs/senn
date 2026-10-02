use crate::domain::services::jwt_service;
use crate::presentation::state::AppState;
/// presentation/middleware/jwt_auth.rs — JWT Bearer トークン認証ミドルウェア
///
/// JSON APIルート用のBearer トークン認証。
/// Authorization: Bearer <token> ヘッダーから access トークンを取得して検証し、
/// リクエスト拡張に AuthUser を注入する。
use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
    Json,
};

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

    // デモトークンを拒否（API では使用不可）
    if token.trim().eq_ignore_ascii_case("senn-demo-token") {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"detail": "Demo token is not accepted by API"})),
        ));
    }

    // トークンをデコード
    let claims = jwt_service::decode_token(token, &state.config.jwt_secret).map_err(|_| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"detail": "認証情報が正しくありません"})),
        )
    })?;

    // token_type が Access であることを確認
    if claims.token_type != jwt_service::TokenType::Access {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"detail": "認証情報が正しくありません"})),
        ));
    }

    let user_id = claims.user_id().map_err(|_| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"detail": "認証情報が正しくありません"})),
        )
    })?;

    // 無効化されたユーザーは、トークンの期限を待たずに拒否する(アクセス制御の再設計 B-6)。
    // 以前は署名と期限だけを見ていたため、無効化してもアクセストークンの期限まで操作できた。
    let active: Option<bool> =
        crate::infrastructure::repositories::user_repo::is_active(&state.pool, user_id)
            .await
            .map_err(|e| {
                tracing::error!("[認証/JWT] 処理=有効性の確認 結果=失敗 | {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"detail": "サーバーエラーが発生しました"})),
                )
            })?;
    if active != Some(true) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"detail": "認証情報が正しくありません"})),
        ));
    }

    // AuthUser を拡張に挿入
    let mut req = req;
    req.extensions_mut().insert(AuthUser { user_id });

    Ok(next.run(req).await)
}
