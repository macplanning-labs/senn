/// presentation/handlers/project_structure_api.rs — プロジェクトの構造 API
///
/// - GET /api/v1/projects/{id}/structure/     プロジェクトの構造（親子・関連・ロードマップ・候補）
/// - GET /api/v1/projects/{id}/children/      直接の子プロジェクト
/// - POST /api/v1/projects/{id}/relations/    関連を追加
/// - DELETE /api/v1/projects/{id}/relations/{related_id}/  関連を削除
///
/// 設計: docs/詳細設計書_プロジェクト詳細タブ.md §9.4

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::domain::models::resource_api::ProjectOut;
use crate::infrastructure::repositories::{
    project_hierarchy_repo, project_team_repo, resource_repo, roadmap_repo, user_repo, project_activity_repo,
};
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
// 親の変更(PATCH /projects/{id}/ の parentProjectId から呼ばれる)
// =============================================================================

fn project_ref(p: &ProjectOut) -> serde_json::Value {
    json!({ "id": p.id, "prefix": p.prefix, "name": p.name })
}

/// 親を設定・変更・解除する。権限(子側と、新しい親側の両方の can_manage)を検査し、
/// 循環・深さ・存在のエラーを分かりやすい理由で返し、成功したら Activity を記録する。
/// Err の Response をそのまま返せばよい。
pub async fn change_parent(
    state: &AppState,
    auth: &AuthUser,
    project_id: i32,
    new_parent: Option<i32>,
) -> Result<(), Response> {
    use project_hierarchy_repo::ProjectHierarchyError as E;

    // 変更前の状態(Activity の from に使う。set_parent より前に読むこと)
    let old = match resource_repo::find_project_by_id(&state.pool, project_id, None).await {
        Ok(Some(p)) => p,
        Ok(None) => return Err(error(StatusCode::NOT_FOUND, "見つかりません")),
        Err(e) => return Err(server_error(e)),
    };

    let staff = match user_repo::find_by_id(&state.pool, auth.user_id).await {
        Ok(Some(u)) => u.is_staff,
        Ok(None) => return Err(error(StatusCode::UNAUTHORIZED, "ユーザーが見つかりません")),
        Err(e) => return Err(server_error(e)),
    };
    match project_team_repo::can_manage(&state.pool, project_id, auth.user_id, staff).await {
        Ok(true) => {}
        Ok(false) => {
            return Err(error(
                StatusCode::FORBIDDEN,
                "親を変更できるのは、システム管理者・プロジェクトのオーナー・参加チームの管理者です",
            ))
        }
        Err(e) => return Err(server_error(e)),
    }
    if let Some(parent_id) = new_parent {
        match project_team_repo::can_manage(&state.pool, parent_id, auth.user_id, staff).await {
            Ok(true) => {}
            Ok(false) => {
                return Err(error(
                    StatusCode::FORBIDDEN,
                    "親にするプロジェクトの管理権限(システム管理者・オーナー・参加チームの管理者)が必要です",
                ))
            }
            Err(e) => return Err(server_error(e)),
        }
    }

    if let Err(e) = project_hierarchy_repo::set_parent(&state.pool, project_id, new_parent).await {
        return Err(match e.downcast_ref::<E>() {
            Some(E::SelfAsParent) => error(StatusCode::BAD_REQUEST, "自分自身は親にできません"),
            Some(E::ParentNotFound) => error(StatusCode::NOT_FOUND, "親にするプロジェクトが見つかりません"),
            Some(E::Cycle(path)) => {
                // path は 自分 → … → 新しい親(新しい親は、自分の子孫にあたる)
                let label = |p: &project_hierarchy_repo::ProjectPath| format!("{} {}", p.prefix, p.name);
                let me = path.first().map(label).unwrap_or_else(|| old.name.clone());
                let parent = path.last().map(label).unwrap_or_else(|| "親".to_string());
                let route = path.iter().map(label).collect::<Vec<_>>().join(" → ");
                let tail = if path.len() > 1 { format!("(たどった経路: {route})") } else { String::new() };
                error(
                    StatusCode::CONFLICT,
                    &format!("「{parent}」は「{me}」の子孫のため、親にできません{tail}"),
                )
            }
            Some(E::DepthExceeded(_)) => error(
                StatusCode::CONFLICT,
                &format!("階層は{}段までです。この親にすると超えてしまいます", project_hierarchy_repo::MAX_PROJECT_DEPTH),
            ),
            None => server_error(e),
        });
    }

    // Activity(記録の失敗は操作を失敗させない)
    if old.parent_project_id != new_parent {
        let lookup = |id: Option<i32>| async move {
            match id {
                Some(id) => resource_repo::find_project_by_id(&state.pool, id, None).await.ok().flatten(),
                None => None,
            }
        };
        let from = lookup(old.parent_project_id).await;
        let to = lookup(new_parent).await;
        project_activity_repo::record_best_effort(
            &state.pool,
            project_id,
            Some(auth.user_id),
            "parent_changed",
            json!({ "from": from.as_ref().map(project_ref), "to": to.as_ref().map(project_ref) }),
        )
        .await;
        if let Some(from) = &from {
            project_activity_repo::record_best_effort(
                &state.pool, from.id, Some(auth.user_id), "child_removed", json!({ "project": project_ref(&old) }),
            )
            .await;
        }
        if let Some(to) = &to {
            project_activity_repo::record_best_effort(
                &state.pool, to.id, Some(auth.user_id), "child_added", json!({ "project": project_ref(&old) }),
            )
            .await;
        }
    }
    Ok(())
}

