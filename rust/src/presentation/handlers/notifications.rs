/// presentation/handlers/notifications.rs — 通知ハンドラ
///
/// 通知一覧・既読化・全既読・ドロップダウン・未読数・通知設定

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
use crate::presentation::handlers::tickets::build_base_context;
use crate::domain::models::project::Project;
use crate::domain::models::notification::Notification;
use crate::infrastructure::repositories::{notification_repo, user_repo};

#[derive(Template)]
#[template(path = "notifications/list.html")]
struct NotificationListTemplate {
    all_projects: Vec<Project>,
    current_project_id: Option<i32>,
    nav_active: &'static str,
    overdue_count: i64,
    user_is_staff: bool,
    user_initial: String,
    user_display_name: String,
    notifications: Vec<Notification>,
    filter: String,
    unread_count: i64,
    email_notifications_enabled: bool,
}

#[derive(Template)]
#[template(path = "notifications/partials/_dropdown_body.html")]
struct DropdownBodyTemplate {
    notifications: Vec<Notification>,
}

#[derive(Deserialize, Default)]
pub struct NotificationListQuery {
    pub filter: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<SessionUser>,
    Query(query): Query<NotificationListQuery>,
) -> Html<String> {
    let filter = query.filter.unwrap_or_else(|| "all".to_string());

    let all_notifications = notification_repo::find_by_user(&state.pool, user.user_id, 100)
        .await.unwrap_or_default();
    let unread_count = all_notifications.iter().filter(|n| !n.is_read).count() as i64;

    let notifications = match filter.as_str() {
        "unread" => all_notifications.into_iter().filter(|n| !n.is_read).collect(),
        "read" => all_notifications.into_iter().filter(|n| n.is_read).collect(),
        _ => all_notifications,
    };

    let email_notifications_enabled = user_repo::find_by_id(&state.pool, user.user_id)
        .await.ok().flatten()
        .map(|u| u.email_notifications_enabled)
        .unwrap_or(false);

    let base = build_base_context(&state, &user).await;

    let tpl = NotificationListTemplate {
        all_projects: base.all_projects,
        current_project_id: base.current_project_id,
        nav_active: "notifications",
        overdue_count: base.overdue_count,
        user_is_staff: base.user_is_staff,
        user_initial: base.user_initial,
        user_display_name: base.user_display_name,
        notifications,
        filter,
        unread_count,
        email_notifications_enabled,
    };
    Html(tpl.render().unwrap_or_default())
}

pub async fn mark_read(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    if let Ok(Some(n)) = notification_repo::find_by_id(&state.pool, id).await {
        let _ = notification_repo::mark_read(&state.pool, id).await;
        if let Some(ticket_key) = n.ticket_key {
            return Redirect::to(&format!("/tickets/{}/", ticket_key)).into_response();
        }
    }
    Redirect::to("/notifications").into_response()
}

/// 全通知を既読にする
pub async fn mark_all_read(
    State(state): State<AppState>,
    Extension(user): Extension<SessionUser>,
) -> Redirect {
    let _ = notification_repo::mark_all_read(&state.pool, user.user_id).await;
    Redirect::to("/notifications")
}

pub async fn unread_count(
    State(state): State<AppState>,
    Extension(user): Extension<SessionUser>,
) -> Html<String> {
    let count = notification_repo::count_unread(&state.pool, user.user_id)
        .await.unwrap_or(0);
    Html(count.to_string())
}

pub async fn dropdown(
    State(state): State<AppState>,
    Extension(user): Extension<SessionUser>,
) -> Html<String> {
    let notifications = notification_repo::find_by_user(&state.pool, user.user_id, 10)
        .await.unwrap_or_default();
    let tpl = DropdownBodyTemplate { notifications };
    Html(tpl.render().unwrap_or_default())
}

/// メール通知設定の更新
#[derive(Deserialize)]
pub struct NotificationSettingsForm {
    pub email_enabled: Option<String>,
}

pub async fn update_settings(
    State(state): State<AppState>,
    Extension(user): Extension<SessionUser>,
    Form(form): Form<NotificationSettingsForm>,
) -> Redirect {
    let enabled = form.email_enabled.as_deref() == Some("on");
    let _ = notification_repo::update_email_setting(&state.pool, user.user_id, enabled).await;
    Redirect::to("/notifications")
}
