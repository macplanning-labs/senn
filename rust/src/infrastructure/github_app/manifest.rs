/// GitHub App Manifest の組み立て
use serde_json::{json, Value};

/// Phase 2 で必要になる権限・イベントも最初から含める
/// (あとから足すと、利用者に再承認を求めることになるため)。
pub fn build_manifest(base_url: &str, webhook_url: &str, app_name: &str) -> Value {
    let base = base_url.trim_end_matches('/');
    json!({
        "name": app_name,
        "url": base,
        "hook_attributes": { "url": webhook_url, "active": true },
        "redirect_url": format!("{base}/api/v1/github-app/manifest/callback"),
        "setup_url": format!("{base}/api/v1/github-app/install/callback"),
        "setup_on_update": true,
        "public": false,
        "default_permissions": {
            "metadata": "read",
            "contents": "read",
            "pull_requests": "read",
            "checks": "read"
        },
        "default_events": ["push", "pull_request", "pull_request_review", "check_run"]
    })
}

/// 受信先 URL。`GITHUB_APP_WEBHOOK_URL` があればそれを使う(ローカル検証用)。
pub fn webhook_url(base_url: &str) -> String {
    std::env::var("GITHUB_APP_WEBHOOK_URL")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| {
            format!(
                "{}/api/v1/webhooks/github-app/",
                base_url.trim_end_matches('/')
            )
        })
}

/// 受信先のホストが、インターネットから届かない(GitHub が通知を送れない)ものか。
/// localhost・ループバック・プライベートアドレス・`.local` などの社内名・ドット無しの名前。
fn is_non_public_host(host: &str) -> bool {
    let host = host
        .trim_matches(|c| c == '[' || c == ']')
        .to_ascii_lowercase();
    if host.is_empty() || !host.contains('.') && !host.contains(':') {
        return true; // localhost、ドット無しの名前(社内のホスト名)
    }
    if host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".internal")
    {
        return true;
    }
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return match ip {
            std::net::IpAddr::V4(v4) => {
                v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified()
            }
            std::net::IpAddr::V6(v6) => {
                v6.is_loopback()
                    || v6.is_unspecified()
                    || (v6.segments()[0] & 0xfe00) == 0xfc00 // ユニークローカル
                    || (v6.segments()[0] & 0xffc0) == 0xfe80 // リンクローカル
            }
        };
    }
    false
}

/// URL からホスト名を取り出す(ポート・認証情報・パスは除く。`[::1]` のような IPv6 は角括弧を外す)。
fn host_of(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority.rsplit('@').next().unwrap_or("");
    if let Some(v6) = host_port.strip_prefix('[') {
        return v6.split(']').next().unwrap_or("").to_string();
    }
    match host_port.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() && port.chars().all(|c| c.is_ascii_digit()) => {
            host.to_string()
        }
        _ => host_port.to_string(),
    }
}

/// GitHub App を作れない理由(作れるなら None)。
/// 本物の GitHub(github.com)に対して、受信先がインターネットから届かないアドレスだと、GitHub が作成画面を出す前に
/// 英語のエラーで断る。その前に、SENN の画面で分かる言葉で知らせるために使う。
/// 擬似 GitHub を使うローカル検証(`GITHUB_WEB_BASE` が github.com 以外)では、断らない。
pub fn unreachable_webhook_reason(webhook_url: &str, web_base: &str) -> Option<String> {
    if host_of(web_base) != "github.com" {
        return None;
    }
    let host = host_of(webhook_url);
    is_non_public_host(&host).then(|| {
        format!(
            "この環境のアドレス({host})は、インターネットから届かないため、GitHub が通知を送れず、GitHub App を作れません。\
本番など、インターネットから届くアドレスの環境で行ってください。"
        )
    })
}

/// 開発標準 §13.3.1 の命名規則: `<製品名>-<連携先>-Integration[-<環境>]`
const APP_NAME_BASE: &str = "SENN-GitHub-Integration";
/// GitHub の App 名の上限(文字数)。
const APP_NAME_MAX: usize = 34;

/// 既定の App 名。
/// - `configured`(環境変数 `GITHUB_APP_NAME`)が正しい名前なら、それを使う。
///   本番など、末尾に環境名を付けたくない環境で設定する(例: `SENN-GitHub-Integration`)。
/// - 無ければ、ホスト名の先頭を環境名として末尾に付ける(例: `SENN-GitHub-Integration-stg`)。
///   特定のホスト名を特別扱いしない(公開版でも同じ動きにするため)。
///
/// ランダムな数字は付けない(用途の分かる名前にする)。GitHub 全体で名前が重複していたときは、
/// GitHub の作成画面の名前欄で直せる。
pub fn default_app_name(base_url: &str, configured: Option<&str>) -> String {
    if let Some(name) = configured.map(str::trim).filter(|n| is_valid_app_name(n)) {
        return name.to_string();
    }
    let host = base_url
        .split("://")
        .nth(1)
        .unwrap_or(base_url)
        .split(['/', ':'])
        .next()
        .unwrap_or("local");
    let label: String = host
        .split('.')
        .next()
        .unwrap_or("local")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    if label.is_empty() {
        return APP_NAME_BASE.to_string();
    }
    let room = APP_NAME_MAX - APP_NAME_BASE.len() - 1;
    let env: String = label.chars().take(room).collect();
    let env = env.trim_end_matches('-');
    if env.is_empty() {
        return APP_NAME_BASE.to_string();
    }
    format!("{APP_NAME_BASE}-{env}")
}

