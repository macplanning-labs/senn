/// domain/services/wiki_service.rs — Wiki ビジネスロジック
///
/// Markdown → HTML 変換、[[Wikiリンク]] 解決、リビジョン管理。
use sqlx::PgPool;

use crate::infrastructure::repositories::wiki_repo;

/// タイトルから slug を生成
pub fn generate_slug(title: &str) -> String {
    title.to_lowercase().replace(' ', "-").replace('　', "-")
}

/// Markdown を HTML に変換（pulldown-cmark）
pub fn render_markdown(content: &str) -> String {
    use pulldown_cmark::{html, Options, Parser};

    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(content, options);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    html_output
}

/// Markdown → HTML → [[Wikiリンク]] 解決 → 無害化 までを行い、画面に出す HTML を返す。
///
/// 無害化は必ず最後に行う(Wiki リンクで作った `<a>` に入力由来の属性が混ざらないように)。
pub fn render_wiki_html(content: &str, project_id: Option<i32>) -> String {
    let html = render_markdown(content);
    let linked = resolve_wiki_links(&html, project_id);
    sanitize_html(&linked)
}

/// 生の HTML 断片を無害化する(許可リスト方式)。
///
/// - script / style / iframe / イベント属性 / javascript:・data: などは除かれる
/// - a[href] は http(s)・mailto・`/` で始まる相対パスのみ。rel="noopener noreferrer" を付ける
/// - img[src] は http(s) のみ
/// - input は type="checkbox" のみ(タスクリスト用)
/// - 表の揃え(style="text-align:…" / align)、code の class="language-…" は許可
fn sanitize_html(html: &str) -> String {
    use std::borrow::Cow;
    use std::collections::HashSet;

    let mut builder = ammonia::Builder::default();
    builder
        .add_tags(["input"])
        .add_tag_attributes("input", ["type", "checked", "disabled"])
        .add_tag_attributes("a", ["class"])
        .add_tag_attributes("code", ["class"])
        .add_tag_attributes("th", ["align", "style"])
        .add_tag_attributes("td", ["align", "style"])
        .filter_style_properties(HashSet::from(["text-align"]))
        .link_rel(Some("noopener noreferrer"))
        .url_schemes(HashSet::from(["http", "https", "mailto"]))
        .attribute_filter(|element, attribute, value| -> Option<Cow<'_, str>> {
            let keep = match (element, attribute) {
                ("a", "href") => is_allowed_link(value),
                ("img", "src") => is_http_url(value),
                ("input", "type") => value == "checkbox",
                ("a", "class") => value == "wiki-link",
                ("code", "class") => is_language_class(value),
                ("th" | "td", "align") => matches!(value, "left" | "right" | "center"),
                _ => true,
            };
            if keep {
                Some(Cow::Borrowed(value))
            } else {
                None
            }
        });

    builder.clean(html).to_string()
}

/// リンク先として許可する値: http(s)・mailto・`/` で始まる相対パス(`//` は不可)
fn is_allowed_link(value: &str) -> bool {
    let v = value.trim();
    if v.starts_with("//") {
        return false;
    }
    if v.starts_with('/') {
        return true;
    }
    let lower = v.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")
}

fn is_http_url(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// `language-xxx` 形式のクラスだけ許可する(xxx は英数字・`-`・`_`・`+`)
fn is_language_class(value: &str) -> bool {
    match value.strip_prefix("language-") {
        Some(lang) => {
            !lang.is_empty()
                && lang
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '+'))
        }
        None => false,
    }
}

