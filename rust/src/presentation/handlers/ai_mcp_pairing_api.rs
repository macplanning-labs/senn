/// presentation/handlers/ai_mcp_pairing_api.rs — 端末の接続(コピー・貼り付けなし)(DEMO-000192 フェーズ4)
///
/// 利用者の Mac の MCP が、キーを自分で作って「指紋」だけを渡す(平文は Mac の外に出ない)。
/// 利用者が画面で「許可を求める」を押し、管理者が承認すると、預かった指紋でキーを自動登録する。
/// 設計: docs/design/詳細設計書_AIAgent_MCP操作の申請.md §8
///
/// 開始と状態の問い合わせ(`/api/v1/ai-agent/pairing/…`)は、AIキーを使わない。
/// 本番では、Cloudflare Access(社内のサービストークン)を持つエージェントだけが届く。
/// 状態の問い合わせには、開始のときに一度だけ返した秘密(deviceSecret)が要る。
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;

use crate::domain::access::Viewer;
use crate::infrastructure::repositories::ai_mcp_pairing_repo::{
    self as repo, ApplyError, StartError, CODE_VALID_MINUTES, LABEL_MAX_CHARS,
};
use crate::presentation::handlers::ai_mcp_request_api::{
    announce_new_request, audit, err, human_user_id, notify, server_error, CATEGORY_DECISION,
};
use crate::presentation::state::AppState;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartIn {
    pub key_hash: String,
    pub key_prefix: String,
    #[serde(default)]
    pub label: String,
}

/// ラベル(Mac の名前)を、表示できる形に整える
fn clean_label(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_control())
        .take(LABEL_MAX_CHARS)
        .collect::<String>()
        .trim()
        .to_string()
}

/// 接続ページの URL(基準 URL + /connect?code=…)
pub fn connect_url(base_url: &str, code: &str) -> String {
    format!(
        "{}/connect?code={}",
        base_url.trim().trim_end_matches('/'),
        code
    )
}

/// POST /api/v1/ai-agent/pairing/ — MCP が接続を始める
pub async fn start(State(state): State<AppState>, Json(body): Json<StartIn>) -> Response {
    if !repo::is_valid_key_hash(&body.key_hash) || !repo::is_valid_key_prefix(&body.key_prefix) {
        return err(
            StatusCode::BAD_REQUEST,
            "keyHash / keyPrefix の形が正しくありません",
        );
    }
    let label = clean_label(&body.label);
    match repo::too_many_recent_starts(&state.pool).await {
        Ok(false) => {}
        Ok(true) => {
            return err(
                StatusCode::TOO_MANY_REQUESTS,
                "接続の開始が多すぎます。しばらくしてから、もう一度試してください",
            )
        }
        Err(e) => return server_error(e),
    }
    for _ in 0..5 {
        match repo::start(&state.pool, &body.key_hash, &body.key_prefix, &label).await {
            Ok(Ok(s)) => {
                return (
                    StatusCode::CREATED,
                    Json(json!({
                        "code": s.code,
                        "deviceSecret": s.device_secret,
                        "url": connect_url(&state.config.base_url, &s.code),
                        "expiresInSeconds": CODE_VALID_MINUTES * 60,
                    })),
                )
                    .into_response();
            }
            Ok(Err(StartError::CodeCollision)) => continue,
            Ok(Err(StartError::KeyExists)) => {
                return err(StatusCode::CONFLICT, "このキーは、すでに使われています")
            }
            Err(e) => return server_error(e),
        }
    }
    err(
        StatusCode::SERVICE_UNAVAILABLE,
        "確認コードを作れませんでした。もう一度試してください",
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusIn {
    pub code: String,
    pub device_secret: String,
}

/// POST /api/v1/ai-agent/pairing/status/ — MCP が状態を確認する(確認コードと秘密の両方が要る)
pub async fn status(State(state): State<AppState>, Json(body): Json<StatusIn>) -> Response {
    match repo::find_for_device(&state.pool, &body.code, &body.device_secret).await {
        Ok(Some(p)) => {
            let v = p.view(Utc::now());
            Json(json!({ "status": v.status, "message": v.message })).into_response()
        }
        // 確認コードが無い場合と、秘密が違う場合を区別しない
        Ok(None) => err(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => server_error(e),
    }
}

/// GET /api/v1/ai-mcp/pairings/{code}/ — 接続ページの表示(画面のログインが要る)
pub async fn info(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(code): Path<String>,
) -> Response {
    let user_id = match human_user_id(&viewer) {
        Ok(id) => id,
        Err(r) => return r,
    };
    match repo::find_by_code(&state.pool, &code).await {
        Ok(Some(p)) => {
            let v = p.view(Utc::now());
            // 申請前(待ち・期限切れ)は誰でも見られる。申請後は、申請した本人だけ
            let visible =
                matches!(v.status.as_str(), "waiting" | "expired") || p.user_id == Some(user_id);
            if !visible {
                return err(StatusCode::NOT_FOUND, "見つかりません");
            }
            Json(v).into_response()
        }
        Ok(None) => err(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => server_error(e),
    }
}

/// POST /api/v1/ai-mcp/pairings/{code}/apply/ — 「許可を求める」を押した
pub async fn apply(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(code): Path<String>,
) -> Response {
    let user_id = match human_user_id(&viewer) {
        Ok(id) => id,
        Err(r) => return r,
    };
    let applied =
        match repo::apply(&state.pool, &code, user_id).await {
            Ok(Ok(a)) => a,
            Ok(Err(ApplyError::NotFound)) => return err(StatusCode::NOT_FOUND, "見つかりません"),
            Ok(Err(ApplyError::Expired)) => {
                return err(
                    StatusCode::GONE,
                    "このリンクは、期限切れです。Claude で、もう一度やり直してください",
                )
            }
            Ok(Err(ApplyError::NotWaiting)) => {
                return err(StatusCode::CONFLICT, "このリンクは、すでに使われています")
            }
            Ok(Err(ApplyError::TooManyKeys)) => return err(
                StatusCode::CONFLICT,
                "有効なキーが上限に達しています。設定の「詳細」で、使わないキーを失効してください",
            ),
            Ok(Err(ApplyError::RequestExists)) => {
                return err(
                    StatusCode::CONFLICT,
                    "ほかの端末の接続を、申請中です。先に、そちらの承認を待ってください",
                )
            }
            Err(e) => return server_error(e),
        };
    if applied.new_request {
        announce_new_request(&state, user_id, applied.request_id).await;
    }
    if applied.registered {
        audit(
            &state,
            user_id,
            "ai_mcp.pair_register",
            Some(user_id),
            json!({ "request_id": applied.request_id }),
        )
        .await;
        notify(
            &state,
            user_id,
            CATEGORY_DECISION,
            "AI Agent を利用した MCP 操作: 使えるようになりました",
            "この Mac が接続されました。Claude から、そのまま使えます(キーの操作は要りません)。",
        )
        .await;
    }
    match repo::find_by_code(&state.pool, &code).await {
        Ok(Some(p)) => Json(p.view(Utc::now())).into_response(),
        Ok(None) => err(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => server_error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_url_trims_the_trailing_slash() {
        assert_eq!(
            connect_url("https://senn.example.com/", "ABCD-EFGH"),
            "https://senn.example.com/connect?code=ABCD-EFGH"
        );
    }

    #[test]
    fn label_drops_control_characters_and_is_capped() {
        assert_eq!(clean_label("  My\nMac\t "), "MyMac");
        assert_eq!(
            clean_label(&"あ".repeat(300)).chars().count(),
            LABEL_MAX_CHARS
        );
    }
}
