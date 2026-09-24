/// presentation/handlers/roadmap_api.rs — ロードマップ API
///
/// - GET /api/v1/roadmaps/                       一覧
/// - POST /api/v1/roadmaps/                      作成
/// - GET /api/v1/roadmaps/{id}/                  詳細
/// - PUT /api/v1/roadmaps/{id}/                  編集
/// - DELETE /api/v1/roadmaps/{id}/               削除
/// - POST /api/v1/roadmaps/{id}/projects/        プロジェクトを追加
/// - DELETE /api/v1/roadmaps/{id}/projects/{project_id}/  プロジェクトを削除
///
/// 設計: docs/詳細設計書_プロジェクト詳細タブ.md §9.3 / §9.4.4

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::infrastructure::repositories::{project_team_repo, resource_repo, roadmap_repo, user_repo, project_activity_repo};
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::presentation::state::AppState;

#[derive(Serialize)]
pub struct ErrorResponse {
    pub detail: String,
}

fn error(status: StatusCode, detail: &str) -> Response {
    (status, Json(ErrorResponse { detail: detail.to_string() })).into_response()
}

fn server_error(e: anyhow::Error) -> Response {
    tracing::error!("DB operation failed: {:?}", e);
    error(StatusCode::INTERNAL_SERVER_ERROR, "サーバーエラーが発生しました")
}

// =============================================================================
// Response types
// =============================================================================

#[derive(Debug, Clone, Serialize)]
pub struct RoadmapOut {
    pub id: i32,
    pub name: String,
    pub description: String,
    #[serde(rename = "ownerId")]
    pub owner_id: Option<i32>,
    #[serde(rename = "projectCount")]
    pub project_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoadmapDetailOut {
    pub id: i32,
    pub name: String,
    pub description: String,
    #[serde(rename = "ownerId")]
    pub owner_id: Option<i32>,
    #[serde(rename = "canManage")]
    pub can_manage: bool,
    pub projects: Vec<RoadmapProjectOut>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoadmapProjectOut {
    pub id: i32,
    pub prefix: String,
    pub name: String,
    pub status: String,
    #[serde(rename = "ticketCount")]
    pub ticket_count: i64,
    #[serde(rename = "completedCount")]
    pub completed_count: i64,
    pub progress: Option<f64>,
}

// =============================================================================
// Input types
// =============================================================================

#[derive(Deserialize)]
pub struct RoadmapCreateIn {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Deserialize)]
pub struct AddProjectIn {
    #[serde(rename = "projectId")]
    pub project_id: i32,
}

// =============================================================================
// GET /api/v1/roadmaps/
// =============================================================================

/// 一覧
pub async fn roadmaps_list(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
) -> Response {
    match roadmap_repo::list_roadmaps(&state.pool).await {
        Ok(roadmaps) => {
            let res: Vec<RoadmapOut> = roadmaps
                .into_iter()
                .map(|r| RoadmapOut {
                    id: r.id,
                    name: r.name,
                    description: r.description,
                    owner_id: r.owner_id,
                    project_count: r.project_count,
                })
                .collect();
            (StatusCode::OK, Json(res)).into_response()
        }
        Err(e) => server_error(e),
    }
}

// =============================================================================
// POST /api/v1/roadmaps/
// =============================================================================

/// 作成
pub async fn roadmaps_create(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<RoadmapCreateIn>,
) -> Response {
    // 名前が 1〜100 文字か確認
    let name = body.name.trim().to_string();
    if !roadmap_repo::is_valid_roadmap_name(&name) {
        return error(StatusCode::BAD_REQUEST, "名前は1〜100文字で入力してください");
    }

    let input = roadmap_repo::RoadmapCreateIn {
        name,
        description: body.description,
    };

    match roadmap_repo::create_roadmap(&state.pool, &input, Some(auth.user_id)).await {
        Ok(id) => {
            // 作成したロードマップを返す
            match roadmap_repo::find_roadmap_by_id(&state.pool, id).await {
                Ok(Some(detail)) => {
                    let res = RoadmapDetailOut {
                        id: detail.id,
                        name: detail.name,
                        description: detail.description,
                        owner_id: detail.owner_id,
                        can_manage: detail.owner_id == Some(auth.user_id),
                        projects: detail
                            .projects
                            .into_iter()
                            .map(|p| RoadmapProjectOut {
                                id: p.id,
                                prefix: p.prefix,
                                name: p.name,
                                status: p.status,
                                ticket_count: p.ticket_count,
                                completed_count: p.completed_count,
                                progress: p.progress,
                            })
                            .collect(),
                    };
                    (StatusCode::CREATED, Json(res)).into_response()
                }
                Ok(None) => {
                    tracing::error!("created roadmap not found (id={})", id);
                    server_error(anyhow::anyhow!("ロードマップが見つかりません"))
                }
                Err(e) => server_error(e),
            }
        }
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("already exists") {
                error(StatusCode::CONFLICT, "このロードマップ名は既に使用されています")
            } else {
                server_error(e)
            }
        }
    }
}

