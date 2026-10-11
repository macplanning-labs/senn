//! 本文・コメントからの決定的な事実検出
//!
//! 正規表現クレートは使わず、文字列操作だけで判定する純関数。
//! 状況サマリ(`summary.rs`)が「既存ドキュメント」「実行時エラーの有無」を決めるために使う。

/// 語の区切りになる文字(空白に加えて、これらで語を分ける)
fn is_delimiter(c: char) -> bool {
    matches!(
        c,
        '`' | '「'
            | '」'
            | '('
            | ')'
            | '（'
            | '）'
            | '、'
            | ','
            | ';'
            | ':'
            | '"'
            | '\''
            | '<'
            | '>'
            | '['
            | ']'
    )
}

/// `docs/` で始まり `.md` で終わる、安全な相対パスか
fn is_doc_path(tok: &str) -> bool {
    tok.chars().count() <= 200
        && tok.starts_with("docs/")
        && tok.ends_with(".md")
        // "docs/.md" のような中身の無いパスは除く
        && tok.len() > "docs/.md".len()
        && !tok.contains("..")
        // 出力プロトコルの区切り(|)や制御文字を含むパスは扱わない
        && !tok.contains('|')
        && !tok.chars().any(|c| c.is_control())
}

/// 本文・コメント中の `docs/**.md` 形式のパスを出現順に列挙する(最大10件・重複なし)。
///
/// Markdown リンク `[名](docs/a.md)` の中のパスも拾う。語末の `.` `。` は除いて判定する。
pub fn docs(text: &str) -> Vec<String> {
    const MAX: usize = 10;
    let mut found: Vec<String> = Vec::new();
    for raw in text.split(|c: char| c.is_whitespace() || is_delimiter(c)) {
        let tok = raw.trim_end_matches(['.', '。']);
        if !is_doc_path(tok) || found.iter().any(|d| d == tok) {
            continue;
        }
        found.push(tok.to_string());
        if found.len() >= MAX {
            break;
        }
    }
    found
}

/// 実行時エラーを示す語が含まれるか。
///
/// - 次のどれかで true:
///   - ASCII 小文字化した本文が `panic` / `unable to extract key` / `internal server error` を含む
///   - 本文が `タイムアウト` / `接続失敗` を含む
///   - `NaN` が単独の語(前後が ASCII 英数字でない)として現れる
///   - 単独の数字語として `401` `403` `500` `502` `503` `504` のいずれかが現れ、
///     かつ本文が `エラー` / `error` / `失敗` / `fail`(大文字小文字無視)のいずれかを含む
pub fn has_runtime_error_signal(text: &str) -> bool {
    const PHRASES: [&str; 3] = ["panic", "unable to extract key", "internal server error"];
    const HTTP_CODES: [&str; 6] = ["401", "403", "500", "502", "503", "504"];

    let lower = text.to_ascii_lowercase();
    if PHRASES.iter().any(|p| lower.contains(p)) {
        return true;
    }
    if text.contains("タイムアウト") || text.contains("接続失敗") {
        return true;
    }
    if has_standalone_word(text, "NaN") {
        return true;
    }
    let has_error_word = lower.contains("エラー")
        || lower.contains("error")
        || lower.contains("失敗")
        || lower.contains("fail");
    has_error_word
        && HTTP_CODES
            .iter()
            .any(|code| has_standalone_word(text, code))
}

/// `word` が前後に ASCII 英数字を伴わず単独で現れるか
fn has_standalone_word(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(i, _)| {
        let before = text.get(..i).and_then(|s| s.chars().next_back());
        let after = text.get(i + word.len()..).and_then(|s| s.chars().next());
        !before.is_some_and(|c| c.is_ascii_alphanumeric())
            && !after.is_some_and(|c| c.is_ascii_alphanumeric())
    })
}

/// 検索語の最大数(重複を除き、出現順を保つ)
const TERM_MAX: usize = 24;
/// これ以下の長さの塊は、そのまま検索語にする
const TERM_SHORT_MAX: usize = 8;
/// 長い塊を刻む窓の長さ・刻み幅(文字数)
const TERM_WINDOW: usize = 6;
const TERM_STEP: usize = 3;
/// 検索語にする最短の長さ(文字数)
const TERM_MIN: usize = 3;

