//! クローズ判定ロジック

use super::{atom, facts::Facts, parse, sections, CloseCheckResult, KbError};

/// チケット情報(close check 入力)
#[derive(Debug, Clone)]
pub struct TicketRow {
    pub key: String,
    /// DB の ticket_type(`bug` / `task` などの英語コード、または旧来の日本語名)
    pub ticket_type: String,
    /// 完了扱いか。チームのワークフロー設定のカテゴリ(completed / cancelled)から DB 側で決める。
    /// ステータスの slug は自由に増やせるため、名前での判定はしない。
    pub done: bool,
    pub parent_key: Option<String>,
}

/// クローズ判定の入力
#[derive(Debug, Clone)]
pub struct CloseCheckInput {
    /// 判定対象チケットのキー
    pub target: String,
    /// 対象とその全子孫(対象を含む)
    pub tickets: Vec<TicketRow>,
    /// 対象チケットのラベル名一覧
    pub target_labels: Vec<String>,
    /// 対象チケットの本文(Markdown)
    pub target_description: String,
    /// 対象チケットの担当者数
    pub target_assignee_count: i64,
    /// 対象がサブチケット(親を持つ)か。サブチケットには「担当者なし」を求めない
    /// (担当者は親で決めるのが普通で、子ごとに求めると警告が多すぎるため)
    pub target_is_subticket: bool,
}

pub const CLOSE_RULES: &str = include_str!("../../../../kb_rules/close.pl");

/// 入力からプロローグ事実を生成
///
/// # 検証
/// - target キーが有効か確認
/// - tickets に target が含まれているか確認
/// - tickets が 500 件以上ないか確認
/// - 各チケットキーが有効か確認
///
/// # 生成される事実
/// - `target(Key)` - 判定対象
/// - `ticket(Key, Kind, Status, Parent|none)` - 各チケット
/// - `label(Key, Name)` - 対象チケットのラベル(対象だけ)
/// - `strict(Key)` - 必須セクションを課す対象(対象だけ。type: ラベル付きのとき)
/// - `section_len(Key, 正規化名, 文字数)` - セクション長(対象だけ)
/// - `assignee_count(Key, N)` - 担当者数(対象だけ)
/// - `status_done(Status)` - 完了ステータス値
/// - `required_section(Kind, 名)` - 必須セクション定義
/// - `requires_assignee(Kind)` - 担当者が必須かどうか
/// - `min_len(20)` - 最小文字数
pub fn build_facts(input: &CloseCheckInput) -> Result<Facts, KbError> {
    let mut facts = Facts::new();

    // target キーの検証
    if !atom::valid_key(&input.target) {
        return Err(KbError::InvalidInput(format!(
            "invalid target key: {}",
            input.target
        )));
    }

    // チケット件数チェック(500 件上限)
    if input.tickets.len() > 500 {
        return Err(KbError::TooLarge);
    }

    // target がチケット一覧に含まれているか確認
    let has_target = input.tickets.iter().any(|t| t.key == input.target);
    if !has_target {
        return Err(KbError::InvalidInput(
            "target not found in tickets".to_string(),
        ));
    }

    // target 事実を追加
    facts.fact("target", &[atom::quote(&input.target)])?;

    // ticket 事実を追加
    for ticket in &input.tickets {
        if !atom::valid_key(&ticket.key) {
            return Err(KbError::InvalidInput(format!(
                "invalid ticket key: {}",
                ticket.key
            )));
        }

        // Kind を決定(対象チケットはラベルで補正)
        let kind = determine_kind(
            &ticket.key,
            &ticket.ticket_type,
            &input.target,
            &input.target_labels,
        );

        // Parent を解決
        let parent = if let Some(parent_key) = &ticket.parent_key {
            // parent_key が tickets に含まれているか確認
            if input.tickets.iter().any(|t| t.key == *parent_key) {
                atom::quote(parent_key)
            } else {
                // 含まれていなければ親として記録しない(孤立チケット)
                "none".to_string()
            }
        } else {
            "none".to_string()
        };

        facts.fact(
            "ticket",
            &[
                atom::quote(&ticket.key),
                atom::quote(&kind),
                if ticket.done { "done" } else { "open" }.to_string(),
                parent,
            ],
        )?;
    }

    // 対象チケット固有の事実
    // label 事実(対象だけ)
    for label in &input.target_labels {
        facts.fact("label", &[atom::quote(&input.target), atom::quote(label)])?;
    }

    // strict 判定(type: ラベルが付いているか)
    let has_type_label = input
        .target_labels
        .iter()
        .any(|l| l == "type:feature" || l == "type:incident" || l == "type:infra");
    if has_type_label {
        facts.fact("strict", &[atom::quote(&input.target)])?;
    }

    // section_len 事実(対象だけ)
    let sections = sections::split(&input.target_description);
    for section in sections {
        let len = sections::body_len(&section.body);
        facts.fact(
            "section_len",
            &[
                atom::quote(&input.target),
                atom::quote(&section.name),
                len.to_string(),
            ],
        )?;
    }

    // サブチケットか(対象だけ)。close.pl が「担当者なし」を、サブチケットには求めない
    if input.target_is_subticket {
        facts.fact("subticket", &[atom::quote(&input.target)])?;
    }

    // assignee_count 事実(対象だけ。負数は0)
    let count = if input.target_assignee_count < 0 {
        0
    } else {
        input.target_assignee_count
    };
    facts.fact(
        "assignee_count",
        &[atom::quote(&input.target), count.to_string()],
    )?;

    // 定数事実
    facts.fact("status_done", &["done".to_string()])?;

    // 必須セクション定義(feature)
    facts.fact(
        "required_section",
        &[atom::quote("feature"), atom::quote("実施した内容")],
    )?;
    facts.fact(
        "required_section",
        &[atom::quote("feature"), atom::quote("実装方針")],
    )?;
    facts.fact(
        "required_section",
        &[atom::quote("feature"), atom::quote("まとめ")],
    )?;

    // 必須セクション定義(bug)
    facts.fact(
        "required_section",
        &[atom::quote("bug"), atom::quote("症状")],
    )?;
    facts.fact(
        "required_section",
        &[atom::quote("bug"), atom::quote("調査")],
    )?;
    facts.fact(
        "required_section",
        &[atom::quote("bug"), atom::quote("対策")],
    )?;
    facts.fact(
        "required_section",
        &[atom::quote("bug"), atom::quote("検証結果")],
    )?;

    // requires_assignee 定義
    facts.fact("requires_assignee", &[atom::quote("feature")])?;
    facts.fact("requires_assignee", &[atom::quote("bug")])?;

    // min_len 定義
    facts.fact("min_len", &["20".to_string()])?;

    Ok(facts)
}

