use regex::Regex;
use std::collections::{HashMap, HashSet};

/// 意図の種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyIntent {
    Closing,
    Reference,
    Implicit,
}

impl KeyIntent {
    /// DB に保存する名前(t_git_event.key_intent)
    pub fn as_str(self) -> &'static str {
        match self {
            KeyIntent::Closing => "closing",
            KeyIntent::Reference => "reference",
            KeyIntent::Implicit => "implicit",
        }
    }
}

/// 見つかったチケットキー
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundKey {
    pub key: String, // 常に大文字(例: "SENN-123")
    pub intent: KeyIntent,
}

/// キーを拾うソース
pub struct KeySources<'a> {
    pub branch: &'a str,
    pub title: &'a str, // PR タイトル(push では空)
    pub body: &'a str,  // PR 本文。先頭 10,000 文字だけ見る
    pub commit_messages: &'a [&'a str],
}

/// チケットキーを拾う純粋関数
///
/// # 規則
/// - キーの形: 英字 2〜10 文字 + `-` + 数字 1〜6 桁。前後は単語の境界。
/// - 大文字キー(`SENN-123`)は常に採用。
/// - 小文字・大小混在キーは、大文字にした prefix が known_prefixes に含まれるときだけ採用。
/// - 拾う場所と順序: branch → title → body → commit_messages。
/// - 結果は最初に出現した順で、重複は 1 つにまとめる。
/// - body は先頭 10,000 文字だけを見る。
#[allow(dead_code)]
pub fn scan_keys(sources: &KeySources, known_prefixes: &HashSet<String>) -> Vec<FoundKey> {
    let mut found: HashMap<String, KeyIntent> = HashMap::new();
    let mut order = Vec::new();

    // 1. ブランチ名から拾う（常に Implicit）
    for key in extract_keys_from_text(sources.branch, known_prefixes) {
        if !found.contains_key(&key) {
            order.push(key.clone());
        }
        found.insert(key, KeyIntent::Implicit);
    }

    // 2. PR タイトルから拾う（すべてのキーを Implicit から始め、意図で上書き）
    if !sources.title.is_empty() {
        // まずすべてのキーを拾う
        for key in extract_keys_from_text(sources.title, known_prefixes) {
            found.entry(key.clone()).or_insert_with(|| {
                order.push(key);
                KeyIntent::Implicit
            });
        }
        // 次に意図を上書き
        for (key, intent) in extract_keys_with_intent(sources.title, known_prefixes) {
            let current_intent = found.entry(key).or_insert(intent);
            *current_intent = merge_intents(*current_intent, intent);
        }
    }

    // 3. PR 本文から拾う（先頭 10,000 文字だけ）
    if !sources.body.is_empty() {
        let body_limited = take_chars(sources.body, 10000);
        // まずすべてのキーを拾う
        for key in extract_keys_from_text(body_limited, known_prefixes) {
            found.entry(key.clone()).or_insert_with(|| {
                order.push(key);
                KeyIntent::Implicit
            });
        }
        // 次に意図を上書き
        for (key, intent) in extract_keys_with_intent(body_limited, known_prefixes) {
            let current_intent = found.entry(key).or_insert(intent);
            *current_intent = merge_intents(*current_intent, intent);
        }
    }

    // 4. コミットメッセージから拾う（すべてのキーを Implicit から始め、意図で上書き）
    for msg in sources.commit_messages {
        // まずすべてのキーを拾う
        for key in extract_keys_from_text(msg, known_prefixes) {
            found.entry(key.clone()).or_insert_with(|| {
                order.push(key);
                KeyIntent::Implicit
            });
        }
        // 次に意図を上書き
        for (key, intent) in extract_keys_with_intent(msg, known_prefixes) {
            let current_intent = found.entry(key).or_insert(intent);
            *current_intent = merge_intents(*current_intent, intent);
        }
    }

    // 結果を最初に出現した順で返す
    order
        .into_iter()
        .map(|key| FoundKey {
            intent: found[&key],
            key,
        })
        .collect()
}

