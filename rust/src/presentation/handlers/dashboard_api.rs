/// presentation/handlers/dashboard_api.rs — ダッシュボード JSON API
///
/// Django /api/v1/dashboard/* と挙動を一致させるハンドラー。
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::domain::access::Viewer;
use crate::infrastructure::access::shadow::{self, Mode, Resource};
use crate::infrastructure::repositories::dashboard_api_repo as repo;
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::presentation::state::AppState;

fn err(detail: &str) -> Value {
    json!({"error": detail})
}

/// 閲覧者と、集計の見える範囲(アクセス制御の再設計 D-5。設計書 §5.2「集計の中身は、閲覧者が見える
/// データだけで計算する」)。切り替えはチケットと同じ `ACCESS_ENFORCE_TICKET`。
/// ダッシュボードそのものは作成者本人の物(リポジトリ関数が user_id で絞る。今のまま)。
#[allow(clippy::result_large_err)] // 認可の失敗は、そのまま応答として返す
async fn access_of(
    state: &AppState,
    viewer: &Viewer,
) -> Result<(AuthUser, repo::DashAccess), axum::response::Response> {
    let auth = AuthUser {
        user_id: viewer.require_user_id()?,
    };
    if shadow::mode(Resource::Ticket) == Mode::On {
        return Ok((auth, repo::DashAccess::Scope(viewer.scope())));
    }
    let staff = repo::is_staff(&state.pool, auth.user_id)
        .await
        .map_err(|e| {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        })?;
    let user_id = auth.user_id;
    Ok((auth, repo::DashAccess::Legacy { user_id, staff }))
}

/// 新しい判定(on)のときの見える範囲(本人の物を返す API の、追加の絞り込み用)
fn scope_if_on(viewer: &Viewer) -> Option<crate::domain::access::Scope> {
    (shadow::mode(Resource::Ticket) == Mode::On).then(|| viewer.scope())
}

