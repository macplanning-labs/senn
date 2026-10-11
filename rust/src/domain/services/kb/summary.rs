//! 状況サマリ(6項目)の型・出力パーサ・整形
//!
//! Prolog(`summary.pl`)は固定語彙の行プロトコルだけを返す。文章はここで固定文から組み立てる。
//! パーサは `parse.rs` と同じく厳格にする。Scryer は異常を stdout に出して終了コード 0 で終わるため、
//! 形式外の行が1行でもあれば必ず Err にする。

use super::messages::{blocker_message, consistency_message};
use super::{Blocker, KbError};

/// フェーズ(状況の段階)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Investigating,
    Implementing,
    Verifying,
    Done,
}

/// 推奨アクション
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    BugFix,
    ContinueImpl,
    VerifyOnly,
    CloseReady,
}

impl Action {
    /// Prolog 出力・ミッション文で使う識別子
    pub fn as_str(&self) -> &'static str {
        match self {
            Action::BugFix => "bug_fix",
            Action::ContinueImpl => "continue_impl",
            Action::VerifyOnly => "verify_only",
            Action::CloseReady => "close_ready",
        }
    }
}

/// 状況サマリ(Prolog の判定結果)
#[derive(Debug, Clone, PartialEq)]
pub struct SituationSummary {
    pub phase: Phase,
    pub action: Action,
    pub remain: Vec<Blocker>,
    pub docs: Vec<String>,
    pub consistency: Vec<Blocker>,
}

fn bad(msg: impl Into<String>) -> KbError {
    KbError::BadOutput(msg.into())
}

fn parse_phase(v: &str) -> Result<Phase, KbError> {
    match v {
        "investigating" => Ok(Phase::Investigating),
        "implementing" => Ok(Phase::Implementing),
        "verifying" => Ok(Phase::Verifying),
        "done" => Ok(Phase::Done),
        _ => Err(bad(format!("unknown phase: {v}"))),
    }
}

fn parse_action(v: &str) -> Result<Action, KbError> {
    match v {
        "bug_fix" => Ok(Action::BugFix),
        "continue_impl" => Ok(Action::ContinueImpl),
        "verify_only" => Ok(Action::VerifyOnly),
        "close_ready" => Ok(Action::CloseReady),
        _ => Err(bad(format!("unknown action: {v}"))),
    }
}

/// コード・引数のフィールドは空を許さない
fn code_arg(code: &str, arg: &str) -> Result<Blocker, KbError> {
    if code.is_empty() {
        return Err(bad("empty code field"));
    }
    Ok(Blocker {
        code: code.to_string(),
        arg: arg.to_string(),
    })
}

