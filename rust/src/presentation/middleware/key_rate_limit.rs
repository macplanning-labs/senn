//! キーごとの回数制限(アクセス制御の再設計 F-5。設計書 §10「回数制限」)
//!
//! 個人の AI キー・外部連携のキーは、IP ではなくキー単位で数える(同じキーを複数の場所で使っても上限は 1 つ)。
//! 上限は `SENN_KEY_RATE_LIMIT_PER_MIN`(既定 300 回/分。今の IP ごとの上限と同じ。値は利用量を計測してから決める)。
//! 切り替え `ACCESS_ENFORCE_KEY` が on のときだけ働く(off / shadow は今のまま)。
//! 数え方は 1 分ごとの区切り(固定窓)。プロセスごとに数える(サーバーは 1 台)。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::{
    extract::Request,
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use sha2::{Digest, Sha256};

use crate::infrastructure::access::shadow::{self, Mode, Resource};

const WINDOW: Duration = Duration::from_secs(60);
const DEFAULT_LIMIT: u32 = 300;
/// 数えているキーがこれを超えたら、終わった窓を片付ける
const PRUNE_AT: usize = 10_000;

/// キーごとの数(固定窓)。DB を使わない純粋な部品
#[derive(Default)]
pub struct Counter {
    windows: HashMap<[u8; 32], (Instant, u32)>,
}

impl Counter {
    /// 1 回数える。上限を超えていたら、次の窓が始まるまでの秒数を返す
    pub fn hit(&mut self, key: [u8; 32], limit: u32, now: Instant) -> Result<(), u64> {
        if self.windows.len() > PRUNE_AT {
            self.windows
                .retain(|_, (start, _)| now.duration_since(*start) < WINDOW);
        }
        let entry = self.windows.entry(key).or_insert((now, 0));
        if now.duration_since(entry.0) >= WINDOW {
            *entry = (now, 0);
        }
        if entry.1 >= limit {
            let rest = WINDOW.saturating_sub(now.duration_since(entry.0));
            return Err(rest.as_secs().max(1));
        }
        entry.1 += 1;
        Ok(())
    }
}

static COUNTER: Mutex<Option<Counter>> = Mutex::new(None);

fn limit() -> u32 {
    std::env::var("SENN_KEY_RATE_LIMIT_PER_MIN")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .filter(|&n| n > 0)
        .unwrap_or(DEFAULT_LIMIT)
}

/// `X-AI-Api-Key` / `X-API-Key` のキーごとに数える。キーの無い要求は数えない(認証で断られる)
pub async fn key_rate_limit(req: Request, next: Next) -> Response {
    if shadow::mode(Resource::Key) != Mode::On {
        return next.run(req).await;
    }
    let key = ["X-AI-Api-Key", "X-API-Key"]
        .iter()
        .find_map(|h| req.headers().get(*h).and_then(|v| v.to_str().ok()))
        .filter(|k| !k.is_empty())
        .map(|k| -> [u8; 32] { Sha256::digest(k.as_bytes()).into() });
    if let Some(key) = key {
        let result = {
            let mut guard = COUNTER.lock().unwrap_or_else(|e| e.into_inner());
            guard
                .get_or_insert_with(Counter::default)
                .hit(key, limit(), Instant::now())
        };
        if let Err(retry_after) = result {
            let mut resp = (
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({
                    "error": "このキーの利用回数の上限を超えました。しばらく待ってから再度実行してください"
                })),
            )
                .into_response();
            if let Ok(v) = HeaderValue::from_str(&retry_after.to_string()) {
                resp.headers_mut().insert("Retry-After", v);
            }
            return resp;
        }
    }
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_per_key_and_resets_each_window() {
        let mut c = Counter::default();
        let t0 = Instant::now();
        let a = [1u8; 32];
        let b = [2u8; 32];
        assert!(c.hit(a, 2, t0).is_ok());
        assert!(c.hit(a, 2, t0).is_ok());
        assert!(c.hit(a, 2, t0).is_err(), "3 回目は上限を超える");
        assert!(c.hit(b, 2, t0).is_ok(), "別のキーは別に数える");
        assert!(c.hit(a, 2, t0 + WINDOW).is_ok(), "次の窓では数え直す");
    }
}
