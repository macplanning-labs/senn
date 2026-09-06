/// presentation/handlers/wiki.rs — Wiki 全機能ハンドラ
///
/// CRUD + カテゴリーフィルタ + エクスポート + 一括ZIP + 履歴 + 差分

use axum::{
    extract::{State, Path, Query},
    response::{Html, Redirect, IntoResponse},
    http::{header, StatusCode},
    Extension, Form,
};
use serde::Deserialize;
use crate::presentation::state::AppState;
use crate::presentation::middleware::auth::SessionUser;
use crate::infrastructure::repositories::wiki_repo;
use crate::domain::services::wiki_service;

#[derive(Deserialize, Default)]
pub struct WikiListQuery {
    pub category: Option<String>,
}

pub async fn project_list(
    State(state): State<AppState>,
    Path(project_id): Path<i32>,
    Query(query): Query<WikiListQuery>,
) -> Html<String> {
    let pages = wiki_repo::find_by_project_with_category(
        &state.pool, Some(project_id), query.category.as_deref()
    ).await.unwrap_or_default();
    Html(format!("<h1>Wiki</h1><p>{}ページ</p>", pages.len()))
}

pub async fn shared_list(
    State(state): State<AppState>,
    Query(query): Query<WikiListQuery>,
) -> Html<String> {
    let pages = wiki_repo::find_by_project_with_category(
        &state.pool, None, query.category.as_deref()
    ).await.unwrap_or_default();
    Html(format!("<h1>共有Wiki</h1><p>{}ページ</p>", pages.len()))
}

pub async fn create_page() -> Html<&'static str> {
    Html("<h1>Wiki作成</h1><form>TODO</form>")
}

#[derive(Deserialize)]
pub struct WikiForm {
    pub title: String,
    pub category: String,
    pub content: String,
}

pub async fn create_submit(
    State(state): State<AppState>,
    Extension(user): Extension<SessionUser>,
    Path(project_id): Path<i32>,
    Form(form): Form<WikiForm>,
) -> impl IntoResponse {
    let slug = wiki_service::generate_slug(&form.title);
    match wiki_repo::create(
        &state.pool, Some(project_id), &form.title, &slug,
        &form.category, &form.content, user.user_id,
    ).await {
        Ok(_) => Redirect::to(&format!("/wiki/{}/{}", project_id, slug)).into_response(),
        Err(e) => Html(format!("<script>alert('エラー: {}');history.back();</script>", e)).into_response(),
    }
}

pub async fn detail(
    State(state): State<AppState>,
    Path((project_id, slug)): Path<(i32, String)>,
) -> Html<String> {
    match wiki_repo::find_by_slug(&state.pool, Some(project_id), &slug).await {
        Ok(Some(page)) => {
            let html_content = wiki_service::render_markdown(&page.content);
            let html_content = wiki_service::resolve_wiki_links(&html_content, Some(project_id));
            Html(format!("<h1>{}</h1><div>{}</div>", page.title, html_content))
        }
        _ => Html("<h1>ページが見つかりません</h1>".to_string()),
    }
}

pub async fn edit_page(
    State(state): State<AppState>,
    Path((project_id, slug)): Path<(i32, String)>,
) -> Html<String> {
    match wiki_repo::find_by_slug(&state.pool, Some(project_id), &slug).await {
        Ok(Some(page)) => Html(format!("<h1>{} 編集</h1><form>TODO</form>", page.title)),
        _ => Html("<h1>ページが見つかりません</h1>".to_string()),
    }
}

pub async fn edit_submit(
    State(state): State<AppState>,
    Extension(user): Extension<SessionUser>,
    Path((project_id, slug)): Path<(i32, String)>,
    Form(form): Form<WikiForm>,
) -> impl IntoResponse {
    if let Ok(Some(page)) = wiki_repo::find_by_slug(&state.pool, Some(project_id), &slug).await {
        // 旧内容をリビジョンとして保存
        let _ = wiki_repo::create_revision(&state.pool, page.id, &page.content, user.user_id, "").await;
        // 新内容で更新
        let _ = wiki_repo::update(&state.pool, page.id, &form.title, &form.category, &form.content, user.user_id).await;
    }
    Redirect::to(&format!("/wiki/{}/{}", project_id, slug))
}

pub async fn delete(
    State(state): State<AppState>,
    Path((project_id, slug)): Path<(i32, String)>,
) -> Redirect {
    if let Ok(Some(page)) = wiki_repo::find_by_slug(&state.pool, Some(project_id), &slug).await {
        let _ = wiki_repo::delete(&state.pool, page.id).await;
    }
    Redirect::to(&format!("/wiki/{}", project_id))
}

pub async fn history(
    State(state): State<AppState>,
    Path((project_id, slug)): Path<(i32, String)>,
) -> Html<String> {
    if let Ok(Some(page)) = wiki_repo::find_by_slug(&state.pool, Some(project_id), &slug).await {
        let revisions = wiki_repo::find_revisions(&state.pool, page.id).await.unwrap_or_default();
        Html(format!("<h1>{} 履歴</h1><p>{}件</p>", page.title, revisions.len()))
    } else {
        Html("<h1>ページが見つかりません</h1>".to_string())
    }
}

pub async fn revision(
    State(state): State<AppState>,
    Path((_project_id, _slug, rev_id)): Path<(i32, String, i32)>,
) -> Html<String> {
    match wiki_repo::find_revision_by_id(&state.pool, rev_id).await {
        Ok(Some(rev)) => Html(format!("<h1>リビジョン #{}</h1><pre>{}</pre>", rev.id, rev.content)),
        _ => Html("<h1>リビジョンが見つかりません</h1>".to_string()),
    }
}

/// Wiki ページを Markdown ファイルとしてエクスポート
pub async fn export(
    State(state): State<AppState>,
    Path((project_id, slug)): Path<(i32, String)>,
) -> impl IntoResponse {
    match wiki_repo::find_by_slug(&state.pool, Some(project_id), &slug).await {
        Ok(Some(page)) => {
            let filename = format!("{}.md", slug);
            let headers = [
                (header::CONTENT_TYPE, "text/markdown; charset=utf-8".to_string()),
                (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{}\"", filename)),
            ];
            (headers, page.content).into_response()
        }
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

/// プロジェクト全ページを ZIP エクスポート
pub async fn export_all(
    State(state): State<AppState>,
    Path(project_id): Path<i32>,
) -> impl IntoResponse {
    let pages = wiki_repo::find_by_project(&state.pool, Some(project_id))
        .await.unwrap_or_default();

    // 簡易 ZIP 生成（各ページを個別 .md ファイルとして）
    // TODO: zip crate を使った本格的な ZIP 生成に差し替え
    let mut content = String::new();
    for page in &pages {
        content.push_str(&format!("# {}\n\n{}\n\n---\n\n", page.title, page.content));
    }

    let headers = [
        (header::CONTENT_TYPE, "text/markdown; charset=utf-8".to_string()),
        (header::CONTENT_DISPOSITION, format!("attachment; filename=\"wiki_export_{}.md\"", project_id)),
    ];
    (headers, content).into_response()
}

/// 共有Wikiページのエクスポート
pub async fn shared_export(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> impl IntoResponse {
    match wiki_repo::find_by_slug(&state.pool, None, &slug).await {
        Ok(Some(page)) => {
            let filename = format!("{}.md", slug);
            let headers = [
                (header::CONTENT_TYPE, "text/markdown; charset=utf-8".to_string()),
                (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{}\"", filename)),
            ];
            (headers, page.content).into_response()
        }
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}
