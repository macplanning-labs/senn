/// presentation/handlers/integration_api.rs — Git連携 JSON API
///
/// integrations CRUD(JWT認証必須)+ GitHub Webhook受信(認証不要、HMAC署名検証)。
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::domain::access::Viewer;
use crate::domain::models::integration_api::*;
use crate::domain::services::git_key_scan;
use crate::domain::services::git_webhook_service::{self, GitTrigger, RuleResolution};
use crate::infrastructure::repositories::git_status_rule_repo;
use crate::infrastructure::repositories::integration_repo;
use crate::presentation::state::AppState;

fn err(detail: &str) -> serde_json::Value {
    json!({"detail": detail})
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub project: Option<i32>,
    pub team: Option<i32>,
}

/// GET /api/v1/integrations/?project=<id> または ?team=<id>
pub async fn list(
    State(state): State<AppState>,
    viewer: Viewer,
    Query(params): Query<ListQuery>,
) -> impl IntoResponse {
    if let Err(r) = crate::presentation::handlers::access_guard::require_scope_manager_v(
        &state.pool,
        &viewer,
        params.project,
        params.team,
        "GET /api/v1/integrations/",
    )
    .await
    {
        return r;
    }

    match integration_repo::find_all(&state.pool, params.project, params.team).await {
        Ok(items) => (StatusCode::OK, Json(items)).into_response(),
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

/// POST /api/v1/integrations/
pub async fn create(
    State(state): State<AppState>,
    viewer: Viewer,
    Json(body): Json<GitIntegrationWriteIn>,
) -> impl IntoResponse {
    if let Err(r) = crate::presentation::handlers::access_guard::require_scope_manager_v(
        &state.pool,
        &viewer,
        body.project,
        body.team,
        "POST /api/v1/integrations/",
    )
    .await
    {
        return r;
    }

    match integration_repo::create(
        &state.pool,
        &body,
        match viewer.require_user_id() {
            Ok(id) => id,
            Err(resp) => return resp,
        },
    )
    .await
    {
        Ok(id) => match integration_repo::find_by_id(&state.pool, id).await {
            Ok(Some(item)) => (StatusCode::CREATED, Json(item)).into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response(),
        },
        Err(e) if e.to_string().contains("exactly one of project or team") => (
            StatusCode::BAD_REQUEST,
            Json(err("project か team のどちらか一方が必要です")),
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

/// PATCH /api/v1/integrations/{id}/
pub async fn update(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
    Json(body): Json<GitIntegrationUpdateIn>,
) -> impl IntoResponse {
    let existing = match integration_repo::find_by_id(&state.pool, id).await {
        Ok(Some(item)) => item,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(err("見つかりません"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
    };
    if let Err(r) = crate::presentation::handlers::access_guard::require_scope_manager_v(
        &state.pool,
        &viewer,
        existing.project,
        existing.team,
        "PATCH /api/v1/integrations/{id}/",
    )
    .await
    {
        return r;
    }
    // 付け先を変えるときは、変更先も管理できること(管理していない別チームへ移させない。DEMO-000169)。
    // project / team のどちらかが指定されたら、付け先は (body.project, body.team) になる(integration_repo::update)
    if body.project.is_some() || body.team.is_some() {
        let target = (body.project, body.team);
        if target != (existing.project, existing.team) {
            if let Err(r) = crate::presentation::handlers::access_guard::require_scope_manager_v(
                &state.pool,
                &viewer,
                target.0,
                target.1,
                "PATCH /api/v1/integrations/{id}/",
            )
            .await
            {
                return r;
            }
        }
    }

    match integration_repo::update(&state.pool, id, &body).await {
        Ok(true) => match integration_repo::find_by_id(&state.pool, id).await {
            Ok(Some(item)) => (StatusCode::OK, Json(item)).into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response(),
        },
        Ok(false) => (StatusCode::NOT_FOUND, Json(err("見つかりません"))).into_response(),
        Err(e) if e.to_string().contains("exactly one of project or team") => (
            StatusCode::BAD_REQUEST,
            Json(err("project か team のどちらか一方が必要です")),
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

/// DELETE /api/v1/integrations/{id}/
pub async fn delete(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    let existing = match integration_repo::find_by_id(&state.pool, id).await {
        Ok(Some(item)) => item,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(err("見つかりません"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
    };
    if let Err(r) = crate::presentation::handlers::access_guard::require_scope_manager_v(
        &state.pool,
        &viewer,
        existing.project,
        existing.team,
        "DELETE /api/v1/integrations/{id}/",
    )
    .await
    {
        return r;
    }

    match integration_repo::delete(&state.pool, id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, Json(err("見つかりません"))).into_response(),
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

/// GET /api/v1/integrations/{id}/deliveries/ — 受信履歴(新しい順、直近 20 件)
pub async fn deliveries(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    let existing = match integration_repo::find_by_id(&state.pool, id).await {
        Ok(Some(item)) => item,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(err("見つかりません"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
    };
    if let Err(r) = crate::presentation::handlers::access_guard::require_scope_manager_v(
        &state.pool,
        &viewer,
        existing.project,
        existing.team,
        "GET /api/v1/integrations/{id}/deliveries/",
    )
    .await
    {
        return r;
    }
    match crate::infrastructure::github_app::store::list_history(&state.pool, id, 20).await {
        Ok(rows) => {
            let out: Vec<serde_json::Value> = rows
                .into_iter()
                .map(|(event, result, note, received_at)| {
                    json!({"event": event, "result": result, "note": note, "receivedAt": received_at})
                })
                .collect();
            (StatusCode::OK, Json(out)).into_response()
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

// =============================================================================
// GitHub Webhook 受信(認証不要)
// =============================================================================

/// POST /api/v1/webhooks/github/
pub async fn github_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let event_type = headers
        .get("X-GitHub-Event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let signature = headers
        .get("X-Hub-Signature-256")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

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

    let integrations =
        match integration_repo::find_active_integrations_by_repo_url(&state.pool, repo_url).await {
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

    let verified = integrations.iter().find(|i| {
        git_webhook_service::verify_github_signature(&body, signature, &i.webhook_secret)
    });

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

    let created_count = dispatch_git_event(&state, &integration, event_type, &payload).await;
    let (result, note) = delivery_outcome(&state, event_type, &payload, created_count).await;
    crate::infrastructure::github_app::store::record_delivery(
        &state.pool,
        integration.id,
        event_type,
        result,
        &note,
    )
    .await;

    tracing::info!(
        "Processed {} event: {} events linked",
        event_type,
        created_count
    );
    (
        StatusCode::OK,
        format!("OK: {} events linked", created_count),
    )
        .into_response()
}

/// イベント種別に応じて紐付け・ステータス遷移を行う(手動 Webhook と App Webhook で共通)。
/// 戻り値は、紐付けたイベント数。
pub(crate) async fn dispatch_git_event(
    state: &AppState,
    integration: &crate::domain::models::integration_api::GitIntegrationOut,
    event_type: &str,
    payload: &serde_json::Value,
) -> i32 {
    match event_type {
        "push" => process_push_event(state, integration, payload).await,
        "pull_request" => process_pull_request_event(state, integration, payload).await,
        _ => 0,
    }
}

/// 処理結果の診断(画面に出す「最後に何が起きたか」)。(result, note) を返す。
/// result: "ok" | "no_ticket" | "ignored"
pub(crate) async fn delivery_outcome(
    state: &AppState,
    event_type: &str,
    payload: &serde_json::Value,
    created: i32,
) -> (&'static str, String) {
    if !matches!(event_type, "push" | "pull_request") {
        return ("ignored", format!("{event_type} は連携の対象外です"));
    }
    if created > 0 {
        return ("ok", String::new());
    }
    let known_prefixes = known_prefixes_or_empty(state).await;
    let keys: Vec<String> =
        git_webhook_service::scan_payload_keys(event_type, payload, &known_prefixes)
            .into_iter()
            .map(|f| f.key)
            .collect();
    if keys.is_empty() {
        // 番号が無いのが正常なもの(タグ・main/develop などへの push・それらから出た PR)は、警告にしない
        if let Some(note) = git_webhook_service::untracked_source_note(event_type, payload) {
            return ("ignored", note);
        }
        return (
            "no_ticket",
            "コミット・PRタイトル・ブランチ名にチケット番号(例: SENN-123)が含まれていません"
                .to_string(),
        );
    }
    for key in &keys {
        if let Ok(Some(_)) = integration_repo::find_ticket_id_by_key(&state.pool, key).await {
            return ("ok", "記録済みです".to_string());
        }
    }
    (
        "no_ticket",
        format!("チケット {} が見つかりません", keys[0]),
    )
}

async fn process_push_event(
    state: &AppState,
    integration: &crate::domain::models::integration_api::GitIntegrationOut,
    payload: &serde_json::Value,
) -> i32 {
    let mut created = 0;
    let commits = payload
        .get("commits")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let branch = payload
        .get("ref")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .replace("refs/heads/", "");

    // 同一 push 全体で ticket ごとの status 遷移は1回
    let mut processed_tickets = std::collections::HashSet::new();
    let known_prefixes = known_prefixes_or_empty(state).await;

    for commit in &commits {
        let message = commit.get("message").and_then(|v| v.as_str()).unwrap_or("");
        let sha = commit.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let author_name = commit
            .get("author")
            .and_then(|a| a.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let commit_url = commit.get("url").and_then(|v| v.as_str()).unwrap_or("");

        // ブランチ名とコミットメッセージからキーを拾う(小文字ブランチ名・Closes/Refs に対応。設計 §4)
        let found_keys = git_key_scan::scan_keys(
            &git_key_scan::KeySources {
                branch: &branch,
                title: "",
                body: "",
                commit_messages: &[message],
            },
            &known_prefixes,
        );

        let title: String = message
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .take(500)
            .collect();

        for found in &found_keys {
            let key = &found.key;
            let ticket_id = match integration_repo::find_ticket_id_by_key(&state.pool, key).await {
                Ok(Some(id)) => id,
                Ok(None) => continue,
                Err(e) => {
                    tracing::error!("DB operation failed: {:?}", e);
                    continue;
                }
            };

            match integration_repo::create_commit_event_if_new(
                &state.pool,
                integration.id,
                ticket_id,
                sha,
                &title,
                commit_url,
                &branch,
                author_name,
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

            // Status 遷移（フラグ ON・同一 push 内 ticket 1回）。Refs のキーは紐付けのみで、遷移しない
            if !integration.auto_status_transition || !git_key_scan::allows_transition(found.intent)
            {
                continue;
            }
            if !processed_tickets.insert(ticket_id) {
                continue;
            }

            try_auto_status_for_ticket(state, integration, ticket_id, GitTrigger::BranchPushed)
                .await;
        }
    }

    created
}

/// PR の状態が変わった(レビュー結果・CI)とき、その PR に紐付いたチケットでトリガーを動かす。
/// 連携の自動遷移が OFF のとき、Refs(紐付けのみ)のチケットは動かさない。動かしたチケットの数を返す。
pub(crate) async fn fire_pr_trigger(
    state: &AppState,
    integration: &crate::domain::models::integration_api::GitIntegrationOut,
    pr_number: i32,
    trigger: GitTrigger,
) -> usize {
    if !integration.auto_status_transition {
        return 0;
    }
    let tickets =
        match integration_repo::pr_event_tickets(&state.pool, integration.id, pr_number).await {
            Ok(t) => t,
            Err(e) => {
                tracing::error!("DB operation failed: {:?}", e);
                return 0;
            }
        };
    let mut fired = 0;
    for (ticket_id, intent) in tickets {
        if intent == git_key_scan::KeyIntent::Reference.as_str() {
            continue;
        }
        try_auto_status_for_ticket(state, integration, ticket_id, trigger).await;
        fired += 1;
    }
    fired
}

/// 既存のプロジェクト/チームの prefix(小文字ブランチ名からキーを拾うときの検証用)。取れなければ空。
async fn known_prefixes_or_empty(state: &AppState) -> std::collections::HashSet<String> {
    match git_status_rule_repo::known_ticket_prefixes(&state.pool).await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("known prefixes load failed: {:?}", e);
            std::collections::HashSet::new()
        }
    }
}

/// チケット単位で、トリガーに対する規則(プロジェクト → チーム → 標準)を解決して遷移する。
/// 失敗しても他チケットへ影響しない。
async fn try_auto_status_for_ticket(
    state: &AppState,
    integration: &crate::domain::models::integration_api::GitIntegrationOut,
    ticket_id: i32,
    trigger: GitTrigger,
) {
    let actor_user_id = match git_webhook_service::resolve_git_actor(
        &state.pool,
        integration.created_by.as_ref().map(|u| u.id),
        integration.project,
        integration.team,
    )
    .await
    {
        Ok(Some(actor)) => actor.user_id,
        Ok(None) => {
            tracing::warn!(
                "git auto-status: no actor for integration {} (project={:?} team={:?}); skip ticket {}",
                integration.id,
                integration.project,
                integration.team,
                ticket_id
            );
            return;
        }
        Err(e) => {
            tracing::error!(
                "git auto-status: resolve actor failed for ticket {}: {:?}",
                ticket_id,
                e
            );
            return;
        }
    };

    let target_slug = match git_webhook_service::resolve_trigger_target(
        &state.pool,
        ticket_id,
        trigger,
    )
    .await
    {
        Ok(RuleResolution::Set(slug)) => slug,
        // 「何もしない」(標準で何もしない、または規則で選んだ)
        Ok(RuleResolution::Nothing) => return,
        Ok(RuleResolution::MissingStatus(what)) => {
            tracing::info!(
                "git auto-status: target status {:?} is not in the workflow of ticket {} (trigger={}); skip",
                what,
                ticket_id,
                trigger.as_str()
            );
            return;
        }
        Err(e) => {
            tracing::error!(
                "git auto-status: resolve rule failed for ticket {} (trigger={}): {:?}",
                ticket_id,
                trigger.as_str(),
                e
            );
            return;
        }
    };

    // 通知に載せる「変更前のステータス」
    let old_status: Option<String> =
        sqlx::query_scalar("SELECT status FROM tickets_ticket WHERE id = $1")
            .bind(ticket_id)
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten();

    match git_webhook_service::apply_git_status_transition(
        &state.pool,
        ticket_id,
        &target_slug,
        actor_user_id,
        trigger.as_str(),
    )
    .await
    {
        Ok(git_webhook_service::StatusTransitionResult::Applied) => {
            // 自動で進んだ変更も、ウォッチャーとチャット連携に知らせる。失敗しても遷移は成功のまま
            if let Err(e) =
                crate::domain::services::notification_service::notify_status_change_from_git(
                    &state.pool,
                    &state.mail_sender,
                    ticket_id,
                    actor_user_id,
                    old_status.as_deref().unwrap_or(""),
                    &target_slug,
                    &format!("GitHub: {}", trigger.label_ja()),
                )
                .await
            {
                tracing::warn!(
                    "git auto-status: notify failed for ticket {}: {:?}",
                    ticket_id,
                    e
                );
            }
        }
        Ok(_) => {}
        Err(e) => {
            tracing::error!(
                "git auto-status: apply failed for ticket {}: {:?}",
                ticket_id,
                e
            );
        }
    }
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

    let title: String = pr
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .chars()
        .take(500)
        .collect();
    let pr_url = pr.get("html_url").and_then(|v| v.as_str()).unwrap_or("");
    let pr_number = pr.get("number").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    let head_branch = pr
        .get("head")
        .and_then(|h| h.get("ref"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let author_name = pr
        .get("user")
        .and_then(|u| u.get("login"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let author_avatar = pr
        .get("user")
        .and_then(|u| u.get("avatar_url"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let pr_state = if pr.get("merged").and_then(|v| v.as_bool()).unwrap_or(false) {
        "merged"
    } else if pr.get("state").and_then(|v| v.as_str()) == Some("closed") {
        "closed"
    } else {
        "open"
    };

    // タイトル・本文・ブランチ名からキーを拾う(小文字ブランチ名・PR 本文・Closes/Refs に対応。設計 §4)
    let body = pr.get("body").and_then(|v| v.as_str()).unwrap_or("");
    let known_prefixes = known_prefixes_or_empty(state).await;
    let found_keys = git_key_scan::scan_keys(
        &git_key_scan::KeySources {
            branch: head_branch,
            title: &title,
            body,
            commit_messages: &[],
        },
        &known_prefixes,
    );
    let triggers = git_webhook_service::pr_triggers(action, draft, pr_state);
    let details = integration_repo::PrDetails {
        is_draft: draft,
        reviewers: pr
            .get("requested_reviewers")
            .and_then(|v| v.as_array())
            .map(|rs| {
                rs.iter()
                    .filter_map(|r| r.get("login").and_then(|v| v.as_str()))
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default(),
        additions: pr
            .get("additions")
            .and_then(|v| v.as_i64())
            .map(|n| n as i32),
        deletions: pr
            .get("deletions")
            .and_then(|v| v.as_i64())
            .map(|n| n as i32),
        changed_files: pr
            .get("changed_files")
            .and_then(|v| v.as_i64())
            .map(|n| n as i32),
        base_branch: pr
            .get("base")
            .and_then(|b| b.get("ref"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .chars()
            .take(200)
            .collect(),
    };

    for found in &found_keys {
        let key = &found.key;
        let ticket_id = match integration_repo::find_ticket_id_by_key(&state.pool, key).await {
            Ok(Some(id)) => id,
            Ok(None) => continue,
            Err(e) => {
                tracing::error!("DB operation failed: {:?}", e);
                continue;
            }
        };

        match integration_repo::upsert_pr_event(
            &state.pool,
            integration.id,
            ticket_id,
            pr_number,
            &title,
            pr_url,
            head_branch,
            author_name,
            author_avatar,
            pr_state,
            &details,
            found.intent.as_str(),
        )
        .await
        {
            Ok(_created_new) => {
                created += 1;
                tracing::info!("Linked PR #{} to {}", pr_number, key);

                // トリガーごとに、規則(プロジェクト → チーム → 標準)で遷移させる。
                // Refs のキーは紐付けのみで、遷移しない
                if integration.auto_status_transition
                    && git_key_scan::allows_transition(found.intent)
                {
                    for trigger in &triggers {
                        try_auto_status_for_ticket(state, integration, ticket_id, *trigger).await;
                    }
                }
            }
            Err(e) => tracing::error!("DB operation failed: {:?}", e),
        }
    }

    created
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        add_test_team_member, create_test_project, create_test_team, create_test_user, test_pool,
        test_state, test_viewer,
    };

    async fn integration_on_team(pool: &sqlx::PgPool, team: i32, by: i32) -> i32 {
        sqlx::query_scalar(
            "INSERT INTO t_git_integration
                (project_id, team_id, provider, repository_url, webhook_secret, is_active, auto_status_transition, created_by_id, created_at)
             VALUES (NULL, $1, 'github', 'https://example.test/r', 's', true, true, $2, NOW())
             RETURNING id::int4",
        )
        .bind(team)
        .bind(by)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn scope_of(pool: &sqlx::PgPool, id: i32) -> (Option<i32>, Option<i32>) {
        sqlx::query_as(
            "SELECT project_id::int4, team_id::int4 FROM t_git_integration WHERE id = $1",
        )
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn patch(pool: &sqlx::PgPool, user: i32, id: i32, body: serde_json::Value) -> StatusCode {
        let state = test_state(pool).await;
        let viewer = test_viewer(pool, user).await;
        let body: GitIntegrationUpdateIn = serde_json::from_value(body).unwrap();
        update(State(state), viewer, Path(id), Json(body))
            .await
            .into_response()
            .status()
    }

    /// 管理しているチームの連携を、管理していない別チームへ移せない(DEMO-000169)
    #[tokio::test]
    async fn cannot_move_integration_to_unmanaged_team() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let me = create_test_user(&pool, "gi-me").await;
        let other_owner = create_test_user(&pool, "gi-oo").await;
        let mine = create_test_team(&pool, "gi-a").await;
        let theirs = create_test_team(&pool, "gi-b").await;
        add_test_team_member(&pool, mine, me, "admin").await;
        add_test_team_member(&pool, theirs, other_owner, "admin").await;
        let id = integration_on_team(&pool, mine, me).await;

        let status = patch(&pool, me, id, json!({ "team": theirs })).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(scope_of(&pool, id).await, (None, Some(mine)));
    }

    /// 管理していないプロジェクトへも移せない
    #[tokio::test]
    async fn cannot_move_integration_to_unmanaged_project() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let me = create_test_user(&pool, "gi-me").await;
        let mine = create_test_team(&pool, "gi-a").await;
        add_test_team_member(&pool, mine, me, "admin").await;
        let project = create_test_project(&pool, "GIP", me).await;
        let id = integration_on_team(&pool, mine, me).await;

        let status = patch(&pool, me, id, json!({ "project": project })).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(scope_of(&pool, id).await, (None, Some(mine)));
    }

    /// 両方を管理していれば移せる(付け先を変えない更新も、今までどおり通る)
    #[tokio::test]
    async fn can_move_integration_between_managed_teams() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let me = create_test_user(&pool, "gi-me").await;
        let a = create_test_team(&pool, "gi-a").await;
        let b = create_test_team(&pool, "gi-b").await;
        add_test_team_member(&pool, a, me, "admin").await;
        add_test_team_member(&pool, b, me, "admin").await;
        let id = integration_on_team(&pool, a, me).await;

        assert_eq!(
            patch(&pool, me, id, json!({ "is_active": false })).await,
            StatusCode::OK
        );
        assert_eq!(
            patch(&pool, me, id, json!({ "team": a })).await,
            StatusCode::OK
        );
        assert_eq!(
            patch(&pool, me, id, json!({ "team": b })).await,
            StatusCode::OK
        );
        assert_eq!(scope_of(&pool, id).await, (None, Some(b)));
    }

    /// 受信履歴は、その連携を管理できる人だけが見られる
    #[tokio::test]
    async fn deliveries_are_visible_to_managers_only() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let state = test_state(&pool).await;
        let admin = create_test_user(&pool, "dl-admin").await;
        let outsider = create_test_user(&pool, "dl-out").await;
        let team = create_test_team(&pool, "dl-team").await;
        add_test_team_member(&pool, team, admin, "admin").await;
        let id = integration_on_team(&pool, team, admin).await;
        crate::infrastructure::github_app::store::record_delivery(&pool, id, "push", "ok", "確認")
            .await;

        let r = deliveries(
            State(state.clone()),
            test_viewer(&pool, admin).await,
            Path(id),
        )
        .await
        .into_response();
        assert_eq!(r.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(r.into_body(), 1 << 20).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v.as_array().unwrap().len(), 1);
        assert_eq!(v[0]["event"], "push");
        assert_eq!(v[0]["note"], "確認");

        let r = deliveries(
            State(state.clone()),
            test_viewer(&pool, outsider).await,
            Path(id),
        )
        .await
        .into_response();
        assert_eq!(r.status(), StatusCode::FORBIDDEN);
        let r = deliveries(
            State(state),
            test_viewer(&pool, admin).await,
            Path(2_000_000_000),
        )
        .await
        .into_response();
        assert_eq!(r.status(), StatusCode::NOT_FOUND);
    }
}