// =============================================================================
// GET /api/v1/roadmaps/{id}/
// =============================================================================

/// 詳細
pub async fn roadmap_detail(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> Response {
    match roadmap_repo::find_roadmap_by_id(&state.pool, id).await {
        Ok(Some(detail)) => {
            let can_manage = detail.owner_id == Some(auth.user_id) || {
                match user_repo::find_by_id(&state.pool, auth.user_id).await {
                    Ok(Some(u)) => u.is_staff,
                    _ => false,
                }
            };

            let res = RoadmapDetailOut {
                id: detail.id,
                name: detail.name,
                description: detail.description,
                owner_id: detail.owner_id,
                can_manage,
                projects: detail
                    .projects
                    .into_iter()
                    .map(|p| RoadmapProjectOut {
                        id: p.id,
                        prefix: p.prefix,
                        name: p.name,
                        status: p.status,
                        ticket_count: p.ticket_count,
                        completed_count: p.completed_count,
                        progress: p.progress,
                    })
                    .collect(),
            };
            (StatusCode::OK, Json(res)).into_response()
        }
        Ok(None) => error(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => server_error(e),
    }
}

// =============================================================================
// PUT /api/v1/roadmaps/{id}/
// =============================================================================

/// 編集
pub async fn roadmap_update(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<i32>,
    Json(mut body): Json<roadmap_repo::RoadmapUpdateIn>,
) -> Response {
    // 存在確認・権限確認
    let detail = match roadmap_repo::find_roadmap_by_id(&state.pool, id).await {
        Ok(Some(d)) => d,
        Ok(None) => return error(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => return server_error(e),
    };

    let is_staff = match user_repo::find_by_id(&state.pool, auth.user_id).await {
        Ok(Some(u)) => u.is_staff,
        Ok(None) => return error(StatusCode::UNAUTHORIZED, "ユーザーが見つかりません"),
        Err(e) => return server_error(e),
    };

    let can_manage = is_staff || detail.owner_id == Some(auth.user_id);
    if !can_manage {
        return error(StatusCode::FORBIDDEN, "ロードマップを編集することはできません");
    }

    // 名前が更新される場合、1〜100 文字か確認
    if let Some(ref name) = body.name {
        if !roadmap_repo::is_valid_roadmap_name(name) {
            return error(StatusCode::BAD_REQUEST, "名前は1〜100文字で入力してください");
        }
    }
    // 名前は前後の空白を取り除いて保存する
    if let Some(n) = body.name.as_mut() {
        *n = n.trim().to_string();
    }

    match roadmap_repo::update_roadmap(&state.pool, id, &body).await {
        Ok(_) => {
            match roadmap_repo::find_roadmap_by_id(&state.pool, id).await {
                Ok(Some(updated)) => {
                    let res = RoadmapDetailOut {
                        id: updated.id,
                        name: updated.name,
                        description: updated.description,
                        owner_id: updated.owner_id,
                        can_manage,
                        projects: updated
                            .projects
                            .into_iter()
                            .map(|p| RoadmapProjectOut {
                                id: p.id,
                                prefix: p.prefix,
                                name: p.name,
                                status: p.status,
                                ticket_count: p.ticket_count,
                                completed_count: p.completed_count,
                                progress: p.progress,
                            })
                            .collect(),
                    };
                    (StatusCode::OK, Json(res)).into_response()
                }
                Ok(None) => error(StatusCode::NOT_FOUND, "見つかりません"),
                Err(e) => server_error(e),
            }
        }
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("already exists") {
                error(StatusCode::CONFLICT, "このロードマップ名は既に使用されています")
            } else {
                server_error(e)
            }
        }
    }
}

// =============================================================================
// DELETE /api/v1/roadmaps/{id}/
// =============================================================================

/// 削除
pub async fn roadmap_delete(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> Response {
    // 存在確認・権限確認
    let detail = match roadmap_repo::find_roadmap_by_id(&state.pool, id).await {
        Ok(Some(d)) => d,
        Ok(None) => return error(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => return server_error(e),
    };

    let is_staff = match user_repo::find_by_id(&state.pool, auth.user_id).await {
        Ok(Some(u)) => u.is_staff,
        Ok(None) => return error(StatusCode::UNAUTHORIZED, "ユーザーが見つかりません"),
        Err(e) => return server_error(e),
    };

    let can_manage = is_staff || detail.owner_id == Some(auth.user_id);
    if !can_manage {
        return error(StatusCode::FORBIDDEN, "ロードマップを削除することはできません");
    }

    match roadmap_repo::delete_roadmap(&state.pool, id).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => server_error(e),
    }
}

// =============================================================================
// POST /api/v1/roadmaps/{id}/projects/
// =============================================================================

/// プロジェクトを追加
pub async fn roadmap_add_project(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<i32>,
    Json(body): Json<AddProjectIn>,
) -> Response {
    // ロードマップ存在確認
    let roadmap = match roadmap_repo::find_roadmap_by_id(&state.pool, id).await {
        Ok(Some(r)) => r,
        Ok(None) => return error(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => return server_error(e),
    };

    // プロジェクト存在確認
    if let Err(resp) = resource_repo::find_project_by_id(&state.pool, body.project_id, Some(auth.user_id))
        .await
        .map_err(|e| server_error(e))
        .and_then(|opt| opt.ok_or_else(|| error(StatusCode::NOT_FOUND, "見つかりません")))
    {
        return resp;
    }

    let is_staff = match user_repo::find_by_id(&state.pool, auth.user_id).await {
        Ok(Some(u)) => u.is_staff,
        Ok(None) => return error(StatusCode::UNAUTHORIZED, "ユーザーが見つかりません"),
        Err(e) => return server_error(e),
    };

    // 権限：システム管理者 / ロードマップの owner / そのプロジェクトの can_manage
    let can_manage_roadmap = is_staff || roadmap.owner_id == Some(auth.user_id);
    let can_manage_project = match project_team_repo::can_manage(&state.pool, body.project_id, auth.user_id, is_staff).await {
        Ok(cm) => cm,
        Err(e) => return server_error(e),
    };

    if !can_manage_roadmap && !can_manage_project {
        return error(
            StatusCode::FORBIDDEN,
            "ロードマップまたはプロジェクトを変更することはできません",
        );
    }

    match roadmap_repo::add_project(&state.pool, id, body.project_id, Some(auth.user_id)).await {
        Ok(is_new) => {
            // Activity を記録（プロジェクト側に）
            match roadmap_repo::find_roadmap_by_id(&state.pool, id).await {
                Ok(Some(rm)) => {
                    let status = if is_new {
                        StatusCode::CREATED
                    } else {
                        StatusCode::OK
                    };
                    project_activity_repo::record_best_effort(
                        &state.pool,
                        body.project_id,
                        Some(auth.user_id),
                        "roadmap_added",
                        json!({ "roadmap": { "id": rm.id, "name": rm.name } }),
                    )
                    .await;

                    (status, Json(json!({"id": id, "projectId": body.project_id}))).into_response()
                }
                Ok(None) => {
                    tracing::error!("ロードマップが見つかりません (id={})", id);
                    server_error(anyhow::anyhow!("ロードマップが見つかりません"))
                }
                Err(e) => server_error(e),
            }
        }
        Err(e) => {
            tracing::error!("プロジェクトの追加に失敗: {:?}", e);
            server_error(e)
        }
    }
}

// =============================================================================
// DELETE /api/v1/roadmaps/{id}/projects/{project_id}/
// =============================================================================

/// プロジェクトを削除
pub async fn roadmap_remove_project(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((id, project_id)): Path<(i32, i32)>,
) -> Response {
    // ロードマップ存在確認
    let roadmap = match roadmap_repo::find_roadmap_by_id(&state.pool, id).await {
        Ok(Some(r)) => r,
        Ok(None) => return error(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => return server_error(e),
    };

    let is_staff = match user_repo::find_by_id(&state.pool, auth.user_id).await {
        Ok(Some(u)) => u.is_staff,
        Ok(None) => return error(StatusCode::UNAUTHORIZED, "ユーザーが見つかりません"),
        Err(e) => return server_error(e),
    };

    // 権限：システム管理者 / ロードマップの owner / そのプロジェクトの can_manage
    let can_manage_roadmap = is_staff || roadmap.owner_id == Some(auth.user_id);
    let can_manage_project = match project_team_repo::can_manage(&state.pool, project_id, auth.user_id, is_staff).await {
        Ok(cm) => cm,
        Err(e) => return server_error(e),
    };

    if !can_manage_roadmap && !can_manage_project {
        return error(
            StatusCode::FORBIDDEN,
            "ロードマップまたはプロジェクトを変更することはできません",
        );
    }

    match roadmap_repo::remove_project(&state.pool, id, project_id).await {
        Ok(found) => {
            if !found {
                return error(StatusCode::NOT_FOUND, "見つかりません");
            }

            // Activity を記録（プロジェクト側に）
            match roadmap_repo::find_roadmap_by_id(&state.pool, id).await {
                Ok(Some(rm)) => {
                    project_activity_repo::record_best_effort(
                        &state.pool,
                        project_id,
                        Some(auth.user_id),
                        "roadmap_removed",
                        json!({ "roadmap": { "id": rm.id, "name": rm.name } }),
                    )
                    .await;

                    StatusCode::NO_CONTENT.into_response()
                }
                Ok(None) => {
                    tracing::error!("ロードマップが見つかりません (id={})", id);
                    server_error(anyhow::anyhow!("ロードマップが見つかりません"))
                }
                Err(e) => server_error(e),
            }
        }
        Err(e) => {
            tracing::error!("プロジェクトの削除に失敗: {:?}", e);
            server_error(e)
        }
    }
}
