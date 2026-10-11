//! 状況サマリの決定(第2弾): DB の入力 → 事実 → Scryer(summary.pl) → 6項目の文章
//!
//! 安全側の原則は close_check と同じ。判定に使う事実はすべて Rust が決定的に作り、
//! Prolog の出力は `summary::parse_summary` が厳密に検査する(形式外・END 欠落・件数不一致は Err)。

use super::close_check::{self, CloseCheckInput};
use super::facts::Facts;
use super::runner::Runner;
use super::summary::{self, SituationSummary};
use super::{atom, detect, KbError};
use crate::infrastructure::repositories::kb_repo;

pub const SUMMARY_RULES: &str = include_str!("../../../../kb_rules/summary.pl");

/// 検出に使う本文+コメントの最大文字数(巨大な本文でも処理時間を一定に保つ)
const DETECT_MAX_CHARS: usize = 200_000;

/// 状況サマリの入力
#[derive(Debug, Clone)]
pub struct SummaryInput {
    /// 第1弾と同じ(対象 + 全子孫、完了判定、ラベル、担当者数、本文)
    pub close: CloseCheckInput,
    /// 対象のステータス分類: completed / cancelled / started / その他
    pub status_category: String,
    /// コメント全文(設計書パスと実行時エラー語の検出にだけ使う。Prolog には渡さない)
    pub comments_text: String,
}

/// `close_check::build_facts` の事実 + 状況サマリ用の事実
pub fn build_facts(input: &SummaryInput) -> Result<Facts, KbError> {
    let mut facts = close_check::build_facts(&input.close)?;

    // 分類は閉じた集合に正規化(想定外の文字列を事実に入れない)
    let category = match input.status_category.as_str() {
        c @ ("completed" | "cancelled" | "started") => c,
        _ => "other",
    };
    facts.fact("status_category", &[category.to_string()])?;

    let text: String = format!(
        "{}\n{}",
        input.close.target_description, input.comments_text
    )
    .chars()
    .take(DETECT_MAX_CHARS)
    .collect();
    for d in detect::docs(&text) {
        facts.fact("doc", &[atom::quote(&d)])?;
    }
    if detect::has_runtime_error_signal(&text) {
        facts.fact("signal", &["runtime_error".to_string()])?;
    }
    Ok(facts)
}

/// Scryer で状況サマリを決める
pub async fn run_summary(
    runner: &Runner,
    input: &SummaryInput,
) -> Result<SituationSummary, KbError> {
    let facts = build_facts(input)?;
    // close.pl の述語(blocker/3 など)を再利用するため、連結して 1 ファイルにする
    let rules = format!("{}\n{}", close_check::CLOSE_RULES, SUMMARY_RULES);
    let out = runner.run(&rules, &facts.finish(), "run_summary").await?;
    summary::parse_summary(&out)
}

/// チケットキーから、プロンプトに差し込む「状況サマリ」(6項目の文章)を作る。
/// 失敗(DB・Scryer・出力の不整合・子孫が多すぎる等)はすべて Err。呼び出し側は固定の
/// フォールバック文に切り替える。
pub async fn build_situation_summary(
    pool: &sqlx::PgPool,
    runner: &Runner,
    ticket_key: &str,
    comments_text: &str,
) -> Result<String, KbError> {
    let db = |e: anyhow::Error| KbError::Db(e.to_string());
    let close = kb_repo::load_close_check_input(pool, ticket_key)
        .await
        .map_err(db)?
        .ok_or_else(|| KbError::InvalidInput("ticket not found".to_string()))?;
    let status_category = kb_repo::load_status_category(pool, ticket_key)
        .await
        .map_err(db)?
        .ok_or_else(|| KbError::InvalidInput("ticket not found".to_string()))?;
    let comment_count = kb_repo::count_comments(pool, ticket_key)
        .await
        .map_err(db)?;

    let input = SummaryInput {
        close,
        status_category,
        comments_text: comments_text.to_string(),
    };
    let s = run_summary(runner, &input).await?;
    Ok(summary::render_summary(&s, comment_count))
}

#[cfg(test)]
mod tests {
    //! 規則テスト(実際の Scryer Prolog を使う)。scryer-prolog が実行できないときはスキップ、
    //! REQUIRE_SCRYER=1 のときは失敗にする(ローカルCIは REQUIRE_SCRYER=1 で実行する)。
    use super::*;
    use crate::domain::services::kb::close_check::TicketRow;
    use crate::domain::services::kb::summary::{Action, Phase};
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

    const T: &str = "DEMO-000001";

    fn body(n: usize) -> String {
        "あ".repeat(n)
    }

