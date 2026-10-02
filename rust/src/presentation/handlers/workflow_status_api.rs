/// presentation/handlers/workflow_status_api.rs — Workflow Status JSON API
///
/// Django /api/v1/workflow-statuses/ と挙動を一致させるハンドラー。
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::domain::access::{Action, ResourceRef, Viewer};
use crate::domain::models::workflow_status_api::*;
use crate::infrastructure::access::{
    facts_repo,
    shadow::{self, Mode, Resource},
};
use crate::infrastructure::repositories::workflow_status_repo;
use crate::presentation::extractors::authorize;
use crate::presentation::state::AppState;

// =============================================================================
// リクエスト構造体
// =============================================================================

#[derive(Deserialize)]
pub struct WorkflowStatusListQuery {
    pub project: Option<i32>,
    pub team: Option<i32>,
    pub page: Option<i64>,
}

// =============================================================================
// レスポンス構造体
// =============================================================================

#[derive(Serialize)]
pub struct PaginatedWorkflowStatusesOut {
    pub count: i64,
    pub next: Option<String>,
    pub previous: Option<String>,
    pub results: Vec<WorkflowStatusOut>,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub detail: String,
}

// アクセス制御の再設計(D-4)。切り替えは `ACCESS_ENFORCE_MASTER`。今の判定には確認が無い。
// 閲覧: チームの状態はチームが見える人、プロジェクトの状態はプロジェクトが見える人、全体の状態は全員
// (Guest は、見えるチケットで使われている物だけ。一覧の SQL の規則)。
// 変更: チーム・プロジェクトの状態はチームの設定を管理できる人、全体の状態はシステム管理者だけ。

fn db_error(e: anyhow::Error) -> axum::response::Response {
    tracing::error!("DB operation failed: {:?}", e);
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            detail: "サーバーエラーが発生しました".to_string(),
        }),
    )
        .into_response()
}

/// 状態 1 件の判定(`team_id`・`project` は判定する状態の所属)
async fn status_gate(
    state: &AppState,
    viewer: &Viewer,
    team_id: Option<i32>,
    project: Option<i32>,
    action: Action,
    id: i32,
    route: &'static str,
) -> Result<(), axum::response::Response> {
    authorize::gate_scoped(
        &state.pool,
        viewer,
        team_id,
        project,
        action,
        Action::ManageSettings,
        Resource::Master,
        id,
        route,
    )
    .await
}

/// 既存の状態の判定(見つからない場合は、今の処理に任せる。`on` では gate_scoped の中で扱わない
/// ため、ここで 404 にする)
async fn existing_status_gate(
    state: &AppState,
    viewer: &Viewer,
    id: i32,
    action: Action,
    route: &'static str,
) -> Result<(), axum::response::Response> {
    match workflow_status_repo::find_workflow_status_by_id(&state.pool, id).await {
        Ok(Some(ws)) => status_gate(state, viewer, ws.team_id, ws.project, action, id, route).await,
        Ok(None) if shadow::mode(Resource::Master) == Mode::On => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "見つかりません".to_string(),
            }),
        )
            .into_response()),
        Ok(None) => Ok(()),
        Err(e) => Err(db_error(e)),
    }
}

// =============================================================================
// Workflow Statuses
// =============================================================================

/// ワークフロー状態一覧 GET /api/v1/workflow-statuses/
pub async fn workflow_status_list(
    State(state): State<AppState>,
    viewer: Viewer,
    Query(params): Query<WorkflowStatusListQuery>,
) -> impl IntoResponse {
    let page = params.page.unwrap_or(1).max(1);
    const PAGE_SIZE: i64 = 50;

    // 新しい判定(on)では SQL で絞る(件数・ページ送りも合わせる)
    let scope = (shadow::mode(Resource::Master) == Mode::On).then(|| viewer.scope());

    // ワークフロー状態一覧取得
    let statuses = match workflow_status_repo::find_all_workflow_statuses(
        &state.pool,
        page,
        params.project,
        params.team,
        scope.as_ref(),
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response();
        }
    };

    // 試運転: 見えなくなる行を記録する(プロジェクトの参加チームは、出てきたプロジェクトの分だけ読む)
    let statuses = if shadow::mode(Resource::Master) == Mode::Shadow {
        let mut projects = std::collections::HashMap::new();
        for pid in statuses
            .iter()
            .filter(|w| w.team_id.is_none())
            .filter_map(|w| w.project)
        {
            if projects.contains_key(&pid) {
                continue;
            }
            match facts_repo::facts_for_project(&state.pool, pid).await {
                Ok(Some(f)) => {
                    projects.insert(pid, f);
                }
                Ok(None) => {}
                Err(e) => return db_error(e),
            }
        }
        authorize::filter_list(
            &state.pool,
            &viewer,
            Resource::Master,
            "GET /api/v1/workflow-statuses/",
            statuses,
            |w| w.id as i64,
            |w| match (w.team_id, w.project) {
                (Some(t), _) => ResourceRef::Team(viewer.team_facts_for_read(t)),
                (None, Some(p)) => projects.get(&p).cloned().unwrap_or(ResourceRef::Project {
                    project_id: p,
                    teams: vec![],
                }),
                (None, None) => ResourceRef::GlobalMaster,
            },
        )
    } else {
        statuses
    };

    // 件数取得
    let count = match workflow_status_repo::count_workflow_statuses(
        &state.pool,
        params.project,
        params.team,
        scope.as_ref(),
    )
    .await
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response();
        }
    };

    let has_next = page * PAGE_SIZE < count;
    let has_previous = page > 1;

    let next = if has_next {
        if let Some(project) = params.project {
            Some(format!("?project={}&page={}", project, page + 1))
        } else {
            Some(format!("?page={}", page + 1))
        }
    } else {
        None
    };

    let previous = if has_previous {
        if page == 2 {
            if let Some(project) = params.project {
                Some(format!("?project={}", project))
            } else {
                Some("?".to_string())
            }
        } else {
            if let Some(project) = params.project {
                Some(format!("?project={}&page={}", project, page - 1))
            } else {
                Some(format!("?page={}", page - 1))
            }
        }
    } else {
        None
    };

    (
        StatusCode::OK,
        Json(PaginatedWorkflowStatusesOut {
            count,
            next,
            previous,
            results: statuses,
        }),
    )
        .into_response()
}

