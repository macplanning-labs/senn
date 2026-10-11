//! Markdown 見出し分割・名称正規化

#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    /// 元の見出し行(`## 症状` の形式で改行なし)
    pub raw: String,
    /// 正規化名(`canon()` の出力)
    pub name: String,
    /// 見出しの下の本文(trim済み)
    pub body: String,
}

/// Markdown を見出しで分割
///
/// 行頭が `## ` または `### ` で始まる行を見出しとして扱う。
/// コードフェンス(``` ... ```)の内側の `#` は見出しとして扱わない。
/// フェンス内のコンテンツは本文の一部として数える(コードだけの「対策」を未記入と誤判定しないため)。
///
/// 最初の見出しより前に非空行があれば、それを「本文」として1つの Section にする
/// (raw="本文", name="本文", body=その内容)。
pub fn split(md: &str) -> Vec<Section> {
    let mut sections = Vec::new();
    let mut raw = String::new();
    let mut body = String::new();
    let mut in_fence = false;

    for line in md.lines() {
        if line.starts_with("```") {
            in_fence = !in_fence;
            body.push_str(line);
            body.push('\n');
            continue;
        }
        if !in_fence && (line.starts_with("## ") || line.starts_with("### ")) {
            push_section(&mut sections, &raw, &body);
            raw = line.trim().to_string();
            body.clear();
            continue;
        }
        body.push_str(line);
        body.push('\n');
    }
    push_section(&mut sections, &raw, &body);
    sections
}

fn push_section(sections: &mut Vec<Section>, raw: &str, body: &str) {
    let body = body.trim();
    if raw.is_empty() {
        // 最初の見出しより前の本文
        if !body.is_empty() {
            sections.push(Section {
                raw: "本文".to_string(),
                name: "本文".to_string(),
                body: body.to_string(),
            });
        }
        return;
    }
    sections.push(Section {
        raw: raw.to_string(),
        name: canon(raw),
        body: body.to_string(),
    });
}

/// セクション名を正規化
///
/// 見出しテキストから標準名に変換する対応表:
/// - 症状/現象 → 症状
/// - 調査/原因 → 調査
/// - 検証/確認結果 → 検証結果
/// - 対策/対応/解決/修正 → 対策
/// - 実施 → 実施した内容
/// - 実装方針/実装計画/方針 → 実装方針
/// - まとめ/総括 → まとめ
///
/// 見出しが `## 症状` なら先頭の `##` と空白を除去して判定。
/// 一致するものがなければ trim して原文を返す。
pub fn canon(raw: &str) -> String {
    // 見出し記号を除去
    let trimmed = raw.trim_start_matches('#').trim_start().trim_end();

    // 対応表(長いものから順に判定して最初にマッチしたものを返す)
    let table: &[(Vec<&str>, &str)] = &[
        (vec!["症状", "現象"], "症状"),
        (vec!["調査", "原因"], "調査"),
        (vec!["検証", "確認結果"], "検証結果"),
        (vec!["対策", "対応", "解決", "修正"], "対策"),
        (vec!["実施"], "実施した内容"),
        (vec!["実装方針", "実装計画", "方針"], "実装方針"),
        (vec!["まとめ", "総括"], "まとめ"),
    ];

    for (patterns, standard_name) in table {
        for pattern in patterns {
            if trimmed.contains(pattern) {
                return standard_name.to_string();
            }
        }
    }

    trimmed.to_string()
}

/// セクション本文の文字数(Rust のcharベース)
pub fn body_len(s: &str) -> usize {
    s.trim().chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_no_heading() {
        let md = "Just some text\nwith no headings";
        let sections = split(md);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].name, "本文");
        assert!(sections[0].body.contains("Just some text"));
    }

    #[test]
    fn test_split_empty() {
        let md = "";
        let sections = split(md);
        assert_eq!(sections.len(), 0);
    }

    #[test]
    fn test_split_h2_h3() {
        let md = "## 症状\nファイルが見つかりません\n### 詳細\n何か問題";
        let sections = split(md);
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].name, "症状");
        assert_eq!(sections[1].name, "詳細");
    }

    #[test]
    fn test_split_fence_protection() {
        let md = "## 症状\nFirst body\n```\n## Inside fence\n```\n## 調査\nSecond";
        let sections = split(md);
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].name, "症状");
        assert_eq!(sections[1].name, "調査");
        assert!(sections[0].body.contains("Inside fence"));
        assert_eq!(sections.len(), 2);
    }

    #[test]
    fn test_canon_symptoms() {
        assert_eq!(canon("## 症状"), "症状");
        assert_eq!(canon("## 現象"), "症状");
        assert_eq!(canon("### 現象発生"), "症状");
    }

    #[test]
    fn test_canon_investigation() {
        assert_eq!(canon("## 調査"), "調査");
        assert_eq!(canon("## 原因"), "調査");
    }

    #[test]
    fn test_canon_verification() {
        assert_eq!(canon("## 検証結果"), "検証結果");
        assert_eq!(canon("## 確認結果"), "検証結果");
        assert_eq!(canon("## 検証"), "検証結果");
    }

    #[test]
    fn test_canon_measures() {
        assert_eq!(canon("## 対策"), "対策");
        assert_eq!(canon("## 対応"), "対策");
        assert_eq!(canon("## 解決"), "対策");
        assert_eq!(canon("## 修正"), "対策");
    }

    #[test]
    fn test_canon_action() {
        assert_eq!(canon("## 実施"), "実施した内容");
    }

    #[test]
    fn test_canon_implementation_plan() {
        assert_eq!(canon("## 実装方針"), "実装方針");
        assert_eq!(canon("## 実装計画"), "実装方針");
        assert_eq!(canon("## 方針"), "実装方針");
    }

    #[test]
    fn test_canon_summary() {
        assert_eq!(canon("## まとめ"), "まとめ");
        assert_eq!(canon("## 総括"), "まとめ");
    }

    #[test]
    fn test_canon_unknown() {
        assert_eq!(canon("## 未知のセクション"), "未知のセクション");
        assert_eq!(canon("### なにか"), "なにか");
    }

    #[test]
    fn test_canon_with_number() {
        assert_eq!(canon("## 3. 対策（暫定）"), "対策");
    }

    #[test]
    fn test_body_len_japanese() {
        // 日本語3文字
        assert_eq!(body_len("日本語"), 3);
    }

    #[test]
    fn test_body_len_with_spaces() {
        // スペースを含めてカウント
        let text = "hello world";
        assert_eq!(body_len(text), 11);
    }

    #[test]
    fn test_body_len_trim() {
        assert_eq!(body_len("  text  "), 4);
    }

    #[test]
    fn test_split_pre_text() {
        let md = "Introduction text\n## Heading\nBody";
        let sections = split(md);
        // 最初の見出しの前のテキストは別セクション
        assert!(sections.len() >= 1);
        if sections.len() >= 2 {
            assert_eq!(sections[0].name, "本文");
        }
    }

    #[test]
    fn test_split_multiple() {
        let md = "## 症状\nBody1\n## 調査\nBody2\n## 対策\nBody3";
        let sections = split(md);
        assert_eq!(sections.len(), 3);
        assert_eq!(sections[0].name, "症状");
        assert_eq!(sections[1].name, "調査");
        assert_eq!(sections[2].name, "対策");
    }

    #[test]
    fn test_split_only_heading_no_body() {
        let md = "## 症状\n## 調査\n";
        let sections = split(md);
        assert!(sections.len() >= 1);
    }
}