// =============================================================================
// /projects/{id}/structure/
// =============================================================================

#[derive(Debug, Clone, Serialize)]
pub struct ProjectStructureOut {
    #[serde(rename = "canManage")]
    pub can_manage: bool,
    pub ancestors: Vec<project_hierarchy_repo::ProjectPath>,
    pub parent: Option<ProjectSummary>,
    pub children: Vec<ProjectChild>,
    pub rollup: RollupOut,
    pub related: Vec<RelatedProjectOut>,
    pub roadmaps: Vec<RoadmapBadge>,
    pub candidates: CandidatesOut,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectSummary {
    pub id: i32,
    pub prefix: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectChild {
    pub id: i32,
    pub prefix: String,
    pub name: String,
    pub status: String,
    pub priority: String,
    #[serde(rename = "ownerId")]
    pub owner_id: Option<i32>,
    #[serde(rename = "ticketCount")]
    pub ticket_count: i64,
    #[serde(rename = "completedCount")]
    pub completed_count: i64,
    pub progress: Option<f64>,
    #[serde(rename = "childCount")]
    pub child_count: i64,
    pub teams: Vec<ProjectTeamOut>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectTeamOut {
    pub id: i32,
    pub name: String,
    pub slug: String,
    pub icon: String,
    pub color: String,
    pub archived: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RollupOut {
    #[serde(rename = "projectCount")]
    pub project_count: i64,
    #[serde(rename = "ticketCount")]
    pub ticket_count: i64,
    #[serde(rename = "completedCount")]
    pub completed_count: i64,
    pub progress: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RelatedProjectOut {
    pub id: i32,
    pub prefix: String,
    pub name: String,
    pub status: String,
    #[serde(rename = "ticketCount")]
    pub ticket_count: i64,
    pub progress: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoadmapBadge {
    pub id: i32,
    pub name: String,
    #[serde(rename = "canRemove")]
    pub can_remove: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CandidatesOut {
    pub parent: Vec<ProjectSummary>,
    pub related: Vec<ProjectSummary>,
    pub roadmaps: Vec<RoadmapCandidate>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoadmapCandidate {
    pub id: i32,
    pub name: String,
}

/// プロジェクトの構造 GET /api/v1/projects/{id}/structure/
pub async fn structure(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> Response {
    // プロジェクトが存在するか確認
    let project = match resource_repo::find_project_by_id(&state.pool, id, Some(auth.user_id)).await {
        Ok(Some(p)) => p,
        Ok(None) => return error(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => return server_error(e),
    };

    // caller_is_staff
    let caller_is_staff = match user_repo::find_by_id(&state.pool, auth.user_id).await {
        Ok(Some(u)) => u.is_staff,
        Ok(None) => return error(StatusCode::UNAUTHORIZED, "ユーザーが見つかりません"),
        Err(e) => return server_error(e),
    };

    // can_manage
    let can_manage = match project_team_repo::can_manage(&state.pool, id, auth.user_id, caller_is_staff).await {
        Ok(cm) => cm,
        Err(e) => return server_error(e),
    };

    // 祖先（ルート → 直近の親）
    let ancestors = match project_hierarchy_repo::ancestors(&state.pool, id).await {
        Ok(a) => a,
        Err(e) => return server_error(e),
    };

    // 直接の親
    let parent = if let Some(parent_id) = project.parent_project_id {
        match resource_repo::find_project_by_id(&state.pool, parent_id, None).await {
            Ok(Some(p)) => Some(ProjectSummary {
                id: p.id,
                prefix: p.prefix,
                name: p.name,
            }),
            _ => None,
        }
    } else {
        None
    };

    // 直接の子
    let children = match project_hierarchy_repo::children(&state.pool, id).await {
        Ok(ch) => ch
            .into_iter()
            .map(|c| {
                let teams = c
                    .teams
                    .into_iter()
                    .map(|t| ProjectTeamOut {
                        id: t.id,
                        name: t.name,
                        slug: t.slug,
                        icon: t.icon,
                        color: t.color,
                        archived: t.archived,
                    })
                    .collect();
                ProjectChild {
                    id: c.id,
                    prefix: c.prefix,
                    name: c.name,
                    status: c.status,
                    priority: c.priority,
                    owner_id: c.owner_id,
                    ticket_count: c.ticket_count,
                    completed_count: c.completed_count,
                    progress: c.progress,
                    child_count: c.child_count,
                    teams,
                }
            })
            .collect(),
        Err(e) => return server_error(e),
    };

    // rollup（自分を含む全子孫）
    let rollup = match project_hierarchy_repo::rollup(&state.pool, id).await {
        Ok(r) => RollupOut {
            project_count: r.project_count,
            ticket_count: r.ticket_count,
            completed_count: r.completed_count,
            progress: r.progress,
        },
        Err(e) => return server_error(e),
    };

    // 関連
    let related = match project_hierarchy_repo::relations(&state.pool, id).await {
        Ok(rels) => rels
            .into_iter()
            .map(|r| RelatedProjectOut {
                id: r.id,
                prefix: r.prefix,
                name: r.name,
                status: r.status,
                ticket_count: r.ticket_count,
                progress: r.progress,
            })
            .collect(),
        Err(e) => return server_error(e),
    };

    // このプロジェクトが所属するロードマップ
    let roadmaps: Vec<RoadmapBadge> = match roadmap_repo::for_project(&state.pool, id).await {
        Ok(rms) => rms
            .into_iter()
            .map(|rm| {
                let can_remove = caller_is_staff || rm.owner_id == Some(auth.user_id) || can_manage;
                RoadmapBadge {
                    id: rm.id,
                    name: rm.name,
                    can_remove,
                }
            })
            .collect(),
        Err(e) => return server_error(e),
    };

    // candidates
    let candidates = if can_manage {
        let parent_candidates = match project_hierarchy_repo::parent_candidates(&state.pool, id, auth.user_id, caller_is_staff)
            .await
        {
            Ok(pc) => pc
                .into_iter()
                .map(|p| ProjectSummary {
                    id: p.id,
                    prefix: p.prefix,
                    name: p.name,
                })
                .collect(),
            Err(e) => {
                tracing::warn!("parent_candidates 取得失敗: {:?}", e);
                vec![]
            }
        };

        let related_candidates = match project_hierarchy_repo::relation_candidates(&state.pool, id, auth.user_id, caller_is_staff)
            .await
        {
            Ok(rc) => rc
                .into_iter()
                .map(|p| ProjectSummary {
                    id: p.id,
                    prefix: p.prefix,
                    name: p.name,
                })
                .collect(),
            Err(e) => {
                tracing::warn!("relation_candidates 取得失敗: {:?}", e);
                vec![]
            }
        };

        let roadmap_candidates = match roadmap_repo::list_roadmaps(&state.pool).await {
            Ok(all) => {
                let existing_ids: std::collections::HashSet<_> = roadmaps.iter().map(|r| r.id).collect();
                all.into_iter()
                    .filter(|rm| !existing_ids.contains(&rm.id))
                    .map(|rm| RoadmapCandidate { id: rm.id, name: rm.name })
                    .collect()
            }
            Err(e) => {
                tracing::warn!("roadmap_candidates 取得失敗: {:?}", e);
                vec![]
            }
        };

        CandidatesOut {
            parent: parent_candidates,
            related: related_candidates,
            roadmaps: roadmap_candidates,
        }
    } else {
        CandidatesOut {
            parent: vec![],
            related: vec![],
            roadmaps: match roadmap_repo::for_owner(&state.pool, auth.user_id).await {
                Ok(all) => {
                    let existing_ids: std::collections::HashSet<_> = roadmaps.iter().map(|r| r.id).collect();
                    all.into_iter()
                        .filter(|rm| !existing_ids.contains(&rm.id))
                        .map(|rm| RoadmapCandidate { id: rm.id, name: rm.name })
                        .collect()
                }
                Err(e) => {
                    tracing::warn!("roadmap_candidates (owner only) 取得失敗: {:?}", e);
                    vec![]
                }
            },
        }
    };

    let resp = ProjectStructureOut {
        can_manage,
        ancestors,
        parent,
        children,
        rollup,
        related,
        roadmaps,
        candidates,
    };

    (StatusCode::OK, Json(resp)).into_response()
}

// =============================================================================
// /projects/{id}/children/
// =============================================================================

/// 直接の子プロジェクト GET /api/v1/projects/{id}/children/
pub async fn children(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> Response {
    // プロジェクトが存在するか確認
    if let Err(resp) = resource_repo::find_project_by_id(&state.pool, id, Some(auth.user_id))
        .await
        .map_err(|e| server_error(e))
        .and_then(|opt| opt.ok_or_else(|| error(StatusCode::NOT_FOUND, "見つかりません")))
    {
        return resp;
    }

    match project_hierarchy_repo::children(&state.pool, id).await {
        Ok(ch) => {
            let res: Vec<ProjectChild> = ch
                .into_iter()
                .map(|c| {
                    let teams = c
                        .teams
                        .into_iter()
                        .map(|t| ProjectTeamOut {
                            id: t.id,
                            name: t.name,
                            slug: t.slug,
                            icon: t.icon,
                            color: t.color,
                            archived: t.archived,
                        })
                        .collect();
                    ProjectChild {
                        id: c.id,
                        prefix: c.prefix,
                        name: c.name,
                        status: c.status,
                        priority: c.priority,
                        owner_id: c.owner_id,
                        ticket_count: c.ticket_count,
                        completed_count: c.completed_count,
                        progress: c.progress,
                        child_count: c.child_count,
                        teams,
                    }
                })
                .collect();
            (StatusCode::OK, Json(res)).into_response()
        }
        Err(e) => server_error(e),
    }
}

// =============================================================================
// /projects/{id}/relations/
// =============================================================================

#[derive(Deserialize)]
pub struct AddRelationIn {
    #[serde(rename = "relatedProjectId")]
    pub related_project_id: i32,
    #[serde(default)]
    pub relation_type: Option<String>,
}

/// 関連を追加 POST /api/v1/projects/{id}/relations/
pub async fn add_relation(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<i32>,
    Json(body): Json<AddRelationIn>,
) -> Response {
    // プロジェクトが存在するか確認
    if let Err(resp) = resource_repo::find_project_by_id(&state.pool, id, Some(auth.user_id))
        .await
        .map_err(|e| server_error(e))
        .and_then(|opt| opt.ok_or_else(|| error(StatusCode::NOT_FOUND, "見つかりません")))
    {
        return resp;
    }

    // 相手が存在するか確認
    if let Err(resp) = resource_repo::find_project_by_id(&state.pool, body.related_project_id, Some(auth.user_id))
        .await
        .map_err(|e| server_error(e))
        .and_then(|opt| opt.ok_or_else(|| error(StatusCode::NOT_FOUND, "見つかりません")))
    {
        return resp;
    }

    // 自己関連は 400
    if id == body.related_project_id {
        return error(StatusCode::BAD_REQUEST, "同じプロジェクトと関連付けることはできません");
    }

    let caller_is_staff = match user_repo::find_by_id(&state.pool, auth.user_id).await {
        Ok(Some(u)) => u.is_staff,
        Ok(None) => return error(StatusCode::UNAUTHORIZED, "ユーザーが見つかりません"),
        Err(e) => return server_error(e),
    };

    // 両方のプロジェクトに can_manage があるか確認
    let can_manage_self = match project_team_repo::can_manage(&state.pool, id, auth.user_id, caller_is_staff).await {
        Ok(cm) => cm,
        Err(e) => return server_error(e),
    };

    let can_manage_other = match project_team_repo::can_manage(&state.pool, body.related_project_id, auth.user_id, caller_is_staff)
        .await
    {
        Ok(cm) => cm,
        Err(e) => return server_error(e),
    };

    if !can_manage_self || !can_manage_other {
        return error(StatusCode::FORBIDDEN, "両方のプロジェクトを変更することはできません");
    }

    // 既に関連があるか確認（既に関連なら 200）
    match project_hierarchy_repo::add_relation(&state.pool, id, body.related_project_id).await {
        // 既に関連だった場合は、何も変えず 200(冪等。Activity も記録しない)
        Ok(false) => (StatusCode::OK, Json(json!({"id": id, "relatedProjectId": body.related_project_id}))).into_response(),
        Ok(true) => {
            // Activity を記録（両方に relation_added）
            let proj = match resource_repo::find_project_by_id(&state.pool, body.related_project_id, None).await {
                Ok(Some(p)) => Some(json!({ "id": p.id, "prefix": p.prefix, "name": p.name })),
                _ => None,
            };

            let self_proj = match resource_repo::find_project_by_id(&state.pool, id, None).await {
                Ok(Some(p)) => Some(json!({ "id": p.id, "prefix": p.prefix, "name": p.name })),
                _ => None,
            };

            if let Some(proj_json) = &proj {
                project_activity_repo::record_best_effort(
                    &state.pool,
                    id,
                    Some(auth.user_id),
                    "relation_added",
                    json!({ "project": proj_json }),
                )
                .await;
            }

            if let Some(self_json) = &self_proj {
                project_activity_repo::record_best_effort(
                    &state.pool,
                    body.related_project_id,
                    Some(auth.user_id),
                    "relation_added",
                    json!({ "project": self_json }),
                )
                .await;
            }

            // 新しく関連を作った
            (StatusCode::CREATED, Json(json!({"id": id, "relatedProjectId": body.related_project_id}))).into_response()
        }
        Err(e) => {
            tracing::error!("関連の追加に失敗: {:?}", e);
            server_error(e)
        }
    }
}

/// 関連を削除 DELETE /api/v1/projects/{id}/relations/{related_id}/
pub async fn remove_relation(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((id, related_id)): Path<(i32, i32)>,
) -> Response {
    // プロジェクトが存在するか確認
    if let Err(resp) = resource_repo::find_project_by_id(&state.pool, id, Some(auth.user_id))
        .await
        .map_err(|e| server_error(e))
        .and_then(|opt| opt.ok_or_else(|| error(StatusCode::NOT_FOUND, "見つかりません")))
    {
        return resp;
    }

    let caller_is_staff = match user_repo::find_by_id(&state.pool, auth.user_id).await {
        Ok(Some(u)) => u.is_staff,
        Ok(None) => return error(StatusCode::UNAUTHORIZED, "ユーザーが見つかりません"),
        Err(e) => return server_error(e),
    };

    // どちらか一方に can_manage があるか確認
    let can_manage_self = match project_team_repo::can_manage(&state.pool, id, auth.user_id, caller_is_staff).await {
        Ok(cm) => cm,
        Err(e) => return server_error(e),
    };

    let can_manage_other = match project_team_repo::can_manage(&state.pool, related_id, auth.user_id, caller_is_staff).await {
        Ok(cm) => cm,
        Err(e) => return server_error(e),
    };

    if !can_manage_self && !can_manage_other {
        return error(StatusCode::FORBIDDEN, "プロジェクトを変更することはできません");
    }

    match project_hierarchy_repo::remove_relation(&state.pool, id, related_id).await {
        Ok(found) => {
            if !found {
                return error(StatusCode::NOT_FOUND, "見つかりません");
            }

            // Activity を記録（両方に relation_removed）
            let proj = match resource_repo::find_project_by_id(&state.pool, related_id, None).await {
                Ok(Some(p)) => Some(json!({ "id": p.id, "prefix": p.prefix, "name": p.name })),
                _ => None,
            };

            let self_proj = match resource_repo::find_project_by_id(&state.pool, id, None).await {
                Ok(Some(p)) => Some(json!({ "id": p.id, "prefix": p.prefix, "name": p.name })),
                _ => None,
            };

            if let Some(proj_json) = &proj {
                project_activity_repo::record_best_effort(
                    &state.pool,
                    id,
                    Some(auth.user_id),
                    "relation_removed",
                    json!({ "project": proj_json }),
                )
                .await;
            }

            if let Some(self_json) = &self_proj {
                project_activity_repo::record_best_effort(
                    &state.pool,
                    related_id,
                    Some(auth.user_id),
                    "relation_removed",
                    json!({ "project": self_json }),
                )
                .await;
            }

            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => {
            tracing::error!("関連の削除に失敗: {:?}", e);
            server_error(e)
        }
    }
}