/// Prolog の行プロトコル出力を SituationSummary に変換する。
///
/// 受け付ける形式(空行は無視):
/// - `PHASE|<investigating|implementing|verifying|done>`   ちょうど1行
/// - `ACTION|<bug_fix|continue_impl|verify_only|close_ready>`  ちょうど1行
/// - `REMAIN|<code>|<arg>`        0件以上
/// - `DOC|<path>`                 0件以上
/// - `CONSISTENCY|<code>|<arg>`   0件以上
/// - `END|<n>`                    最後の行。n は END 以外の行数と一致すること
/// - `ERR|<reason>`               エンジンが判定不能と報告(1行でもあれば常に Err)
///
/// 上記以外の行、未知の値、フィールド数の違い、END の欠落・不一致・END 後の行、
/// PHASE/ACTION の重複・欠落はすべて `Err(BadOutput)`。
pub fn parse_summary(out: &str) -> Result<SituationSummary, KbError> {
    let mut phase: Option<Phase> = None;
    let mut action: Option<Action> = None;
    let mut remain: Vec<Blocker> = Vec::new();
    let mut docs: Vec<String> = Vec::new();
    let mut consistency: Vec<Blocker> = Vec::new();
    let mut body_lines = 0usize;
    let mut end: Option<usize> = None;

    for line in out.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // END より後ろに何かが続く出力は不正
        if end.is_some() {
            return Err(bad("output continues after END line"));
        }
        let parts: Vec<&str> = line.split('|').collect();
        if parts.first() != Some(&"END") {
            body_lines += 1;
        }
        match parts.as_slice() {
            ["ERR", ..] => {
                return Err(bad(format!("engine reported error: {line}")));
            }
            ["PHASE", v] => {
                if phase.is_some() {
                    return Err(bad("duplicate PHASE line"));
                }
                phase = Some(parse_phase(v)?);
            }
            ["ACTION", v] => {
                if action.is_some() {
                    return Err(bad("duplicate ACTION line"));
                }
                action = Some(parse_action(v)?);
            }
            ["REMAIN", code, arg] => remain.push(code_arg(code, arg)?),
            ["CONSISTENCY", code, arg] => consistency.push(code_arg(code, arg)?),
            ["DOC", path] => {
                if path.is_empty() {
                    return Err(bad("empty DOC path"));
                }
                docs.push((*path).to_string());
            }
            ["END", n] => {
                let n: usize = n
                    .parse()
                    .map_err(|_| bad("END line count is not a number"))?;
                end = Some(n);
            }
            _ => {
                // Prolog の警告・構文エラー・存在エラーなど、プロトコル外の行
                let head: String = line.chars().take(80).collect();
                return Err(bad(format!("unexpected line: {head}")));
            }
        }
    }

    let end = end.ok_or_else(|| bad("missing END line"))?;
    if end != body_lines {
        return Err(bad(format!(
            "END count {end} does not match {body_lines} result lines"
        )));
    }
    let phase = phase.ok_or_else(|| bad("missing PHASE line"))?;
    let action = action.ok_or_else(|| bad("missing ACTION line"))?;

    Ok(SituationSummary {
        phase,
        action,
        remain,
        docs,
        consistency,
    })
}

