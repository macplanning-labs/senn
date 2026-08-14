/// presentation/handlers/tickets.rs — チケット CRUD ハンドラ

use askama::Template;
use axum::{
    extract::{State, Path, Query},
    response::{Html, Redirect, IntoResponse},
    Extension, Form,
};
use serde::Deserialize;
use crate::presentation::state::AppState;
use crate::presentation::middleware::auth::SessionUser;
use crate::presentation::filters;
use crate::domain::models::project::Project;
use crate::domain::models::ticket::{Ticket, TicketStatus, Priority};
use crate::domain::models::user::User;
use crate::domain::models::milestone::Milestone;
use crate::domain::models::comment::Comment;
use crate::domain::services::{ticket_service, dashboard_service};
use crate::infrastructure::repositories::{ticket_repo, comment_repo, project_repo, user_repo, milestone_repo};

/// base.html が要求する共通コンテキスト（サイドバー・ヘッダー用）
struct BaseContext {
    all_projects: Vec<Project>,
    current_project_id: Option<i32>,
    overdue_count: i64,
    user_is_staff: bool,
    user_initial: String,
    user_display_name: String,
}

async fn build_base_context(state: &AppState, user: &SessionUser) -> BaseContext {
    let all_projects = project_repo::find_all(&state.pool).await.unwrap_or_default();
    let overdue_count = dashboard_service::get_overdue_count(&state.pool, user.current_project_id)
        .await
        .unwrap_or(0);
    let user_initial = user
        .display_name
        .chars()
        .next()
        .map(|c| c.to_string())
        .unwrap_or_default();
    BaseContext {
        all_projects,
        current_project_id: user.current_project_id,
        overdue_count,
        user_is_staff: user.is_staff,
        user_initial,
        user_display_name: user.display_name.clone(),
    }
}

fn status_labels() -> Vec<String> {
    TicketStatus::all().iter().map(|s| s.label().to_string()).collect()
}

#[derive(Deserialize, Default)]
pub struct TicketListQuery {
    pub status: Option<String>,
    pub priority: Option<String>,
    pub assignee_id: Option<i32>,
    pub category_id: Option<i32>,
    pub milestone_id: Option<i32>,
    pub keyword: Option<String>,
}

#[derive(Template)]
#[template(path = "tickets/list.html")]
struct TicketListTemplate {
    all_projects: Vec<Project>,
    current_project_id: Option<i32>,
    nav_active: &'static str,
    overdue_count: i64,
    user_is_staff: bool,
    user_initial: String,
    user_display_name: String,
    tickets: Vec<Ticket>,
    f_keyword: String,
    f_status: String,
    f_priority: String,
    f_assignee_id: Option<i32>,
    f_milestone_id: Option<i32>,
    statuses: Vec<String>,
    priorities: Vec<String>,
    users: Vec<User>,
    milestones: Vec<Milestone>,
}

pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<SessionUser>,
    Query(query): Query<TicketListQuery>,
) -> Html<String> {
    let filter = ticket_repo::TicketFilter {
        project_id: user.current_project_id,
        status: query.status.clone(),
        priority: query.priority.clone(),
        assignee_id: query.assignee_id,
        category_id: query.category_id,
        milestone_id: query.milestone_id,
        keyword: query.keyword.clone(),
        ..Default::default()
    };

    let tickets = ticket_repo::find_all(&state.pool, &filter)
        .await.unwrap_or_default();

    let users = match user.current_project_id {
        Some(pid) => user_repo::find_project_members(&state.pool, pid).await.unwrap_or_default(),
        None => user_repo::find_all(&state.pool).await.unwrap_or_default(),
    };
    let milestones = milestone_repo::find_all(&state.pool, user.current_project_id)
        .await.unwrap_or_default();

    let base = build_base_context(&state, &user).await;

    let tpl = TicketListTemplate {
        all_projects: base.all_projects,
        current_project_id: base.current_project_id,
        nav_active: "tickets",
        overdue_count: base.overdue_count,
        user_is_staff: base.user_is_staff,
        user_initial: base.user_initial,
        user_display_name: base.user_display_name,
        tickets,
        f_keyword: query.keyword.unwrap_or_default(),
        f_status: query.status.unwrap_or_default(),
        f_priority: query.priority.unwrap_or_default(),
        f_assignee_id: query.assignee_id,
        f_milestone_id: query.milestone_id,
        statuses: status_labels(),
        priorities: Priority::all().iter().map(|p| p.label().to_string()).collect(),
        users,
        milestones,
    };
    match tpl.render() {
        Ok(html) => Html(html),
        Err(e) => Html(format!("<h1>チケット一覧</h1><p>テンプレートエラー: {}</p>", e)),
    }
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

#[derive(Template)]
#[template(path = "tickets/detail.html")]
struct TicketDetailTemplate {
    all_projects: Vec<Project>,
    current_project_id: Option<i32>,
    nav_active: &'static str,
    overdue_count: i64,
    user_is_staff: bool,
    user_initial: String,
    user_display_name: String,
    ticket: Ticket,
    comments: Vec<Comment>,
    available_statuses: Vec<String>,
    is_watching: bool,
}

pub async fn detail(
    State(state): State<AppState>,
    Extension(user): Extension<SessionUser>,
    Path(key): Path<String>,
) -> Html<String> {
    let ticket = ticket_repo::find_by_key(&state.pool, &key).await;
    match ticket {
        Ok(Some(t)) => {
            let comments = comment_repo::find_by_ticket(&state.pool, t.id)
                .await.unwrap_or_default();
            let is_watching = ticket_repo::is_watching(&state.pool, t.id, user.user_id)
                .await.unwrap_or(false);
            let base = build_base_context(&state, &user).await;

            let tpl = TicketDetailTemplate {
                all_projects: base.all_projects,
                current_project_id: base.current_project_id,
                nav_active: "tickets",
                overdue_count: base.overdue_count,
                user_is_staff: base.user_is_staff,
                user_initial: base.user_initial,
                user_display_name: base.user_display_name,
                ticket: t,
                comments,
                available_statuses: status_labels(),
                is_watching,
            };
            match tpl.render() {
                Ok(html) => Html(html),
                Err(e) => Html(format!("<h1>テンプレートエラー</h1><p>{}</p>", e)),
            }
        }
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
