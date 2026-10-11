/// github_app — GitHub App 連携(かんたん接続)
///
/// 設計: docs/design/詳細設計_GitHub連携かんたん接続.md
pub mod client;
pub mod manifest;
pub mod store;

/// コールバック後の戻り先として許す相対パスか(オープンリダイレクト対策)。
/// `/` で始まり、`//`・`\`・制御文字・`:` を含まないものだけ許可する。
pub fn is_safe_return_path(path: &str) -> bool {
    path.starts_with('/')
        && path.len() <= 300
        && !path.starts_with("//")
        && !path.contains('\\')
        && !path.contains(':')
        && !path.chars().any(|c| c.is_control())
}

/// 戻り先に `githubApp=<value>` を足す(既存のクエリがあれば `&` で連結)。
pub fn with_result_param(path: &str, value: &str) -> String {
    let sep = if path.contains('?') { '&' } else { '?' };
    format!("{path}{sep}githubApp={value}")
}

/// GitHub の組織名・リポジトリ名として使える文字だけか。
pub fn is_valid_github_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name.len() <= 100
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

/// `owner/repo` 形式か。
pub fn is_valid_full_name(full_name: &str) -> bool {
    match full_name.split_once('/') {
        Some((owner, repo)) => is_valid_github_name(owner) && is_valid_github_name(repo),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn return_path_accepts_only_relative_paths() {
        assert!(is_safe_return_path("/"));
        assert!(is_safe_return_path("/projects/1/settings?tab=git"));
        assert!(!is_safe_return_path("//evil.example"));
        assert!(!is_safe_return_path("https://evil.example"));
        assert!(!is_safe_return_path("/\\evil"));
        assert!(!is_safe_return_path("/a\nb"));
        assert!(!is_safe_return_path(""));
        assert!(!is_safe_return_path("relative"));
    }

    #[test]
    fn result_param_joins_with_query() {
        assert_eq!(with_result_param("/x", "created"), "/x?githubApp=created");
        assert_eq!(
            with_result_param("/x?tab=git", "installed"),
            "/x?tab=git&githubApp=installed"
        );
    }

    #[test]
    fn github_names_are_validated() {
        assert!(is_valid_github_name("example-org"));
        assert!(is_valid_github_name("a.b_c-d"));
        assert!(!is_valid_github_name(""));
        assert!(!is_valid_github_name("a/b"));
        assert!(!is_valid_github_name("a b"));
        assert!(is_valid_full_name("example-org/senn"));
        assert!(!is_valid_full_name("example-org"));
        assert!(!is_valid_full_name("a/b/c"));
        assert!(!is_valid_full_name("../etc"));
    }
}
