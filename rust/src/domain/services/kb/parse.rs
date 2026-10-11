//! Prolog 出力パーサー(クローズ判定)
//!
//! Scryer Prolog は、エラー・警告・ゴール失敗を **stdout に出し、終了コードは常に 0** にする。
//! そのため「判定行以外が1行でも混ざったら失敗」「終端行 END が無ければ失敗」として、
//! 異常を必ず検知する(誤って「クローズ可」と読み取らないための最後の砦)。

use super::{Blocker, CloseCheckResult, KbError};

/// Prolog の行プロトコル出力を CloseCheckResult に変換する。
///
/// 受け付ける形式(空行は無視):
/// - `OK|<key>`                    クローズ可能(これ1行だけ)
/// - `NG|<key>|<code>|<arg>`       クローズ不可の理由(1行1件、1件以上)
/// - `END|<n>`                     終端。n は直前までの OK/NG 行数と一致し、最後の行であること
/// - `ERR|<reason>`                エンジンが判定不能と報告(常に Err)
///
/// 上記以外の行(Prolog の警告・エラー文を含む)、`END` の欠落・不一致、
/// `OK` と `NG` の混在、別チケットのキーはすべて `Err(BadOutput)`。
pub fn parse_close(out: &str, key: &str) -> Result<CloseCheckResult, KbError> {
    let bad = |msg: String| Err(KbError::BadOutput(msg));

    let mut ok_lines = 0usize;
    let mut blockers: Vec<Blocker> = Vec::new();
    let mut ng_lines = 0usize;
    let mut end: Option<usize> = None;

    for line in out.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // END より後ろに何かが続く出力は不正
        if end.is_some() {
            return bad("output continues after END line".to_string());
        }
        let parts: Vec<&str> = line.split('|').collect();
        match parts.as_slice() {
            ["OK", k] => {
                if *k != key {
                    return bad(format!("key mismatch in OK line: {k}"));
                }
                ok_lines += 1;
            }
            ["NG", k, code, arg] => {
                if *k != key {
                    return bad(format!("key mismatch in NG line: {k}"));
                }
                ng_lines += 1;
                let b = Blocker {
                    code: (*code).to_string(),
                    arg: (*arg).to_string(),
                };
                // 同じ (code, arg) の重複だけ除外する。code が同じで arg が違うものは別件
                if !blockers.contains(&b) {
                    blockers.push(b);
                }
            }
            ["END", n] => {
                let n: usize = n.parse().map_err(|_| {
                    KbError::BadOutput("END line count is not a number".to_string())
                })?;
                end = Some(n);
            }
            ["ERR", reason] => {
                return bad(format!("engine reported error: {reason}"));
            }
            _ => {
                // Prolog の警告・構文エラー・存在エラーなど、プロトコル外の行
                let head: String = line.chars().take(80).collect();
                return bad(format!("unexpected line: {head}"));
            }
        }
    }

    let Some(end) = end else {
        return bad("missing END line".to_string());
    };
    if end != ok_lines + ng_lines {
        return bad(format!(
            "END count {end} does not match {} result lines",
            ok_lines + ng_lines
        ));
    }
    match (ok_lines, ng_lines) {
        (1, 0) => Ok(CloseCheckResult {
            can_close: true,
            blockers: Vec::new(),
        }),
        (0, n) if n >= 1 => Ok(CloseCheckResult {
            can_close: false,
            blockers,
        }),
        _ => bad("result must be a single OK line or one or more NG lines".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const K: &str = "DEMO-000001";

    fn blockers(out: &str) -> Vec<(String, String)> {
        parse_close(out, K)
            .expect("parse ok")
            .blockers
            .into_iter()
            .map(|b| (b.code, b.arg))
            .collect()
    }

    #[test]
    fn ok_only() {
        let r = parse_close("OK|DEMO-000001\nEND|1\n", K).expect("ok");
        assert!(r.can_close);
        assert!(r.blockers.is_empty());
    }

    #[test]
    fn ng_only_keeps_order() {
        let out = "NG|DEMO-000001|missing_section|まとめ\nNG|DEMO-000001|open_subtask|DEMO-000003\nEND|2\n";
        let r = parse_close(out, K).expect("ok");
        assert!(!r.can_close);
        assert_eq!(
            blockers(out),
            vec![
                ("missing_section".to_string(), "まとめ".to_string()),
                ("open_subtask".to_string(), "DEMO-000003".to_string()),
            ]
        );
    }

    #[test]
    fn same_code_different_arg_are_both_kept() {
        // 未完了サブタスクが複数ある場合に、2件目以降が消えてはいけない
        let out = "NG|DEMO-000001|open_subtask|DEMO-000002\nNG|DEMO-000001|open_subtask|DEMO-000003\nEND|2\n";
        assert_eq!(blockers(out).len(), 2);
    }

    #[test]
    fn identical_lines_are_deduplicated_but_end_still_counts_lines() {
        let out = "NG|DEMO-000001|no_assignee|none\nNG|DEMO-000001|no_assignee|none\nEND|2\n";
        assert_eq!(blockers(out).len(), 1);
    }

    #[test]
    fn blank_lines_are_ignored() {
        let r = parse_close("\nOK|DEMO-000001\n\nEND|1\n\n", K).expect("ok");
        assert!(r.can_close);
    }

    #[test]
    fn empty_output_is_error() {
        assert!(parse_close("", K).is_err());
        assert!(parse_close("\n\n", K).is_err());
    }

    #[test]
    fn missing_end_is_error() {
        assert!(parse_close("OK|DEMO-000001\n", K).is_err());
        assert!(parse_close("NG|DEMO-000001|a|b\n", K).is_err());
    }

    #[test]
    fn end_count_mismatch_is_error() {
        assert!(parse_close("OK|DEMO-000001\nEND|2\n", K).is_err());
        assert!(parse_close("NG|DEMO-000001|a|b\nEND|0\n", K).is_err());
        assert!(parse_close("OK|DEMO-000001\nEND|x\n", K).is_err());
    }

    #[test]
    fn output_after_end_is_error() {
        assert!(parse_close("OK|DEMO-000001\nEND|1\nOK|DEMO-000001\n", K).is_err());
    }

    #[test]
    fn mixed_ok_and_ng_is_error() {
        let out = "OK|DEMO-000001\nNG|DEMO-000001|a|b\nEND|2\n";
        assert!(parse_close(out, K).is_err());
    }

    #[test]
    fn two_ok_lines_is_error() {
        assert!(parse_close("OK|DEMO-000001\nOK|DEMO-000001\nEND|2\n", K).is_err());
    }

    #[test]
    fn key_mismatch_is_error() {
        assert!(parse_close("OK|DEMO-000002\nEND|1\n", K).is_err());
        assert!(parse_close("NG|DEMO-000002|a|b\nEND|1\n", K).is_err());
    }

    #[test]
    fn engine_err_line_is_error() {
        assert!(parse_close("ERR|fact_mismatch\n", K).is_err());
        assert!(parse_close("ERR|fact_mismatch\nOK|DEMO-000001\nEND|1\n", K).is_err());
    }

    #[test]
    fn prolog_warning_or_error_lines_are_error() {
        // 実測した Scryer の出力例: ゴール失敗・構文エラー・存在エラー
        let cases = [
            "% Warning: initialization failed for: run_close\n",
            "   error(syntax_error(invalid_single_quoted_character),read_term/3).\nOK|DEMO-000001\nEND|1\n",
            "no_such_goal causes: error(existence_error(procedure,no_such_goal/0),no_such_goal/0)\n",
        ];
        for c in cases {
            assert!(parse_close(c, K).is_err(), "must fail: {c}");
        }
    }

    #[test]
    fn malformed_field_counts_are_error() {
        assert!(parse_close("OK|DEMO-000001|extra\nEND|1\n", K).is_err());
        assert!(parse_close("NG|DEMO-000001|code\nEND|1\n", K).is_err());
        assert!(parse_close("NG|DEMO-000001|code|a|b\nEND|1\n", K).is_err());
        assert!(parse_close("END\n", K).is_err());
    }

    #[test]
    fn lowercase_or_prefixed_lines_are_not_accepted() {
        assert!(parse_close("ok|DEMO-000001\nEND|1\n", K).is_err());
        assert!(parse_close("OKAY|DEMO-000001\nEND|1\n", K).is_err());
    }
}
