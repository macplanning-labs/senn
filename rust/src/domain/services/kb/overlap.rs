//! 類似障害検索(3B・7K)の「文字の重なり」方式。
//!
//! 入力と過去の障害を、特徴(gram)の集合にして、重なりの割合で点数をつける。
//! 日本語を単語に分けず、2 文字ずつの組を使うので、言い回しが少し違っても一致する。
//! DB の拡張は使わない(アプリ内で計算する純粋な関数)。設計: docs/design/KB類似障害検索_文字重なり方式_設計.md
//!
//! - 日本語(ひらがな・カタカナ・漢字)の連続部分: 2 文字ずつの組(1 文字だけの塊は、その 1 文字)
//! - 英数字・`_`: 3 文字以上の語(小文字)
//! - 重み: まれな特徴ほど重い。英字の語は ASCII_BOOST 倍、入力のタイトルの特徴は TITLE_BOOST 倍

use std::collections::{HashMap, HashSet};

/// 英字の語の重みの倍率(日本語の組より数が少ないので持ち上げる)
const ASCII_BOOST: f64 = 3.0;
/// 入力のタイトルに含まれる特徴の重みの倍率
const TITLE_BOOST: f64 = 2.0;
/// 重みの指数(大きいほど、まれな特徴を重く見る)
const IDF_POWER: i32 = 3;
/// 英数字の語として採る最短の長さ(文字数)
const ASCII_MIN_CHARS: usize = 3;
/// 「共通する部分」として見せる最短の長さ(文字数)
const PHRASE_MIN_CHARS: usize = 3;

/// 全角の英数字・記号を半角に、英字を小文字にそろえる。
fn normalize_char(c: char) -> char {
    let c = match c {
        '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
        _ => c,
    };
    c.to_lowercase().next().unwrap_or(c)
}

fn normalize(text: &str) -> Vec<char> {
    text.chars().map(normalize_char).collect()
}

/// 日本語の文字か(ひらがな・カタカナ・長音・漢字・半角カナ)
fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{3040}'..='\u{30FF}' | '\u{3400}'..='\u{9FFF}' | '\u{FF66}'..='\u{FF9F}')
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// 同じ種類の文字が続く塊に分ける(種類: 日本語 / 英数字 / その他)。その他は捨てる。
fn runs(chars: &[char]) -> Vec<(bool, &[char])> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let kind = if is_cjk(chars[i]) {
            Some(true)
        } else if is_word(chars[i]) {
            Some(false)
        } else {
            None
        };
        let Some(k) = kind else {
            i += 1;
            continue;
        };
        let start = i;
        while i < chars.len() && ((k && is_cjk(chars[i])) || (!k && is_word(chars[i]))) {
            i += 1;
        }
        out.push((k, &chars[start..i]));
    }
    out
}

/// 文章から特徴の集合を作る。
pub fn grams(text: &str) -> HashSet<String> {
    let chars = normalize(text);
    let mut set = HashSet::new();
    for (cjk, run) in runs(&chars) {
        if cjk {
            if run.len() == 1 {
                set.insert(run.iter().collect());
            }
            for w in run.windows(2) {
                set.insert(w.iter().collect());
            }
        } else if run.len() >= ASCII_MIN_CHARS {
            set.insert(run.iter().collect());
        }
    }
    set
}

fn is_ascii_gram(g: &str) -> bool {
    g.chars().next().is_some_and(is_word)
}

/// 候補全体での、特徴の出現数(重みの計算に使う)
pub struct Corpus {
    n: usize,
    df: HashMap<String, usize>,
}

impl Corpus {
    pub fn new(docs: &[HashSet<String>]) -> Self {
        let mut df: HashMap<String, usize> = HashMap::new();
        for d in docs {
            for g in d {
                *df.entry(g.clone()).or_insert(0) += 1;
            }
        }
        Self { n: docs.len(), df }
    }

    /// 特徴 1 つの重み。まれなほど重い。
    fn weight(&self, g: &str, in_title: bool) -> f64 {
        let df = self.df.get(g).copied().unwrap_or(0);
        let idf = (((self.n + 1) as f64) / ((df + 1) as f64)).ln() + 1.0;
        let mut w = idf.powi(IDF_POWER);
        if is_ascii_gram(g) {
            w *= ASCII_BOOST;
        }
        if in_title {
            w *= TITLE_BOOST;
        }
        w
    }
}

/// 入力の特徴 `query`(うちタイトルのものが `query_title`)のうち、候補 `doc` にもあるものの重みの割合(0〜1)。
pub fn score(
    corpus: &Corpus,
    query: &HashSet<String>,
    query_title: &HashSet<String>,
    doc: &HashSet<String>,
) -> f64 {
    let mut total = 0.0;
    let mut hit = 0.0;
    for g in query {
        let w = corpus.weight(g, query_title.contains(g));
        total += w;
        if doc.contains(g) {
            hit += w;
        }
    }
    if total <= 0.0 {
        0.0
    } else {
        hit / total
    }
}