/// 状況サマリを、既存と同じ見出し形式の6行に整形する(末尾改行なし)。
pub fn render_summary(s: &SituationSummary, comment_count: i64) -> String {
    let phase = match s.phase {
        Phase::Investigating => "調査中",
        Phase::Implementing => "実装中",
        Phase::Verifying => "検証待ち",
        Phase::Done => "完了相当",
    };

    let remain = if s.remain.is_empty() {
        "なし（クローズ判定可）".to_string()
    } else {
        s.remain
            .iter()
            .map(|b| blocker_message(&b.code, &b.arg))
            .collect::<Vec<_>>()
            .join("、")
    };

    let action_desc = match s.action {
        Action::BugFix => "既存実装の不具合修正が主",
        Action::ContinueImpl => "未完の実装を続ける",
        Action::VerifyOnly => "実装済みの検証・DoD確認が主",
        Action::CloseReady => "残作業なしでクローズ判断が主",
    };

    let docs = if s.docs.is_empty() {
        "記載なし".to_string()
    } else {
        format!(
            "{}（既存を更新/参照。新規作成し直さない）",
            s.docs.join("、")
        )
    };

    let comments = if comment_count <= 0 {
        "特になし".to_string()
    } else {
        format!("コメント {comment_count} 件（全文を読んで論点を把握してください）")
    };

    let consistency = if s.consistency.is_empty() {
        "整合".to_string()
    } else {
        s.consistency
            .iter()
            .map(|b| consistency_message(&b.code, &b.arg))
            .collect::<Vec<_>>()
            .join("、")
    };

    [
        format!("- フェーズ: {phase}"),
        format!("- 残作業: {remain}"),
        format!("- 推奨アクション: {} （{action_desc}）", s.action.as_str()),
        format!("- 既存ドキュメント: {docs}"),
        format!("- コメント論点: {comments}"),
        format!("- ステータス整合: {consistency}"),
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(code: &str, arg: &str) -> Blocker {
        Blocker {
            code: code.to_string(),
            arg: arg.to_string(),
        }
    }

    fn base() -> SituationSummary {
        SituationSummary {
            phase: Phase::Implementing,
            action: Action::ContinueImpl,
            remain: vec![],
            docs: vec![],
            consistency: vec![],
        }
    }

    // ---- parse_summary: 正常系 ----

    #[test]
    fn parse_minimal_ok() {
        let out = "PHASE|done\nACTION|close_ready\nEND|2\n";
        let s = parse_summary(out).expect("ok");
        assert_eq!(s.phase, Phase::Done);
        assert_eq!(s.action, Action::CloseReady);
        assert!(s.remain.is_empty());
        assert!(s.docs.is_empty());
        assert!(s.consistency.is_empty());
    }

    #[test]
    fn parse_full_ok_keeps_order() {
        let out = "PHASE|verifying\n\
                   ACTION|verify_only\n\
                   REMAIN|missing_section|検証結果\n\
                   REMAIN|open_subtask|DEMO-000003\n\
                   DOC|docs/design/a.md\n\
                   DOC|docs/b.md\n\
                   CONSISTENCY|ready_but_open|none\n\
                   END|7\n";
        let s = parse_summary(out).expect("ok");
        assert_eq!(s.phase, Phase::Verifying);
        assert_eq!(s.action, Action::VerifyOnly);
        assert_eq!(
            s.remain,
            vec![
                b("missing_section", "検証結果"),
                b("open_subtask", "DEMO-000003")
            ]
        );
        assert_eq!(s.docs, vec!["docs/design/a.md", "docs/b.md"]);
        assert_eq!(s.consistency, vec![b("ready_but_open", "none")]);
    }

    #[test]
    fn parse_all_phase_and_action_values() {
        let phases = [
            ("investigating", Phase::Investigating),
            ("implementing", Phase::Implementing),
            ("verifying", Phase::Verifying),
            ("done", Phase::Done),
        ];
        let actions = [
            ("bug_fix", Action::BugFix),
            ("continue_impl", Action::ContinueImpl),
            ("verify_only", Action::VerifyOnly),
            ("close_ready", Action::CloseReady),
        ];
        for (ps, pv) in phases {
            for (as_, av) in actions {
                let out = format!("PHASE|{ps}\nACTION|{as_}\nEND|2\n");
                let s = parse_summary(&out).expect("ok");
                assert_eq!(s.phase, pv);
                assert_eq!(s.action, av);
            }
        }
    }

    #[test]
    fn parse_blank_lines_and_surrounding_spaces_are_ignored() {
        let out = "\n  PHASE|investigating  \n\nACTION|continue_impl\n\nEND|2\n\n";
        let s = parse_summary(out).expect("ok");
        assert_eq!(s.phase, Phase::Investigating);
    }

    #[test]
    fn parse_duplicate_same_code_and_arg_lines_are_kept() {
        // parse.rs と違い、サマリは件数をそのまま保持する(END は行数と一致)
        let out = "PHASE|implementing\nACTION|continue_impl\nREMAIN|no_assignee|none\nREMAIN|no_assignee|none\nEND|4\n";
        let s = parse_summary(out).expect("ok");
        assert_eq!(s.remain.len(), 2);
    }

    #[test]
    fn action_as_str_values() {
        assert_eq!(Action::BugFix.as_str(), "bug_fix");
        assert_eq!(Action::ContinueImpl.as_str(), "continue_impl");
        assert_eq!(Action::VerifyOnly.as_str(), "verify_only");
        assert_eq!(Action::CloseReady.as_str(), "close_ready");
    }

    // ---- parse_summary: 異常系 ----

    #[test]
    fn parse_empty_output_is_error() {
        assert!(parse_summary("").is_err());
        assert!(parse_summary("\n\n").is_err());
    }

    #[test]
    fn parse_missing_end_is_error() {
        assert!(parse_summary("PHASE|done\nACTION|close_ready\n").is_err());
    }

    #[test]
    fn parse_end_count_mismatch_is_error() {
        assert!(parse_summary("PHASE|done\nACTION|close_ready\nEND|3\n").is_err());
        assert!(parse_summary("PHASE|done\nACTION|close_ready\nEND|1\n").is_err());
        assert!(parse_summary("PHASE|done\nACTION|close_ready\nEND|x\n").is_err());
        assert!(parse_summary("PHASE|done\nACTION|close_ready\nEND|-1\n").is_err());
    }

    #[test]
    fn parse_output_after_end_is_error() {
        assert!(parse_summary("PHASE|done\nACTION|close_ready\nEND|2\nDOC|docs/a.md\n").is_err());
        assert!(parse_summary("PHASE|done\nACTION|close_ready\nEND|2\nEND|2\n").is_err());
    }

    #[test]
    fn parse_err_line_is_always_error() {
        assert!(parse_summary("ERR|fact_mismatch\n").is_err());
        assert!(parse_summary("PHASE|done\nACTION|close_ready\nERR|x\nEND|3\n").is_err());
        assert!(parse_summary("PHASE|done\nACTION|close_ready\nEND|2\nERR|x\n").is_err());
    }

    #[test]
    fn parse_prolog_warning_lines_are_error() {
        let cases = [
            "% Warning: initialization failed for: run_summary\n",
            "PHASE|done\n% Warning: initialization failed for: run_summary\nACTION|close_ready\nEND|3\n",
            "   error(syntax_error(invalid_single_quoted_character),read_term/3).\nPHASE|done\nACTION|close_ready\nEND|2\n",
            "no_such_goal causes: error(existence_error(procedure,no_such_goal/0),no_such_goal/0)\n",
        ];
        for c in cases {
            assert!(parse_summary(c).is_err(), "must fail: {c}");
        }
    }

    #[test]
    fn parse_unknown_phase_or_action_is_error() {
        assert!(parse_summary("PHASE|unknown\nACTION|close_ready\nEND|2\n").is_err());
        assert!(parse_summary("PHASE|done\nACTION|nope\nEND|2\n").is_err());
        assert!(parse_summary("PHASE|DONE\nACTION|close_ready\nEND|2\n").is_err());
    }

    #[test]
    fn parse_duplicate_phase_or_action_is_error() {
        assert!(parse_summary("PHASE|done\nPHASE|done\nACTION|close_ready\nEND|3\n").is_err());
        assert!(parse_summary("PHASE|done\nACTION|close_ready\nACTION|bug_fix\nEND|3\n").is_err());
    }

    #[test]
    fn parse_missing_phase_or_action_is_error() {
        assert!(parse_summary("ACTION|close_ready\nEND|1\n").is_err());
        assert!(parse_summary("PHASE|done\nEND|1\n").is_err());
        assert!(parse_summary("REMAIN|open_subtask|DEMO-000002\nEND|1\n").is_err());
    }

    #[test]
    fn parse_malformed_field_counts_are_error() {
        assert!(parse_summary("PHASE|done|extra\nACTION|close_ready\nEND|2\n").is_err());
        assert!(parse_summary("PHASE|done\nACTION\nEND|2\n").is_err());
        assert!(parse_summary("PHASE|done\nACTION|close_ready\nREMAIN|code\nEND|3\n").is_err());
        assert!(parse_summary("PHASE|done\nACTION|close_ready\nREMAIN|a|b|c\nEND|3\n").is_err());
        assert!(parse_summary("PHASE|done\nACTION|close_ready\nDOC\nEND|3\n").is_err());
        assert!(parse_summary("PHASE|done\nACTION|close_ready\nCONSISTENCY|a\nEND|3\n").is_err());
        assert!(parse_summary("END\n").is_err());
    }

    #[test]
    fn parse_empty_code_or_path_is_error() {
        assert!(parse_summary("PHASE|done\nACTION|close_ready\nREMAIN||x\nEND|3\n").is_err());
        assert!(parse_summary("PHASE|done\nACTION|close_ready\nCONSISTENCY||x\nEND|3\n").is_err());
        assert!(parse_summary("PHASE|done\nACTION|close_ready\nDOC|\nEND|3\n").is_err());
    }

    #[test]
    fn parse_lowercase_or_prefixed_keywords_are_not_accepted() {
        assert!(parse_summary("phase|done\nACTION|close_ready\nEND|2\n").is_err());
        assert!(parse_summary("PHASES|done\nACTION|close_ready\nEND|2\n").is_err());
        assert!(parse_summary("PHASE|done\nACTION|close_ready\nend|2\n").is_err());
    }

    #[test]
    fn parse_error_message_contains_no_secrets_only_protocol_text() {
        // 異常時のメッセージに入力行の全文を出しすぎない(先頭80文字まで)
        let long = "x".repeat(300);
        let out = format!("{long}\nEND|1\n");
        match parse_summary(&out) {
            Err(KbError::BadOutput(msg)) => assert!(msg.chars().count() < 120),
            other => panic!("expected BadOutput, got {other:?}"),
        }
    }

    // ---- render_summary ----

    fn lines(s: &str) -> Vec<String> {
        s.split('\n').map(|l| l.to_string()).collect()
    }

    #[test]
    fn render_phase_labels_for_all_phases() {
        let cases = [
            (Phase::Investigating, "- フェーズ: 調査中"),
            (Phase::Implementing, "- フェーズ: 実装中"),
            (Phase::Verifying, "- フェーズ: 検証待ち"),
            (Phase::Done, "- フェーズ: 完了相当"),
        ];
        for (phase, expected) in cases {
            let s = SituationSummary { phase, ..base() };
            assert_eq!(lines(&render_summary(&s, 0))[0], expected);
        }
    }

    #[test]
    fn render_action_labels_for_all_actions() {
        let cases = [
            (
                Action::BugFix,
                "- 推奨アクション: bug_fix （既存実装の不具合修正が主）",
            ),
            (
                Action::ContinueImpl,
                "- 推奨アクション: continue_impl （未完の実装を続ける）",
            ),
            (
                Action::VerifyOnly,
                "- 推奨アクション: verify_only （実装済みの検証・DoD確認が主）",
            ),
            (
                Action::CloseReady,
                "- 推奨アクション: close_ready （残作業なしでクローズ判断が主）",
            ),
        ];
        for (action, expected) in cases {
            let s = SituationSummary { action, ..base() };
            assert_eq!(lines(&render_summary(&s, 0))[2], expected);
        }
    }

    #[test]
    fn render_every_phase_action_pair_has_six_lines() {
        let phases = [
            Phase::Investigating,
            Phase::Implementing,
            Phase::Verifying,
            Phase::Done,
        ];
        let actions = [
            Action::BugFix,
            Action::ContinueImpl,
            Action::VerifyOnly,
            Action::CloseReady,
        ];
        for phase in phases {
            for action in actions {
                let s = SituationSummary {
                    phase,
                    action,
                    ..base()
                };
                let out = render_summary(&s, 3);
                let ls = lines(&out);
                assert_eq!(ls.len(), 6);
                assert!(ls.iter().all(|l| l.starts_with("- ")));
                assert!(!out.ends_with('\n'));
            }
        }
    }

    #[test]
    fn render_empty_remain_says_closable() {
        let out = render_summary(&base(), 0);
        assert_eq!(lines(&out)[1], "- 残作業: なし（クローズ判定可）");
    }

    #[test]
    fn render_single_remain() {
        let s = SituationSummary {
            remain: vec![b("no_assignee", "none")],
            ..base()
        };
        assert_eq!(
            lines(&render_summary(&s, 0))[1],
            "- 残作業: 担当者が未設定です"
        );
    }

    #[test]
    fn render_multiple_remain_joined_with_comma() {
        let s = SituationSummary {
            remain: vec![
                b("open_subtask", "DEMO-000002"),
                b("missing_section", "まとめ"),
            ],
            ..base()
        };
        assert_eq!(
            lines(&render_summary(&s, 0))[1],
            "- 残作業: 未完了のサブチケットがあります: DEMO-000002、必須セクション「まとめ」が未記入または短すぎます"
        );
    }

    #[test]
    fn render_docs_empty_and_single_and_multiple() {
        assert_eq!(
            lines(&render_summary(&base(), 0))[3],
            "- 既存ドキュメント: 記載なし"
        );

        let one = SituationSummary {
            docs: vec!["docs/a.md".to_string()],
            ..base()
        };
        assert_eq!(
            lines(&render_summary(&one, 0))[3],
            "- 既存ドキュメント: docs/a.md（既存を更新/参照。新規作成し直さない）"
        );

        let many = SituationSummary {
            docs: vec!["docs/a.md".to_string(), "docs/design/b.md".to_string()],
            ..base()
        };
        assert_eq!(
            lines(&render_summary(&many, 0))[3],
            "- 既存ドキュメント: docs/a.md、docs/design/b.md（既存を更新/参照。新規作成し直さない）"
        );
    }

    #[test]
    fn render_comment_count_zero_one_many() {
        assert_eq!(
            lines(&render_summary(&base(), 0))[4],
            "- コメント論点: 特になし"
        );
        assert_eq!(
            lines(&render_summary(&base(), 1))[4],
            "- コメント論点: コメント 1 件（全文を読んで論点を把握してください）"
        );
        assert_eq!(
            lines(&render_summary(&base(), 12))[4],
            "- コメント論点: コメント 12 件（全文を読んで論点を把握してください）"
        );
    }

    #[test]
    fn render_consistency_ok_and_messages() {
        assert_eq!(
            lines(&render_summary(&base(), 0))[5],
            "- ステータス整合: 整合"
        );

        let s = SituationSummary {
            consistency: vec![
                b("closed_with_open_subtask", "DEMO-000004"),
                b("ready_but_open", "none"),
            ],
            ..base()
        };
        assert_eq!(
            lines(&render_summary(&s, 0))[5],
            "- ステータス整合: 完了ですが、未完了のサブチケットがあります: DEMO-000004、完了条件は満たしていますが、ステータスが未完了です"
        );
    }

    #[test]
    fn render_full_example_matches_protocol() {
        let s = SituationSummary {
            phase: Phase::Verifying,
            action: Action::VerifyOnly,
            remain: vec![b("missing_section", "検証結果")],
            docs: vec!["docs/design/a.md".to_string()],
            consistency: vec![],
        };
        let expected = [
            "- フェーズ: 検証待ち",
            "- 残作業: 必須セクション「検証結果」が未記入または短すぎます",
            "- 推奨アクション: verify_only （実装済みの検証・DoD確認が主）",
            "- 既存ドキュメント: docs/design/a.md（既存を更新/参照。新規作成し直さない）",
            "- コメント論点: コメント 2 件（全文を読んで論点を把握してください）",
            "- ステータス整合: 整合",
        ]
        .join("\n");
        assert_eq!(render_summary(&s, 2), expected);
    }

    #[test]
    fn parse_then_render_roundtrip() {
        let out = "PHASE|implementing\nACTION|bug_fix\nREMAIN|open_subtask|DEMO-000005\nDOC|docs/x.md\nEND|4\n";
        let s = parse_summary(out).expect("ok");
        let rendered = render_summary(&s, 0);
        assert!(rendered.contains("- フェーズ: 実装中"));
        assert!(rendered.contains("未完了のサブチケットがあります: DEMO-000005"));
        assert!(rendered.contains("docs/x.md"));
    }
}
