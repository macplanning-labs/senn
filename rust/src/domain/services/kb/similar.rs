//! 類似障害検索(3B): 新しく起票する障害(またはAIの調査の入力)に似た、過去の完了済み障害を探す。
//!
//! 順位は SQL の一致語数(互いに重ならない検索語の数)の多い順。Prolog は使わない
//! (プロンプトの知見と違い、結果をそのまま一覧にして返すだけで、規則による選別が無いため)。
//! 本文は Prolog に渡さない。症状・原因・対策は、説明文の節から Rust が取り出して返す。

use super::{detect, sections, KbError};
use crate::infrastructure::repositories::kb_repo::{self, CandidateKind};

/// 検索語のもとにする、入力のタイトル+本文の最大文字数
const SEARCH_TEXT_MAX_CHARS: usize = 4_000;
/// 候補に必要な、一致した(重ならない)語の最小数
const MIN_HITS: i64 = 2;
/// 検索語が 1 つしか作れない短い入力(例: 「ログインできない」)のとき、その 1 語がこの文字数以上なら、
/// 1 語の一致でも候補にする(日本語は単語に分けないため、短い題だと語が 1 つしかできず、常に 0 件になるのを防ぐ)
const SINGLE_TERM_MIN_CHARS: usize = 6;

/// 検索語の数と長さから、候補に必要な一致語数を決める。
/// 語が 1 つだけで、それが十分長い複合語のときだけ 1 にゆるめる。それ以外は `MIN_HITS`。
fn min_hits_for(terms: &[String]) -> i64 {
    match terms {
        [only] if only.chars().count() >= SINGLE_TERM_MIN_CHARS => 1,
        _ => MIN_HITS,
    }
}
/// 返す最大件数
pub const MAX_RESULTS: usize = 5;
/// 症状・原因・対策の抜粋の最大文字数
const EXCERPT_MAX_CHARS: usize = 300;
/// 一致の理由に出す、共通する語・ファイルの最大数
const MAX_REASON_ITEMS: usize = 4;

/// 類似障害 1 件
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimilarIncident {
    pub ticket_key: String,
    pub title: String,
    /// 一致の点数(互いに重ならない一致語の数)
    pub score: usize,
    /// 一致の理由(共通する語、同じファイル)
    pub reasons: Vec<String>,
    pub symptom: Option<String>,
    pub cause: Option<String>,
    pub solution: Option<String>,
}

/// 節の本文から、表示用の抜粋を作る。制御文字を除き、空白(改行を含む)を 1 つにまとめ、
/// 300 文字で切る。何も残らなければ None。
pub fn excerpt(body: &str) -> Option<String> {
    let flat: String = body
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if flat.is_empty() {
        return None;
    }
    if flat.chars().count() <= EXCERPT_MAX_CHARS {
        return Some(flat);
    }
    let head: String = flat.chars().take(EXCERPT_MAX_CHARS).collect();
    Some(format!("{head}…"))
}

/// 説明文から、症状・原因(調査)・対策の抜粋を取り出す
pub fn excerpts_of(description: &str) -> (Option<String>, Option<String>, Option<String>) {
    let (mut symptom, mut cause, mut solution) = (None, None, None);
    for sec in sections::split(description) {
        match sec.name.as_str() {
            "症状" if symptom.is_none() => symptom = excerpt(&sec.body),
            "調査" if cause.is_none() => cause = excerpt(&sec.body),
            "対策" if solution.is_none() => solution = excerpt(&sec.body),
            _ => {}
        }
    }
    (symptom, cause, solution)
}

/// 一致の理由を、固定文で作る
fn reasons_of(matched_terms: &[String], shared_files: &[String]) -> Vec<String> {
    let mut v = Vec::new();
    if !matched_terms.is_empty() {
        let terms: Vec<&str> = matched_terms
            .iter()
            .take(MAX_REASON_ITEMS)
            .map(String::as_str)
            .collect();
        v.push(format!("共通する語: {}", terms.join("、")));
    }
    if !shared_files.is_empty() {
        let files: Vec<&str> = shared_files
            .iter()
            .take(MAX_REASON_ITEMS)
            .map(String::as_str)
            .collect();
        v.push(format!("同じファイル: {}", files.join("、")));
    }
    v
}

