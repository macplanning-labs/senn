/// presentation/handlers/notifications.rs — 通知ハンドラ
///
/// 通知一覧・既読化・全既読・ドロップダウン・未読数・通知設定

use axum::{
    extract::{State, Path},
    response::{Html, Redirect, IntoResponse},
    Extension, Form,
};
use serde::Deserialize;
use crate::presentation::state::AppState;
use crate::presentation::middleware::auth::SessionUser;
use crate::infrastructure::repositories::notification_repo;

pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<SessionUser>,
) -> Html<String> {
    let notifications = notification_repo::find_by_user(&state.pool, user.user_id, 100)
        .await.unwrap_or_default();
    Html(format!("<h1>通知一覧</h1><p>{}件</p>", notifications.len()))
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
    if count > 0 {
        Html(format!("<span class=\"badge\">{}</span>", count))
    } else {
        Html(String::new())
    }
}

pub async fn dropdown(
    State(state): State<AppState>,
    Extension(user): Extension<SessionUser>,
) -> Html<String> {
    let notifications = notification_repo::find_by_user(&state.pool, user.user_id, 10)
        .await.unwrap_or_default();
    // TODO: Askamaテンプレートに差し替え
    Html(format!("<div class='dropdown'>{}件の通知</div>", notifications.len()))
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