/// Kind を決定
///
/// - 通常は ticket_type から決定(bug/不具合→bug、他→feature)
/// - 対象チケットで type:incident/type:infra ラベルがあれば bug
/// - 対象チケットで type:feature と障害ラベル両方がある場合は bug だが
///   mixed_type エラーで検出される
fn determine_kind(key: &str, ticket_type: &str, target: &str, target_labels: &[String]) -> String {
    // 対象チケットの場合のみラベルで補正
    if key == target {
        let has_incident = target_labels.iter().any(|l| l == "type:incident");
        let has_infra = target_labels.iter().any(|l| l == "type:infra");
        if has_incident || has_infra {
            return "bug".to_string();
        }
    }

    // ticket_type から決定(DB には英語コードと旧来の日本語名が混在し得る)
    if ticket_type == "bug" || ticket_type == "不具合" {
        "bug".to_string()
    } else {
        "feature".to_string()
    }
}

/// クローズ判定を実行
pub async fn run_close_check(
    runner: &crate::domain::services::kb::runner::Runner,
    input: &CloseCheckInput,
) -> Result<CloseCheckResult, KbError> {
    let facts = build_facts(input)?;
    let facts_text = facts.finish();
    let out = runner.run(CLOSE_RULES, &facts_text, "run_close").await?;
    parse::parse_close(&out, &input.target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ticket(key: &str, ticket_type: &str, status: &str, parent: Option<&str>) -> TicketRow {
        TicketRow {
            key: key.to_string(),
            ticket_type: ticket_type.to_string(),
            // テストでは「完了」だけを完了扱いにする(本番は DB のワークフロー分類で決まる)
            done: status == "完了",
            parent_key: parent.map(|s| s.to_string()),
        }
    }

    #[test]
    fn test_build_facts_basic() {
        let input = CloseCheckInput {
            target: "DEMO-000001".to_string(),
            tickets: vec![make_ticket("DEMO-000001", "課題", "完了", None)],
            target_labels: vec![],
            target_description: "## 実施した内容\nTest content".to_string(),
            target_assignee_count: 1,
            target_is_subticket: false,
        };

        let facts = build_facts(&input).unwrap();
        let text = facts.as_str();

        assert!(text.contains("target('DEMO-000001')"));
        assert!(text.contains("ticket('DEMO-000001','feature',done,none)"));
        assert!(text.contains("assignee_count('DEMO-000001',1)"));
    }

    #[test]
    fn test_build_facts_invalid_target_key() {
        let input = CloseCheckInput {
            target: "INVALID_KEY".to_string(),
            tickets: vec![make_ticket("INVALID_KEY", "課題", "完了", None)],
            target_labels: vec![],
            target_description: "test".to_string(),
            target_assignee_count: 0,
            target_is_subticket: false,
        };

        assert!(matches!(build_facts(&input), Err(KbError::InvalidInput(_))));
    }

    #[test]
    fn test_build_facts_target_not_in_tickets() {
        let input = CloseCheckInput {
            target: "DEMO-000001".to_string(),
            tickets: vec![make_ticket("DEMO-000002", "課題", "完了", None)],
            target_labels: vec![],
            target_description: "test".to_string(),
            target_assignee_count: 0,
            target_is_subticket: false,
        };

        assert!(matches!(build_facts(&input), Err(KbError::InvalidInput(_))));
    }

    #[test]
    fn test_build_facts_too_many_tickets() {
        let mut tickets = vec![];
        for i in 0..501 {
            tickets.push(make_ticket(&format!("DEMO-{:06}", i), "課題", "完了", None));
        }
        tickets[0].key = "DEMO-000001".to_string();

        let input = CloseCheckInput {
            target: "DEMO-000001".to_string(),
            tickets,
            target_labels: vec![],
            target_description: "test".to_string(),
            target_assignee_count: 0,
            target_is_subticket: false,
        };

        assert!(matches!(build_facts(&input), Err(KbError::TooLarge)));
    }

    #[test]
    fn test_build_facts_kind_from_ticket_type_bug() {
        let input = CloseCheckInput {
            target: "DEMO-000001".to_string(),
            tickets: vec![make_ticket("DEMO-000001", "不具合", "完了", None)],
            target_labels: vec![],
            target_description: "## 症状\ntest".to_string(),
            target_assignee_count: 0,
            target_is_subticket: false,
        };

        let facts = build_facts(&input).unwrap();
        let text = facts.as_str();
        assert!(text.contains("ticket('DEMO-000001','bug',done,none)"));
    }

    #[test]
    fn test_build_facts_kind_label_incident() {
        let input = CloseCheckInput {
            target: "DEMO-000001".to_string(),
            tickets: vec![make_ticket("DEMO-000001", "課題", "完了", None)],
            target_labels: vec!["type:incident".to_string()],
            target_description: "## 症状\ntest".to_string(),
            target_assignee_count: 0,
            target_is_subticket: false,
        };

        let facts = build_facts(&input).unwrap();
        let text = facts.as_str();
        assert!(text.contains("ticket('DEMO-000001','bug',done,none)"));
    }

    #[test]
    fn test_build_facts_kind_label_infra() {
        let input = CloseCheckInput {
            target: "DEMO-000001".to_string(),
            tickets: vec![make_ticket("DEMO-000001", "課題", "完了", None)],
            target_labels: vec!["type:infra".to_string()],
            target_description: "## 症状\ntest".to_string(),
            target_assignee_count: 0,
            target_is_subticket: false,
        };

        let facts = build_facts(&input).unwrap();
        let text = facts.as_str();
        assert!(text.contains("ticket('DEMO-000001','bug',done,none)"));
    }

    #[test]
    fn test_build_facts_strict_with_type_label() {
        let input = CloseCheckInput {
            target: "DEMO-000001".to_string(),
            tickets: vec![make_ticket("DEMO-000001", "課題", "完了", None)],
            target_labels: vec!["type:feature".to_string()],
            target_description: "test".to_string(),
            target_assignee_count: 0,
            target_is_subticket: false,
        };

        let facts = build_facts(&input).unwrap();
        let text = facts.as_str();
        assert!(text.contains("strict('DEMO-000001')"));
    }

    #[test]
    fn test_build_facts_no_strict_without_type_label() {
        let input = CloseCheckInput {
            target: "DEMO-000001".to_string(),
            tickets: vec![make_ticket("DEMO-000001", "課題", "完了", None)],
            target_labels: vec!["other-label".to_string()],
            target_description: "test".to_string(),
            target_assignee_count: 0,
            target_is_subticket: false,
        };

        let facts = build_facts(&input).unwrap();
        let text = facts.as_str();
        assert!(!text.contains("strict('DEMO-000001')"));
    }

    #[test]
    fn test_build_facts_section_len() {
        let input = CloseCheckInput {
            target: "DEMO-000001".to_string(),
            tickets: vec![make_ticket("DEMO-000001", "課題", "完了", None)],
            target_labels: vec![],
            target_description: "## セクション1\nThis is 20 characters!".to_string(),
            target_assignee_count: 0,
            target_is_subticket: false,
        };

        let facts = build_facts(&input).unwrap();
        let text = facts.as_str();
        // section_len should be present with the exact character count
        assert!(text.contains("section_len('DEMO-000001','セクション1'"));
    }

    #[test]
    fn test_build_facts_parent_resolution() {
        let input = CloseCheckInput {
            target: "DEMO-000001".to_string(),
            tickets: vec![
                make_ticket("DEMO-000001", "課題", "完了", None),
                make_ticket("DEMO-000002", "課題", "完了", Some("DEMO-000001")),
            ],
            target_labels: vec![],
            target_description: "test".to_string(),
            target_assignee_count: 0,
            target_is_subticket: false,
        };

        let facts = build_facts(&input).unwrap();
        let text = facts.as_str();
        assert!(text.contains("ticket('DEMO-000002','feature',done,'DEMO-000001')"));
    }

    #[test]
    fn test_build_facts_missing_parent() {
        let input = CloseCheckInput {
            target: "DEMO-000001".to_string(),
            tickets: vec![
                make_ticket("DEMO-000001", "課題", "完了", None),
                make_ticket("DEMO-000002", "課題", "完了", Some("MISSING-KEY")),
            ],
            target_labels: vec![],
            target_description: "test".to_string(),
            target_assignee_count: 0,
            target_is_subticket: false,
        };

        let facts = build_facts(&input).unwrap();
        let text = facts.as_str();
        // 存在しない親は none に変換される
        assert!(text.contains("ticket('DEMO-000002','feature',done,none)"));
    }

    #[test]
    fn test_build_facts_negative_assignee_count() {
        let input = CloseCheckInput {
            target: "DEMO-000001".to_string(),
            tickets: vec![make_ticket("DEMO-000001", "課題", "完了", None)],
            target_labels: vec![],
            target_description: "test".to_string(),
            target_assignee_count: -5,
            target_is_subticket: false,
        };

        let facts = build_facts(&input).unwrap();
        let text = facts.as_str();
        // 負数は0に変換される
        assert!(text.contains("assignee_count('DEMO-000001',0)"));
    }

    #[test]
    fn test_build_facts_label_with_quote() {
        let input = CloseCheckInput {
            target: "DEMO-000001".to_string(),
            tickets: vec![make_ticket("DEMO-000001", "課題", "完了", None)],
            target_labels: vec!["label'with'quote".to_string()],
            target_description: "test".to_string(),
            target_assignee_count: 0,
            target_is_subticket: false,
        };

        let facts = build_facts(&input).unwrap();
        let text = facts.as_str();
        // Quote されていることを確認
        assert!(text.contains("label('DEMO-000001','label\\'with\\'quote')"));
    }

    #[test]
    fn test_build_facts_invalid_ticket_key() {
        let input = CloseCheckInput {
            target: "DEMO-000001".to_string(),
            tickets: vec![
                make_ticket("DEMO-000001", "課題", "完了", None),
                make_ticket("INVALID_KEY", "課題", "完了", None),
            ],
            target_labels: vec![],
            target_description: "test".to_string(),
            target_assignee_count: 0,
            target_is_subticket: false,
        };

        assert!(matches!(build_facts(&input), Err(KbError::InvalidInput(_))));
    }

    #[test]
    fn test_build_facts_includes_required_sections() {
        let input = CloseCheckInput {
            target: "DEMO-000001".to_string(),
            tickets: vec![make_ticket("DEMO-000001", "課題", "完了", None)],
            target_labels: vec![],
            target_description: "test".to_string(),
            target_assignee_count: 0,
            target_is_subticket: false,
        };

        let facts = build_facts(&input).unwrap();
        let text = facts.as_str();

        // feature 必須セクション
        assert!(text.contains("required_section('feature','実施した内容')"));
        assert!(text.contains("required_section('feature','実装方針')"));
        assert!(text.contains("required_section('feature','まとめ')"));

        // bug 必須セクション
        assert!(text.contains("required_section('bug','症状')"));
        assert!(text.contains("required_section('bug','調査')"));
        assert!(text.contains("required_section('bug','対策')"));
        assert!(text.contains("required_section('bug','検証結果')"));

        // requires_assignee
        assert!(text.contains("requires_assignee('feature')"));
        assert!(text.contains("requires_assignee('bug')"));

        // min_len
        assert!(text.contains("min_len(20)"));
    }

    #[test]
    fn test_build_facts_includes_status_done() {
        let input = CloseCheckInput {
            target: "DEMO-000001".to_string(),
            tickets: vec![make_ticket("DEMO-000001", "課題", "完了", None)],
            target_labels: vec![],
            target_description: "test".to_string(),
            target_assignee_count: 0,
            target_is_subticket: false,
        };

        let facts = build_facts(&input).unwrap();
        let text = facts.as_str();
        assert!(text.contains("status_done(done)"));
    }

    // ───────────── 規則テスト(実際の Scryer Prolog を使う) ─────────────
    // scryer-prolog が実行できないときはスキップする。REQUIRE_SCRYER=1 のときは失敗にする
    // (ローカルCIは REQUIRE_SCRYER=1 で実行するので、CI でスキップされて素通りすることはない)。

    use crate::domain::services::kb::runner::Runner;
    use std::time::Duration;

    fn scryer() -> Option<Runner> {
        let bin = std::env::var("SENN_KB_SCRYER_BIN").unwrap_or_else(|_| "scryer-prolog".into());
        let runnable = std::process::Command::new(&bin)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !runnable {
            assert!(
                std::env::var("REQUIRE_SCRYER").as_deref() != Ok("1"),
                "REQUIRE_SCRYER=1 but scryer-prolog is not runnable: {bin}"
            );
            eprintln!("skip: scryer-prolog is not available");
            return None;
        }
        Some(Runner::new(bin, Duration::from_secs(10), 4))
    }

    fn body(n: usize) -> String {
        "あ".repeat(n)
    }

    fn feature_desc(matome_len: usize) -> String {
        format!(
            "## 実施した内容\n{}\n## 実装方針\n{}\n## まとめ\n{}\n",
            body(30),
            body(30),
            body(matome_len)
        )
    }

    fn input(
        target: &str,
        tickets: Vec<TicketRow>,
        labels: &[&str],
        desc: &str,
        assignees: i64,
    ) -> CloseCheckInput {
        CloseCheckInput {
            target: target.to_string(),
            tickets,
            target_labels: labels.iter().map(|l| l.to_string()).collect(),
            target_description: desc.to_string(),
            target_assignee_count: assignees,
            target_is_subticket: false,
        }
    }

    fn pairs(r: &CloseCheckResult) -> Vec<(String, String)> {
        let mut v: Vec<(String, String)> = r
            .blockers
            .iter()
            .map(|b| (b.code.clone(), b.arg.clone()))
            .collect();
        v.sort();
        v
    }

    fn p(code: &str, arg: &str) -> (String, String) {
        (code.to_string(), arg.to_string())
    }

    /// 親(処理済み)→ 子 → 孫 の3階層。子・孫の状態を指定する
    fn tree(child: &str, grandchild: &str) -> Vec<TicketRow> {
        vec![
            make_ticket("DEMO-000001", "課題", "処理済み", None),
            make_ticket("DEMO-000002", "課題", child, Some("DEMO-000001")),
            make_ticket("DEMO-000003", "課題", grandchild, Some("DEMO-000002")),
        ]
    }

    #[tokio::test]
    async fn rules_closable_when_everything_is_done() {
        let Some(r) = scryer() else { return };
        let i = input(
            "DEMO-000001",
            tree("完了", "完了"),
            &["type:feature"],
            &feature_desc(25),
            1,
        );
        let res = run_close_check(&r, &i).await.expect("ok");
        assert!(res.can_close, "{res:?}");
        assert!(res.blockers.is_empty());
    }

    #[tokio::test]
    async fn rules_open_child_blocks() {
        let Some(r) = scryer() else { return };
        let i = input(
            "DEMO-000001",
            tree("処理中", "完了"),
            &["type:feature"],
            &feature_desc(25),
            1,
        );
        let res = run_close_check(&r, &i).await.expect("ok");
        assert!(!res.can_close);
        assert_eq!(pairs(&res), vec![p("open_subtask", "DEMO-000002")]);
    }

    #[tokio::test]
    async fn rules_open_grandchild_blocks_even_if_child_is_done() {
        let Some(r) = scryer() else { return };
        let i = input(
            "DEMO-000001",
            tree("完了", "未対応"),
            &["type:feature"],
            &feature_desc(25),
            1,
        );
        let res = run_close_check(&r, &i).await.expect("ok");
        assert_eq!(pairs(&res), vec![p("open_subtask", "DEMO-000003")]);
    }

    #[tokio::test]
    async fn rules_every_open_descendant_is_reported() {
        let Some(r) = scryer() else { return };
        let i = input(
            "DEMO-000001",
            tree("処理中", "処理済み"),
            &["type:feature"],
            &feature_desc(25),
            1,
        );
        let res = run_close_check(&r, &i).await.expect("ok");
        assert_eq!(
            pairs(&res),
            vec![
                p("open_subtask", "DEMO-000002"),
                p("open_subtask", "DEMO-000003")
            ]
        );
    }

    #[tokio::test]
    async fn rules_cyclic_parents_do_not_hang() {
        let Some(r) = scryer() else { return };
        // A の親が B、B の親が A(壊れたデータ)。無限ループせず結果が返ること
        let tickets = vec![
            make_ticket("DEMO-000001", "課題", "処理済み", Some("DEMO-000002")),
            make_ticket("DEMO-000002", "課題", "処理中", Some("DEMO-000001")),
        ];
        let i = input("DEMO-000001", tickets, &[], "", 1);
        let res = run_close_check(&r, &i).await.expect("must return");
        assert_eq!(pairs(&res), vec![p("open_subtask", "DEMO-000002")]);
    }

    #[tokio::test]
    async fn rules_required_section_length_boundary_is_20_chars() {
        let Some(r) = scryer() else { return };
        let t = || vec![make_ticket("DEMO-000001", "課題", "処理済み", None)];
        let short = input("DEMO-000001", t(), &["type:feature"], &feature_desc(19), 1);
        let res = run_close_check(&r, &short).await.expect("ok");
        assert_eq!(pairs(&res), vec![p("missing_section", "まとめ")]);

        let just = input("DEMO-000001", t(), &["type:feature"], &feature_desc(20), 1);
        let res = run_close_check(&r, &just).await.expect("ok");
        assert!(res.can_close, "{res:?}");
    }

    #[tokio::test]
    async fn rules_missing_heading_is_reported() {
        let Some(r) = scryer() else { return };
        let desc = format!("## 実施した内容\n{}\n## 実装方針\n{}\n", body(30), body(30));
        let i = input(
            "DEMO-000001",
            vec![make_ticket("DEMO-000001", "課題", "処理済み", None)],
            &["type:feature"],
            &desc,
            1,
        );
        let res = run_close_check(&r, &i).await.expect("ok");
        assert_eq!(pairs(&res), vec![p("missing_section", "まとめ")]);
    }

    #[tokio::test]
    async fn rules_heading_variants_are_normalized() {
        let Some(r) = scryer() else { return };
        // 「実施した内容」「実装方針の要点」「総括」でも必須3節として認識される
        let desc = format!(
            "### 実施した内容\n{}\n### 実装方針の要点\n{}\n### 総括\n{}\n",
            body(30),
            body(30),
            body(30)
        );
        let i = input(
            "DEMO-000001",
            vec![make_ticket("DEMO-000001", "課題", "処理済み", None)],
            &["type:feature"],
            &desc,
            1,
        );
        let res = run_close_check(&r, &i).await.expect("ok");
        assert!(res.can_close, "{res:?}");
    }

    #[tokio::test]
    async fn rules_code_only_section_counts_as_filled() {
        let Some(r) = scryer() else { return };
        let desc = format!(
            "## 実施した内容\n{}\n## 実装方針\n{}\n## まとめ\n```\n{}\n```\n",
            body(30),
            body(30),
            "x".repeat(40)
        );
        let i = input(
            "DEMO-000001",
            vec![make_ticket("DEMO-000001", "課題", "処理済み", None)],
            &["type:feature"],
            &desc,
            1,
        );
        let res = run_close_check(&r, &i).await.expect("ok");
        assert!(res.can_close, "{res:?}");
    }

    #[tokio::test]
    async fn rules_ticket_without_type_label_needs_no_sections() {
        let Some(r) = scryer() else { return };
        let i = input(
            "DEMO-000001",
            vec![make_ticket("DEMO-000001", "作業依頼", "処理済み", None)],
            &[],
            "",
            1,
        );
        let res = run_close_check(&r, &i).await.expect("ok");
        assert!(res.can_close, "{res:?}");
    }

    #[tokio::test]
    async fn rules_no_assignee_blocks() {
        let Some(r) = scryer() else { return };
        let i = input(
            "DEMO-000001",
            vec![make_ticket("DEMO-000001", "作業依頼", "処理済み", None)],
            &[],
            "",
            0,
        );
        let res = run_close_check(&r, &i).await.expect("ok");
        assert_eq!(pairs(&res), vec![p("no_assignee", "none")]);
    }

    #[tokio::test]
    async fn rules_mixed_feature_and_incident_labels_are_detected() {
        let Some(r) = scryer() else { return };
        let i = input(
            "DEMO-000001",
            vec![make_ticket("DEMO-000001", "課題", "処理済み", None)],
            &["type:feature", "type:incident"],
            &feature_desc(25),
            1,
        );
        let res = run_close_check(&r, &i).await.expect("ok");
        assert!(!res.can_close);
        assert!(pairs(&res).contains(&p("mixed_type", "none")), "{res:?}");
    }

    #[tokio::test]
    async fn rules_bug_requires_its_four_sections() {
        let Some(r) = scryer() else { return };
        let desc = format!(
            "## 症状\n{}\n## 調査\n{}\n## 対策\n{}\n",
            body(30),
            body(30),
            body(30)
        );
        let i = input(
            "DEMO-000001",
            vec![make_ticket("DEMO-000001", "不具合", "処理済み", None)],
            &["type:incident"],
            &desc,
            1,
        );
        let res = run_close_check(&r, &i).await.expect("ok");
        assert_eq!(pairs(&res), vec![p("missing_section", "検証結果")]);

        let full = format!("{desc}## 検証結果\n{}\n", body(30));
        let i = input(
            "DEMO-000001",
            vec![make_ticket("DEMO-000001", "不具合", "処理済み", None)],
            &["type:incident"],
            &full,
            1,
        );
        assert!(run_close_check(&r, &i).await.expect("ok").can_close);
    }

    #[tokio::test]
    async fn rules_hostile_label_text_cannot_break_the_program() {
        let Some(r) = scryer() else { return };
        // 引用符・括弧・halt・改行・バックスラッシュを含むラベルでも判定が正しいこと
        let hostile = "x'); halt. foo('\\\n bar";
        let i = input(
            "DEMO-000001",
            tree("処理中", "完了"),
            &["type:feature", hostile],
            &feature_desc(25),
            1,
        );
        let res = run_close_check(&r, &i).await.expect("ok");
        assert_eq!(pairs(&res), vec![p("open_subtask", "DEMO-000002")]);
    }

    #[tokio::test]
    async fn rules_hostile_ticket_type_text_is_harmless() {
        let Some(r) = scryer() else { return };
        // ticket_type に事実を注入しようとする文字列が入っていても、判定は変わらない
        let mut tickets = tree("処理中", "完了");
        tickets[1].ticket_type = "task'). status_done(open). x('".to_string();
        let i = input(
            "DEMO-000001",
            tickets,
            &["type:feature"],
            &feature_desc(25),
            1,
        );
        let res = run_close_check(&r, &i).await.expect("ok");
        assert_eq!(pairs(&res), vec![p("open_subtask", "DEMO-000002")]);
    }

    #[tokio::test]
    async fn rules_legacy_and_english_bug_types_are_both_bug() {
        let Some(r) = scryer() else { return };
        for ty in ["bug", "不具合"] {
            let i = input(
                "DEMO-000001",
                vec![make_ticket("DEMO-000001", ty, "処理済み", None)],
                &["type:incident"],
                "",
                1,
            );
            let res = run_close_check(&r, &i).await.expect("ok");
            // bug は4節が必要 = 全部欠落
            assert_eq!(res.blockers.len(), 4, "{ty}: {res:?}");
        }
    }

    /// 事実の1行が構文エラーで黙って捨てられたとき、必ず Err になる(誤って「可」にならない)
    #[tokio::test]
    async fn rules_dropped_fact_is_detected_and_never_yields_ok() {
        let Some(r) = scryer() else { return };
        let i = input(
            "DEMO-000001",
            tree("処理中", "完了"),
            &["type:feature"],
            &feature_desc(25),
            1,
        );
        let good = build_facts(&i).expect("facts").finish();
        // 未完了の子の事実 (DEMO-000002) を、構文エラーになる行に置き換える
        let broken: String = good
            .lines()
            .map(|l| {
                if l.starts_with("ticket('DEMO-000002'") {
                    "ticket('DEMO-000002', feature, 'unterminated, none).".to_string()
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        assert_ne!(good, broken);
        let out = r
            .run(CLOSE_RULES, &broken, "run_close")
            .await
            .expect("process itself runs");
        assert!(
            parse::parse_close(&out, "DEMO-000001").is_err(),
            "must not be readable as a verdict: {out}"
        );
    }

    #[tokio::test]
    async fn rules_missing_facts_count_is_detected() {
        let Some(r) = scryer() else { return };
        let i = input(
            "DEMO-000001",
            tree("完了", "完了"),
            &["type:feature"],
            &feature_desc(25),
            1,
        );
        let facts = build_facts(&i).expect("facts");
        // finish() を通さず facts_count を付けない
        let out = r
            .run(CLOSE_RULES, facts.as_str(), "run_close")
            .await
            .expect("process itself runs");
        assert!(parse::parse_close(&out, "DEMO-000001").is_err(), "{out}");
    }

    #[tokio::test]
    async fn rules_wrong_facts_count_is_detected() {
        let Some(r) = scryer() else { return };
        let i = input(
            "DEMO-000001",
            tree("完了", "完了"),
            &["type:feature"],
            &feature_desc(25),
            1,
        );
        let facts = build_facts(&i).expect("facts");
        let text = format!("{}facts_count({}).\n", facts.as_str(), facts.len() + 1);
        let out = r
            .run(CLOSE_RULES, &text, "run_close")
            .await
            .expect("process itself runs");
        assert!(parse::parse_close(&out, "DEMO-000001").is_err(), "{out}");
    }

    // ───────────── サブチケットには「担当者なし」を求めない ─────────────

    #[tokio::test]
    async fn rules_subticket_without_assignee_is_not_blocked() {
        let Some(r) = scryer() else { return };
        let mut i = input(
            "DEMO-000002",
            vec![make_ticket(
                "DEMO-000002",
                "課題",
                "処理済み",
                Some("DEMO-000001"),
            )],
            &[],
            "",
            0,
        );
        // 親は取得対象に含まれない(対象とその子孫だけを渡す)ので、サブチケットかどうかは別に渡す
        i.target_is_subticket = true;
        let res = run_close_check(&r, &i).await.expect("ok");
        assert!(res.can_close, "{res:?}");
    }

    #[tokio::test]
    async fn rules_top_level_ticket_without_assignee_is_still_blocked() {
        let Some(r) = scryer() else { return };
        let i = input(
            "DEMO-000001",
            vec![make_ticket("DEMO-000001", "課題", "処理済み", None)],
            &[],
            "",
            0,
        );
        let res = run_close_check(&r, &i).await.expect("ok");
        assert_eq!(pairs(&res), vec![p("no_assignee", "none")]);
    }

    #[tokio::test]
    async fn rules_subticket_still_needs_its_other_requirements() {
        let Some(r) = scryer() else { return };
        // 担当者は免除されるが、必須セクションと、未完了の子は、従来どおり求める
        let mut i = input(
            "DEMO-000002",
            vec![
                make_ticket("DEMO-000002", "課題", "処理済み", Some("DEMO-000001")),
                make_ticket("DEMO-000003", "課題", "処理中", Some("DEMO-000002")),
            ],
            &["type:feature"],
            &feature_desc(0),
            0,
        );
        i.target_is_subticket = true;
        let res = run_close_check(&r, &i).await.expect("ok");
        assert_eq!(
            pairs(&res),
            vec![
                p("missing_section", "まとめ"),
                p("open_subtask", "DEMO-000003")
            ]
        );
    }

    #[test]
    fn test_build_facts_marks_subticket_only_when_asked() {
        let mut i = CloseCheckInput {
            target: "DEMO-000001".to_string(),
            tickets: vec![make_ticket("DEMO-000001", "課題", "完了", None)],
            target_labels: vec![],
            target_description: String::new(),
            target_assignee_count: 0,
            target_is_subticket: false,
        };
        assert!(!build_facts(&i)
            .expect("facts")
            .as_str()
            .contains("subticket("));
        i.target_is_subticket = true;
        assert!(build_facts(&i)
            .expect("facts")
            .as_str()
            .contains("subticket('DEMO-000001')."));
    }

    /// 事実の種類が混ざって並んでも(ticket の途中に label など)、前の節が捨てられない。
    /// discontiguous の宣言が無いと、後から出た節で前の節が黙って置き換わる(件数照合が止める)。
    #[tokio::test]
    async fn rules_survive_facts_whose_kinds_are_interleaved() {
        let Some(r) = scryer() else { return };
        let i = input(
            "DEMO-000001",
            tree("処理中", "完了"),
            &["type:feature"],
            &feature_desc(25),
            1,
        );
        let sorted = build_facts(&i).expect("facts");
        // 事実の行を、種類が交互になるように並べ替える(件数は同じ)
        let mut lines: Vec<&str> = sorted.as_str().lines().collect();
        lines.sort_by_key(|l| l.len() % 7);
        let mixed: String = lines.iter().map(|l| format!("{l}\n")).collect::<String>()
            + &format!("facts_count({}).\n", sorted.len());
        let out = r.run(CLOSE_RULES, &mixed, "run_close").await.expect("runs");
        let res =
            parse::parse_close(&out, "DEMO-000001").expect("must not be rejected as inconsistent");
        assert_eq!(pairs(&res), vec![p("open_subtask", "DEMO-000002")]);
    }
}