/// 検索語を抽出する(成功事例・類似検索の候補絞り込みに使う)。
///
/// 英数字と `_` 以外の文字(空白・記号・句読点)で区切った塊ごとに判定する。
/// - 3文字未満の塊は捨てる
/// - 3〜8文字の塊は、そのまま検索語にする
/// - 9文字以上の塊は、6文字の窓を3文字刻みでずらした部分文字列を全部採る。
///   最後の窓が末尾に届かないときは、末尾6文字の窓も足す
///
/// 英字は小文字化しない。重複は除き、出現順を保ち、最大24語で打ち切る。
pub fn search_terms(text: &str) -> Vec<String> {
    let mut terms: Vec<String> = Vec::new();
    'segments: for seg in text.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
        for cand in term_candidates(seg) {
            if terms.len() >= TERM_MAX {
                break 'segments;
            }
            if !terms.contains(&cand) {
                terms.push(cand);
            }
        }
    }
    terms
}

/// 1つの塊から検索語の候補を出現順に作る(重複除去は呼び出し側)
fn term_candidates(seg: &str) -> Vec<String> {
    let chars: Vec<char> = seg.chars().collect();
    let len = chars.len();
    if len < TERM_MIN {
        return Vec::new();
    }
    if len <= TERM_SHORT_MAX {
        return vec![seg.to_string()];
    }
    let window = |start: usize| -> String {
        chars
            .get(start..start + TERM_WINDOW)
            .map(|w| w.iter().collect())
            .unwrap_or_default()
    };
    let mut out: Vec<String> = Vec::new();
    let mut start = 0;
    let mut covered_to = 0;
    while start + TERM_WINDOW <= len {
        out.push(window(start));
        covered_to = start + TERM_WINDOW;
        start += TERM_STEP;
    }
    if covered_to < len {
        out.push(window(len - TERM_WINDOW));
    }
    out
}

/// ファイルとして扱う拡張子(末尾がこのいずれかで終わるパスを拾う)
const FILE_EXTS: &[&str] = &[
    ".rs", ".py", ".ts", ".tsx", ".js", ".sh", ".sql", ".yml", ".yaml", ".toml", ".conf", ".md",
    ".html", ".css", ".pl",
];

/// 本文中のファイルパスとして扱えるか(URL・親ディレクトリ参照・区切り文字を除く)
fn is_file_path(tok: &str) -> bool {
    let len = tok.chars().count();
    (3..=120).contains(&len)
        && tok.contains('/')
        && !tok.contains("://")
        && !tok.contains("..")
        && !tok.contains('|')
        && !tok.chars().any(|c| c.is_control())
        && FILE_EXTS.iter().any(|ext| tok.ends_with(ext))
}

/// 本文・コメント中のファイルパス(`rust/src/a.rs` など)を出現順に列挙する(最大30件・重複なし)。
///
/// `docs()` と同じ区切りで語に分け、語末の `.` `。` は除いて判定する。
/// `https://…` のような URL は、`:` で割れて `x.test/a.md` の形に見えるため、空白で区切った語ごと除く。
pub fn files(text: &str) -> Vec<String> {
    const MAX: usize = 30;
    let mut found: Vec<String> = Vec::new();
    let toks = text
        .split(char::is_whitespace)
        .filter(|w| !w.contains("://"))
        .flat_map(|w| w.split(is_delimiter));
    for raw in toks {
        let tok = raw.trim_end_matches(['.', '。']);
        if !is_file_path(tok) || found.iter().any(|f| f == tok) {
            continue;
        }
        found.push(tok.to_string());
        if found.len() >= MAX {
            break;
        }
    }
    found
}

#[cfg(test)]
mod tests {
    #[test]
    fn docs_rejects_paths_with_protocol_separator_or_control_chars() {
        assert!(docs("docs/a|b.md").is_empty());
        assert!(docs("docs/a\u{7}b.md").is_empty());
        assert_eq!(docs("docs/ok.md"), vec!["docs/ok.md".to_string()]);
    }

    use super::*;

    // ---- docs ----

    #[test]
    fn docs_plain_and_backtick_and_japanese_delimiters() {
        assert_eq!(docs("docs/a.md"), vec!["docs/a.md"]);
        assert_eq!(docs("`docs/x.md`"), vec!["docs/x.md"]);
        assert_eq!(docs("「docs/y.md」を参照"), vec!["docs/y.md"]);
        assert_eq!(docs("仕様は(docs/z.md)、"), vec!["docs/z.md"]);
    }

    #[test]
    fn docs_markdown_link_and_trailing_japanese_period() {
        assert_eq!(docs("[設計](docs/design/y.md)。"), vec!["docs/design/y.md"]);
    }

    #[test]
    fn docs_trailing_ascii_period_is_trimmed() {
        assert_eq!(docs("see docs/a.md."), vec!["docs/a.md"]);
    }