    fn feature_desc(impl_len: usize, plan_len: usize, matome_len: usize) -> String {
        let mut s = String::new();
        for (h, n) in [
            ("実施した内容", impl_len),
            ("実装方針", plan_len),
            ("まとめ", matome_len),
        ] {
            if n > 0 {
                s.push_str(&format!("## {h}\n{}\n", body(n)));
            }
        }
        s
    }

    fn bug_desc(sections: &[&str]) -> String {
        sections
            .iter()
            .map(|h| format!("## {h}\n{}\n", body(30)))
            .collect()
    }

    fn row(key: &str, ty: &str, done: bool, parent: Option<&str>) -> TicketRow {
        TicketRow {
            key: key.to_string(),
            ticket_type: ty.to_string(),
            done,
            parent_key: parent.map(String::from),
        }
    }

    struct Case {
        ty: &'static str,
        labels: Vec<&'static str>,
        desc: String,
        assignees: i64,
        is_subticket: bool,
        category: &'static str,
        children: Vec<(&'static str, bool)>, // (キー, 完了か)
        comments: String,
    }

    impl Case {
        fn new() -> Self {
            Case {
                ty: "task",
                labels: vec![],
                desc: String::new(),
                assignees: 1,
                is_subticket: false,
                category: "other",
                children: vec![],
                comments: String::new(),
            }
        }
        fn input(&self) -> SummaryInput {
            let mut tickets = vec![row(T, self.ty, false, None)];
            for (k, done) in &self.children {
                tickets.push(row(k, "task", *done, Some(T)));
            }
            SummaryInput {
                close: CloseCheckInput {
                    target: T.to_string(),
                    tickets,
                    target_labels: self.labels.iter().map(|l| l.to_string()).collect(),
                    target_description: self.desc.clone(),
                    target_assignee_count: self.assignees,
                    target_is_subticket: self.is_subticket,
                },
                status_category: self.category.to_string(),
                comments_text: self.comments.clone(),
            }
        }
        async fn run(&self, r: &Runner) -> SituationSummary {
            run_summary(r, &self.input()).await.expect("summary")
        }
    }

    fn codes(v: &[crate::domain::services::kb::Blocker]) -> Vec<(String, String)> {
        let mut c: Vec<_> = v.iter().map(|b| (b.code.clone(), b.arg.clone())).collect();
        c.sort();
        c
    }

    fn p(code: &str, arg: &str) -> (String, String) {
        (code.to_string(), arg.to_string())
    }

    // ── フェーズ × 推奨アクション ──

    /// ラベルなし・子なしの新規チケットは、阻害要因が無くても close_ready にしてはいけない
    #[tokio::test]
    async fn fresh_plain_ticket_is_not_close_ready() {
        let Some(r) = scryer() else { return };
        let s = Case::new().run(&r).await;
        assert_eq!(s.phase, Phase::Investigating);
        assert_eq!(s.action, Action::ContinueImpl);
        assert!(s.remain.is_empty());
        assert!(
            s.consistency.is_empty(),
            "完了の証拠が無いので「条件は満たす」とは言わない"
        );
    }

    #[tokio::test]
    async fn started_plain_ticket_is_implementing() {
        let Some(r) = scryer() else { return };
        let mut c = Case::new();
        c.category = "started";
        let s = c.run(&r).await;
        assert_eq!(
            (s.phase, s.action),
            (Phase::Implementing, Action::ContinueImpl)
        );
    }

    #[tokio::test]
    async fn open_subtask_means_implementing_and_continue() {
        let Some(r) = scryer() else { return };
        let mut c = Case::new();
        c.children = vec![("DEMO-000002", false)];
        let s = c.run(&r).await;
        assert_eq!(
            (s.phase, s.action),
            (Phase::Implementing, Action::ContinueImpl)
        );
        assert_eq!(codes(&s.remain), vec![p("open_subtask", "DEMO-000002")]);
        // 完了していないチケットに「完了なのに未完了の子がある」と指摘してはいけない
        assert!(s.consistency.is_empty(), "{:?}", s.consistency);
    }

    #[tokio::test]
    async fn all_subtasks_done_is_close_ready_with_evidence() {
        let Some(r) = scryer() else { return };
        let mut c = Case::new();
        c.children = vec![("DEMO-000002", true), ("DEMO-000003", true)];
        let s = c.run(&r).await;
        assert_eq!(s.action, Action::CloseReady);
        // 子がすべて完了しているのに「調査中」と言わない(stg の目視で見つけた不具合)
        assert_eq!(s.phase, Phase::Verifying);
        assert!(s.remain.is_empty());
        // 未完了のまま完了条件を満たしている → ステータス整合で指摘
        assert_eq!(codes(&s.consistency), vec![p("ready_but_open", "none")]);
    }