/// 画面から渡された App 名の検証(GitHub の制限: 34 文字まで)。
pub fn is_valid_app_name(name: &str) -> bool {
    let n = name.chars().count();
    (1..=34).contains(&n)
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == ' ')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_has_required_fields() {
        let m = build_manifest(
            "https://senn.example.com/",
            "https://senn.example.com/hook/",
            "SENN-x",
        );
        assert_eq!(
            m["redirect_url"],
            "https://senn.example.com/api/v1/github-app/manifest/callback"
        );
        assert_eq!(
            m["setup_url"],
            "https://senn.example.com/api/v1/github-app/install/callback"
        );
        assert_eq!(
            m["hook_attributes"]["url"],
            "https://senn.example.com/hook/"
        );
        assert_eq!(m["public"], false);
        assert_eq!(m["default_permissions"]["pull_requests"], "read");
        assert!(m["default_events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e == "push"));
    }

    #[test]
    fn default_name_uses_configured_name() {
        // 本番など、環境名を付けたくない環境は GITHUB_APP_NAME で名前を決める
        assert_eq!(
            default_app_name("https://senn.example.com", Some("SENN-GitHub-Integration")),
            "SENN-GitHub-Integration"
        );
        // 前後の空白は落とす
        assert_eq!(
            default_app_name("https://senn.example.com", Some("  My-SENN  ")),
            "My-SENN"
        );
        // 正しくない名前(長すぎる・使えない文字・空)は使わず、ホスト名から決める
        for bad in ["", "   ", "名前", &"x".repeat(35)] {
            assert_eq!(
                default_app_name("https://senn.example.com", Some(bad)),
                "SENN-GitHub-Integration-senn",
                "{bad:?}"
            );
        }
    }

    #[test]
    fn default_name_does_not_special_case_any_host() {
        // 特定のホスト名を特別扱いしない(どのホストでも、設定が無ければ環境名が付く)
        assert_eq!(
            default_app_name("https://senn.example.com", None),
            "SENN-GitHub-Integration-senn"
        );
        // 末尾のスラッシュ・大文字でも同じ規則
        assert_eq!(
            default_app_name("https://SENN.example.com/", None),
            "SENN-GitHub-Integration-SENN"
        );
    }

    #[test]
    fn default_name_non_production_gets_environment_suffix() {
        assert_eq!(
            default_app_name("http://localhost:8150", None),
            "SENN-GitHub-Integration-localhost"
        );
        assert_eq!(
            default_app_name("https://stg.example.com", None),
            "SENN-GitHub-Integration-stg"
        );
    }

    #[test]
    fn default_name_is_always_valid_and_has_no_random_part() {
        for url in [
            "https://senn.example.com",
            "https://senn-stg-temp.example.com",
            "http://10.0.0.5:8160",
            "http://localhost:8150",
            "https://a-very-very-long-host-label-name.example.com",
            "",
        ] {
            let name = default_app_name(url, None);
            assert!(is_valid_app_name(&name), "{url} -> {name}");
            assert!(name.starts_with("SENN-GitHub-Integration"), "{name}");
            assert!(!name.ends_with('-'), "{name}");
            // 同じ入力なら同じ名前(乱数を使わない)
            assert_eq!(name, default_app_name(url, None));
        }
    }

    #[test]
    fn unreachable_reason_for_non_public_webhook_hosts() {
        let gh = "https://github.com";
        for url in [
            "http://localhost:8150/api/v1/webhooks/github-app/",
            "http://127.0.0.1:8150/x",
            "http://10.0.0.5:8160/x",
            "http://10.0.0.5/x",
            "http://172.16.1.1/x",
            "http://[::1]:8150/x",
            "http://myhost/x",
            "https://stg.example.local/x",
            "https://svc.internal/x",
        ] {
            assert!(unreachable_webhook_reason(url, gh).is_some(), "{url}");
        }
    }

    #[test]
    fn no_unreachable_reason_for_public_hosts_or_mock_github() {
        let gh = "https://github.com";
        for url in [
            "https://senn.example.com/api/v1/webhooks/github-app/",
            "https://senn-stg-temp.example.com/x",
            "http://8.8.8.8:8080/x",
        ] {
            assert!(unreachable_webhook_reason(url, gh).is_none(), "{url}");
        }
        // 擬似 GitHub(ローカル検証)では、localhost でも断らない
        assert!(
            unreachable_webhook_reason("http://localhost:8150/x", "https://github.test").is_none()
        );
        assert!(
            unreachable_webhook_reason("http://localhost:8150/x", "http://127.0.0.1:9090")
                .is_none()
        );
    }

    #[test]
    fn host_of_extracts_host_only() {
        assert_eq!(
            host_of("https://a.example.com:8443/x?y=1#z"),
            "a.example.com"
        );
        assert_eq!(
            host_of("http://user:pw@host.example.com/x"),
            "host.example.com"
        );
        assert_eq!(host_of("http://[::1]:8150/x"), "::1");
        assert_eq!(host_of("http://10.0.0.5:8160/api"), "10.0.0.5");
        assert_eq!(host_of("https://github.com"), "github.com");
    }

    #[test]
    fn unreachable_reason_names_the_host() {
        let r = unreachable_webhook_reason("http://10.0.0.5:8160/x", "https://github.com").unwrap();
        assert!(r.contains("10.0.0.5"), "{r}");
        assert!(r.contains("GitHub App を作れません"), "{r}");
    }

    #[test]
    fn app_name_validation() {
        assert!(is_valid_app_name("SENN my app"));
        assert!(!is_valid_app_name(""));
        assert!(!is_valid_app_name("<script>"));
        assert!(!is_valid_app_name(&"a".repeat(35)));
    }
}
