//! 成功事例の知見(2B): DB の候補 → 事実 → Scryer(pattern.pl) → 知見ブロック(固定文)
//!
//! 付加情報なので、「該当なし」と「失敗」は、どちらも空文字列(何も添えない)として扱う。
//! 失敗はログにだけ出し、プロンプト全体は止めない(呼び出し側は Err を空文字列にする)。

use super::facts::Facts;
use super::pattern::{self, SuccessPattern};
use super::runner::Runner;
use super::{atom, detect, sections, KbError};
use crate::infrastructure::repositories::kb_repo::{self, PatternCandidate};
use std::collections::HashMap;

pub const PATTERN_RULES: &str = include_str!("../../../../kb_rules/pattern.pl");

/// 候補として取り出す最大件数 / 候補に必要な、一致した異なる語の最小数
const MAX_CANDIDATES: i64 = 8;
const MIN_HITS: i64 = 2;
/// 1 候補につき、事実にするファイルパスの最大数・落とし穴(後続の不具合)の最大数
const MAX_FILES_PER_CANDIDATE: usize = 30;
const MAX_PITFALLS_PER_CANDIDATE: i64 = 3;
/// 検索語のもとにする、対象のタイトル+本文の最大文字数(巨大な本文でも処理を一定にする)
const SEARCH_TEXT_MAX_CHARS: usize = 4_000;

/// 事実(節の名前・ラベル)として扱ってよい文字列か。出力の区切り(`|`)や制御文字、長すぎるものは捨てる
/// (1 つでも混ざると、出力全体が形式外として拒否されるため、入口で除く)
fn safe_name(s: &str) -> bool {
    let n = s.chars().count();
    (1..=40).contains(&n) && !s.contains('|') && !s.chars().any(|c| c.is_control())
}

/// 1 候補ぶんの事実(節・ラベル・ファイル)を作る。同じ値は 1 候補につき 1 回だけ。
fn candidate_facts(facts: &mut Facts, key: &str, c: &PatternCandidate) -> Result<(), KbError> {
    let q = atom::quote(key);

    let mut seen: Vec<String> = Vec::new();
    for sec in sections::split(&c.description) {
        // 見出しより前の本文(「本文」)や、中身が空の節は、構成の手がかりにならない
        if sec.name == "本文" || sections::body_len(&sec.body) == 0 {
            continue;
        }
        if safe_name(&sec.name) && !seen.contains(&sec.name) {
            facts.fact("sim_section", &[q.clone(), atom::quote(&sec.name)])?;
            seen.push(sec.name);
        }
    }

    let mut seen: Vec<&String> = Vec::new();
    for l in &c.labels {
        // 「AI が解決した」の印は、構成の手がかりにならない
        if l.starts_with("resolved-by-") || !safe_name(l) || seen.contains(&l) {
            continue;
        }
        facts.fact("sim_label", &[q.clone(), atom::quote(l)])?;
        seen.push(l);
    }

    for f in detect::files(&c.text_for_files)
        .into_iter()
        .take(MAX_FILES_PER_CANDIDATE)
    {
        facts.fact("sim_file", &[q.clone(), atom::quote(&f)])?;
    }
    Ok(())
}

/// 候補と、各候補の後続の不具合から、Prolog に渡す事実を作る
pub fn build_facts(
    candidates: &[PatternCandidate],
    pitfalls: &HashMap<String, Vec<String>>,
) -> Result<Facts, KbError> {
    let mut facts = Facts::new();
    for (i, c) in candidates.iter().enumerate() {
        if !atom::valid_key(&c.key) {
            return Err(KbError::InvalidInput(format!(
                "invalid ticket key: {}",
                c.key
            )));
        }
        let q = atom::quote(&c.key);
        facts.fact("sim_ticket", &[q.clone(), (i + 1).to_string()])?;
        candidate_facts(&mut facts, &c.key, c)?;
        for b in pitfalls.get(&c.key).into_iter().flatten() {
            if atom::valid_key(b) {
                facts.fact("pitfall", &[q.clone(), atom::quote(b)])?;
            }
        }
    }
    Ok(facts)
}

