/// presentation/handlers/project_activity_api.rs — プロジェクト Activity / 進捗報告 API
///
/// - GET    /api/v1/projects/{id}/activity/                 変更履歴(監査ログ)
/// - GET    /api/v1/projects/{id}/updates/                  進捗報告の一覧
/// - POST   /api/v1/projects/{id}/updates/                  進捗報告の投稿
/// - PUT    /api/v1/projects/{id}/updates/{update_id}/      編集(投稿者・プロジェクトのオーナー・管理者)
/// - DELETE /api/v1/projects/{id}/updates/{update_id}/      削除(同上)
///
/// 設計: docs/design/詳細設計書_プロジェクト詳細タブ.md
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use serde_json::json;

use crate::domain::access::{Action, Viewer};
use crate::domain::models::project_activity_api::{
    PageQuery, ProjectUpdateIn, HEALTH_VALUES, UPDATE_BODY_MAX_CHARS,
};
use crate::infrastructure::repositories::{project_activity_repo, resource_repo, user_repo};
use crate::presentation::extractors::authorize;
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::presentation::state::AppState;

#[derive(Serialize)]
pub struct ErrorResponse {
    pub detail: String,
}

fn error(status: StatusCode, detail: &str) -> Response {
    (
        status,
        Json(ErrorResponse {
            detail: detail.to_string(),
        }),
    )
        .into_response()
}

fn server_error(e: anyhow::Error) -> Response {
    tracing::error!("DB operation failed: {:?}", e);
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "サーバーエラーが発生しました",
    )
}

/// プロジェクトが存在するか(無ければ 404 レスポンスを返す)。
async fn require_project(
    state: &AppState,
    project_id: i32,
    viewer_user_id: i32,
) -> Result<crate::domain::models::resource_api::ProjectOut, Response> {
    match resource_repo::find_project_by_id(&state.pool, project_id, Some(viewer_user_id)).await {
        Ok(Some(p)) => Ok(p),
        Ok(None) => Err(error(StatusCode::NOT_FOUND, "見つかりません")),
        Err(e) => Err(server_error(e)),
    }
}

/// 閲覧者の確認と、プロジェクトの閲覧の判定(D-3)。今の判定は「存在すること」だけ
async fn require_readable_project(
    state: &AppState,
    viewer: &Viewer,
    project_id: i32,
    route: &'static str,
) -> Result<(AuthUser, crate::domain::models::resource_api::ProjectOut), Response> {
    let auth = AuthUser {
        user_id: viewer.require_user_id()?,
    };
    let found = require_project(state, project_id, auth.user_id).await;
    let legacy = match &found {
        Ok(_) => Ok(()),
        Err(_) => Err(error(StatusCode::NOT_FOUND, "見つかりません")),
    };
    authorize::gate_project(&state.pool, viewer, project_id, Action::Read, legacy, route).await?;
    // `on` で新しい判定が許しても、存在しなければ今の応答(404・500)を返す
    found.map(|p| (auth, p))
}

async fn is_staff(state: &AppState, user_id: i32) -> Result<bool, Response> {
    match user_repo::find_by_id(&state.pool, user_id).await {
        Ok(Some(user)) => Ok(user.is_staff),
        Ok(None) => Err(error(StatusCode::UNAUTHORIZED, "ユーザーが見つかりません")),
        Err(e) => Err(server_error(e)),
    }
}

/// 入力の検証。問題があれば 400 レスポンスを返す。
fn validate_update(body: &ProjectUpdateIn) -> Result<(String, String), Response> {
    let health = body.health.trim().to_string();
    if !HEALTH_VALUES.contains(&health.as_str()) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            &format!("health must be one of: {}", HEALTH_VALUES.join(", ")),
        ));
    }
    let text = body.body.trim().to_string();
    if text.chars().count() > UPDATE_BODY_MAX_CHARS {
        return Err(error(
            StatusCode::BAD_REQUEST,
            &format!("本文は{UPDATE_BODY_MAX_CHARS}文字以内で入力してください"),
        ));
    }
    Ok((health, text))
}

/// 変更履歴 GET /api/v1/projects/{id}/activity/
pub async fn activity_list(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
    Query(q): Query<PageQuery>,
) -> Response {
    if let Err(resp) =
        require_readable_project(&state, &viewer, id, "GET /api/v1/projects/{id}/activity/").await
    {
        return resp;
    }
    match project_activity_repo::list_activity(&state.pool, id, q.limit, q.before).await {
        Ok(page) => (StatusCode::OK, Json(page)).into_response(),
        Err(e) => server_error(e),
    }
}

/// 進捗報告の一覧 GET /api/v1/projects/{id}/updates/
pub async fn updates_list(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
    Query(q): Query<PageQuery>,
) -> Response {
    if let Err(resp) =
        require_readable_project(&state, &viewer, id, "GET /api/v1/projects/{id}/updates/").await
    {
        return resp;
    }
    if let Some(ref h) = q.health {
        if !HEALTH_VALUES.contains(&h.as_str()) {
            return error(
                StatusCode::BAD_REQUEST,
                &format!("health must be one of: {}", HEALTH_VALUES.join(", ")),
            );
        }
    }
    match project_activity_repo::list_updates(
        &state.pool,
        id,
        q.limit,
        q.before,
        q.health.as_deref(),
    )
    .await
    {
        Ok(page) => (StatusCode::OK, Json(page)).into_response(),
        Err(e) => server_error(e),
    }
}