#[allow(clippy::result_large_err)]
fn auth_of(viewer: &Viewer) -> Result<AuthUser, axum::response::Response> {
    Ok(AuthUser {
        user_id: viewer.require_user_id()?,
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
    viewer: Viewer,
    Query(params): Query<ProjectQuery>,
) -> impl IntoResponse {
    let (_auth, access) = match access_of(&state, &viewer).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    match repo::get_dashboard_stats(&state.pool, &access, params.project).await {
        Ok(data) => (StatusCode::OK, Json(data)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
    }
}

/// GET /api/v1/dashboard/my-tickets/
pub async fn my_tickets(
    State(state): State<AppState>,
    viewer: Viewer,
    Query(params): Query<LimitQuery>,
) -> impl IntoResponse {
    let auth = match auth_of(&viewer) {
        Ok(a) => a,
        Err(resp) => return resp,
    };
    let limit = params.limit.unwrap_or(10).min(50);
    match repo::get_my_tickets(
        &state.pool,
        auth.user_id,
        limit,
        scope_if_on(&viewer).as_ref(),
    )
    .await
    {
        Ok(data) => (StatusCode::OK, Json(data)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
    }
}

/// GET /api/v1/dashboard/activity/
pub async fn activity(
    State(state): State<AppState>,
    viewer: Viewer,
    Query(params): Query<LimitQuery>,
) -> impl IntoResponse {
    let (_auth, access) = match access_of(&state, &viewer).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let limit = params.limit.unwrap_or(15).min(50);
    match repo::get_recent_activity(&state.pool, &access, limit).await {
        Ok(data) => (StatusCode::OK, Json(data)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
    }
}

/// GET /api/v1/dashboard/default/
pub async fn default_dashboard(State(state): State<AppState>, viewer: Viewer) -> impl IntoResponse {
    let (auth, access) = match access_of(&state, &viewer).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let dashboard_id = match repo::get_or_create_default_dashboard(&state.pool, auth.user_id).await
    {
        Ok(id) => id,
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
    };
    match repo::serialize_dashboard(&state.pool, dashboard_id, auth.user_id, &access).await {
        Ok(data) => (StatusCode::OK, Json(data)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
    }
}

/// GET /api/v1/dashboard/list/
pub async fn list_dashboards(State(state): State<AppState>, viewer: Viewer) -> impl IntoResponse {
    let auth = match auth_of(&viewer) {
        Ok(a) => a,
        Err(resp) => return resp,
    };
    match repo::list_dashboards(&state.pool, auth.user_id).await {
        Ok(data) => (StatusCode::OK, Json(data)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
    }
}

/// GET /api/v1/dashboard/detail/{id}/
pub async fn detail_dashboard(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(dashboard_id): Path<i32>,
) -> impl IntoResponse {
    let (auth, access) = match access_of(&state, &viewer).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    match repo::find_dashboard_owned(&state.pool, dashboard_id, auth.user_id).await {
        Ok(true) => {}
        Ok(false) => return (StatusCode::NOT_FOUND, Json(err("Not found"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
    }
    match repo::serialize_dashboard(&state.pool, dashboard_id, auth.user_id, &access).await {
        Ok(data) => (StatusCode::OK, Json(data)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
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
fn default_dashboard_name() -> String {
    "New Dashboard".to_string()
}
fn default_layout() -> String {
    "grid-2col".to_string()
}
fn default_true() -> bool {
    true
}

/// POST /api/v1/dashboard/create/
pub async fn create_dashboard(
    State(state): State<AppState>,
    viewer: Viewer,
    Json(body): Json<CreateDashboardIn>,
) -> impl IntoResponse {
    let auth = match auth_of(&viewer) {
        Ok(a) => a,
        Err(resp) => return resp,
    };
    match repo::create_dashboard(
        &state.pool,
        auth.user_id,
        &body.name,
        &body.layout,
        body.with_defaults,
    )
    .await
    {
        Ok((id, name)) => {
            (StatusCode::CREATED, Json(json!({"id": id, "name": name}))).into_response()
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
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
    viewer: Viewer,
    Json(body): Json<UpdateDashboardIn>,
) -> impl IntoResponse {
    let auth = match auth_of(&viewer) {
        Ok(a) => a,
        Err(resp) => return resp,
    };
    match repo::update_dashboard(
        &state.pool,
        body.dashboard_id,
        auth.user_id,
        body.name.as_deref(),
        body.layout.as_deref(),
    )
    .await
    {
        Ok(Some((name, layout))) => (
            StatusCode::OK,
            Json(json!({"id": body.dashboard_id, "name": name, "layout": layout})),
        )
            .into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, Json(err("Not found"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
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
    viewer: Viewer,
    Json(body): Json<DeleteDashboardIn>,
) -> impl IntoResponse {
    let auth = match auth_of(&viewer) {
        Ok(a) => a,
        Err(resp) => return resp,
    };
    use repo::DeleteDashboardResult;
    match repo::delete_dashboard(&state.pool, body.dashboard_id, auth.user_id).await {
        Ok(DeleteDashboardResult::Deleted) => StatusCode::NO_CONTENT.into_response(),
        Ok(DeleteDashboardResult::NotFound) => {
            (StatusCode::NOT_FOUND, Json(err("Not found"))).into_response()
        }
        Ok(DeleteDashboardResult::IsDefault) => (
            StatusCode::BAD_REQUEST,
            Json(err("Cannot delete default dashboard")),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
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
fn default_span() -> i32 {
    1
}
fn default_config() -> Value {
    json!({})
}

/// POST /api/v1/dashboard/add-widget/
pub async fn add_widget(
    State(state): State<AppState>,
    viewer: Viewer,
    Json(body): Json<AddWidgetIn>,
) -> impl IntoResponse {
    let auth = match auth_of(&viewer) {
        Ok(a) => a,
        Err(resp) => return resp,
    };
    use repo::AddWidgetResult;
    match repo::add_widget(
        &state.pool,
        body.dashboard_id,
        auth.user_id,
        &body.widget_type,
        body.span,
        &body.config,
    )
    .await
    {
        Ok(AddWidgetResult::Success {
            id,
            widget_type,
            position,
        }) => (
            StatusCode::CREATED,
            Json(json!({"id": id, "widgetType": widget_type, "position": position})),
        )
            .into_response(),
        Ok(AddWidgetResult::DashboardNotFound) => {
            (StatusCode::NOT_FOUND, Json(err("Not found"))).into_response()
        }
        Ok(AddWidgetResult::UnknownWidgetType) => (
            StatusCode::BAD_REQUEST,
            Json(err(&format!("Unknown widget type: {}", body.widget_type))),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
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
    viewer: Viewer,
    Json(body): Json<RemoveWidgetIn>,
) -> impl IntoResponse {
    let auth = match auth_of(&viewer) {
        Ok(a) => a,
        Err(resp) => return resp,
    };
    let widget_id = match body.widget_id {
        Some(id) => id,
        None => return (StatusCode::BAD_REQUEST, Json(err("widgetId required"))).into_response(),
    };
    match repo::remove_widget(&state.pool, widget_id, auth.user_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, Json(err("Not found"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
    }
}

/// GET /api/v1/dashboard/team/{team_slug}/summary/
pub async fn team_summary(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(team_slug): Path<String>,
) -> impl IntoResponse {
    let auth = match auth_of(&viewer) {
        Ok(a) => a,
        Err(resp) => return resp,
    };
    match repo::get_team_summary(
        &state.pool,
        auth.user_id,
        &team_slug,
        scope_if_on(&viewer).as_ref(),
    )
    .await
    {
        Ok(Some(data)) => (StatusCode::OK, Json(data)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, Json(err("Team not found"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
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
    viewer: Viewer,
    Json(body): Json<ReorderWidgetsIn>,
) -> impl IntoResponse {
    let auth = match auth_of(&viewer) {
        Ok(a) => a,
        Err(resp) => return resp,
    };
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
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
    }
}