/// 事実からパターンを決める(Scryer 実行 + 出力の厳密な解釈)
pub async fn run_pattern(
    runner: &Runner,
    candidates: &[PatternCandidate],
    pitfalls: &HashMap<String, Vec<String>>,
) -> Result<SuccessPattern, KbError> {
    let facts = build_facts(candidates, pitfalls)?;
    let out = runner
        .run(PATTERN_RULES, &facts.finish(), "run_pattern")
        .await?;
    pattern::parse_pattern(&out)
}

/// 対象チケットのプロンプトに添える知見ブロックを作る。
/// 対象が障害でなければ成功事例(2B)、障害なら類似障害(3A)の知見。
/// 該当なし(検索語が無い、候補が無い、共通の事例が無い、チケットが無い)は `Ok("")`。
pub async fn build_learned_block(
    pool: &sqlx::PgPool,
    runner: &Runner,
    ticket_key: &str,
    title: &str,
    description: &str,
) -> Result<String, KbError> {
    let db = |e: anyhow::Error| KbError::Db(e.to_string());

    // 対象が障害なら類似障害の知見(3A)、障害でなければ成功事例の知見(2B)。チケットが無いときは何も添えない
    let kind = match kb_repo::is_not_incident(pool, ticket_key)
        .await
        .map_err(db)?
    {
        Some(true) => kb_repo::CandidateKind::Feature,
        Some(false) => kb_repo::CandidateKind::Incident,
        None => return Ok(String::new()),
    };

    let text: String = format!("{title}\n{description}")
        .chars()
        .take(SEARCH_TEXT_MAX_CHARS)
        .collect();
    let terms = detect::search_terms(&text);
    if terms.is_empty() {
        return Ok(String::new());
    }

    let candidates =
        kb_repo::find_pattern_candidates(pool, ticket_key, kind, &terms, MIN_HITS, MAX_CANDIDATES)
            .await
            .map_err(db)?;
    if candidates.is_empty() {
        return Ok(String::new());
    }

    let mut pitfalls: HashMap<String, Vec<String>> = HashMap::new();
    let mut titles: HashMap<String, String> = candidates
        .iter()
        .map(|c| (c.key.clone(), c.title.clone()))
        .collect();
    for c in &candidates {
        let bugs = kb_repo::find_followup_bugs(pool, &c.key, MAX_PITFALLS_PER_CANDIDATE)
            .await
            .map_err(db)?;
        for b in &bugs {
            if !titles.contains_key(b) {
                let t = kb_repo::title_of(pool, b).await.map_err(db)?;
                titles.insert(b.clone(), t.unwrap_or_default());
            }
        }
        pitfalls.insert(c.key.clone(), bugs);
    }

    let p = run_pattern(runner, &candidates, &pitfalls).await?;
    let title_of = |k: &str| titles.get(k).cloned().unwrap_or_default();
    Ok(match kind {
        kb_repo::CandidateKind::Feature => pattern::render_pattern(&p, &title_of),
        kb_repo::CandidateKind::Incident => {
            let notes: HashMap<&str, pattern::IncidentNotes> = candidates
                .iter()
                .map(|c| (c.key.as_str(), pattern::incident_notes(&c.description)))
                .collect();
            pattern::render_incident_pattern(&p, &title_of, &|k| {
                notes.get(k).cloned().unwrap_or_default()
            })
        }
    })
}

#[cfg(test)]
mod tests {
    //! 規則テスト(実際の Scryer Prolog を使う)。実行できないときはスキップ、
    //! REQUIRE_SCRYER=1 のときは失敗にする(ローカルCIは REQUIRE_SCRYER=1)。
    use super::*;
    use std::time::Duration;

    fn scryer() -> Option<Runner> {
        let bin = std::env::var("SENN_KB_SCRYER_BIN").unwrap_or_else(|_| "scryer-prolog".into());
        let ok = std::process::Command::new(&bin)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !ok {
            assert!(
                std::env::var("REQUIRE_SCRYER").as_deref() != Ok("1"),
                "REQUIRE_SCRYER=1 but scryer-prolog is not runnable: {bin}"
            );
            eprintln!("skip: scryer-prolog is not available");
            return None;
        }
        Some(Runner::new(bin, Duration::from_secs(10), 4))
    }

    /// 本文に、指定した節(見出し)を、中身つきで並べる
    fn desc(headings: &[&str]) -> String {
        headings
            .iter()
            .map(|h| format!("## {h}\n本文です。\n"))
            .collect()
    }