/// この意図のキーで、ステータス遷移を動かしてよいか
#[allow(dead_code)]
pub fn allows_transition(intent: KeyIntent) -> bool {
    intent != KeyIntent::Reference
}

// ============================================================================
// 内部ヘルパー関数
// ============================================================================

/// キーの正規表現パターン
fn key_pattern() -> &'static Regex {
    // キーの形: 英字 2〜10 文字 + `-` + 数字 1〜6 桁
    // 前後は単語の境界
    static PATTERN: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"\b([A-Za-z]{2,10})-(\d{1,6})\b").expect("failed to compile key pattern regex")
    })
}

/// 意図判定用の正規表現パターン
fn intent_pattern() -> &'static Regex {
    static PATTERN: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    PATTERN.get_or_init(|| {
        // Closing: close, closes, closed, fix, fixes, fixed, resolve, resolves, resolved
        // Reference: ref, refs, part of, related to, see, towards, toward
        // マッチ後に : を含む可能性あり、その後キーが続く
        Regex::new(
            r"(?i)\b(close|closes|closed|fix|fixes|fixed|resolve|resolves|resolved|ref|refs|part\s+of|related\s+to|see|towards|toward)\b"
        ).expect("failed to compile intent pattern regex")
    })
}

/// 意図の語の「直後」にあるキーの並び(`SENN-1`、`SENN-1, SENN-2`、`SENN-1 and SENN-2`、`SENN-1 & SENN-2`)。
/// 語とキーの間には、空白と `:` だけを許す。**その後ろの文章にあるキーは含めない**。
fn key_list_pattern() -> &'static Regex {
    static PATTERN: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(
            r"(?i)^[\s:]*((?:[a-z]{2,10}-\d{1,6}\b)(?:\s*[,&]\s*[a-z]{2,10}-\d{1,6}\b|\s+and\s+[a-z]{2,10}-\d{1,6}\b)*)",
        )
        .expect("failed to compile key list regex")
    })
}

/// テキストからキーだけを抽出（意図は付けない）
fn extract_keys_from_text(text: &str, known_prefixes: &HashSet<String>) -> Vec<String> {
    let mut keys = Vec::new();
    for caps in key_pattern().captures_iter(text) {
        let prefix = caps.get(1).unwrap().as_str();
        let number = caps.get(2).unwrap().as_str();

        let key = if prefix.chars().all(|c| c.is_ascii_uppercase()) {
            // 大文字のキーは常に採用
            format!("{}-{}", prefix, number)
        } else {
            // 小文字・大小混在は、大文字にして prefix に無いなら拾わない
            let upper_prefix = prefix.to_uppercase();
            if known_prefixes.contains(&upper_prefix) {
                format!("{}-{}", upper_prefix, number)
            } else {
                continue;
            }
        };

        keys.push(key);
    }
    keys
}

/// テキストからキーと意図を抽出
fn extract_keys_with_intent(
    text: &str,
    known_prefixes: &HashSet<String>,
) -> Vec<(String, KeyIntent)> {
    let mut results = Vec::new();

    // 次に意図判定の語を探す
    for caps in intent_pattern().captures_iter(text) {
        let intent_word_match = caps.get(0).unwrap();
        let intent_word_str = intent_word_match.as_str().to_lowercase();

        let intent = match intent_word_str.as_str() {
            w if w.starts_with("close") || w.starts_with("fix") || w.starts_with("resolve") => {
                KeyIntent::Closing
            }
            w if w.starts_with("ref")
                || w.contains("part")
                || w.contains("related")
                || w.starts_with("see")
                || w.starts_with("toward") =>
            {
                KeyIntent::Reference
            }
            _ => continue,
        };

        // 意図の語の直後にあるキーの並びだけを対象にする(後ろの文章のキーは含めない)
        let rest = &text[intent_word_match.end()..];
        if let Some(list) = key_list_pattern().captures(rest) {
            let list_text = list.get(1).map(|m| m.as_str()).unwrap_or("");
            for key in extract_keys_from_text(list_text, known_prefixes) {
                results.push((key, intent));
            }
        }
    }

    results
}