/// 進捗報告の投稿 POST /api/v1/projects/{id}/updates/
/// プロジェクトのメンバー(または管理者)のみ投稿できる。
pub async fn update_create(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
    Json(body): Json<ProjectUpdateIn>,
) -> Response {
    const ROUTE: &str = "POST /api/v1/projects/{id}/updates/";
    let (auth, project) = match require_readable_project(&state, &viewer, id, ROUTE).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let staff = match is_staff(&state, auth.user_id).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let legacy = if staff || project.is_member {
        Ok(())
    } else {
        Err(error(
            StatusCode::FORBIDDEN,
            "プロジェクトのメンバーのみ進捗を投稿できます",
        ))
    };
    // 新しい判定(D-3): プロジェクトに書き込めること(Full Member で、参加チームが見える)
    if let Err(resp) =
        authorize::gate_project(&state.pool, &viewer, id, Action::Write, legacy, ROUTE).await
    {
        return resp;
    }
    let (health, text) = match validate_update(&body) {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    match project_activity_repo::create_update(&state.pool, id, auth.user_id, &health, &text).await
    {
        Ok(created) => {
            // Activity には「投稿があった」ことと参照(update_id)だけを残す。本文は持たない。
            project_activity_repo::record_best_effort(
                &state.pool,
                id,
                Some(auth.user_id),
                "update_posted",
                json!({"update_id": created.id, "health": created.health}),
            )
            .await;
            (StatusCode::CREATED, Json(created)).into_response()
        }
        Err(e) => server_error(e),
    }
}

/// 編集・削除の権限(投稿者・プロジェクトのオーナー・管理者)を確認する。
///
/// 新しい判定(D-3): 投稿者はプロジェクトが見えること、それ以外はプロジェクトの設定を管理できること
/// (プロジェクトのオーナーは権限を持たない。設計書の決定)。
async fn require_edit_permission(
    state: &AppState,
    viewer: &Viewer,
    auth: &AuthUser,
    project_id: i32,
    update_id: i64,
    route: &'static str,
) -> Result<(), Response> {
    let existing =
        match project_activity_repo::find_update(&state.pool, project_id, update_id).await {
            Ok(Some(u)) => u,
            Ok(None) => return Err(error(StatusCode::NOT_FOUND, "見つかりません")),
            Err(e) => return Err(server_error(e)),
        };
    let is_author = existing.author_id == Some(auth.user_id as i64);
    let legacy = if is_author || is_staff(state, auth.user_id).await? {
        Ok(())
    } else {
        match resource_repo::get_project_owner_id(&state.pool, project_id).await {
            Ok(Some(owner_id)) if owner_id == auth.user_id => Ok(()),
            Ok(_) => Err(error(
                StatusCode::FORBIDDEN,
                "投稿者・プロジェクトのオーナー・管理者のみ編集できます",
            )),
            Err(e) => return Err(server_error(e)),
        }
    };
    let action = if is_author {
        Action::Read
    } else {
        Action::ManageSettings
    };
    authorize::gate_project(&state.pool, viewer, project_id, action, legacy, route).await
}

/// 進捗報告の編集 PUT /api/v1/projects/{id}/updates/{update_id}/
pub async fn update_edit(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, update_id)): Path<(i32, i64)>,
    Json(body): Json<ProjectUpdateIn>,
) -> Response {
    const ROUTE: &str = "PUT /api/v1/projects/{id}/updates/{update_id}/";
    let auth = match require_readable_project(&state, &viewer, id, ROUTE).await {
        Ok((auth, _)) => auth,
        Err(resp) => return resp,
    };
    if let Err(resp) = require_edit_permission(&state, &viewer, &auth, id, update_id, ROUTE).await {
        return resp;
    }
    let (health, text) = match validate_update(&body) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    match project_activity_repo::edit_update(&state.pool, id, update_id, &health, &text).await {
        Ok(Some(updated)) => (StatusCode::OK, Json(updated)).into_response(),
        Ok(None) => error(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => server_error(e),
    }
}

/// 進捗報告の削除 DELETE /api/v1/projects/{id}/updates/{update_id}/
pub async fn update_delete(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, update_id)): Path<(i32, i64)>,
) -> Response {
    const ROUTE: &str = "DELETE /api/v1/projects/{id}/updates/{update_id}/";
    let auth = match require_readable_project(&state, &viewer, id, ROUTE).await {
        Ok((auth, _)) => auth,
        Err(resp) => return resp,
    };
    if let Err(resp) = require_edit_permission(&state, &viewer, &auth, id, update_id, ROUTE).await {
        return resp;
    }
    match project_activity_repo::delete_update(&state.pool, id, update_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => error(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => server_error(e),
    }
}
