/// presentation/handlers/health.rs — ヘルスチェック
///
/// GET /health → JSON { "status": "ok" }

use axum::Json;
use serde_json::{json, Value};

pub async fn check() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": "wip",
        "version": env!("CARGO_PKG_VERSION")
    }))
}