    #[tokio::test]
    async fn strict_feature_fully_written_is_close_ready() {
        let Some(r) = scryer() else { return };
        let mut c = Case::new();
        c.labels = vec!["type:feature"];
        c.desc = feature_desc(30, 30, 30);
        let s = c.run(&r).await;
        assert_eq!(s.action, Action::CloseReady);
        assert_eq!(
            s.phase,
            Phase::Verifying,
            "実装は書かれていて、残りの阻害要因が無い"
        );
    }

    #[tokio::test]
    async fn strict_feature_missing_only_matome_is_verifying_and_verify_only() {
        let Some(r) = scryer() else { return };
        let mut c = Case::new();
        c.labels = vec!["type:feature"];
        c.desc = feature_desc(30, 30, 0);
        let s = c.run(&r).await;
        assert_eq!((s.phase, s.action), (Phase::Verifying, Action::VerifyOnly));
        assert_eq!(codes(&s.remain), vec![p("missing_section", "まとめ")]);
        // 未完了なので、「完了なのに必須セクションが無い」とは指摘しない
        assert!(s.consistency.is_empty(), "{:?}", s.consistency);
    }

    #[tokio::test]
    async fn strict_feature_without_implementation_is_not_verifying() {
        let Some(r) = scryer() else { return };
        let mut c = Case::new();
        c.labels = vec!["type:feature"];
        c.category = "started";
        c.desc = feature_desc(0, 0, 0);
        let s = c.run(&r).await;
        assert_eq!(
            (s.phase, s.action),
            (Phase::Implementing, Action::ContinueImpl)
        );
        assert_eq!(s.remain.len(), 3, "3節とも未記入");
    }

    #[tokio::test]
    async fn open_subtask_beats_verifying() {
        let Some(r) = scryer() else { return };
        let mut c = Case::new();
        c.labels = vec!["type:feature"];
        c.desc = feature_desc(30, 30, 0);
        c.children = vec![("DEMO-000002", false)];
        let s = c.run(&r).await;
        // 未完了の子があれば、検証待ちとは言わない
        assert_eq!(
            (s.phase, s.action),
            (Phase::Implementing, Action::ContinueImpl)
        );
    }

    // ── bug_fix の条件 ──

    #[tokio::test]
    async fn bug_ticket_is_bug_fix_until_ready() {
        let Some(r) = scryer() else { return };
        let mut c = Case::new();
        c.ty = "bug";
        c.labels = vec!["type:incident"];
        c.desc = bug_desc(&["症状", "調査"]);
        let s = c.run(&r).await;
        assert_eq!(s.action, Action::BugFix);
        assert_eq!(codes(&s.remain).len(), 2, "対策・検証結果が未記入");
    }

    #[tokio::test]
    async fn fully_written_bug_is_close_ready_not_bug_fix() {
        let Some(r) = scryer() else { return };
        let mut c = Case::new();
        c.ty = "bug";
        c.labels = vec!["type:incident"];
        c.desc = bug_desc(&["症状", "調査", "対策", "検証結果"]);
        let s = c.run(&r).await;
        assert_eq!(
            s.action,
            Action::CloseReady,
            "阻害要因が空なら close_ready が優先"
        );
    }

    #[tokio::test]
    async fn runtime_error_with_implementation_is_bug_fix() {
        let Some(r) = scryer() else { return };
        let mut c = Case::new();
        c.labels = vec!["type:feature"];
        c.desc = format!("{}\n本番で500エラーが出る", feature_desc(30, 30, 0));
        let s = c.run(&r).await;
        assert_eq!(s.action, Action::BugFix);
    }

    #[tokio::test]
    async fn runtime_error_word_alone_does_not_make_a_feature_bug_fix() {
        let Some(r) = scryer() else { return };
        // 実装(実施した内容)がまだ無いのに「エラー」の語があるだけでは、bug_fix にしない
        let mut c = Case::new();
        c.labels = vec!["type:feature"];
        c.desc = "## 実装方針\n500エラーを返さないようにする(これから作る)".to_string();
        let s = c.run(&r).await;
        assert_eq!(s.action, Action::ContinueImpl);
    }

    #[tokio::test]
    async fn runtime_error_in_comments_counts_too() {
        let Some(r) = scryer() else { return };
        let mut c = Case::new();
        c.labels = vec!["type:feature"];
        c.desc = feature_desc(30, 30, 0);
        c.comments = "stg で panic した".to_string();
        let s = c.run(&r).await;
        assert_eq!(s.action, Action::BugFix);
    }

    // ── 完了済みとステータス整合 ──

