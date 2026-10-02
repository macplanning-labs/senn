//! 閲覧者(Viewer)の抽出器。ハンドラは `viewer: Viewer` を引数に取る(詳細設計書 §6.1)
//!
//! JWT の検証は `jwt_auth` ミドルウェアが行い、`AuthUser` をリクエストに置く。ここでは、それを元に
//! 役割・有効な所属を DB から読み、閲覧者を組み立てる。無効化されたユーザーは 401。
//! 同じリクエストで 2 回目以降は、リクエストの拡張に置いた物を使う(DB を読み直さない)。
//!
//! 個人キー・外部連携(X-AI-Api-Key / X-API-Key)はフェーズ F で対応する。

use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    Json,
};
use serde_json::{json, Value};

use crate::domain::access::{Principal, Viewer};
use crate::infrastructure::access::viewer_repo;
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::presentation::state::AppState;

fn unauthorized() -> (StatusCode, Json<Value>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({"detail": "認証情報が正しくありません"})),
    )
}

impl FromRequestParts<AppState> for Viewer {
    type Rejection = (StatusCode, Json<Value>);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if let Some(v) = parts.extensions.get::<Viewer>() {
            return Ok(v.clone());
        }
        let Some(auth) = parts.extensions.get::<AuthUser>().copied() else {
            // jwt_auth を通らないルートで Viewer を使った(配線の誤り)。拒否側に倒す
            tracing::error!("Viewer extractor used on a route without jwt_auth");
            return Err(unauthorized());
        };
        let viewer = viewer_repo::load(
            &state.pool,
            Principal::Human {
                user_id: auth.user_id,
            },
            viewer_repo::today_utc(),
        )
        .await
        .map_err(|e| {
            tracing::error!("[認可] 閲覧者の読み込みに失敗: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"detail": "サーバーエラーが発生しました"})),
            )
        })?
        .ok_or_else(unauthorized)?;
        parts.extensions.insert(viewer.clone());
        Ok(viewer)
    }
}

impl Viewer {
    /// 人のユーザー ID が必要な操作(本人の設定など)で使う。外部連携(ユーザーが無い)は 401 の応答を返す。
    /// ```ignore
    /// let user_id = match viewer.require_user_id() { Ok(id) => id, Err(resp) => return resp };
    /// ```
    #[allow(clippy::result_large_err)] // 呼び出し側でそのまま応答として返すため、Response を持つ
    pub fn require_user_id(&self) -> Result<i32, axum::response::Response> {
        use axum::response::IntoResponse;
        self.user_id().ok_or_else(|| unauthorized().into_response())
    }
}
