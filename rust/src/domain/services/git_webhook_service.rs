/// domain/services/git_webhook_service.rs — Git Webhook 署名検証・チケットキー抽出
///
/// Django apps/integrations/services.py の GitWebhookService の移植。

use hmac::{Hmac, Mac};
use sha2::Sha256;
use regex::Regex;
use std::sync::OnceLock;

type HmacSha256 = Hmac<Sha256>;

/// GitHub Webhook の HMAC-SHA256 署名(X-Hub-Signature-256)を検証する。
pub fn verify_github_signature(payload_body: &[u8], signature_header: &str, secret: &str) -> bool {
    if signature_header.is_empty() {
        return false;
    }

    let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(payload_body);
    let expected = format!("sha256={}", hex_encode(&mac.finalize().into_bytes()));

    constant_time_eq(expected.as_bytes(), signature_header.as_bytes())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// タイミング攻撃を避けるための定数時間比較。
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn ticket_key_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"\b([A-Z]{2,10}-\d{1,6})\b").unwrap())
}

/// テキストからチケットキーを抽出する(出現順・重複排除)。
pub fn extract_ticket_keys(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for cap in ticket_key_pattern().captures_iter(text) {
        let key = cap[1].to_string();
        if seen.insert(key.clone()) {
            result.push(key);
        }
    }
    result
}