    #[tokio::test]
    async fn completed_without_blockers_is_done_and_close_ready() {
        let Some(r) = scryer() else { return };
        let mut c = Case::new();
        c.category = "completed";
        let s = c.run(&r).await;
        assert_eq!((s.phase, s.action), (Phase::Done, Action::CloseReady));
        assert!(s.consistency.is_empty());
    }

    #[tokio::test]
    async fn cancelled_is_done() {
        let Some(r) = scryer() else { return };
        let mut c = Case::new();
        c.category = "cancelled";
        let s = c.run(&r).await;
        assert_eq!(s.phase, Phase::Done);
    }

    #[tokio::test]
    async fn completed_with_open_subtask_is_flagged() {
        let Some(r) = scryer() else { return };
        let mut c = Case::new();
        c.category = "completed";
        c.children = vec![("DEMO-000002", false)];
        let s = c.run(&r).await;
        assert_eq!(s.phase, Phase::Done);
        assert_eq!(
            codes(&s.consistency),
            vec![p("closed_with_open_subtask", "DEMO-000002")]
        );
    }

    #[tokio::test]
    async fn completed_missing_section_is_flagged_but_cancelled_is_not() {
        let Some(r) = scryer() else { return };
        let mut c = Case::new();
        c.labels = vec!["type:feature"];
        c.desc = feature_desc(30, 30, 0);
        c.category = "completed";
        let s = c.run(&r).await;
        assert_eq!(
            codes(&s.consistency),
            vec![p("closed_missing_section", "まとめ")]
        );

        c.category = "cancelled";
        let s = c.run(&r).await;
        assert!(
            s.consistency.is_empty(),
            "キャンセルは必須セクションを求めない"
        );
    }

    #[tokio::test]
    async fn no_assignee_is_listed_as_remaining_work() {
        let Some(r) = scryer() else { return };
        let mut c = Case::new();
        c.assignees = 0;
        let s = c.run(&r).await;
        assert_eq!(codes(&s.remain), vec![p("no_assignee", "none")]);
        assert_eq!(s.action, Action::ContinueImpl);
    }

    // ── 既存ドキュメント ──

    #[tokio::test]
    async fn docs_are_listed_in_order_from_description_then_comments() {
        let Some(r) = scryer() else { return };
        let mut c = Case::new();
        c.desc = "設計は docs/design/a.md と [別](docs/design/b.md) を参照".to_string();
        c.comments = "docs/design/c.md も見て。docs/design/a.md(重複)".to_string();
        let s = c.run(&r).await;
        assert_eq!(
            s.docs,
            vec!["docs/design/a.md", "docs/design/b.md", "docs/design/c.md"]
        );
    }

    #[tokio::test]
    async fn docs_with_quotes_survive_atom_quoting() {
        let Some(r) = scryer() else { return };
        let mut c = Case::new();
        // クォートや括弧を含む本文でも、パスの抽出と出力が壊れない
        c.desc = "x'); halt. docs/ok.md \\ 'y".to_string();
        let s = c.run(&r).await;
        assert_eq!(s.docs, vec!["docs/ok.md"]);
    }

    // ── 異常の検知(第1弾と同じ観点) ──

    #[tokio::test]
    async fn dropped_fact_is_detected() {
        let Some(r) = scryer() else { return };
        let mut c = Case::new();
        c.children = vec![("DEMO-000002", false)];
        let good = build_facts(&c.input()).expect("facts").finish();
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
        let rules = format!("{}\n{}", close_check::CLOSE_RULES, SUMMARY_RULES);
        let out = r.run(&rules, &broken, "run_summary").await.expect("runs");
        assert!(summary::parse_summary(&out).is_err(), "{out}");
    }

    #[tokio::test]
    async fn missing_summary_fact_count_is_detected() {
        let Some(r) = scryer() else { return };
        let facts = build_facts(&Case::new().input()).expect("facts");
        // summary 用の事実(status_category など)を数えない件数にすると、整合性違反になる
        let text = format!(
            "{}facts_count({}).\n",
            facts.as_str(),
            facts.len().saturating_sub(1)
        );
        let rules = format!("{}\n{}", close_check::CLOSE_RULES, SUMMARY_RULES);
        let out = r.run(&rules, &text, "run_summary").await.expect("runs");
        assert!(summary::parse_summary(&out).is_err(), "{out}");
    }

    #[test]
    fn category_is_restricted_to_a_closed_set_in_facts() {
        let mut c = Case::new();
        c.category = "x'); halt. ('";
        let facts = build_facts(&c.input()).expect("facts");
        assert!(facts.as_str().contains("status_category(other)."));
        assert!(!facts.as_str().contains("halt"));
    }
}
