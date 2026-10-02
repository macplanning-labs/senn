//! メールに載せるリンクの起点(登録確認・パスワード再設定・招待)
//!
//! リンクの起点は、設定の `BASE_URL` と `ADDITIONAL_ALLOWED_ORIGINS` の中からだけ選ぶ。
//! 以前は、リクエストの Origin / Referer / Host をそのまま使っていた。これらの窓口には送り元の確認
//! (origin_check)がかからないため、攻撃者が Origin を書き換えて送ると、本人に届くメールのリンクが
//! 攻撃者のサイトを向き、トークンを盗まれる(DEMO-000169)。
//!
//! Origin(無ければ Referer の起点)が許可リストにあれば、それを使う(staging など、別の画面の URL で
//! 動かしているときに、その画面へ戻すため)。それ以外は、すべて `BASE_URL`。

use axum::http::HeaderMap;

use crate::config::AppConfig;

fn normalize(origin: &str) -> String {
    origin.trim().trim_end_matches('/').to_ascii_lowercase()
}

/// リクエストが名乗る起点(Origin、無ければ Referer の起点)
fn claimed_origin(headers: &HeaderMap) -> Option<String> {
    if let Some(origin) = headers
        .get("origin")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
    {
        return Some(origin.to_string());
    }
    let referer = headers.get("referer").and_then(|v| v.to_str().ok())?;
    let origin = url::Url::parse(referer)
        .ok()?
        .origin()
        .ascii_serialization();
    (origin != "null").then_some(origin)
}

/// メールのリンクの起点。許可リスト(`BASE_URL`・`ADDITIONAL_ALLOWED_ORIGINS`)にある物だけを返す
pub fn mail_link_base(headers: &HeaderMap, config: &AppConfig) -> String {
    let base = config.base_url.trim_end_matches('/').to_string();
    let Some(claimed) = claimed_origin(headers) else {
        return base;
    };
    let claimed = normalize(&claimed);
    let allowed = std::iter::once(&config.base_url)
        .chain(config.additional_allowed_origins.iter())
        .find(|a| normalize(a) == claimed);
    match allowed {
        Some(a) => a.trim().trim_end_matches('/').to_string(),
        None => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_config;

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(*k, v.parse().unwrap());
        }
        h
    }

    #[test]
    fn attacker_origin_falls_back_to_base_url() {
        let c = test_config();
        let h = headers(&[("origin", "https://evil.example")]);
        assert_eq!(mail_link_base(&h, &c), "https://senn.test");
    }

    #[test]
    fn attacker_referer_falls_back_to_base_url() {
        let c = test_config();
        let h = headers(&[("referer", "https://evil.example/login")]);
        assert_eq!(mail_link_base(&h, &c), "https://senn.test");
    }

    #[test]
    fn host_headers_are_ignored() {
        let c = test_config();
        let h = headers(&[
            ("host", "evil.example"),
            ("x-forwarded-host", "evil.example"),
            ("x-forwarded-proto", "https"),
        ]);
        assert_eq!(mail_link_base(&h, &c), "https://senn.test");
    }

    #[test]
    fn allowed_origins_are_kept() {
        let c = test_config();
        let h = headers(&[("origin", "https://stg.senn.test")]);
        assert_eq!(mail_link_base(&h, &c), "https://stg.senn.test");
        let h = headers(&[("referer", "https://stg.senn.test/forgot-password?x=1")]);
        assert_eq!(mail_link_base(&h, &c), "https://stg.senn.test");
        let h = headers(&[("origin", "HTTPS://SENN.TEST/")]);
        assert_eq!(mail_link_base(&h, &c), "https://senn.test");
    }

    #[test]
    fn lookalike_origins_are_rejected() {
        let c = test_config();
        for o in [
            "https://senn.test.evil.example",
            "http://senn.test",
            "https://senn.test:8443",
            "https://evil.example@senn.test",
        ] {
            let h = headers(&[("origin", o)]);
            assert_eq!(mail_link_base(&h, &c), "https://senn.test", "{o}");
        }
    }

    #[test]
    fn no_headers_uses_base_url() {
        let mut c = test_config();
        c.base_url = "https://senn.test/".to_string();
        assert_eq!(mail_link_base(&HeaderMap::new(), &c), "https://senn.test");
    }
}