/// [[Wikiリンク]] を HTML リンクに変換
///
/// `[[ページ名]]` → `<a href="/wiki/{project_id}/{slug}">ページ名</a>`
///
/// 入力は pulldown-cmark の出力(通常の文字は既にエスケープ済み)。生の HTML 断片の中の
/// `[[...]]` にも対応するため、`" ' < >` は実体参照に置き換える(`&` は既存の実体参照を
/// 壊さないように触らない)。
pub fn resolve_wiki_links(html: &str, project_id: Option<i32>) -> String {
    let base_path = match project_id {
        Some(pid) => format!("/wiki/{}", pid),
        None => "/wiki/shared".to_string(),
    };

    let mut result = html.to_string();
    let re_pattern = "[[";
    let mut search_start = 0;

    loop {
        let open = match result[search_start..].find(re_pattern) {
            Some(pos) => search_start + pos,
            None => break,
        };
        let close = match result[open + 2..].find("]]") {
            Some(pos) => open + 2 + pos,
            None => break,
        };

        let page_name = &result[open + 2..close];
        let slug = page_name
            .to_lowercase()
            .replace(' ', "-")
            .replace('　', "-");

        let link = format!(
            "<a href=\"{}/{}\" class=\"wiki-link\">{}</a>",
            base_path,
            escape_raw_quotes_and_angles(&slug),
            escape_raw_quotes_and_angles(page_name)
        );

        result = format!("{}{}{}", &result[..open], link, &result[close + 2..]);
        search_start = open + link.len();
    }

    result
}