    #[test]
    fn docs_rejects_parent_traversal_and_other_prefixes() {
        assert!(docs("../docs/z.md").is_empty());
        assert!(docs("docs/../secret.md").is_empty());
        assert!(docs("src/docs/a.md").is_empty());
        assert!(docs("docs/a.txt").is_empty());
        assert!(docs("docs/.md").is_empty());
    }

    #[test]
    fn docs_rejects_too_long_path() {
        let long = format!("docs/{}.md", "a".repeat(200));
        assert!(docs(&long).is_empty());
        let ok = format!("docs/{}.md", "a".repeat(100));
        assert_eq!(docs(&ok).len(), 1);
    }

    #[test]
    fn docs_removes_duplicates_keeping_first_order() {
        assert_eq!(
            docs("docs/b.md docs/a.md docs/b.md"),
            vec!["docs/b.md", "docs/a.md"]
        );
    }

    #[test]
    fn docs_caps_at_ten() {
        let text: String = (0..11).map(|i| format!("docs/{i}.md ")).collect();
        let found = docs(&text);
        assert_eq!(found.len(), 10);
        assert_eq!(found[0], "docs/0.md");
        assert_eq!(found[9], "docs/9.md");
    }

    #[test]
    fn docs_empty_input() {
        assert!(docs("").is_empty());
        assert!(docs("関係のない本文です").is_empty());
    }

    // ---- has_runtime_error_signal ----

    #[test]
    fn number_without_error_word_is_not_a_signal() {
        assert!(!has_runtime_error_signal("500 件を処理"));
    }

    #[test]
    fn http_code_with_japanese_error_word_is_a_signal() {
        assert!(has_runtime_error_signal("500エラー"));
    }

    #[test]
    fn http_code_with_english_error_word_is_a_signal() {
        assert!(has_runtime_error_signal("HTTP 500 error"));
    }

    #[test]
    fn http_code_with_fail_or_failure_word_is_a_signal() {
        assert!(has_runtime_error_signal("503 Fail on save"));
        assert!(has_runtime_error_signal("403 で失敗"));
    }

    #[test]
    fn digits_embedded_in_longer_number_are_not_a_signal() {
        assert!(!has_runtime_error_signal("5001 エラー"));
        assert!(!has_runtime_error_signal("1500 error"));
    }

    #[test]
    fn panic_is_a_signal_case_insensitively() {
        assert!(has_runtime_error_signal("panic at src/main.rs"));
        assert!(has_runtime_error_signal("PANIC! something"));
    }

    #[test]
    fn fixed_phrases_are_signals() {
        assert!(has_runtime_error_signal(
            "Unable To Extract Key from response"
        ));
        assert!(has_runtime_error_signal("Internal Server Error"));
        assert!(has_runtime_error_signal("処理がタイムアウトした"));
        assert!(has_runtime_error_signal("接続失敗のため中断"));
    }

    #[test]
    fn nan_as_standalone_word_is_a_signal() {
        assert!(has_runtime_error_signal("値が NaN になる"));
        assert!(has_runtime_error_signal("NaN"));
        assert!(has_runtime_error_signal("(NaN)"));
    }

    #[test]
    fn nan_inside_word_is_not_a_signal() {
        assert!(!has_runtime_error_signal("banana"));
        assert!(!has_runtime_error_signal("NaNa"));
        assert!(!has_runtime_error_signal("xNaN"));
    }

    #[test]
    fn empty_text_is_not_a_signal() {
        assert!(!has_runtime_error_signal(""));
    }

    #[test]
    fn error_word_alone_is_not_a_signal() {
        assert!(!has_runtime_error_signal("エラー処理を追加した"));
        assert!(!has_runtime_error_signal("error handling"));
    }

    #[test]
    fn docs_and_signal_are_independent_checks() {
        // パス中の数字は実行時エラーとして扱わない
        assert!(!has_runtime_error_signal("docs/500.md を更新"));
    }

