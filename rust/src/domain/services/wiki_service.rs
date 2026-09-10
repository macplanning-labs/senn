/// domain/services/wiki_service.rs — Wiki ビジネスロジック
///
/// Markdown → HTML 変換、[[Wikiリンク]] 解決、リビジョン管理。

use sqlx::PgPool;

use crate::infrastructure::repositories::wiki_repo;

/// タイトルから slug を生成
pub fn generate_slug(title: &str) -> String {
    title
        .to_lowercase()
        .replace(' ', "-")
        .replace('　', "-")
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

/// [[Wikiリンク]] を HTML リンクに変換
///
/// `[[ページ名]]` → `<a href="/wiki/{project_id}/{slug}">ページ名</a>`
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
            base_path, slug, page_name
        );

        result = format!("{}{}{}", &result[..open], link, &result[close + 2..]);
        search_start = open + link.len();
    }

    result
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
    let slug = title
        .to_lowercase()
        .replace(' ', "-")
        .replace('　', "-");

    let page_id = wiki_repo::create(
        pool, project_id, title, &slug, category, content, author_id,
    ).await?;

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
    wiki_repo::create_revision(
        pool, page_id, content, editor_id, revision_comment,
    ).await?;

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
