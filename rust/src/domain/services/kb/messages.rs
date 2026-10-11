//! Prolog が返すコードを日本語の固定文に変換する(単一の出どころ)
//!
//! 文言は `presentation/handlers/kb_api.rs` の `message_for` と同じものを使う。
//! 未知のコードは `{code}: {arg}` の形で返し、握りつぶさない。

/// ブロッカー(クローズを妨げる理由)のコードを日本語にする。
pub fn blocker_message(code: &str, arg: &str) -> String {
    match code {
        "open_subtask" => format!("未完了のサブチケットがあります: {arg}"),
        "missing_section" => format!("必須セクション「{arg}」が未記入または短すぎます"),
        "no_assignee" => "担当者が未設定です".to_string(),
        "mixed_type" => {
            "機能追加と障害のラベルが混在しています(別チケットに分けてください)".to_string()
        }
        "unknown_ticket" => "チケットが見つかりません".to_string(),
        _ => format!("{code}: {arg}"),
    }
}

/// ステータス整合(完了・未完了と実際の状態の食い違い)のコードを日本語にする。
pub fn consistency_message(code: &str, arg: &str) -> String {
    match code {
        "closed_with_open_subtask" => {
            format!("完了ですが、未完了のサブチケットがあります: {arg}")
        }
        "closed_missing_section" => {
            format!("完了ですが、必須セクション「{arg}」が未記入または短すぎます")
        }
        "ready_but_open" => "完了条件は満たしていますが、ステータスが未完了です".to_string(),
        _ => format!("{code}: {arg}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocker_open_subtask() {
        assert_eq!(
            blocker_message("open_subtask", "DEMO-000003"),
            "未完了のサブチケットがあります: DEMO-000003"
        );
    }

    #[test]
    fn blocker_missing_section() {
        assert_eq!(
            blocker_message("missing_section", "まとめ"),
            "必須セクション「まとめ」が未記入または短すぎます"
        );
    }

    #[test]
    fn blocker_no_assignee_ignores_arg() {
        assert_eq!(blocker_message("no_assignee", "none"), "担当者が未設定です");
    }

    #[test]
    fn blocker_mixed_type() {
        assert_eq!(
            blocker_message("mixed_type", ""),
            "機能追加と障害のラベルが混在しています(別チケットに分けてください)"
        );
    }

    #[test]
    fn blocker_unknown_ticket() {
        assert_eq!(
            blocker_message("unknown_ticket", "DEMO-000009"),
            "チケットが見つかりません"
        );
    }

    #[test]
    fn blocker_unknown_code_falls_back() {
        assert_eq!(blocker_message("weird_code", "x"), "weird_code: x");
    }

    #[test]
    fn consistency_closed_with_open_subtask() {
        assert_eq!(
            consistency_message("closed_with_open_subtask", "DEMO-000004"),
            "完了ですが、未完了のサブチケットがあります: DEMO-000004"
        );
    }

    #[test]
    fn consistency_closed_missing_section() {
        assert_eq!(
            consistency_message("closed_missing_section", "検証結果"),
            "完了ですが、必須セクション「検証結果」が未記入または短すぎます"
        );
    }

    #[test]
    fn consistency_ready_but_open_ignores_arg() {
        assert_eq!(
            consistency_message("ready_but_open", "none"),
            "完了条件は満たしていますが、ステータスが未完了です"
        );
    }

    #[test]
    fn consistency_unknown_code_falls_back() {
        assert_eq!(consistency_message("other", "y"), "other: y");
    }
}
