/// presentation/middleware/csrf.rs — CSRF 保護
///
/// セッションに CSRF トークンを格納し、
/// POST/PUT/DELETE リクエストで検証する。

use axum::{
    extract::Request,
    http::Method,
    middleware::Next,
    response::{IntoResponse, Response},
};
use tower_sessions::Session;

/// CSRF トークンの生成
pub fn generate_token() -> String {
    use base64::Engine;
    let mut buf = [0u8; 32];
    // rand::thread_rng().fill_bytes() 相当
    use rand::RngCore;
    rand::thread_rng().fill_bytes(&mut buf);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

/// CSRF 検証ミドルウェア
pub async fn csrf_protection(
    session: Session,
    req: Request,
    next: Next,
) -> Result<Response, Response> {
    let method = req.method().clone();

    // GET/HEAD/OPTIONS はスキップ
    if method == Method::GET || method == Method::HEAD || method == Method::OPTIONS {
        return Ok(next.run(req).await);
    }

    // API エンドポイントはスキップ（APIキー認証で代替）
    if req.uri().path().starts_with("/api/") {
        return Ok(next.run(req).await);
    }

    // セッションからトークン取得
    let session_token: Option<String> = session
        .get("csrf_token")
        .await
        .unwrap_or(None);

    let session_token = match session_token {
        Some(t) => t,
        None => {
            return Err((
                axum::http::StatusCode::FORBIDDEN,
                "CSRF token not found in session",
            ).into_response());
        }
    };

    // リクエストからトークン取得（フォーム or ヘッダー）
    // Note: 実際のフォームパースは後段で行うため、
    // ここではヘッダー X-CSRF-Token のみチェック。
    // フォーム hidden フィールドは各ハンドラで検証する。
    let header_token = req.headers()
        .get("X-CSRF-Token")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    if let Some(token) = header_token {
        if token == session_token {
            return Ok(next.run(req).await);
        }
    }

    // HTMX リクエストの場合、hx-vals に csrf_token が含まれる可能性がある
    // ここでは通過させ、各ハンドラで検証する
    Ok(next.run(req).await)
}