/// `" ' < >` を実体参照にする(`&` は既存の実体参照を壊さないため触らない)
fn escape_raw_quotes_and_angles(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 出力のどのタグにも `on…=` 形式のイベント属性が無いことを確認する
    fn assert_no_event_attribute(html: &str) {
        for tag in html.split('<').skip(1) {
            let body = tag.split('>').next().unwrap_or("");
            for token in body.split_whitespace().skip(1) {
                let name = token.split('=').next().unwrap_or("").to_ascii_lowercase();
                assert!(
                    !(name.starts_with("on") && token.contains('=')),
                    "イベント属性が残っている: {token} (in <{body}>)"
                );
            }
        }
    }

    #[test]
    fn xss_script_tag_is_removed() {
        let out = render_wiki_html("本文\n\n<script>alert(1)</script>\n", Some(1));
        assert!(!out.contains("<script"), "出力: {out}");
        assert!(!out.contains("alert(1)</script"), "出力: {out}");
    }

    #[test]
    fn xss_img_onerror_is_removed() {
        let out = render_wiki_html("<img src=x onerror=alert(1)>", Some(1));
        assert!(!out.contains("onerror"), "出力: {out}");
        assert_no_event_attribute(&out);
    }

    #[test]
    fn xss_javascript_link_is_removed() {
        let out = render_wiki_html("[x](javascript:alert(1))", Some(1));
        assert!(!out.contains("javascript:"), "出力: {out}");
        assert!(out.contains('x'), "リンク文字は残す。出力: {out}");
    }

    #[test]
    fn xss_data_and_protocol_relative_links_are_removed() {
        let out = render_wiki_html(
            "[a](data:text/html,<script>alert(1)</script>) [b](//evil.example/x)",
            Some(1),
        );
        assert!(!out.contains("data:"), "出力: {out}");
        assert!(!out.contains("evil.example"), "出力: {out}");
        assert!(!out.contains("<script"), "出力: {out}");
    }

    #[test]
    fn xss_wiki_link_attribute_breakout_is_neutralized() {
        // Markdown 経由(pulldown-cmark が引用符をエスケープする)
        let out = render_wiki_html(r#"[[Foo "onmouseover="alert(1)]]"#, Some(1));
        assert_no_event_attribute(&out);

        // 生の HTML 断片の中の [[...]] (引用符が素通しになる経路)
        let raw = render_wiki_html(r#"<div>[[x" onmouseover="alert(1)]]</div>"#, Some(1));
        // 属性として出てこないこと(テキスト中の引用符は無害)
        assert!(
            raw.contains(r#"<a href="/wiki/1/x&quot;-onmouseover=&quot;alert(1)""#),
            "出力: {raw}"
        );
        assert_no_event_attribute(&raw);
    }

    #[test]
    fn normal_markdown_still_renders() {
        let md = "# 見出し\n\n段落です。[リンク](https://example.com/a)\n\n\
                  | 左 | 右 |\n|:--|--:|\n| a | b |\n\n\
                  - [x] 完了\n- [ ] 未完了\n\n\
                  ```rust\nfn main() {}\n```\n\n\
                  > 引用\n\n---\n\n~~取消~~\n\n[[ほかのページ]]\n";
        let out = render_wiki_html(md, Some(7));

        assert!(out.contains("<h1>見出し</h1>"), "出力: {out}");
        assert!(out.contains("<p>段落です。"), "出力: {out}");
        assert!(
            out.contains(r#"href="https://example.com/a""#),
            "出力: {out}"
        );
        assert!(out.contains("<table>"), "出力: {out}");
        assert!(
            out.contains("<thead>") && out.contains("<tbody>"),
            "出力: {out}"
        );
        assert!(out.contains("<th") && out.contains("<td"), "出力: {out}");
        assert!(out.contains("text-align:left"), "表の揃えが消えた: {out}");
        assert!(
            out.contains("<input") && out.contains(r#"type="checkbox""#),
            "出力: {out}"
        );
        assert!(
            out.contains("disabled") && out.contains("checked"),
            "出力: {out}"
        );
        assert!(
            out.contains(r#"<code class="language-rust">"#),
            "出力: {out}"
        );
        assert!(out.contains("<blockquote>"), "出力: {out}");
        assert!(out.contains("<hr"), "出力: {out}");
        assert!(out.contains("<del>取消</del>"), "出力: {out}");
        assert!(
            out.contains(r#"<a href="/wiki/7/ほかのページ" class="wiki-link""#),
            "Wiki リンクが出ない: {out}"
        );
        assert_no_event_attribute(&out);
    }

    #[test]
    fn external_links_get_safe_rel() {
        let out = render_wiki_html("[a](https://example.com)", None);
        assert!(out.contains(r#"rel="noopener noreferrer""#), "出力: {out}");
    }
}

/// Wiki ページ作成（初回リビジョン自動保存）
pub async fn create_page(
    pool: &PgPool,
    project_id: Option<i32>,
    title: &str,
    category: &str,
    content: &str,
    author_id: i32,
) -> anyhow::Result<i32> {
    let slug = title.to_lowercase().replace(' ', "-").replace('　', "-");

    let page_id =
        wiki_repo::create(pool, project_id, title, &slug, category, content, author_id).await?;

    // 初回リビジョン保存
    wiki_repo::create_revision(pool, page_id, content, author_id, "初回作成").await?;

    Ok(page_id)
}

/// Wiki ページ更新（リビジョン自動保存）
pub async fn update_page(
    pool: &PgPool,
    page_id: i32,
    title: &str,
    category: &str,
    content: &str,
    editor_id: i32,
    revision_comment: &str,
) -> anyhow::Result<()> {
    wiki_repo::update(pool, page_id, title, category, content, editor_id).await?;

    // リビジョン保存
    wiki_repo::create_revision(pool, page_id, content, editor_id, revision_comment).await?;

    Ok(())
}

/// 2つのリビジョン間の簡易差分（行単位）
pub fn simple_diff(old_content: &str, new_content: &str) -> Vec<DiffLine> {
    let old_lines: Vec<&str> = old_content.lines().collect();
    let new_lines: Vec<&str> = new_content.lines().collect();

    let mut result = Vec::new();
    let max_len = old_lines.len().max(new_lines.len());

    for i in 0..max_len {
        match (old_lines.get(i), new_lines.get(i)) {
            (Some(old), Some(new)) => {
                if old != new {
                    result.push(DiffLine::Removed(old.to_string()));
                    result.push(DiffLine::Added(new.to_string()));
                } else {
                    result.push(DiffLine::Unchanged(old.to_string()));
                }
            }
            (Some(old), None) => {
                result.push(DiffLine::Removed(old.to_string()));
            }
            (None, Some(new)) => {
                result.push(DiffLine::Added(new.to_string()));
            }
            (None, None) => break,
        }
    }

    result
}

/// 差分行の種類
#[derive(Debug, Clone)]
pub enum DiffLine {
    Added(String),
    Removed(String),
    Unchanged(String),
}

impl DiffLine {
    pub fn css_class(&self) -> &str {
        match self {
            Self::Added(_) => "diff-added",
            Self::Removed(_) => "diff-removed",
            Self::Unchanged(_) => "diff-unchanged",
        }
    }

    pub fn prefix(&self) -> &str {
        match self {
            Self::Added(_) => "+",
            Self::Removed(_) => "-",
            Self::Unchanged(_) => " ",
        }
    }

    pub fn content(&self) -> &str {
        match self {
            Self::Added(s) | Self::Removed(s) | Self::Unchanged(s) => s,
        }
    }
}
