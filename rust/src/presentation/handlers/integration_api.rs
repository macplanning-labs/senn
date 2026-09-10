/// presentation/handlers/integration_api.rs — Git連携 JSON API
///
/// integrations CRUD(JWT認証必須)+ GitHub Webhook受信(認証不要、HMAC署名検証)。

use axum::{
    extract::{State, Path, Query},
    response::IntoResponse,
    http::{StatusCode, HeaderMap},
    Json,
    Extension,
};
use serde::Deserialize;
use serde_json::json;

use crate::presentation::state::AppState;
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::infrastructure::repositories::integration_repo;
use crate::domain::models::integration_api::*;
use crate::domain::services::git_webhook_service;

fn err(detail: &str) -> serde_json::Value {
    json!({"detail": detail})
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub project: Option<i32>,
}

/// GET /api/v1/integrations/?project=<id>
pub async fn list(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Query(params): Query<ListQuery>,
) -> impl IntoResponse {
    match integration_repo::find_all(&state.pool, params.project).await {
        Ok(items) => (StatusCode::OK, Json(items)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

/// POST /api/v1/integrations/
pub async fn create(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<GitIntegrationWriteIn>,
) -> impl IntoResponse {
    match integration_repo::create(&state.pool, &body, auth.user_id).await {
        Ok(id) => match integration_repo::find_by_id(&state.pool, id).await {
            Ok(Some(item)) => (StatusCode::CREATED, Json(item)).into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response(),
        },
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

/// PATCH /api/v1/integrations/{id}/
pub async fn update(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
    Json(body): Json<GitIntegrationUpdateIn>,
) -> impl IntoResponse {
    match integration_repo::update(&state.pool, id, &body).await {
        Ok(true) => match integration_repo::find_by_id(&state.pool, id).await {
            Ok(Some(item)) => (StatusCode::OK, Json(item)).into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response(),
        },
        Ok(false) => (StatusCode::NOT_FOUND, Json(err("見つかりません"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

/// DELETE /api/v1/integrations/{id}/
pub async fn delete(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match integration_repo::delete(&state.pool, id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, Json(err("見つかりません"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

// =============================================================================
// GitHub Webhook 受信(認証不要)
// =============================================================================

/// POST /api/v1/webhooks/github/
pub async fn github_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let event_type = headers.get("X-GitHub-Event").and_then(|v| v.to_str().ok()).unwrap_or("");
    let signature = headers.get("X-Hub-Signature-256").and_then(|v| v.to_str().ok()).unwrap_or("");

    if event_type.is_empty() {
        return (StatusCode::BAD_REQUEST, "Missing X-GitHub-Event").into_response();
    }

    let payload: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid JSON").into_response(),
    };

    let repo_url = payload
        .get("repository")
        .and_then(|r| r.get("html_url"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if repo_url.is_empty() {
        return (StatusCode::BAD_REQUEST, "Missing repository URL").into_response();
    }

    let integrations = match integration_repo::find_active_integrations_by_repo_url(&state.pool, repo_url).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response();
        }
    };

    if integrations.is_empty() {
        tracing::warn!("No integration found for {}", repo_url);
        return (StatusCode::OK, "No matching integration").into_response();
    }

    let verified = integrations
        .iter()
        .find(|i| git_webhook_service::verify_github_signature(&body, signature, &i.webhook_secret));

    let active_integration = match verified {
        Some(i) => i,
        None => {
            tracing::warn!("Invalid signature for {}", repo_url);
            return (StatusCode::FORBIDDEN, "Invalid signature").into_response();
        }
    };

    // 完全な integration 情報を取得
    let integration = match integration_repo::find_by_id(&state.pool, active_integration.id).await {
        Ok(Some(i)) => i,
        Ok(None) => {
            tracing::warn!("Integration {} not found", active_integration.id);
            return (StatusCode::INTERNAL_SERVER_ERROR, "integration not found").into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response();
        }
    };

    let mut created_count = 0;

    if event_type == "push" {
        created_count = process_push_event(&state, &integration, &payload).await;
    } else if event_type == "pull_request" {
        created_count = process_pull_request_event(&state, &integration, &payload).await;
    }

    tracing::info!("Processed {} event: {} events linked", event_type, created_count);
    (StatusCode::OK, format!("OK: {} events linked", created_count)).into_response()
}

async fn process_push_event(
    state: &AppState,
    integration: &crate::domain::models::integration_api::GitIntegrationOut,
    payload: &serde_json::Value,
) -> i32 {
    let mut created = 0;
    let commits = payload.get("commits").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let branch = payload
        .get("ref")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .replace("refs/heads/", "");

    // 同一 push 全体で ticket ごとの status 遷移は1回
    let mut processed_tickets = std::collections::HashSet::new();

    for commit in &commits {
        let message = commit.get("message").and_then(|v| v.as_str()).unwrap_or("");
        let sha = commit.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let author_name = commit.get("author").and_then(|a| a.get("name")).and_then(|v| v.as_str()).unwrap_or("");
        let commit_url = commit.get("url").and_then(|v| v.as_str()).unwrap_or("");

        let mut ticket_keys = git_webhook_service::extract_ticket_keys(message);
        ticket_keys.extend(git_webhook_service::extract_ticket_keys(&branch));
        let mut seen = std::collections::HashSet::new();
        ticket_keys.retain(|k| seen.insert(k.clone()));

        let title: String = message.lines().next().unwrap_or("").chars().take(500).collect();

        for key in &ticket_keys {
            let ticket_id = match integration_repo::find_ticket_id_by_key(&state.pool, key).await {
                Ok(Some(id)) => id,
                Ok(None) => continue,
                Err(e) => {
                    tracing::error!("DB operation failed: {:?}", e);
                    continue;
                }
            };

            match integration_repo::create_commit_event_if_new(
                &state.pool, integration.id, ticket_id, sha, &title, commit_url, &branch, author_name,
            )
            .await
            {
                Ok(true) => {
                    created += 1;
                    tracing::info!("Linked commit {} to {}", &sha[..sha.len().min(7)], key);
                }
                Ok(false) => {}
                Err(e) => {
                    tracing::error!("DB operation failed: {:?}", e);
                    continue;
                }
            }

            // Status 遷移（フラグ ON・同一 push 内 ticket 1回）
            if !integration.auto_status_transition {
                continue;
            }
            if !processed_tickets.insert(ticket_id) {
                continue;
            }

            let actor_user_id = match git_webhook_service::resolve_git_actor(
                &state.pool,
                integration.created_by.as_ref().map(|u| u.id),
                integration.project,
            )
            .await
            {
                Ok(Some(actor)) => actor.user_id,
                Ok(None) => {
                    tracing::error!("git auto-status: no actor found for project {}", integration.project);
                    continue;
                }
                Err(e) => {
                    tracing::error!("git auto-status: resolve actor failed: {:?}", e);
                    continue;
                }
            };

            match crate::infrastructure::repositories::workflow_status_repo::resolve_status_slug_by_category(
                &state.pool, integration.project, "started"
            )
            .await
            {
                Ok(Some(target_slug)) => {
                    if let Err(e) = git_webhook_service::apply_git_status_transition(
                        &state.pool, ticket_id, &target_slug, actor_user_id, "push"
                    )
                    .await
                    {
                        tracing::error!("git auto-status: apply failed for ticket {}: {:?}", ticket_id, e);
                    }
                }
                Ok(None) => {
                    if let Err(e) = git_webhook_service::apply_git_status_transition(
                        &state.pool, ticket_id, "in_progress", actor_user_id, "push"
                    )
                    .await
                    {
                        tracing::error!("git auto-status: apply fallback failed for ticket {}: {:?}", ticket_id, e);
                    }
                }
                Err(e) => {
                    tracing::error!("git auto-status: resolve started slug failed: {:?}", e);
                }
            }
        }
    }

    created
}

async fn process_pull_request_event(
    state: &AppState,
    integration: &crate::domain::models::integration_api::GitIntegrationOut,
    payload: &serde_json::Value,
) -> i32 {
    let mut created = 0;
    let pr = payload.get("pull_request").cloned().unwrap_or(json!({}));

    let action = payload.get("action").and_then(|v| v.as_str()).unwrap_or("");
    let draft = pr.get("draft").and_then(|v| v.as_bool()).unwrap_or(false);

    let title: String = pr.get("title").and_then(|v| v.as_str()).unwrap_or("").chars().take(500).collect();
    let pr_url = pr.get("html_url").and_then(|v| v.as_str()).unwrap_or("");
    let pr_number = pr.get("number").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    let head_branch = pr.get("head").and_then(|h| h.get("ref")).and_then(|v| v.as_str()).unwrap_or("");
    let author_name = pr.get("user").and_then(|u| u.get("login")).and_then(|v| v.as_str()).unwrap_or("");
    let author_avatar = pr.get("user").and_then(|u| u.get("avatar_url")).and_then(|v| v.as_str()).unwrap_or("");

    let pr_state = if pr.get("merged").and_then(|v| v.as_bool()).unwrap_or(false) {
        "merged"
    } else if pr.get("state").and_then(|v| v.as_str()) == Some("closed") {
        "closed"
    } else {
        "open"
    };

    let mut ticket_keys = git_webhook_service::extract_ticket_keys(&title);
    ticket_keys.extend(git_webhook_service::extract_ticket_keys(head_branch));
    let mut seen = std::collections::HashSet::new();
    ticket_keys.retain(|k| seen.insert(k.clone()));

    for key in &ticket_keys {
        let ticket_id = match integration_repo::find_ticket_id_by_key(&state.pool, key).await {
            Ok(Some(id)) => id,
            Ok(None) => continue,
            Err(e) => {
                tracing::error!("DB operation failed: {:?}", e);
                continue;
            }
        };

        match integration_repo::upsert_pr_event(
            &state.pool, integration.id, ticket_id, pr_number, &title, pr_url, head_branch,
            author_name, author_avatar, pr_state,
        )
        .await
        {
            Ok(_created_new) => {
                created += 1;
                tracing::info!("Linked PR #{} to {}", pr_number, key);

                // PR opened/reopened/ready_for_review (かつ draft=false) の場合に in_review 相当へ遷移を試みる
                if integration.auto_status_transition
                    && matches!(action, "opened" | "reopened" | "ready_for_review")
                    && !draft
                {
                    let actor_user_id = match git_webhook_service::resolve_git_actor(
                        &state.pool,
                        integration.created_by.as_ref().map(|u| u.id),
                        integration.project,
                    )
                    .await
                    {
                        Ok(Some(actor)) => actor.user_id,
                        Ok(None) => {
                            tracing::error!("git auto-status: no actor found for project {}", integration.project);
                            continue;
                        }
                        Err(e) => {
                            tracing::error!("git auto-status: resolve actor failed: {:?}", e);
                            continue;
                        }
                    };

                    // in_review/review/in-review slug を取得
                    match crate::infrastructure::repositories::workflow_status_repo::resolve_review_status_slug(
                        &state.pool, integration.project
                    )
                    .await
                    {
                        Ok(Some(target_slug)) => {
                            if let Err(e) = git_webhook_service::apply_git_status_transition(
                                &state.pool, ticket_id, &target_slug, actor_user_id, "pr_opened"
                            )
                            .await
                            {
                                tracing::error!("git auto-status: apply failed for ticket {}: {:?}", ticket_id, e);
                            }
                        }
                        Ok(None) => {
                            tracing::debug!("git auto-status: no review status slug found for project {}", integration.project);
                        }
                        Err(e) => {
                            tracing::error!("git auto-status: resolve review slug failed: {:?}", e);
                        }
                    }
                }

                // PR merged の場合だけ status 遷移を試みる
                if integration.auto_status_transition && pr_state == "merged" {
                    let actor_user_id = match git_webhook_service::resolve_git_actor(
                        &state.pool,
                        integration.created_by.as_ref().map(|u| u.id),
                        integration.project,
                    )
                    .await
                    {
                        Ok(Some(actor)) => actor.user_id,
                        Ok(None) => {
                            tracing::error!("git auto-status: no actor found for project {}", integration.project);
                            continue;
                        }
                        Err(e) => {
                            tracing::error!("git auto-status: resolve actor failed: {:?}", e);
                            continue;
                        }
                    };

                    // completed カテゴリの先頭 slug を取得
                    match crate::infrastructure::repositories::workflow_status_repo::resolve_status_slug_by_category(
                        &state.pool, integration.project, "completed"
                    )
                    .await
                    {
                        Ok(Some(target_slug)) => {
                            if let Err(e) = git_webhook_service::apply_git_status_transition(
                                &state.pool, ticket_id, &target_slug, actor_user_id, "pr_merged"
                            )
                            .await
                            {
                                tracing::error!("git auto-status: apply failed for ticket {}: {:?}", ticket_id, e);
                            }
                        }
                        Ok(None) => {
                            // completed カテゴリがない場合はフォールバック
                            if let Err(e) = git_webhook_service::apply_git_status_transition(
                                &state.pool, ticket_id, "closed", actor_user_id, "pr_merged"
                            )
                            .await
                            {
                                tracing::error!("git auto-status: apply fallback failed for ticket {}: {:?}", ticket_id, e);
                            }
                        }
                        Err(e) => {
                            tracing::error!("git auto-status: resolve completed slug failed: {:?}", e);
                        }
                    }
                }
            }
            Err(e) => tracing::error!("DB operation failed: {:?}", e),
        }
    }

    created
}
