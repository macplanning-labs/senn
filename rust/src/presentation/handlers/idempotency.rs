/// presentation/handlers/idempotency.rs — 作成 API の冪等キー（Idempotency-Key）
///
/// 端末は送信に失敗すると同じ作成要求を送り直す。サーバーでは作成済みなのに応答だけ
/// 届かなかった場合に二重作成しないよう、端末が付けた UUID を `client_request_id`
/// として保存し、同じキーの再送には作成済みの行を返す（詳細設計 §2.5）。
use axum::{
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use uuid::Uuid;

pub const HEADER: &str = "Idempotency-Key";

#[derive(Serialize)]
struct ErrorBody {
    detail: String,
}

/// ヘッダーを読む。無ければ `Ok(None)`、UUID でなければ 400 の応答を `Err` で返す。
pub fn parse_key(headers: &HeaderMap) -> Result<Option<Uuid>, Response> {
    let Some(value) = headers.get(HEADER) else {
        return Ok(None);
    };
    value
        .to_str()
        .ok()
        .and_then(|s| Uuid::parse_str(s.trim()).ok())
        .map(Some)
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorBody {
                    detail: "Idempotency-Key は UUID で指定してください".to_string(),
                }),
            )
                .into_response()
        })
}

/// sqlx のエラーが UNIQUE 違反（SQLSTATE 23505）か。anyhow に包まれていても判定する。
pub fn is_unique_violation(err: &anyhow::Error) -> bool {
    err.chain().any(|cause| {
        cause
            .downcast_ref::<sqlx::Error>()
            .and_then(|e| e.as_database_error())
            .and_then(|d| d.code())
            .is_some_and(|code| code == "23505")
    })
}

/// 同じキーを別の利用者が使っていたときの応答。
pub fn conflict_response() -> Response {
    (
        StatusCode::CONFLICT,
        Json(ErrorBody {
            detail: "この Idempotency-Key は別の利用者の作成要求で使われています".to_string(),
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_key_accepts_uuid_and_rejects_garbage() {
        let mut h = HeaderMap::new();
        assert!(matches!(parse_key(&h), Ok(None)));
        h.insert(HEADER, "3f2b0c8e-6f2a-4a4e-9a55-2b1a3f9d7c10".parse().unwrap());
        assert!(matches!(parse_key(&h), Ok(Some(_))));
        h.insert(HEADER, "not-a-uuid".parse().unwrap());
        assert!(parse_key(&h).is_err());
    }
}