    fn cand(key: &str, headings: &[&str], labels: &[&str], files_text: &str) -> PatternCandidate {
        PatternCandidate {
            matched_terms: Vec::new(),
            score: 0,
            key: key.to_string(),
            title: format!("題名 {key}"),
            description: desc(headings),
            labels: labels.iter().map(|s| s.to_string()).collect(),
            text_for_files: files_text.to_string(),
        }
    }

    async fn run(cs: &[PatternCandidate], pit: &[(&str, &[&str])]) -> SuccessPattern {
        let r = scryer().expect("scryer");
        let map: HashMap<String, Vec<String>> = pit
            .iter()
            .map(|(k, v)| (k.to_string(), v.iter().map(|s| s.to_string()).collect()))
            .collect();
        run_pattern(&r, cs, &map).await.expect("pattern")
    }

    fn keys(p: &SuccessPattern) -> Vec<&str> {
        p.refs.iter().map(|r| r.key.as_str()).collect()
    }

    fn names(v: &[pattern::CountedItem]) -> Vec<(&str, usize)> {
        v.iter().map(|c| (c.name.as_str(), c.count)).collect()
    }

    #[tokio::test]
    async fn quality_is_rank_minus_pitfalls_and_orders_references() {
        if scryer().is_none() {
            return;
        }
        let cs = vec![
            cand("DEMO-000001", &["まとめ"], &[], ""),
            cand("DEMO-000002", &["まとめ"], &[], ""),
            cand("DEMO-000003", &["まとめ"], &[], ""),
        ];
        // 順位 1,2,3 → 品質 92, 84, 76。2 番目に落とし穴が 1 件 → 74 に下がり、3 番目の 76 より下
        let p = run(&cs, &[("DEMO-000002", &["DEMO-000009"])]).await;
        assert_eq!(keys(&p), vec!["DEMO-000001", "DEMO-000003", "DEMO-000002"]);
        let q: Vec<i64> = p.refs.iter().map(|r| r.quality).collect();
        assert_eq!(q, vec![92, 76, 74]);
        assert_eq!(p.pitfalls.len(), 1);
        assert_eq!(p.pitfalls[0].bug_key, "DEMO-000009");
    }

    #[tokio::test]
    async fn only_the_top_five_are_used() {
        if scryer().is_none() {
            return;
        }
        let cs: Vec<_> = (1..=8)
            .map(|i| cand(&format!("DEMO-00000{i}"), &["まとめ"], &[], ""))
            .collect();
        let p = run(&cs, &[]).await;
        assert_eq!(p.refs.len(), 5);
        assert_eq!(keys(&p)[0], "DEMO-000001");
    }

    #[tokio::test]
    async fn common_items_need_a_majority_and_at_least_two() {
        if scryer().is_none() {
            return;
        }
        // 3 件: 「実施した内容」は 3 件、「まとめ」は 2 件(過半数)、「テスト」は 1 件(足りない)
        let cs = vec![
            cand(
                "DEMO-000001",
                &["実施した内容", "まとめ", "テスト"],
                &["type:feature", "ai"],
                "rust/src/a.rs rust/src/b.rs",
            ),
            cand(
                "DEMO-000002",
                &["実施した内容", "まとめ"],
                &["type:feature"],
                "rust/src/a.rs",
            ),
            cand(
                "DEMO-000003",
                &["実施した内容"],
                &["type:feature"],
                "docs/x.md",
            ),
        ];
        let p = run(&cs, &[]).await;
        assert_eq!(names(&p.sections), vec![("実施した内容", 3), ("まとめ", 2)]);
        assert_eq!(names(&p.labels), vec![("type:feature", 3)]);
        assert_eq!(names(&p.files), vec![("rust/src/a.rs", 2)]);
    }

    #[tokio::test]
    async fn a_single_candidate_has_no_common_items_but_is_still_referenced() {
        if scryer().is_none() {
            return;
        }
        // 1 件だけでは「共通」と言えない(2 件以上が条件)。参考事例としては出る
        let cs = vec![cand(
            "DEMO-000001",
            &["まとめ"],
            &["type:feature"],
            "rust/src/a.rs",
        )];
        let p = run(&cs, &[]).await;
        assert_eq!(keys(&p), vec!["DEMO-000001"]);
        assert!(p.sections.is_empty() && p.labels.is_empty() && p.files.is_empty());
    }

