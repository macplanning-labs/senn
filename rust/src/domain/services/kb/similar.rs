//! 類似障害検索(3B): 新しく起票する障害(またはAIの調査の入力)に似た、過去の完了済み障害を探す。
//!
//! 順位は「文字の重なり」の点数(overlap.rs)の高い順。Prolog は使わない
//! (プロンプトの知見と違い、結果をそのまま一覧にして返すだけで、規則による選別が無いため)。
//! 本文は Prolog に渡さない。症状・原因・対策は、説明文の節から Rust が取り出して返す。

use super::{detect, overlap, sections, KbError};
use crate::infrastructure::repositories::kb_repo;

/// 検索語のもとにする、入力のタイトル+本文の最大文字数
const SEARCH_TEXT_MAX_CHARS: usize = 4_000;
/// 候補にする、点数(0〜1)の下限。本番の障害データでの測定で、無関係な入力の最高点(0.02〜0.21)と、
/// 関係ある候補の最低点(0.28)の間に置いた(設計書 3 章)
const MIN_SCORE: f64 = 0.25;
/// 候補の本文として見る最大文字数(タイトルに続けて)
const DOC_TEXT_MAX_CHARS: usize = 1_500;
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
    /// 一致の点数(0〜100。入力の特徴のうち、重みで見て、どれだけ重なったか)
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
    let query = overlap::grams(&text);
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let query_title = overlap::grams(title);
    let candidates =
        kb_repo::list_project_incidents(pool, project_id, None, kb_repo::INCIDENT_POOL_MAX)
            .await
            .map_err(|e| KbError::Db(e.to_string()))?;
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    // 候補ごとの特徴(タイトル + 本文の先頭)。重みは、この候補全体での出現数から決める
    let doc_grams: Vec<_> = candidates
        .iter()
        .map(|c| {
            let body: String = c.description.chars().take(DOC_TEXT_MAX_CHARS).collect();
            let mut g = overlap::grams(&c.title);
            g.extend(overlap::grams(&body));
            g
        })
        .collect();
    let corpus = overlap::Corpus::new(&doc_grams);
    let mut scored: Vec<(f64, usize)> = doc_grams
        .iter()
        .enumerate()
        .map(|(i, g)| (overlap::score(&corpus, &query, &query_title, g), i))
        .filter(|(s, _)| *s >= MIN_SCORE)
        .collect();
    // 点数の高い順。同点は新しい順(読んだ順)のまま
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(MAX_RESULTS);

    let input_files = detect::files(&text);
    Ok(scored
        .into_iter()
        .map(|(s, i)| {
            let c = &candidates[i];
            let past_files = detect::files(&c.text_for_files);
            let shared: Vec<String> = input_files
                .iter()
                .filter(|f| past_files.contains(f))
                .cloned()
                .collect();
            let phrases = overlap::shared_phrases(&text, &doc_grams[i], MAX_REASON_ITEMS);
            let (symptom, cause, solution) = excerpts_of(&c.description);
            SimilarIncident {
                reasons: reasons_of(&phrases, &shared),
                ticket_key: c.key.clone(),
                title: c.title.clone(),
                score: (s * 100.0).round() as usize,
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