/// 入力の中で、候補にも連続して一致した部分(「共通する語」として見せる)。長い順に最大 `max` 個。
pub fn shared_phrases(text: &str, doc: &HashSet<String>, max: usize) -> Vec<String> {
    let chars = normalize(text);
    let mut found: Vec<String> = Vec::new();
    for (cjk, run) in runs(&chars) {
        if !cjk {
            if run.len() >= ASCII_MIN_CHARS {
                let w: String = run.iter().collect();
                if doc.contains(&w) {
                    found.push(w);
                }
            }
            continue;
        }
        // 2 文字の組が続けて一致している範囲を、1 つの語にまとめる
        let hit: Vec<bool> = run
            .windows(2)
            .map(|w| doc.contains(&w.iter().collect::<String>()))
            .collect();
        let mut i = 0;
        while i < hit.len() {
            if !hit[i] {
                i += 1;
                continue;
            }
            let start = i;
            while i < hit.len() && hit[i] {
                i += 1;
            }
            // 組が start..i の間つながっている → 文字は start..=i
            let phrase = &run[start..=i];
            if phrase.len() >= PHRASE_MIN_CHARS {
                found.push(phrase.iter().collect());
            }
        }
    }
    let mut seen = HashSet::new();
    found.retain(|p| seen.insert(p.clone()));
    // 長い順(同じ長さなら出現順のまま)
    found.sort_by_key(|p| std::cmp::Reverse(p.chars().count()));
    found.truncate(max);
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(v: &[&str]) -> HashSet<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn grams_japanese_uses_pairs_and_ascii_uses_words() {
        assert_eq!(grams("ログイン"), set(&["ログ", "グイ", "イン"]));
        // 英字は小文字にそろえ、3 文字未満は捨てる。区切りは記号・空白
        assert_eq!(grams("WebAuthn: ab"), set(&["webauthn"]));
        // 全角の英数字は半角にそろえる
        assert_eq!(grams("ＷｅｂＡｕｔｈｎ"), set(&["webauthn"]));
        // 日本語と英字が混ざっても、種類ごとに分ける
        assert_eq!(
            grams("Docker起動失敗"),
            set(&["docker", "起動", "動失", "失敗"])
        );
        // 1 文字だけの日本語の塊は、その 1 文字
        assert_eq!(grams("a 猫 b"), set(&["猫"]));
        assert!(grams("!!! ?? 、。").is_empty());
    }

    #[test]
    fn rare_features_weigh_more() {
        let docs = vec![set(&["ありふ", "まれ"]), set(&["ありふ"]), set(&["ありふ"])];
        let c = Corpus::new(&docs);
        assert!(c.weight("まれ", false) > c.weight("ありふ", false));
        // 英字の語・タイトルの特徴は重い
        assert!(c.weight("abc", false) > c.weight("ありふ", false));
        assert!(c.weight("ありふ", true) > c.weight("ありふ", false));
    }

    #[test]
    fn score_is_overlap_ratio_between_zero_and_one() {
        let docs = vec![grams("ログインできない"), grams("請求書の出力が失敗する")];
        let c = Corpus::new(&docs);
        let q = grams("ログインできない");
        let qt = q.clone();
        let s_same = score(&c, &q, &qt, &docs[0]);
        let s_other = score(&c, &q, &qt, &docs[1]);
        assert!((s_same - 1.0).abs() < 1e-9, "{s_same}");
        assert!(s_other < 0.1, "{s_other}");
        // 入力が空なら 0
        assert_eq!(score(&c, &set(&[]), &set(&[]), &docs[0]), 0.0);
    }

    #[test]
    fn different_wording_still_overlaps() {
        let docs = vec![
            grams("ドメイン移行に伴うログイン不能障害"),
            grams("請求書の出力が失敗する"),
        ];
        let c = Corpus::new(&docs);
        let q = grams("ログインでエラー");
        let s0 = score(&c, &q, &q, &docs[0]);
        let s1 = score(&c, &q, &q, &docs[1]);
        // 小さな候補集合では絶対値は低めに出る。順位(関係あるほうが上)を確かめる
        assert!(s0 > 0.1 && s0 > s1 * 3.0, "{s0} {s1}");
    }

    #[test]
    fn shared_phrases_join_consecutive_pairs_and_skip_short() {
        let doc = grams("ドメイン移行に伴うログイン不能障害 WebAuthn");
        let p = shared_phrases("ログインでエラー WebAuthn", &doc, 4);
        // 「ログイン」は 4 文字で連続一致、「で」「エラー」は一致しない。英字の語はそのまま
        assert!(p.contains(&"ログイン".to_string()), "{p:?}");
        assert!(p.contains(&"webauthn".to_string()), "{p:?}");
        assert!(!p.iter().any(|x| x.contains("エラ")), "{p:?}");
        // 長い順
        let lens: Vec<usize> = p.iter().map(|x| x.chars().count()).collect();
        assert!(lens.windows(2).all(|w| w[0] >= w[1]), "{p:?}");
        // 上限
        assert!(shared_phrases("ログインでエラー WebAuthn", &doc, 1).len() <= 1);
        // 2 文字だけの一致は見せない
        let doc2 = grams("エラ");
        assert!(shared_phrases("エラー", &doc2, 4).is_empty());
    }
}