    #[tokio::test]
    async fn no_candidates_yields_an_empty_pattern() {
        if scryer().is_none() {
            return;
        }
        let p = run(&[], &[]).await;
        assert!(p.refs.is_empty());
    }

    #[tokio::test]
    async fn counts_are_per_candidate_not_per_heading() {
        if scryer().is_none() {
            return;
        }
        // 同じ節が 1 候補の中に何度出ても、その候補は 1 件と数える
        let cs = vec![
            cand("DEMO-000001", &["まとめ", "まとめ", "まとめ"], &[], ""),
            cand("DEMO-000002", &["実施した内容"], &[], ""),
        ];
        let p = run(&cs, &[]).await;
        assert!(
            p.sections.is_empty(),
            "まとめは 1 候補だけ: {:?}",
            p.sections
        );
    }

    #[tokio::test]
    async fn unsafe_section_and_label_names_are_dropped_not_fatal() {
        if scryer().is_none() {
            return;
        }
        // `|` を含む見出し・ラベルは、出力の区切りと衝突するので、入口で捨てる(全体を壊さない)
        let cs = vec![
            cand(
                "DEMO-000001",
                &["まとめ", "A|B"],
                &["type:feature", "x|y"],
                "",
            ),
            cand(
                "DEMO-000002",
                &["まとめ", "A|B"],
                &["type:feature", "x|y"],
                "",
            ),
        ];
        let p = run(&cs, &[]).await;
        assert_eq!(names(&p.sections), vec![("まとめ", 2)]);
        assert_eq!(names(&p.labels), vec![("type:feature", 2)]);
    }

    #[tokio::test]
    async fn hostile_text_in_names_cannot_break_the_program() {
        if scryer().is_none() {
            return;
        }
        let evil = "x'); halt. ('";
        let cs = vec![
            cand(
                "DEMO-000001",
                &["まとめ", evil],
                &["type:feature", evil],
                "",
            ),
            cand(
                "DEMO-000002",
                &["まとめ", evil],
                &["type:feature", evil],
                "",
            ),
        ];
        let p = run(&cs, &[]).await;
        assert!(names(&p.sections).contains(&("まとめ", 2)));
        assert!(
            names(&p.sections).iter().any(|(n, _)| *n == evil),
            "{:?}",
            p.sections
        );
    }

    #[test]
    fn resolved_by_labels_are_not_facts() {
        let cs = vec![cand(
            "DEMO-000001",
            &["まとめ"],
            &["resolved-by-ai", "type:feature"],
            "",
        )];
        let f = build_facts(&cs, &HashMap::new()).expect("facts");
        assert!(!f.as_str().contains("resolved-by-ai"));
        assert!(f.as_str().contains("type:feature"));
    }

    #[test]
    fn invalid_candidate_key_is_rejected() {
        let cs = vec![cand("DEMO 000001'", &["まとめ"], &[], "")];
        assert!(matches!(
            build_facts(&cs, &HashMap::new()),
            Err(KbError::InvalidInput(_))
        ));
    }

    #[tokio::test]
    async fn dropped_fact_is_detected() {
        let Some(r) = scryer() else { return };
        let cs = vec![
            cand("DEMO-000001", &["まとめ"], &["type:feature"], ""),
            cand("DEMO-000002", &["まとめ"], &["type:feature"], ""),
        ];
        let good = build_facts(&cs, &HashMap::new()).expect("facts").finish();
        let broken: String = good
            .lines()
            .map(|l| {
                if l.starts_with("sim_ticket('DEMO-000002'") {
                    "sim_ticket('DEMO-000002', unterminated, 2).".to_string()
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        assert_ne!(good, broken);
        let out = r
            .run(PATTERN_RULES, &broken, "run_pattern")
            .await
            .expect("runs");
        assert!(pattern::parse_pattern(&out).is_err(), "{out}");
    }

    #[tokio::test]
    async fn missing_facts_count_is_detected() {
        let Some(r) = scryer() else { return };
        let cs = vec![cand("DEMO-000001", &["まとめ"], &[], "")];
        let facts = build_facts(&cs, &HashMap::new()).expect("facts");
        let out = r
            .run(PATTERN_RULES, facts.as_str(), "run_pattern")
            .await
            .expect("runs");
        assert!(pattern::parse_pattern(&out).is_err(), "{out}");
    }
}
