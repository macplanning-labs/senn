//! Prolog アトムのクォート・検証

/// Prolog シングルクォート付きアトムに変換する。
///
/// エスケープ処理:
/// - `\` → `\\`
/// - `'` → `\'`
/// - 改行(`\n`)・タブ(`\t`)・CR(`\r`) → 半角スペース
/// - その他制御文字(`\0` 等) → 除去
/// - 日本語はそのまま
///
/// 結果は常に先頭末尾が `'` で囲まれている。
pub fn quote(s: &str) -> String {
    let mut result = String::from("'");

    for c in s.chars() {
        match c {
            '\\' => result.push_str("\\\\"),
            '\'' => result.push_str("\\'"),
            '\n' | '\t' | '\r' => result.push(' '),
            // その他の制御文字は除去
            c if c.is_control() => {}
            c => result.push(c),
        }
    }

    result.push('\'');
    result
}

/// キーが有効か検証する
///
/// 有効な形式: `[A-Za-z0-9-]+` で長さ 1〜32
pub fn valid_key(s: &str) -> bool {
    if s.is_empty() || s.len() > 32 {
        return false;
    }
    s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quote_basic() {
        assert_eq!(quote("hello"), "'hello'");
        assert_eq!(quote(""), "''");
    }

    #[test]
    fn test_quote_single_quote() {
        let result = quote("it's");
        assert_eq!(result, "'it\\'s'");
        // 出力が正しくエスケープされていることを確認(\ の後ろに ' がある)
        assert!(result.contains("\\'"));
        // 先頭末尾のクォートを除いた内側を確認
        let inner = &result[1..result.len() - 1];
        // 未エスケープの ' は無い(エスケープされた \' はOK)
        for window in inner.as_bytes().windows(2) {
            if window[0] != b'\\' && window[1] == b'\'' {
                panic!("found unescaped single quote");
            }
        }
    }

    #[test]
    fn test_quote_backslash() {
        let result = quote("path\\to\\file");
        assert_eq!(result, "'path\\\\to\\\\file'");
        // エスケープ確認
        let inner = &result[1..result.len() - 1];
        assert!(!inner.contains("'"));
    }

    #[test]
    fn test_quote_newline() {
        let result = quote("line1\nline2");
        assert_eq!(result, "'line1 line2'");
    }

    #[test]
    fn test_quote_tab() {
        let result = quote("col1\tcol2");
        assert_eq!(result, "'col1 col2'");
    }

    #[test]
    fn test_quote_cr() {
        let result = quote("line1\rline2");
        assert_eq!(result, "'line1 line2'");
    }

    #[test]
    fn test_quote_control_char() {
        let result = quote("text\u{0000}end");
        // ヌル文字は除去されるはず
        assert_eq!(result, "'textend'");
    }

    #[test]
    fn test_quote_japanese() {
        let result = quote("日本語");
        assert_eq!(result, "'日本語'");
    }

    #[test]
    fn test_quote_injection_attempt() {
        let result = quote("'; halt.");
        assert!(result.contains("\\'"));
        let inner = &result[1..result.len() - 1];
        // 未エスケープの ' がないことを確認
        for window in inner.as_bytes().windows(2) {
            if window[0] != b'\\' && window[1] == b'\'' {
                panic!("found unescaped single quote");
            }
        }
        // セミコロン・スペース・halt等はそのまま含まれる(これが重要)
        assert!(result.contains("halt"));
    }

    #[test]
    fn test_quote_mixed() {
        let result = quote("it's\nvalid");
        assert_eq!(result, "'it\\'s valid'");
    }

    #[test]
    fn test_valid_key_basic() {
        assert!(valid_key("DEMO-000001"));
        assert!(valid_key("ABC123"));
        assert!(valid_key("a"));
        assert!(valid_key("z"));
        assert!(valid_key("0"));
        assert!(valid_key("a-b-c"));
    }

    #[test]
    fn test_valid_key_invalid() {
        assert!(!valid_key("")); // 空文字
        assert!(!valid_key(&"a".repeat(33))); // 33文字(上限超過)
        assert!(!valid_key("DEMO_000001")); // アンダースコアはNG
        assert!(!valid_key("Demo 000001")); // スペースはNG
        assert!(!valid_key("demo.000001")); // ドットはNG
    }

    #[test]
    fn test_valid_key_boundary() {
        assert!(valid_key("a")); // 1文字OK
        assert!(valid_key(&"a".repeat(32))); // 32文字OK
        assert!(!valid_key(&"a".repeat(33))); // 33文字NG
    }
}