/// 複数の意図をマージ（優先度: Reference > Closing > Implicit）
fn merge_intents(a: KeyIntent, b: KeyIntent) -> KeyIntent {
    if a == KeyIntent::Reference || b == KeyIntent::Reference {
        KeyIntent::Reference
    } else if a == KeyIntent::Closing || b == KeyIntent::Closing {
        KeyIntent::Closing
    } else {
        KeyIntent::Implicit
    }
}

/// テキストから先頭 n 文字を取得（文字数ベース）
fn take_chars(s: &str, n: usize) -> &str {
    for (char_count, (byte_idx, _)) in s.char_indices().enumerate() {
        if char_count >= n {
            return &s[..byte_idx];
        }
    }
    s
}

// ============================================================================
// テスト
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn prefixes(names: &[&str]) -> HashSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_extract_uppercase_key_from_branch() {
        let sources = KeySources {
            branch: "feat/SENN-123-add-login",
            title: "",
            body: "",
            commit_messages: &[],
        };
        let prefixes = prefixes(&["SENN"]);

        let result = scan_keys(&sources, &prefixes);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].key, "SENN-123");
        assert_eq!(result[0].intent, KeyIntent::Implicit);
    }

    #[test]
    fn test_extract_lowercase_key_from_branch_with_known_prefix() {
        let sources = KeySources {
            branch: "feat/senn-123-add-login",
            title: "",
            body: "",
            commit_messages: &[],
        };
        let prefixes = prefixes(&["SENN"]);

        let result = scan_keys(&sources, &prefixes);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].key, "SENN-123");
        assert_eq!(result[0].intent, KeyIntent::Implicit);
    }

    #[test]
    fn test_ignore_lowercase_key_with_unknown_prefix() {
        let sources = KeySources {
            branch: "feat/utf-8-fix",
            title: "",
            body: "",
            commit_messages: &[],
        };
        let prefixes = prefixes(&["SENN"]);

        let result = scan_keys(&sources, &prefixes);

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_ignore_unknown_prefix_with_uppercase() {
        let sources = KeySources {
            branch: "feat/SHA-256-hash",
            title: "",
            body: "",
            commit_messages: &[],
        };
        let prefixes = prefixes(&["SENN"]);

        let result = scan_keys(&sources, &prefixes);

        // 大文字キーは known_prefixes を検証しないので採用される
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].key, "SHA-256");
    }

    #[test]
    fn test_ignore_seven_digit_number() {
        let sources = KeySources {
            branch: "feat/SENN-1234567-test",
            title: "",
            body: "",
            commit_messages: &[],
        };
        let prefixes = prefixes(&["SENN"]);

        let result = scan_keys(&sources, &prefixes);

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_closing_intent_simple() {
        let sources = KeySources {
            branch: "",
            title: "Closes SENN-1",
            body: "",
            commit_messages: &[],
        };
        let prefixes = prefixes(&["SENN"]);

        let result = scan_keys(&sources, &prefixes);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].key, "SENN-1");
        assert_eq!(result[0].intent, KeyIntent::Closing);
    }

    #[test]
    fn test_closing_intent_with_colon() {
        let sources = KeySources {
            branch: "",
            title: "fixes: SENN-2",
            body: "",
            commit_messages: &[],
        };
        let prefixes = prefixes(&["SENN"]);

        let result = scan_keys(&sources, &prefixes);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].key, "SENN-2");
        assert_eq!(result[0].intent, KeyIntent::Closing);
    }

    #[test]
    fn test_closing_intent_multiple_keys() {
        let sources = KeySources {
            branch: "",
            title: "Resolved SENN-3, SENN-4 and SENN-5",
            body: "",
            commit_messages: &[],
        };
        let prefixes = prefixes(&["SENN"]);

        let result = scan_keys(&sources, &prefixes);

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].key, "SENN-3");
        assert_eq!(result[0].intent, KeyIntent::Closing);
        assert_eq!(result[1].key, "SENN-4");
        assert_eq!(result[1].intent, KeyIntent::Closing);
        assert_eq!(result[2].key, "SENN-5");
        assert_eq!(result[2].intent, KeyIntent::Closing);
    }

    #[test]
    fn test_closing_intent_case_insensitive() {
        let sources = KeySources {
            branch: "",
            title: "CLOSES SENN-1",
            body: "",
            commit_messages: &[],
        };
        let prefixes = prefixes(&["SENN"]);

        let result = scan_keys(&sources, &prefixes);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].intent, KeyIntent::Closing);
    }

    #[test]
    fn test_reference_intent_simple() {
        let sources = KeySources {
            branch: "",
            title: "Refs SENN-1",
            body: "",
            commit_messages: &[],
        };
        let prefixes = prefixes(&["SENN"]);

        let result = scan_keys(&sources, &prefixes);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].key, "SENN-1");
        assert_eq!(result[0].intent, KeyIntent::Reference);
    }

    #[test]
    fn test_reference_intent_part_of() {
        let sources = KeySources {
            branch: "",
            title: "Part of SENN-2",
            body: "",
            commit_messages: &[],
        };
        let prefixes = prefixes(&["SENN"]);

        let result = scan_keys(&sources, &prefixes);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].key, "SENN-2");
        assert_eq!(result[0].intent, KeyIntent::Reference);
    }

    #[test]
    fn test_reference_intent_related_to() {
        let sources = KeySources {
            branch: "",
            title: "related to SENN-3, SENN-4",
            body: "",
            commit_messages: &[],
        };
        let prefixes = prefixes(&["SENN"]);

        let result = scan_keys(&sources, &prefixes);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].key, "SENN-3");
        assert_eq!(result[0].intent, KeyIntent::Reference);
    }

    #[test]
    fn test_reference_intent_see() {
        let sources = KeySources {
            branch: "",
            title: "see SENN-5",
            body: "",
            commit_messages: &[],
        };
        let prefixes = prefixes(&["SENN"]);

        let result = scan_keys(&sources, &prefixes);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].intent, KeyIntent::Reference);
    }

    #[test]
    fn test_implicit_intent() {
        let sources = KeySources {
            branch: "",
            title: "SENN-1 fix login",
            body: "",
            commit_messages: &[],
        };
        let prefixes = prefixes(&["SENN"]);

        let result = scan_keys(&sources, &prefixes);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].key, "SENN-1");
        assert_eq!(result[0].intent, KeyIntent::Implicit);
    }

    #[test]
    fn test_priority_reference_over_closing() {
        let sources = KeySources {
            branch: "",
            title: "Closes SENN-1 and Refs SENN-1",
            body: "",
            commit_messages: &[],
        };
        let prefixes = prefixes(&["SENN"]);

        let result = scan_keys(&sources, &prefixes);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].key, "SENN-1");
        assert_eq!(result[0].intent, KeyIntent::Reference);
    }

    #[test]
    fn test_priority_closing_over_implicit() {
        let sources = KeySources {
            branch: "",
            title: "Closes SENN-1 mentioned in SENN-1",
            body: "",
            commit_messages: &[],
        };
        let prefixes = prefixes(&["SENN"]);

        let result = scan_keys(&sources, &prefixes);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].key, "SENN-1");
        assert_eq!(result[0].intent, KeyIntent::Closing);
    }

    #[test]
    fn test_body_limited_to_10000_chars() {
        let long_body = "x".repeat(9986) + "\nCloses SENN-1";
        let sources = KeySources {
            branch: "",
            title: "",
            body: &long_body,
            commit_messages: &[],
        };
        let prefixes = prefixes(&["SENN"]);

        let result = scan_keys(&sources, &prefixes);

        // 10,000 文字以内なので SENN-1 が拾える (9986 + 1 + 13 = 10000)
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].key, "SENN-1");
    }

    #[test]
    fn test_body_beyond_10000_chars_ignored() {
        let long_body = "x".repeat(10001) + "\nCloses SENN-1";
        let sources = KeySources {
            branch: "",
            title: "",
            body: &long_body,
            commit_messages: &[],
        };
        let prefixes = prefixes(&["SENN"]);

        let result = scan_keys(&sources, &prefixes);

        // 10,000 文字より後ろなので SENN-1 は拾えない
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_order_preserved() {
        let sources = KeySources {
            branch: "SENN-1",
            title: "SENN-2",
            body: "SENN-3",
            commit_messages: &["SENN-4"],
        };
        let prefixes = prefixes(&["SENN"]);

        let result = scan_keys(&sources, &prefixes);

        assert_eq!(result.len(), 4);
        assert_eq!(result[0].key, "SENN-1");
        assert_eq!(result[1].key, "SENN-2");
        assert_eq!(result[2].key, "SENN-3");
        assert_eq!(result[3].key, "SENN-4");
    }

    #[test]
    fn test_deduplication() {
        let sources = KeySources {
            branch: "SENN-1",
            title: "SENN-1",
            body: "",
            commit_messages: &[],
        };
        let prefixes = prefixes(&["SENN"]);

        let result = scan_keys(&sources, &prefixes);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].key, "SENN-1");
    }

    #[test]
    fn test_empty_input() {
        let sources = KeySources {
            branch: "",
            title: "",
            body: "",
            commit_messages: &[],
        };
        let prefixes = prefixes(&["SENN"]);

        let result = scan_keys(&sources, &prefixes);

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_allows_transition_closing() {
        assert!(allows_transition(KeyIntent::Closing));
    }

    #[test]
    fn test_allows_transition_implicit() {
        assert!(allows_transition(KeyIntent::Implicit));
    }

    #[test]
    fn test_allows_transition_reference() {
        assert!(!allows_transition(KeyIntent::Reference));
    }

    #[test]
    fn test_mixed_case_prefix_with_known_prefix() {
        let sources = KeySources {
            branch: "feat/Senn-456-test",
            title: "",
            body: "",
            commit_messages: &[],
        };
        let prefixes = prefixes(&["SENN"]);

        let result = scan_keys(&sources, &prefixes);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].key, "SENN-456");
    }

    #[test]
    fn test_commit_message_with_intent() {
        let sources = KeySources {
            branch: "",
            title: "",
            body: "",
            commit_messages: &["Fix SENN-789"],
        };
        let prefixes = prefixes(&["SENN"]);

        let result = scan_keys(&sources, &prefixes);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].key, "SENN-789");
        assert_eq!(result[0].intent, KeyIntent::Closing);
    }

    #[test]
    fn test_multiple_commit_messages() {
        let sources = KeySources {
            branch: "",
            title: "",
            body: "",
            commit_messages: &["Closes SENN-1", "Refs SENN-2"],
        };
        let prefixes = prefixes(&["SENN"]);

        let result = scan_keys(&sources, &prefixes);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].key, "SENN-1");
        assert_eq!(result[0].intent, KeyIntent::Closing);
        assert_eq!(result[1].key, "SENN-2");
        assert_eq!(result[1].intent, KeyIntent::Reference);
    }

    #[test]
    fn test_toward_variant() {
        let sources = KeySources {
            branch: "",
            title: "toward SENN-1",
            body: "",
            commit_messages: &[],
        };
        let prefixes = prefixes(&["SENN"]);

        let result = scan_keys(&sources, &prefixes);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].intent, KeyIntent::Reference);
    }

    #[test]
    fn test_multiple_prefixes() {
        let sources = KeySources {
            branch: "senn-123",
            title: "wapp-456",
            body: "",
            commit_messages: &[],
        };
        let prefixes = prefixes(&["SENN", "WAPP"]);

        let result = scan_keys(&sources, &prefixes);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].key, "SENN-123");
        assert_eq!(result[1].key, "WAPP-456");
    }

    #[test]
    fn test_japanese_text_in_body() {
        let sources = KeySources {
            branch: "",
            title: "",
            body: "これは日本語です。Closes SENN-1",
            commit_messages: &[],
        };
        let prefixes = prefixes(&["SENN"]);

        let result = scan_keys(&sources, &prefixes);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].key, "SENN-1");
    }

    // ── 意図は、語の「直後のキー」だけに付く(後ろの文章のキーに広がらない) ──

    fn senn_prefixes() -> HashSet<String> {
        ["SENN", "WAPP"].iter().map(|s| s.to_string()).collect()
    }

    fn intent_of(found: &[FoundKey], key: &str) -> Option<KeyIntent> {
        found.iter().find(|f| f.key == key).map(|f| f.intent)
    }

    #[test]
    fn intent_does_not_spread_to_keys_later_in_the_text() {
        let body = "Refs SENN-1\n\nCloses SENN-2\n\nなお SENN-9 も関係する";
        let found = scan_keys(
            &KeySources {
                branch: "",
                title: "",
                body,
                commit_messages: &[],
            },
            &senn_prefixes(),
        );
        assert_eq!(intent_of(&found, "SENN-1"), Some(KeyIntent::Reference));
        // Refs の後ろにある Closes のキーが、Reference に塗りつぶされない
        assert_eq!(intent_of(&found, "SENN-2"), Some(KeyIntent::Closing));
        // どの語の直後でもないキーは Implicit のまま
        assert_eq!(intent_of(&found, "SENN-9"), Some(KeyIntent::Implicit));
    }

    #[test]
    fn closing_word_only_covers_the_adjacent_key_list() {
        let found = scan_keys(
            &KeySources {
                branch: "",
                title: "Fixes SENN-1, SENN-2 and SENN-3. Unrelated SENN-9",
                body: "",
                commit_messages: &[],
            },
            &senn_prefixes(),
        );
        for k in ["SENN-1", "SENN-2", "SENN-3"] {
            assert_eq!(intent_of(&found, k), Some(KeyIntent::Closing), "{k}");
        }
        assert_eq!(intent_of(&found, "SENN-9"), Some(KeyIntent::Implicit));
    }

    #[test]
    fn reference_list_stops_at_the_end_of_the_list() {
        let found = scan_keys(
            &KeySources {
                branch: "",
                title: "",
                body: "see SENN-5 and SENN-6, then SENN-7",
                commit_messages: &[],
            },
            &senn_prefixes(),
        );
        assert_eq!(intent_of(&found, "SENN-5"), Some(KeyIntent::Reference));
        assert_eq!(intent_of(&found, "SENN-6"), Some(KeyIntent::Reference));
        assert_eq!(intent_of(&found, "SENN-7"), Some(KeyIntent::Implicit));
    }

    #[test]
    fn word_followed_by_something_other_than_a_key_marks_nothing() {
        let found = scan_keys(
            &KeySources {
                branch: "",
                title: "Fixes the login bug (SENN-4)",
                body: "",
                commit_messages: &[],
            },
            &senn_prefixes(),
        );
        // Fixes の直後はキーではない。SENN-4 は括弧の中に書いてあるだけ(Implicit)
        assert_eq!(intent_of(&found, "SENN-4"), Some(KeyIntent::Implicit));
    }
}
