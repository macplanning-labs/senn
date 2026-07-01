/// presentation/handlers/tickets.rs — チケット CRUD ハンドラ

use axum::{
    extract::{State, Path, Query},
    response::{Html, Redirect, IntoResponse},
    Extension, Form,
};
use serde::Deserialize;
use crate::presentation::state::AppState;
use crate::presentation::middleware::auth::SessionUser;
use crate::domain::services::ticket_service;
use crate::infrastructure::repositories::{ticket_repo, comment_repo, project_repo};

#[derive(Deserialize, Default)]
pub struct TicketListQuery {
    pub status: Option<String>,
    pub priority: Option<String>,
    pub assignee_id: Option<i32>,
    pub category_id: Option<i32>,
    pub milestone_id: Option<i32>,
    pub keyword: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<SessionUser>,
    Query(query): Query<TicketListQuery>,
) -> Html<String> {
    let filter = ticket_repo::TicketFilter {
        project_id: user.current_project_id,
        status: query.status,
        priority: query.priority,
        assignee_id: query.assignee_id,
        category_id: query.category_id,
        milestone_id: query.milestone_id,
        keyword: query.keyword,
        ..Default::default()
    };

    let tickets = ticket_repo::find_all(&state.pool, &filter)
        .await.unwrap_or_default();

    // TODO: Askamaテンプレートに差し替え
    Html(format!("<h1>チケット一覧</h1><p>{}件</p>", tickets.len()))
}

pub async fn create_page(
    State(_state): State<AppState>,
    Extension(_user): Extension<SessionUser>,
) -> Html<&'static str> {
    Html("<h1>チケット作成</h1><form>TODO</form>")
}

#[derive(Deserialize)]
pub struct TicketForm {
    pub title: String,
    pub description: Option<String>,
    pub ticket_type: String,
    pub priority: String,
    pub project_id: i32,
    pub category_id: Option<i32>,
    pub assignee_id: Option<i32>,
    pub milestone_id: Option<i32>,
    pub parent_id: Option<i32>,
    pub start_date: Option<String>,
    pub due_date: Option<String>,
}

fn parse_date(s: Option<&str>) -> Option<chrono::NaiveDate> {
    s.and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
}

pub async fn create_submit(
    State(state): State<AppState>,
    Extension(user): Extension<SessionUser>,
    Form(form): Form<TicketForm>,
) -> impl IntoResponse {
    // プロジェクトのプレフィックスを取得
    let project = project_repo::find_by_id(&state.pool, form.project_id).await;
    let prefix = match project {
        Ok(Some(p)) => p.prefix,
        _ => return Html("<script>alert('プロジェクトが見つかりません');history.back();</script>").into_response(),
    };

    let req = ticket_service::CreateTicketRequest {
        title: form.title,
        description: form.description.unwrap_or_default(),
        ticket_type: form.ticket_type,
        priority: form.priority,
        project_id: Some(form.project_id),
        category_id: form.category_id,
        assignee_id: form.assignee_id,
        milestone_id: form.milestone_id,
        parent_id: form.parent_id,
        start_date: parse_date(form.start_date.as_deref()),
        due_date: parse_date(form.due_date.as_deref()),
    };

    match ticket_service::create_ticket(&state.pool, &req, user.user_id, &prefix).await {
        Ok(ticket_id) => {
            // チケットキーを取得してリダイレクト
            if let Ok(Some(t)) = ticket_repo::find_by_id(&state.pool, ticket_id).await {
                Redirect::to(&format!("/tickets/{}/", t.ticket_key)).into_response()
            } else {
                Redirect::to("/tickets").into_response()
            }
        }
        Err(e) => Html(format!("<script>alert('作成エラー: {}');history.back();</script>", e)).into_response(),
    }
}

pub async fn detail(
    State(state): State<AppState>,
    Extension(_user): Extension<SessionUser>,
    Path(key): Path<String>,
) -> Html<String> {
    let ticket = ticket_repo::find_by_key(&state.pool, &key).await;
    match ticket {
        Ok(Some(t)) => Html(format!("<h1>{} - {}</h1>", t.ticket_key, t.title)),
        _ => Html("<h1>チケットが見つかりません</h1>".to_string()),
    }
}

pub async fn edit_page(
    State(_state): State<AppState>,
    Path(_id): Path<i32>,
) -> Html<&'static str> {
    Html("<h1>チケット編集</h1><form>TODO</form>")
}

pub async fn edit_submit(
    State(state): State<AppState>,
    Extension(user): Extension<SessionUser>,
    Path(id): Path<i32>,
    Form(form): Form<TicketForm>,
) -> impl IntoResponse {
    let req = ticket_service::UpdateTicketRequest {
        title: form.title,
        description: form.description.unwrap_or_default(),
        ticket_type: form.ticket_type,
        priority: form.priority,
        category_id: form.category_id,
        assignee_id: form.assignee_id,
        milestone_id: form.milestone_id,
        start_date: parse_date(form.start_date.as_deref()),
        due_date: parse_date(form.due_date.as_deref()),
    };
    let _ = ticket_service::update_ticket(&state.pool, id, &req).await;
    Redirect::to(&format!("/tickets/{}", id))
}

#[derive(Deserialize)]
pub struct StatusForm {
    pub status: String,
}

pub async fn update_status(
    State(state): State<AppState>,
    Extension(user): Extension<SessionUser>,
    Path(id): Path<i32>,
    Form(form): Form<StatusForm>,
) -> impl IntoResponse {
    let _ = ticket_service::change_status(&state.pool, id, &form.status, user.user_id).await;
    Redirect::to(&format!("/tickets/{}", id))
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Redirect {
    let _ = ticket_service::delete_ticket(&state.pool, id).await;
    Redirect::to("/tickets")
}

pub async fn toggle_watch(
    State(state): State<AppState>,
    Extension(user): Extension<SessionUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    let _ = ticket_service::toggle_watch(&state.pool, id, user.user_id).await;
    Redirect::to(&format!("/tickets/{}", id))
}

#[derive(Deserialize)]
pub struct CommentForm {
    pub body: String,
    /// コメント送信と同時にステータスを変更（オプション）
    pub status: Option<String>,
}

pub async fn add_comment(
    State(state): State<AppState>,
    Extension(user): Extension<SessionUser>,
    Path(id): Path<i32>,
    Form(form): Form<CommentForm>,
) -> Redirect {
    let _ = comment_repo::create(&state.pool, id, user.user_id, &form.body).await;
    // ステータス変更が指定されていれば同時に実行
    if let Some(ref status) = form.status {
        if !status.is_empty() {
            let _ = ticket_service::change_status(&state.pool, id, status, user.user_id).await;
        }
    }
    Redirect::to(&format!("/tickets/{}", id))
}

pub async fn delete_comment(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    let _ = comment_repo::delete(&state.pool, id).await;
    Redirect::to("/tickets")
}
