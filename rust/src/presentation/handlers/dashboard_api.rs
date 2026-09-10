/// presentation/handlers/dashboard_api.rs — ダッシュボード JSON API
///
/// Django /api/v1/dashboard/* と挙動を一致させるハンドラー。

use axum::{
    extract::{State, Path, Query},
    response::IntoResponse,
    http::StatusCode,
    Json,
    Extension,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::presentation::state::AppState;
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::infrastructure::repositories::dashboard_api_repo as repo;

fn err(detail: &str) -> Value {
    json!({"error": detail})
}

async fn resolve_staff(state: &AppState, user_id: i32) -> Result<bool, impl IntoResponse> {
    repo::is_staff(&state.pool, user_id).await.map_err(|e| {
        tracing::error!("DB operation failed: {:?}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
    })
}

#[derive(Deserialize)]
pub struct ProjectQuery {
    pub project: Option<i32>,
}

#[derive(Deserialize)]
pub struct LimitQuery {
    pub limit: Option<i64>,
}

/// GET /api/v1/dashboard/stats/
pub async fn stats(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Query(params): Query<ProjectQuery>,
) -> impl IntoResponse {
    let staff = match resolve_staff(&state, auth.user_id).await {
        Ok(s) => s,
        Err(resp) => return resp.into_response(),
    };
    match repo::get_dashboard_stats(&state.pool, auth.user_id, params.project, staff).await {
        Ok(data) => (StatusCode::OK, Json(data)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

/// GET /api/v1/dashboard/my-tickets/
pub async fn my_tickets(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Query(params): Query<LimitQuery>,
) -> impl IntoResponse {
    let limit = params.limit.unwrap_or(10).min(50);
    match repo::get_my_tickets(&state.pool, auth.user_id, limit).await {
        Ok(data) => (StatusCode::OK, Json(data)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

/// GET /api/v1/dashboard/activity/
pub async fn activity(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Query(params): Query<LimitQuery>,
) -> impl IntoResponse {
    let staff = match resolve_staff(&state, auth.user_id).await {
        Ok(s) => s,
        Err(resp) => return resp.into_response(),
    };
    let limit = params.limit.unwrap_or(15).min(50);
    match repo::get_recent_activity(&state.pool, auth.user_id, limit, staff).await {
        Ok(data) => (StatusCode::OK, Json(data)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

/// GET /api/v1/dashboard/default/
pub async fn default_dashboard(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
) -> impl IntoResponse {
    let staff = match resolve_staff(&state, auth.user_id).await {
        Ok(s) => s,
        Err(resp) => return resp.into_response(),
    };
    let dashboard_id = match repo::get_or_create_default_dashboard(&state.pool, auth.user_id).await {
        Ok(id) => id,
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };
    match repo::serialize_dashboard(&state.pool, dashboard_id, auth.user_id, staff).await {
        Ok(data) => (StatusCode::OK, Json(data)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

/// GET /api/v1/dashboard/list/
pub async fn list_dashboards(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
) -> impl IntoResponse {
    match repo::list_dashboards(&state.pool, auth.user_id).await {
        Ok(data) => (StatusCode::OK, Json(data)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

/// GET /api/v1/dashboard/detail/{id}/
pub async fn detail_dashboard(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(dashboard_id): Path<i32>,
) -> impl IntoResponse {
    let staff = match resolve_staff(&state, auth.user_id).await {
        Ok(s) => s,
        Err(resp) => return resp.into_response(),
    };
    match repo::find_dashboard_owned(&state.pool, dashboard_id, auth.user_id).await {
        Ok(true) => {}
        Ok(false) => return (StatusCode::NOT_FOUND, Json(err("Not found"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    }
    match repo::serialize_dashboard(&state.pool, dashboard_id, auth.user_id, staff).await {
        Ok(data) => (StatusCode::OK, Json(data)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct CreateDashboardIn {
    #[serde(default = "default_dashboard_name")]
    pub name: String,
    #[serde(default = "default_layout")]
    pub layout: String,
    #[serde(default = "default_true")]
    pub with_defaults: bool,
}
fn default_dashboard_name() -> String { "New Dashboard".to_string() }
fn default_layout() -> String { "grid-2col".to_string() }
fn default_true() -> bool { true }

/// POST /api/v1/dashboard/create/
pub async fn create_dashboard(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<CreateDashboardIn>,
) -> impl IntoResponse {
    match repo::create_dashboard(&state.pool, auth.user_id, &body.name, &body.layout, body.with_defaults).await {
        Ok((id, name)) => (StatusCode::CREATED, Json(json!({"id": id, "name": name}))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct UpdateDashboardIn {
    #[serde(rename = "dashboardId")]
    pub dashboard_id: i32,
    pub name: Option<String>,
    pub layout: Option<String>,
}

/// PATCH /api/v1/dashboard/update/
pub async fn update_dashboard(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<UpdateDashboardIn>,
) -> impl IntoResponse {
    match repo::update_dashboard(&state.pool, body.dashboard_id, auth.user_id, body.name.as_deref(), body.layout.as_deref()).await {
        Ok(Some((name, layout))) => (
            StatusCode::OK,
            Json(json!({"id": body.dashboard_id, "name": name, "layout": layout})),
        )
            .into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, Json(err("Not found"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct DeleteDashboardIn {
    #[serde(rename = "dashboardId")]
    pub dashboard_id: i32,
}

/// DELETE /api/v1/dashboard/delete/
pub async fn delete_dashboard(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<DeleteDashboardIn>,
) -> impl IntoResponse {
    use repo::DeleteDashboardResult;
    match repo::delete_dashboard(&state.pool, body.dashboard_id, auth.user_id).await {
        Ok(DeleteDashboardResult::Deleted) => StatusCode::NO_CONTENT.into_response(),
        Ok(DeleteDashboardResult::NotFound) => (StatusCode::NOT_FOUND, Json(err("Not found"))).into_response(),
        Ok(DeleteDashboardResult::IsDefault) => (
            StatusCode::BAD_REQUEST,
            Json(err("Cannot delete default dashboard")),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct AddWidgetIn {
    #[serde(rename = "dashboardId")]
    pub dashboard_id: Option<i32>,
    #[serde(rename = "widgetType", default)]
    pub widget_type: String,
    #[serde(default = "default_span")]
    pub span: i32,
    #[serde(default = "default_config")]
    pub config: Value,
}
fn default_span() -> i32 { 1 }
fn default_config() -> Value { json!({}) }

/// POST /api/v1/dashboard/add-widget/
pub async fn add_widget(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<AddWidgetIn>,
) -> impl IntoResponse {
    use repo::AddWidgetResult;
    match repo::add_widget(&state.pool, body.dashboard_id, auth.user_id, &body.widget_type, body.span, &body.config).await {
        Ok(AddWidgetResult::Success { id, widget_type, position }) => (
            StatusCode::CREATED,
            Json(json!({"id": id, "widgetType": widget_type, "position": position})),
        )
            .into_response(),
        Ok(AddWidgetResult::DashboardNotFound) => (StatusCode::NOT_FOUND, Json(err("Not found"))).into_response(),
        Ok(AddWidgetResult::UnknownWidgetType) => (
            StatusCode::BAD_REQUEST,
            Json(err(&format!("Unknown widget type: {}", body.widget_type))),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct RemoveWidgetIn {
    #[serde(rename = "widgetId")]
    pub widget_id: Option<i32>,
}

/// DELETE /api/v1/dashboard/remove-widget/
pub async fn remove_widget(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<RemoveWidgetIn>,
) -> impl IntoResponse {
    let widget_id = match body.widget_id {
        Some(id) => id,
        None => return (StatusCode::BAD_REQUEST, Json(err("widgetId required"))).into_response(),
    };
    match repo::remove_widget(&state.pool, widget_id, auth.user_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, Json(err("Not found"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

/// GET /api/v1/dashboard/team/{team_slug}/summary/
pub async fn team_summary(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(team_slug): Path<String>,
) -> impl IntoResponse {
    match repo::get_team_summary(&state.pool, auth.user_id, &team_slug).await {
        Ok(Some(data)) => (StatusCode::OK, Json(data)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, Json(err("Team not found"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct ReorderWidgetsIn {
    #[serde(rename = "widgetOrder", default)]
    pub widget_order: Vec<i32>,
}

/// POST /api/v1/dashboard/reorder-widgets/
pub async fn reorder_widgets(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<ReorderWidgetsIn>,
) -> impl IntoResponse {
    if body.widget_order.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(err("widgetOrder required (list of IDs)")),
        )
            .into_response();
    }
    match repo::reorder_widgets(&state.pool, &body.widget_order, auth.user_id).await {
        Ok(()) => (StatusCode::OK, Json(json!({"ok": true}))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}
