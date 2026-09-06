/// presentation/handlers/health.rs — ヘルスチェック
///
/// GET /health → JSON { "status": "ok/degraded", "db": "up/down" }

use axum::{extract::State, http::StatusCode, Json};
use serde_json::{json, Value};
use crate::presentation::state::AppState;

pub async fn check(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    match sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&state.pool).await {
        Ok(_) => {
            (StatusCode::OK, Json(json!({
                "status": "ok",
                "service": "wip",
                "db": "up",
                "version": env!("CARGO_PKG_VERSION")
            })))
        }
        Err(e) => {
            tracing::error!(
                "[ヘルス/DB] 処理=ヘルスチェックDB 結果=失敗 影響=外形監視がDB障害を検知すべき | {}",
                e
            );
            (StatusCode::SERVICE_UNAVAILABLE, Json(json!({
                "status": "degraded",
                "service": "wip",
                "db": "down"
            })))
        }
    }
}