    fn owned(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    // ---- search_terms ----

    #[test]
    fn search_terms_length_boundaries() {
        // 2文字は捨て、3文字・8文字はそのまま、9文字は6文字窓が2つ(末尾の窓は届くので足さない)
        let cases: Vec<(&str, Vec<&str>)> = vec![
            ("", vec![]),
            ("ab", vec![]),
            ("abc", vec!["abc"]),
            ("abcdefgh", vec!["abcdefgh"]),
            ("abcdefghi", vec!["abcdef", "defghi"]),
        ];
        for (text, want) in cases {
            assert_eq!(search_terms(text), owned(&want), "text={text}");
        }
    }

    #[test]
    fn search_terms_long_japanese_segment_uses_windows_and_tail() {
        // 11文字: 0,3 からの窓(0..6, 3..9)と、末尾に届かないので末尾6文字(5..11)を足す
        assert_eq!(
            search_terms("サブチケットの完了確認"),
            owned(&["サブチケット", "ケットの完了", "トの完了確認"])
        );
    }

    #[test]
    fn search_terms_mixed_ascii_and_japanese_in_one_segment() {
        // 英数字と日本語は同じ塊になる(10文字)
        assert_eq!(
            search_terms("Docker起動失敗"),
            owned(&["Docker", "ker起動失", "er起動失敗"])
        );
        // _ は塊の一部(10文字)
        assert_eq!(
            search_terms("ABC_123xyz"),
            owned(&["ABC_12", "_123xy", "123xyz"])
        );
    }

    #[test]
    fn search_terms_keeps_ascii_case() {
        assert_eq!(search_terms("Rust_Code"), owned(&["Rust_C", "t_Code"]));
        assert_eq!(search_terms("ABCdef"), owned(&["ABCdef"]));
    }

    #[test]
    fn search_terms_splits_on_symbols_and_drops_short_pieces() {
        assert!(search_terms("!!! ?? 、。 --").is_empty());
        assert_eq!(search_terms("Rust-Code/ab"), owned(&["Rust", "Code"]));
        assert_eq!(
            search_terms("ABCdef 日本語テスト"),
            owned(&["ABCdef", "日本語テスト"])
        );
    }

    #[test]
    fn search_terms_removes_duplicates_keeping_first_order() {
        assert_eq!(search_terms("テスト テスト"), owned(&["テスト"]));
        assert_eq!(
            search_terms("bbb aaa bbb ccc aaa"),
            owned(&["bbb", "aaa", "ccc"])
        );
    }

    #[test]
    fn search_terms_caps_at_24() {
        let text: String = (0..30).map(|i| format!("{i:03} ")).collect();
        let got = search_terms(&text);
        assert_eq!(got.len(), 24);
        assert_eq!(got[0], "000");
        assert_eq!(got[23], "023");
    }

    // ---- files ----

    #[test]
    fn files_empty_and_symbol_only_inputs() {
        assert!(files("").is_empty());
        assert!(files("ab").is_empty());
        assert!(files("。、 ()").is_empty());
    }

    #[test]
    fn files_picks_paths_with_known_extensions() {
        assert_eq!(
            files("rust/src/a.rs frontend/src/b.tsx"),
            owned(&["rust/src/a.rs", "frontend/src/b.tsx"])
        );
        assert_eq!(
            files("docs/design/x.md を更新"),
            owned(&["docs/design/x.md"])
        );
        assert_eq!(files("「rust/src/a.rs」を修正"), owned(&["rust/src/a.rs"]));
    }

    #[test]
    fn files_rejects_urls_traversal_and_bad_shapes() {
        assert!(files("https://x.test/a.md").is_empty());
        assert!(files("../x.rs").is_empty());
        assert!(files("rust/src/a.rsx").is_empty());
        assert!(files("rust/src/a.txt").is_empty());
        assert!(files("rust/src/a.rs|x").is_empty());
        assert!(files("rust/src/a\u{7}.rs").is_empty());
        // / を含まないものは、拡張子が合っていても除く
        assert!(files("a.rs").is_empty());
    }

    #[test]
    fn files_strips_trailing_japanese_and_ascii_period() {
        assert_eq!(files("docs/a.md。"), owned(&["docs/a.md"]));
        assert_eq!(files("rust/src/a.rs."), owned(&["rust/src/a.rs"]));
    }

    #[test]
    fn files_length_limit_is_120_chars() {
        // "a/" + x*115 + ".rs" = 120文字は通す、121文字は除く
        let ok = format!("a/{}.rs", "x".repeat(115));
        let ng = format!("a/{}.rs", "x".repeat(116));
        assert_eq!(files(&ok).len(), 1);
        assert!(files(&ng).is_empty());
    }

    #[test]
    fn files_removes_duplicates_keeping_first_order() {
        assert_eq!(
            files("rust/a.rs rust/b.rs rust/a.rs"),
            owned(&["rust/a.rs", "rust/b.rs"])
        );
    }

    #[test]
    fn files_caps_at_30() {
        let text: String = (0..31).map(|i| format!("docs/{i}.md ")).collect();
        let got = files(&text);
        assert_eq!(got.len(), 30);
        assert_eq!(got[29], "docs/29.md");
    }
}
