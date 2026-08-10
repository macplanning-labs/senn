/// presentation/handlers/ai_agent_api.rs — AI専用外部API(X-AI-Api-Keyヘッダー認証)
///
/// external_api.rs(WIP_API_KEY)と同じ「共有キー→固定ユーザーとして操作」の形だが、
/// 鍵とユーザーを完全に分離する: Claude等のAIエージェントが起こした操作を、
/// 人間/他システムからのWIP_API_KEY操作と別アカウント・別鍵として区別できるようにする。
/// 対象: プロジェクト作成、チケット作成(ラベル・担当者は名前/ユーザー名指定)。

use axum::{
    extract::{State, Query, Path},
    response::IntoResponse,
    http::{StatusCode, HeaderMap},
    Json,
};
use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::json;

use crate::presentation::state::AppState;
use crate::domain::models::resource_api::ProjectWriteIn;
use crate::domain::models::ticket_api::{TicketWriteIn, TicketPatchIn};
use crate::domain::models::membership_api::MembershipCreateIn;
use crate::infrastructure::repositories::{resource_repo, ticket_repo, user_repo, membership_repo, wiki_repo};

fn err(detail: impl Into<String>) -> serde_json::Value {
    json!({"error": detail.into()})
}

/// AI用APIキーを検証し、成功時は操作主体(AIエージェント用アカウント)のuser_idを返す。
/// wip_api_key(人間/既存システム用)とは完全に別の鍵・別のユーザーを使う。
/// アカウントが未作成なら、ここで一度だけ自動作成する(パスワードは使用不能ハッシュ '!' でログイン不可)。
async fn authenticate_ai(state: &AppState, headers: &HeaderMap) -> Result<i32, (StatusCode, serde_json::Value)> {
    let api_key = headers
        .get("X-AI-Api-Key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let expected_key = match &state.config.wip_ai_api_key {
        Some(k) => k,
        None => {
            return Err((StatusCode::UNAUTHORIZED, err("WIP_AI_API_KEY が設定されていません")));
        }
    };

    if api_key.is_empty() || !constant_time_eq(api_key.as_bytes(), expected_key.as_bytes()) {
        return Err((StatusCode::UNAUTHORIZED, err("AI用APIキーが無効です")));
    }

    let username = &state.config.wip_ai_api_user;
    let existing = user_repo::find_by_username(&state.pool, username).await.map_err(|e| {
        tracing::error!("DB operation failed: {:?}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, err("サーバーエラーが発生しました"))
    })?;

    if let Some(u) = existing {
        return Ok(u.id);
    }

    // 初回アクセス時にAI専用アカウントを自動作成する。
    // password_hash = "!" はDjangoの「使用不能パスワード」規約(check_passwordは常にfalse)で、
    // このアカウントは対話ログインができない= API経由の操作専用であることを保証する。
    let created = user_repo::create_user(
        &state.pool,
        username,
        "ai-agent@macplanning.local",
        "!",
        "AI",
        "Agent",
    )
    .await
    .map_err(|e| {
        tracing::error!("AI agent account creation failed: {:?}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, err("サーバーエラーが発生しました"))
    })?;

    Ok(created.id)
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

// ---------------------------------------------------------------------------
// プロジェクト作成
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateAiProjectIn {
    pub name: Option<String>,
    pub prefix: Option<String>,
    #[serde(default)]
    pub description: String,
}

/// POST /api/v1/ai-agent/projects/
pub async fn create_project(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateAiProjectIn>,
) -> impl IntoResponse {
    if let Err((status, payload)) = authenticate_ai(&state, &headers).await {
        return (status, Json(payload)).into_response();
    }

    let name = match &body.name {
        Some(n) if !n.is_empty() => n,
        _ => return (StatusCode::BAD_REQUEST, Json(err("name と prefix は必須です"))).into_response(),
    };
    let prefix = match &body.prefix {
        Some(p) if !p.is_empty() => p,
        _ => return (StatusCode::BAD_REQUEST, Json(err("name と prefix は必須です"))).into_response(),
    };

    let input = ProjectWriteIn {
        name: name.clone(),
        prefix: prefix.clone(),
        description: body.description.clone(),
        owner_team: None,
    };

    match resource_repo::create_project(&state.pool, &input).await {
        Ok(project_id) => match resource_repo::find_project_by_id(&state.pool, project_id).await {
            Ok(Some(project)) => (StatusCode::CREATED, Json(project)).into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response(),
        },
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            // prefixのUNIQUE制約違反である可能性が高い(既存プロジェクトと重複)
            (StatusCode::CONFLICT, Json(err(format!("プロジェクトを作成できませんでした(prefix '{}' が既に使われている可能性があります)", prefix)))).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// チケット作成(ラベルは名前指定で自動作成、担当者はユーザー名指定)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateAiTicketIn {
    pub project_prefix: Option<String>,
    pub title: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_priority")]
    pub priority: String,
    #[serde(default = "default_ticket_type")]
    pub ticket_type: String,
    pub due_date: Option<NaiveDate>,
    /// ラベル名(存在しなければ同名でプロジェクト内に新規作成する)
    #[serde(default)]
    pub labels: Vec<String>,
    /// 担当者のユーザー名。プロジェクト未参加なら自動的にメンバーへ追加してから割り当てる。
    #[serde(default)]
    pub assignee_usernames: Vec<String>,
}
fn default_priority() -> String { "medium".to_string() }
fn default_ticket_type() -> String { "task".to_string() }

/// POST /api/v1/ai-agent/tickets/
pub async fn create_ticket(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateAiTicketIn>,
) -> impl IntoResponse {
    let ai_user_id = match authenticate_ai(&state, &headers).await {
        Ok(id) => id,
        Err((status, payload)) => return (status, Json(payload)).into_response(),
    };

    let project_prefix = match &body.project_prefix {
        Some(p) if !p.is_empty() => p,
        _ => return (StatusCode::BAD_REQUEST, Json(err("project_prefix と title は必須です"))).into_response(),
    };
    let title = match &body.title {
        Some(t) if !t.is_empty() => t,
        _ => return (StatusCode::BAD_REQUEST, Json(err("project_prefix と title は必須です"))).into_response(),
    };

    let project_id = match ticket_repo::resolve_project_id_by_prefix(&state.pool, project_prefix).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            let available = ticket_repo::list_project_prefixes(&state.pool).await.unwrap_or_default();
            return (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": format!("プロジェクト '{}' が見つかりません", project_prefix),
                    "available_projects": available.into_iter().map(|(p, n)| json!({"prefix": p, "name": n})).collect::<Vec<_>>(),
                })),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    // ラベルは名前指定 → プロジェクト内で見つからなければ新規作成
    let mut label_ids = Vec::with_capacity(body.labels.len());
    for name in &body.labels {
        match resource_repo::find_or_create_label(&state.pool, project_id, name).await {
            Ok(id) => label_ids.push(id),
            Err(e) => {
                tracing::error!("DB operation failed: {:?}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
            }
        }
    }

    // 担当者はユーザー名 → user_id に解決。未参加ならプロジェクトメンバーに追加してから割り当てる。
    let mut assignee_ids = Vec::with_capacity(body.assignee_usernames.len());
    for username in &body.assignee_usernames {
        let user = match user_repo::find_by_username(&state.pool, username).await {
            Ok(Some(u)) => u,
            Ok(None) => {
                return (StatusCode::BAD_REQUEST, Json(err(format!("ユーザー '{}' が見つかりません", username)))).into_response();
            }
            Err(e) => {
                tracing::error!("DB operation failed: {:?}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
            }
        };

        let is_member = match membership_repo::check_membership_exists(&state.pool, project_id, user.id).await {
            Ok(b) => b,
            Err(e) => {
                tracing::error!("DB operation failed: {:?}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
            }
        };
        if !is_member {
            let membership_in = MembershipCreateIn {
                user_id: user.id,
                project: project_id,
                start_date: chrono::Utc::now().date_naive(),
                end_date: None,
                note: Some("AIエージェントによるチケット割り当てのため自動追加".to_string()),
            };
            if let Err(e) = membership_repo::create_membership(&state.pool, &membership_in, ai_user_id).await {
                tracing::error!("DB operation failed: {:?}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
            }
        }

        assignee_ids.push(user.id);
    }

    let ticket_in = TicketWriteIn {
        title: title.clone(),
        description: body.description.clone(),
        status: "open".to_string(),
        priority: body.priority.clone(),
        ticket_type: body.ticket_type.clone(),
        assignees: assignee_ids,
        category: None,
        project: project_id,
        milestone: None,
        parent: None,
        start_date: None,
        due_date: body.due_date,
        labels: label_ids,
        story_points: None,
        cycle: None,
        assigned_team: None,
        linked_rules: Vec::new(),
    };

    let mut tx = match state.pool.begin().await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    let ticket_id = match ticket_repo::api_create(&mut tx, &ticket_in, ai_user_id).await {
        Ok(id) => id,
        Err(e) => {
            tracing::error!("api_create failed: {:?}", e);
            if let Err(e) = tx.rollback().await { tracing::error!("transaction rollback failed: {:?}", e); }
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    if let Err(e) = tx.commit().await {
        tracing::error!("transaction commit failed: {:?}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
    }

    let ticket_key: Option<String> = sqlx::query_scalar("SELECT ticket_key FROM tickets_ticket WHERE id = $1")
        .bind(ticket_id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

    (
        StatusCode::CREATED,
        Json(json!({
            "id": ticket_id,
            "ticket_key": ticket_key,
            "title": title,
            "url": ticket_key.as_ref().map(|k| format!("/tickets/{}/", k)),
        })),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// プロジェクト一覧(読み取り)
// ---------------------------------------------------------------------------

/// GET /api/v1/ai-agent/projects/
pub async fn list_projects(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err((status, payload)) = authenticate_ai(&state, &headers).await {
        return (status, Json(payload)).into_response();
    }

    const PAGE: i64 = 1;
    match resource_repo::find_all_projects(&state.pool, PAGE).await {
        Ok(projects) => (StatusCode::OK, Json(projects)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Wiki ページ一覧(読み取り)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct WikiPagesQuery {
    pub project_prefix: Option<String>,
}

/// GET /api/v1/ai-agent/wiki-pages/?project_prefix=XXX
pub async fn list_wiki_pages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<WikiPagesQuery>,
) -> impl IntoResponse {
    if let Err((status, payload)) = authenticate_ai(&state, &headers).await {
        return (status, Json(payload)).into_response();
    }

    let project_prefix = match &params.project_prefix {
        Some(p) if !p.is_empty() => p,
        _ => return (StatusCode::BAD_REQUEST, Json(err("project_prefix は必須です"))).into_response(),
    };

    let project_id = match ticket_repo::resolve_project_id_by_prefix(&state.pool, project_prefix).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            let available = ticket_repo::list_project_prefixes(&state.pool).await.unwrap_or_default();
            return (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": format!("プロジェクト '{}' が見つかりません", project_prefix),
                    "available_projects": available.into_iter().map(|(p, n)| json!({"prefix": p, "name": n})).collect::<Vec<_>>(),
                })),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    match wiki_repo::find_by_project(&state.pool, Some(project_id)).await {
        Ok(pages) => (StatusCode::OK, Json(pages)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// チケット一覧(読み取り)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct TicketsQuery {
    pub project_prefix: Option<String>,
}

/// GET /api/v1/ai-agent/tickets/?project_prefix=XXX
pub async fn list_tickets(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<TicketsQuery>,
) -> impl IntoResponse {
    if let Err((status, payload)) = authenticate_ai(&state, &headers).await {
        return (status, Json(payload)).into_response();
    }

    let project_prefix = match &params.project_prefix {
        Some(p) if !p.is_empty() => p,
        _ => return (StatusCode::BAD_REQUEST, Json(err("project_prefix は必須です"))).into_response(),
    };

    let project_id = match ticket_repo::resolve_project_id_by_prefix(&state.pool, project_prefix).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            let available = ticket_repo::list_project_prefixes(&state.pool).await.unwrap_or_default();
            return (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": format!("プロジェクト '{}' が見つかりません", project_prefix),
                    "available_projects": available.into_iter().map(|(p, n)| json!({"prefix": p, "name": n})).collect::<Vec<_>>(),
                })),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    let mut filter = ticket_repo::ApiTicketFilter::default();
    filter.project = Some(project_id);

    match ticket_repo::api_find_all(&state.pool, &filter, "-updated_at", None, 1).await {
        Ok(tickets) => (StatusCode::OK, Json(tickets)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// チケット更新(ステータス・優先度等)
// ---------------------------------------------------------------------------

/// PATCH /api/v1/ai-agent/tickets/{ticket_key}/
/// チケットの状態(status, priority等)を更新する。
pub async fn patch_ticket(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(ticket_key): Path<String>,
    Json(body): Json<TicketPatchIn>,
) -> impl IntoResponse {
    let ai_user_id = match authenticate_ai(&state, &headers).await {
        Ok(id) => id,
        Err((status, payload)) => return (status, Json(payload)).into_response(),
    };

    let _ticket_id = match ticket_repo::resolve_ticket_id(&state.pool, &ticket_key).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(err(format!("チケット '{}' が見つかりません", ticket_key))),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    let mut tx = match state.pool.begin().await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    match ticket_repo::api_patch(&mut tx, &ticket_key, &body, ai_user_id).await {
        Err(e) => {
            tracing::error!("api_patch failed: {:?}", e);
            if let Err(e) = tx.rollback().await { tracing::error!("transaction rollback failed: {:?}", e); }
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
        Ok(Some(_)) => {
            if let Err(e) = tx.commit().await {
                tracing::error!("transaction commit failed: {:?}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(err("サーバーエラーが発生しました")),
                )
                    .into_response();
            }

            match ticket_repo::api_find_by_key(&state.pool, &ticket_key).await {
                Ok(Some(ticket)) => (StatusCode::OK, Json(ticket)).into_response(),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(err("サーバーエラーが発生しました")),
                )
                    .into_response(),
            }
        }
        Ok(None) => {
            if let Err(e) = tx.rollback().await { tracing::error!("transaction rollback failed: {:?}", e); }
            (
                StatusCode::NOT_FOUND,
                Json(err("見つかりません")),
            )
                .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// コメント追加
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct AddAiCommentIn {
    pub body: String,
}

/// POST /api/v1/ai-agent/tickets/{ticket_key}/comments/
/// チケットにコメント(解決ノート等)を追加する。
pub async fn add_comment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(ticket_key): Path<String>,
    Json(body): Json<AddAiCommentIn>,
) -> impl IntoResponse {
    let ai_user_id = match authenticate_ai(&state, &headers).await {
        Ok(id) => id,
        Err((status, payload)) => return (status, Json(payload)).into_response(),
    };

    let ticket_id = match ticket_repo::resolve_ticket_id(&state.pool, &ticket_key).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(err(format!("チケット '{}' が見つかりません", ticket_key))),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    match ticket_repo::api_add_comment(&state.pool, ticket_id, ai_user_id, &body.body).await {
        Ok(comment) => (StatusCode::CREATED, Json(comment)).into_response(),
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