/// ワークフロー状態詳細 GET /api/v1/workflow-statuses/{id}/
pub async fn workflow_status_detail(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match workflow_status_repo::find_workflow_status_by_id(&state.pool, id).await {
        Ok(Some(status)) => {
            if let Err(resp) = status_gate(
                &state,
                &viewer,
                status.team_id,
                status.project,
                Action::Read,
                id,
                "GET /api/v1/workflow-statuses/{id}/",
            )
            .await
            {
                return resp;
            }
            (StatusCode::OK, Json(status)).into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "見つかりません".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// ワークフロー状態作成 POST /api/v1/workflow-statuses/
pub async fn workflow_status_create(
    State(state): State<AppState>,
    viewer: Viewer,
    Json(body): Json<WorkflowStatusWriteIn>,
) -> impl IntoResponse {
    if body.project.is_none() && body.team_id.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                detail: "project または teamId を指定してください".to_string(),
            }),
        )
            .into_response();
    }
    if let Err(resp) = status_gate(
        &state,
        &viewer,
        body.team_id,
        body.project,
        Action::Create,
        0,
        "POST /api/v1/workflow-statuses/",
    )
    .await
    {
        return resp;
    }
    match workflow_status_repo::create_workflow_status(&state.pool, &body).await {
        Ok(status_id) => {
            // 作成したワークフロー状態を返す
            match workflow_status_repo::find_workflow_status_by_id(&state.pool, status_id).await {
                Ok(Some(status)) => (StatusCode::CREATED, Json(status)).into_response(),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response(),
            }
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// ワークフロー状態更新 PUT /api/v1/workflow-statuses/{id}/ (フル更新)
pub async fn workflow_status_update(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
    Json(body): Json<WorkflowStatusWriteIn>,
) -> impl IntoResponse {
    // 今の状態と、移し先(チーム・プロジェクトの変更)の両方で判定する
    const ROUTE: &str = "PUT /api/v1/workflow-statuses/{id}/";
    if let Err(resp) = existing_status_gate(&state, &viewer, id, Action::Write, ROUTE).await {
        return resp;
    }
    if let Err(resp) = status_gate(
        &state,
        &viewer,
        body.team_id,
        body.project,
        Action::Write,
        id,
        ROUTE,
    )
    .await
    {
        return resp;
    }
    match workflow_status_repo::update_workflow_status(&state.pool, id, &body).await {
        Ok(true) => {
            // 更新後のワークフロー状態を返す
            match workflow_status_repo::find_workflow_status_by_id(&state.pool, id).await {
                Ok(Some(status)) => (StatusCode::OK, Json(status)).into_response(),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response(),
            }
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "見つかりません".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// ワークフロー状態部分更新 PATCH /api/v1/workflow-statuses/{id}/ (部分更新)
pub async fn workflow_status_partial_update(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
    Json(body): Json<crate::domain::models::workflow_status_api::WorkflowStatusUpdateIn>,
) -> impl IntoResponse {
    const ROUTE: &str = "PATCH /api/v1/workflow-statuses/{id}/";
    if let Err(resp) = existing_status_gate(&state, &viewer, id, Action::Write, ROUTE).await {
        return resp;
    }
    // 移し先が指定された場合は、移し先でも判定する
    if body.team_id.is_some() || body.project.is_some() {
        if let Err(resp) = status_gate(
            &state,
            &viewer,
            body.team_id,
            body.project,
            Action::Write,
            id,
            ROUTE,
        )
        .await
        {
            return resp;
        }
    }
    match workflow_status_repo::partial_update_workflow_status(&state.pool, id, &body).await {
        Ok(true) => {
            // 更新後のワークフロー状態を返す
            match workflow_status_repo::find_workflow_status_by_id(&state.pool, id).await {
                Ok(Some(status)) => (StatusCode::OK, Json(status)).into_response(),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response(),
            }
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "見つかりません".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// ワークフロー状態削除 DELETE /api/v1/workflow-statuses/{id}/
pub async fn workflow_status_delete(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    if let Err(resp) = existing_status_gate(
        &state,
        &viewer,
        id,
        Action::Delete,
        "DELETE /api/v1/workflow-statuses/{id}/",
    )
    .await
    {
        return resp;
    }
    match workflow_status_repo::delete_workflow_status(&state.pool, id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "見つかりません".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// ワークフロー状態並び順一括更新 POST /api/v1/workflow-statuses/reorder/
pub async fn workflow_status_reorder(
    State(state): State<AppState>,
    viewer: Viewer,
    Json(body): Json<ReorderIn>,
) -> impl IntoResponse {
    // 並べ替える状態のすべてで、変更できること
    for &id in &body.order {
        if let Err(resp) = existing_status_gate(
            &state,
            &viewer,
            id,
            Action::Write,
            "POST /api/v1/workflow-statuses/reorder/",
        )
        .await
        {
            return resp;
        }
    }
    match workflow_status_repo::reorder_workflow_statuses(&state.pool, body.order).await {
        Ok(_) => (
            StatusCode::OK,
            Json(ReorderOut {
                detail: "並び順を更新しました".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}