/// プロジェクトの、過去の完了済み障害から、入力に似たものを探す(最大 `MAX_RESULTS` 件)。
/// 該当なしは空の一覧。
pub async fn search_similar_incidents(
    pool: &sqlx::PgPool,
    project_id: i32,
    title: &str,
    description: &str,
) -> Result<Vec<SimilarIncident>, KbError> {
    let text: String = format!("{title}\n{description}")
        .chars()
        .take(SEARCH_TEXT_MAX_CHARS)
        .collect();
    let terms = detect::search_terms(&text);
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    let candidates = kb_repo::find_candidates_in_project(
        pool,
        project_id,
        None,
        CandidateKind::Incident,
        &terms,
        min_hits_for(&terms),
        MAX_RESULTS as i64,
    )
    .await
    .map_err(|e| KbError::Db(e.to_string()))?;

    let input_files = detect::files(&text);
    Ok(candidates
        .into_iter()
        .map(|c| {
            let past_files = detect::files(&c.text_for_files);
            let shared: Vec<String> = input_files
                .iter()
                .filter(|f| past_files.contains(f))
                .cloned()
                .collect();
            let (symptom, cause, solution) = excerpts_of(&c.description);
            SimilarIncident {
                reasons: reasons_of(&c.matched_terms, &shared),
                ticket_key: c.key,
                title: c.title,
                score: c.score,
                symptom,
                cause,
                solution,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn min_hits_relaxes_only_for_one_long_term() {
        // 短い題が 1 語(8 文字)になる → 1 語の一致でも候補にする
        assert_eq!(min_hits_for(&detect::search_terms("ログインできない\n")), 1);
        // 6 文字ちょうどは緩める、5 文字は緩めない
        assert_eq!(min_hits_for(&owned(&["abcdef"])), 1);
        assert_eq!(min_hits_for(&owned(&["abcde"])), MIN_HITS);
        // 語が複数あれば従来どおり 2 語必要
        assert_eq!(
            min_hits_for(&owned(&["ログインできない", "認証エラー"])),
            MIN_HITS
        );
        // 語が無いとき(呼び出し側で先に空を返すが、念のため)
        assert_eq!(min_hits_for(&[]), MIN_HITS);
    }

    #[test]
    fn excerpt_flattens_whitespace_and_control_chars_and_truncates() {
        assert_eq!(excerpt(""), None);
        assert_eq!(excerpt(" \n\t \u{7} "), None);
        assert_eq!(
            excerpt("一行目\n\n二行目\u{7}  三行目").as_deref(),
            Some("一行目 二行目 三行目")
        );
        let long = "あ".repeat(400);
        let got = excerpt(&long).expect("some");
        assert_eq!(got.chars().count(), 301);
        assert!(got.ends_with('…'));
        // ちょうど上限は切らない
        assert_eq!(
            excerpt(&"い".repeat(300)).expect("some").chars().count(),
            300
        );
    }

    #[test]
    fn excerpts_of_reads_symptom_cause_solution_and_aliases() {
        let d = "## 症状\n落ちる\n## 調査\n待ちが短い\n## 対策\n延ばした\n## 検証結果\n合格";
        assert_eq!(
            excerpts_of(d),
            (
                Some("落ちる".into()),
                Some("待ちが短い".into()),
                Some("延ばした".into())
            )
        );
        let d = "## 現象\nA\n## 原因\nB\n## 対応\nC";
        assert_eq!(
            excerpts_of(d),
            (Some("A".into()), Some("B".into()), Some("C".into()))
        );
        assert_eq!(excerpts_of("見出しなし"), (None, None, None));
    }

    #[test]
    fn reasons_are_fixed_sentences_and_capped() {
        assert!(reasons_of(&[], &[]).is_empty());
        let terms: Vec<String> = (1..=6).map(|i| format!("語{i}")).collect();
        let r = reasons_of(&terms, &["rust/src/a.rs".into()]);
        assert_eq!(r[0], "共通する語: 語1、語2、語3、語4");
        assert_eq!(r[1], "同じファイル: rust/src/a.rs");
    }
}
