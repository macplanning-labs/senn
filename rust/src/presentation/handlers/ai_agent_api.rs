/// presentation/handlers/ai_agent_api.rs — AI専用外部API(X-AI-Api-Keyヘッダー認証)
///
/// external_api.rs(WIP_API_KEY)と同じ「共有キー→固定ユーザーとして操作」の形だが、
/// 鍵とユーザーを完全に分離する: Claude等のAIエージェントが起こした操作を、
/// 人間/他システムからのWIP_API_KEY操作と別アカウント・別鍵として区別できるようにする。
/// 対象: プロジェクト作成、チケット作成(ラベル・担当者は名前/ユーザー名指定)。

use axum::{
    extract::{State, Query, Path, Multipart},
    response::IntoResponse,
    http::{StatusCode, HeaderMap, HeaderValue},
    Json,
};
use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::presentation::state::AppState;
use crate::domain::models::resource_api::ProjectWriteIn;
use crate::domain::models::ticket_api::{TicketWriteIn, TicketPatchIn};
use crate::domain::models::membership_api::MembershipCreateIn;
use crate::domain::models::cycle_api::CycleWriteIn;
use crate::infrastructure::repositories::{resource_repo, ticket_repo, user_repo, membership_repo, wiki_repo, cycle_repo, wiki_attachment_repo};

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

    match resource_repo::create_project(&state.pool, &input, None).await {
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
    pub page: Option<i64>,
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

    let page = params.page.unwrap_or(1).max(1);

    let tickets = match ticket_repo::api_find_all(&state.pool, &filter, "-updated_at", None, page).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("[AIエージェントAPI/チケット一覧] 処理=チケット取得 結果=失敗 影響=一覧を返せない | {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    let count = match ticket_repo::api_count_all(&state.pool, &filter, None).await {
        Ok(c) => c,
        Err(e) => {
            // 件数取得の失敗は一覧そのものの失敗ではないので、ヘッダー無しで返す（フェイルソフト）
            tracing::error!("[AIエージェントAPI/チケット一覧] 処理=件数取得 結果=失敗 影響=X-Total-Countヘッダー省略 | {}", e);
            let mut resp = (StatusCode::OK, Json(tickets)).into_response();
            return resp;
        }
    };

    let mut resp = (StatusCode::OK, Json(tickets)).into_response();
    resp.headers_mut().insert(
        "x-total-count",
        HeaderValue::from_str(&count.to_string()).unwrap_or(HeaderValue::from_static("0")),
    );
    resp.into_response()
}

// ---------------------------------------------------------------------------
// チケット詳細取得
// ---------------------------------------------------------------------------

/// GET /api/v1/ai-agent/tickets/{ticket_key}/
pub async fn get_ticket(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(ticket_key): Path<String>,
) -> impl IntoResponse {
    if let Err((status, payload)) = authenticate_ai(&state, &headers).await {
        return (status, Json(payload)).into_response();
    }

    match ticket_repo::api_find_by_key(&state.pool, &ticket_key).await {
        Ok(Some(ticket)) => (StatusCode::OK, Json(ticket)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, Json(err(format!("チケット '{}' が見つかりません", ticket_key)))).into_response(),
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
// チケット論理削除
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct DeleteTicketQuery {
    pub reason: Option<String>,
}

/// DELETE /api/v1/ai-agent/tickets/{ticket_key}/?reason=...
/// 物理削除ではなく論理削除(status を canceled に変更)。削除理由を監査コメントとして残す。
pub async fn delete_ticket(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(ticket_key): Path<String>,
    Query(params): Query<DeleteTicketQuery>,
) -> impl IntoResponse {
    let ai_user_id = match authenticate_ai(&state, &headers).await {
        Ok(id) => id,
        Err((status, payload)) => return (status, Json(payload)).into_response(),
    };

    let reason = match &params.reason {
        Some(r) if !r.trim().is_empty() => r.clone(),
        _ => return (StatusCode::BAD_REQUEST, Json(err("reason(削除理由)は必須です"))).into_response(),
    };

    let ticket_id = match ticket_repo::resolve_ticket_id(&state.pool, &ticket_key).await {
        Ok(Some(id)) => id,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(err(format!("チケット '{}' が見つかりません", ticket_key)))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    // 監査コメントを先に投稿(ステータス変更が失敗しても、少なくとも削除が試みられた記録は残る)
    let comment_body = format!("[AI Agent] このチケットはAIエージェント経由で削除(ステータス変更: canceled)されました。理由: {}", reason);
    if let Err(e) = ticket_repo::api_add_comment(&state.pool, ticket_id, ai_user_id, &comment_body).await {
        tracing::error!("Failed to post audit comment: {:?}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("監査コメントの投稿に失敗しました"))).into_response();
    }

    let patch_in = crate::domain::models::ticket_api::TicketPatchIn {
        status: Some("canceled".to_string()),
        ..Default::default()
    };

    let mut tx = match state.pool.begin().await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    match ticket_repo::api_patch(&mut tx, &ticket_key, &patch_in, ai_user_id).await {
        Ok(Some(_)) => {
            if let Err(e) = tx.commit().await {
                tracing::error!("transaction commit failed: {:?}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
            }
            match ticket_repo::api_find_by_key(&state.pool, &ticket_key).await {
                Ok(Some(ticket)) => (StatusCode::OK, Json(ticket)).into_response(),
                _ => (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response(),
            }
        }
        Ok(None) => {
            if let Err(e) = tx.rollback().await { tracing::error!("transaction rollback failed: {:?}", e); }
            (StatusCode::NOT_FOUND, Json(err("見つかりません"))).into_response()
        }
        Err(e) => {
            tracing::error!("api_patch failed: {:?}", e);
            if let Err(e) = tx.rollback().await { tracing::error!("transaction rollback failed: {:?}", e); }
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
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

// ---------------------------------------------------------------------------
// Cycle作成
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateAiCycleIn {
    pub project_prefix: Option<String>,
    pub name: Option<String>,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    #[serde(default = "default_cycle_status")]
    pub status: String,
}
fn default_cycle_status() -> String { "planned".to_string() }

/// POST /api/v1/ai-agent/cycles/
pub async fn create_cycle(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateAiCycleIn>,
) -> impl IntoResponse {
    let ai_user_id = match authenticate_ai(&state, &headers).await {
        Ok(id) => id,
        Err((status, payload)) => return (status, Json(payload)).into_response(),
    };

    let project_prefix = match &body.project_prefix {
        Some(p) if !p.is_empty() => p,
        _ => return (StatusCode::BAD_REQUEST, Json(err("project_prefix, name, start_date, end_date は必須です"))).into_response(),
    };
    let name = match &body.name {
        Some(n) if !n.is_empty() => n,
        _ => return (StatusCode::BAD_REQUEST, Json(err("project_prefix, name, start_date, end_date は必須です"))).into_response(),
    };
    let (start_date, end_date) = match (body.start_date, body.end_date) {
        (Some(s), Some(e)) => (s, e),
        _ => return (StatusCode::BAD_REQUEST, Json(err("project_prefix, name, start_date, end_date は必須です"))).into_response(),
    };
    if start_date >= end_date {
        return (StatusCode::BAD_REQUEST, Json(err("start_date は end_date より前である必要があります"))).into_response();
    }

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
            ).into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    let cycle_in = CycleWriteIn {
        project: project_id,
        name: name.clone(),
        start_date,
        end_date,
        status: body.status.clone(),
    };

    match cycle_repo::create_cycle(&state.pool, &cycle_in, ai_user_id).await {
        Ok(cycle_id) => match cycle_repo::find_cycle_by_id(&state.pool, cycle_id).await {
            Ok(Some(cycle)) => (StatusCode::CREATED, Json(cycle)).into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response(),
        },
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// 依存関係参照
// ---------------------------------------------------------------------------

/// GET /api/v1/ai-agent/tickets/{ticket_key}/dependencies/
pub async fn list_ticket_dependencies(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(ticket_key): Path<String>,
) -> impl IntoResponse {
    let _ai_user_id = match authenticate_ai(&state, &headers).await {
        Ok(id) => id,
        Err((status, payload)) => return (status, Json(payload)).into_response(),
    };

    let ticket_id = match ticket_repo::resolve_ticket_id(&state.pool, &ticket_key).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, Json(err(format!("チケット '{}' が見つかりません", ticket_key)))).into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    match ticket_repo::find_dependencies_for_ticket(&state.pool, ticket_id).await {
        Ok(deps) => (StatusCode::OK, Json(deps)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

/// GET /api/v1/ai-agent/projects/{project_prefix}/dependencies/
pub async fn get_project_dependency_graph(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(project_prefix): Path<String>,
) -> impl IntoResponse {
    let _ai_user_id = match authenticate_ai(&state, &headers).await {
        Ok(id) => id,
        Err((status, payload)) => return (status, Json(payload)).into_response(),
    };

    let project_id = match ticket_repo::resolve_project_id_by_prefix(&state.pool, &project_prefix).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            let available = ticket_repo::list_project_prefixes(&state.pool).await.unwrap_or_default();
            return (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": format!("プロジェクト '{}' が見つかりません", project_prefix),
                    "available_projects": available.into_iter().map(|(p, n)| json!({"prefix": p, "name": n})).collect::<Vec<_>>(),
                })),
            ).into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    match ticket_repo::find_dependency_graph_for_project(&state.pool, project_id).await {
        Ok(graph) => (StatusCode::OK, Json(graph)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Wiki添付ファイル
// ---------------------------------------------------------------------------

/// POST /api/v1/ai-agent/wiki-pages/{slug}/attachments/?project_prefix=XXX
pub async fn upload_wiki_attachment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Query(params): Query<WikiPagesQuery>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let ai_user_id = match authenticate_ai(&state, &headers).await {
        Ok(id) => id,
        Err((status, payload)) => return (status, Json(payload)).into_response(),
    };

    let project_prefix = match &params.project_prefix {
        Some(p) if !p.is_empty() => p,
        _ => return (StatusCode::BAD_REQUEST, Json(err("project_prefix は必須です"))).into_response(),
    };

    let project_id = match ticket_repo::resolve_project_id_by_prefix(&state.pool, project_prefix).await {
        Ok(Some(id)) => id,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(err(format!("プロジェクト '{}' が見つかりません", project_prefix)))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    let wiki_page = match wiki_repo::find_by_slug(&state.pool, Some(project_id), &slug).await {
        Ok(Some(p)) => p,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(err(format!("Wikiページ '{}' が見つかりません", slug)))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    let mut original_filename = String::new();
    let mut file_bytes = Vec::new();
    let mut has_file = false;

    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some("file") {
            original_filename = field.file_name().unwrap_or("unnamed").to_string();
            match field.bytes().await {
                Ok(bytes) => { file_bytes = bytes.to_vec(); has_file = true; }
                Err(e) => {
                    tracing::error!("Failed to read file bytes: {}", e);
                    return (StatusCode::BAD_REQUEST, Json(err("ファイルの読み込みに失敗しました"))).into_response();
                }
            }
            break;
        }
    }

    if !has_file || file_bytes.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(err("file フィールドが必要です"))).into_response();
    }

    let file_size = file_bytes.len() as i32;
    let media_dir = std::path::PathBuf::from(&state.config.media_dir);
    let attachments_dir = media_dir.join("wiki_attachments").join(wiki_page.id.to_string());

    if let Err(e) = tokio::fs::create_dir_all(&attachments_dir).await {
        tracing::error!("Failed to create wiki attachment directory: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
    }

    let uuid = Uuid::new_v4().to_string();
    let stored_filename = format!("{}_{}", uuid, original_filename);
    let file_path = attachments_dir.join(&stored_filename);

    if let Err(e) = tokio::fs::write(&file_path, &file_bytes).await {
        tracing::error!("Failed to write file: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
    }

    let relative_path = format!("wiki_attachments/{}/{}", wiki_page.id, stored_filename);

    match wiki_attachment_repo::create(
        &state.pool, wiki_page.id, ai_user_id, &original_filename, &relative_path, file_size,
    ).await {
        Ok(id) => (StatusCode::CREATED, Json(json!({
            "id": id,
            "filename": original_filename,
            "fileSize": file_size,
            "fileUrl": format!("/media/{}", relative_path),
        }))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            let _ = tokio::fs::remove_file(&file_path).await;
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

/// GET /api/v1/ai-agent/wiki-pages/{slug}/attachments/?project_prefix=XXX
pub async fn list_wiki_attachments(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Query(params): Query<WikiPagesQuery>,
) -> impl IntoResponse {
    let _ai_user_id = match authenticate_ai(&state, &headers).await {
        Ok(id) => id,
        Err((status, payload)) => return (status, Json(payload)).into_response(),
    };

    let project_prefix = match &params.project_prefix {
        Some(p) if !p.is_empty() => p,
        _ => return (StatusCode::BAD_REQUEST, Json(err("project_prefix は必須です"))).into_response(),
    };

    let project_id = match ticket_repo::resolve_project_id_by_prefix(&state.pool, project_prefix).await {
        Ok(Some(id)) => id,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(err(format!("プロジェクト '{}' が見つかりません", project_prefix)))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    let wiki_page = match wiki_repo::find_by_slug(&state.pool, Some(project_id), &slug).await {
        Ok(Some(p)) => p,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(err(format!("Wikiページ '{}' が見つかりません", slug)))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    match wiki_attachment_repo::find_by_wiki_page(&state.pool, wiki_page.id).await {
        Ok(list) => (StatusCode::OK, Json(list)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Cycle CRUD (一覧/詳細/更新/削除)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CyclesQuery {
    pub project_prefix: Option<String>,
    pub status: Option<String>,
}

/// GET /api/v1/ai-agent/cycles/?project_prefix=XXX&status=YYY
pub async fn list_cycles(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<CyclesQuery>,
) -> impl IntoResponse {
    if let Err((status, payload)) = authenticate_ai(&state, &headers).await {
        return (status, Json(payload)).into_response();
    }

    let project_id = match &params.project_prefix {
        Some(p) if !p.is_empty() => match ticket_repo::resolve_project_id_by_prefix(&state.pool, p).await {
            Ok(Some(id)) => Some(id),
            Ok(None) => return (StatusCode::NOT_FOUND, Json(err(format!("プロジェクト '{}' が見つかりません", p)))).into_response(),
            Err(e) => {
                tracing::error!("DB operation failed: {:?}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
            }
        },
        _ => None,
    };

    match cycle_repo::find_all_cycles(&state.pool, project_id, params.status.as_deref()).await {
        Ok(cycles) => (StatusCode::OK, Json(cycles)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

/// GET /api/v1/ai-agent/cycles/{id}/
pub async fn get_cycle(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    if let Err((status, payload)) = authenticate_ai(&state, &headers).await {
        return (status, Json(payload)).into_response();
    }

    match cycle_repo::find_cycle_by_id(&state.pool, id).await {
        Ok(Some(cycle)) => (StatusCode::OK, Json(cycle)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, Json(err("サイクルが見つかりません"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct CyclePatchIn {
    pub name: Option<String>,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub status: Option<String>,
}

/// PATCH /api/v1/ai-agent/cycles/{id}/
pub async fn patch_cycle(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
    Json(body): Json<CyclePatchIn>,
) -> impl IntoResponse {
    if let Err((status, payload)) = authenticate_ai(&state, &headers).await {
        return (status, Json(payload)).into_response();
    }

    let existing = match cycle_repo::find_cycle_by_id(&state.pool, id).await {
        Ok(Some(c)) => c,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(err("サイクルが見つかりません"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    let merged = CycleWriteIn {
        project: existing.project,
        name: body.name.clone().unwrap_or(existing.name.clone()),
        start_date: body.start_date.unwrap_or(existing.start_date),
        end_date: body.end_date.unwrap_or(existing.end_date),
        status: body.status.clone().unwrap_or(existing.status.clone()),
    };

    if merged.start_date >= merged.end_date {
        return (StatusCode::BAD_REQUEST, Json(err("start_date は end_date より前である必要があります"))).into_response();
    }

    match cycle_repo::update_cycle(&state.pool, id, &merged).await {
        Ok(true) => match cycle_repo::find_cycle_by_id(&state.pool, id).await {
            Ok(Some(cycle)) => (StatusCode::OK, Json(cycle)).into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response(),
        },
        Ok(false) => (StatusCode::NOT_FOUND, Json(err("サイクルが見つかりません"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

/// DELETE /api/v1/ai-agent/cycles/{id}/
pub async fn delete_cycle(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    if let Err((status, payload)) = authenticate_ai(&state, &headers).await {
        return (status, Json(payload)).into_response();
    }

    match cycle_repo::delete_cycle(&state.pool, id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, Json(err("サイクルが見つかりません"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// 依存関係 追加/削除
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct AddAiDependencyIn {
    pub to_task_key: Option<String>,
    #[serde(default = "default_dependency_type")]
    pub dependency_type: String,
}
fn default_dependency_type() -> String { "blocks".to_string() }

/// POST /api/v1/ai-agent/tickets/{ticket_key}/dependencies/
pub async fn add_dependency(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(ticket_key): Path<String>,
    Json(body): Json<AddAiDependencyIn>,
) -> impl IntoResponse {
    use ticket_repo::CreateDependencyResult;

    let ai_user_id = match authenticate_ai(&state, &headers).await {
        Ok(id) => id,
        Err((status, payload)) => return (status, Json(payload)).into_response(),
    };

    let ticket_id = match ticket_repo::resolve_ticket_id(&state.pool, &ticket_key).await {
        Ok(Some(id)) => id,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(err(format!("チケット '{}' が見つかりません", ticket_key)))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    let to_task_key = match &body.to_task_key {
        Some(k) if !k.is_empty() => k,
        _ => return (StatusCode::BAD_REQUEST, Json(err("to_task_key は必須です"))).into_response(),
    };
    let to_task = match ticket_repo::resolve_ticket_id(&state.pool, to_task_key).await {
        Ok(Some(id)) => id,
        Ok(None) => return (StatusCode::BAD_REQUEST, Json(err(format!("チケット '{}' が見つかりません", to_task_key)))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    let result = ticket_repo::create_dependency(&state.pool, ticket_id, to_task, &body.dependency_type, ai_user_id).await;

    match result {
        Ok(CreateDependencyResult::Success(id)) => match ticket_repo::find_dependency_by_id(&state.pool, id).await {
            Ok(Some(dep)) => (StatusCode::CREATED, Json(dep)).into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response(),
        },
        Ok(CreateDependencyResult::SelfReference) => (StatusCode::BAD_REQUEST, Json(err("自分自身への依存関係は作成できません。"))).into_response(),
        Ok(CreateDependencyResult::Duplicate) => (StatusCode::BAD_REQUEST, Json(err("この依存関係は既に存在します。"))).into_response(),
        Ok(CreateDependencyResult::ToTaskNotFound) => (StatusCode::BAD_REQUEST, Json(err("指定されたチケットが見つかりません。"))).into_response(),
        Ok(CreateDependencyResult::CircularDependency) => (StatusCode::BAD_REQUEST, Json(err("この依存関係を追加すると循環依存になります。"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

/// DELETE /api/v1/ai-agent/tickets/{ticket_key}/dependencies/{dep_id}/
pub async fn delete_dependency(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((ticket_key, dep_id)): Path<(String, i32)>,
) -> impl IntoResponse {
    use ticket_repo::DeleteDependencyResult;

    if let Err((status, payload)) = authenticate_ai(&state, &headers).await {
        return (status, Json(payload)).into_response();
    }

    let ticket_id = match ticket_repo::resolve_ticket_id(&state.pool, &ticket_key).await {
        Ok(Some(id)) => id,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(err(format!("チケット '{}' が見つかりません", ticket_key)))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    match ticket_repo::delete_dependency(&state.pool, dep_id, ticket_id).await {
        Ok(DeleteDependencyResult::Deleted) => StatusCode::NO_CONTENT.into_response(),
        Ok(DeleteDependencyResult::NotFound) | Ok(DeleteDependencyResult::NotRelated) => (
            StatusCode::NOT_FOUND, Json(err("この依存関係は指定チケットに関連していません。"))
        ).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Wiki添付 差し替え/削除
// ---------------------------------------------------------------------------

/// PUT /api/v1/ai-agent/wiki-pages/{slug}/attachments/{attachment_id}/
pub async fn replace_wiki_attachment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((_slug, attachment_id)): Path<(String, i32)>,
    mut multipart: axum::extract::Multipart,
) -> impl IntoResponse {
    let ai_user_id = match authenticate_ai(&state, &headers).await {
        Ok(id) => id,
        Err((status, payload)) => return (status, Json(payload)).into_response(),
    };

    let existing = match crate::infrastructure::repositories::wiki_attachment_repo::find_by_id(&state.pool, attachment_id).await {
        Ok(Some(a)) => a,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(err("添付ファイルが見つかりません"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    let mut original_filename = String::new();
    let mut file_bytes = Vec::new();
    let mut has_file = false;

    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some("file") {
            original_filename = field.file_name().unwrap_or("unnamed").to_string();
            match field.bytes().await {
                Ok(bytes) => { file_bytes = bytes.to_vec(); has_file = true; }
                Err(e) => {
                    tracing::error!("Failed to read file bytes: {}", e);
                    return (StatusCode::BAD_REQUEST, Json(err("ファイルの読み込みに失敗しました"))).into_response();
                }
            }
            break;
        }
    }

    if !has_file || file_bytes.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(err("file フィールドが必要です"))).into_response();
    }

    let file_size = file_bytes.len() as i32;
    let media_dir = std::path::PathBuf::from(&state.config.media_dir);
    let attachments_dir = media_dir.join("wiki_attachments").join(existing.wiki_page_id.to_string());

    if let Err(e) = tokio::fs::create_dir_all(&attachments_dir).await {
        tracing::error!("Failed to create wiki attachment directory: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
    }

    let uuid = Uuid::new_v4().to_string();
    let stored_filename = format!("{}_{}", uuid, original_filename);
    let new_file_path = attachments_dir.join(&stored_filename);

    if let Err(e) = tokio::fs::write(&new_file_path, &file_bytes).await {
        tracing::error!("Failed to write file: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
    }

    let relative_path = format!("wiki_attachments/{}/{}", existing.wiki_page_id, stored_filename);
    let old_file_path = media_dir.join(&existing.file_path);

    match crate::infrastructure::repositories::wiki_attachment_repo::update(
        &state.pool, attachment_id, &original_filename, &relative_path, file_size,
    ).await {
        Ok(true) => {
            let _ = tokio::fs::remove_file(&old_file_path).await;
            (StatusCode::OK, Json(json!({
                "id": attachment_id,
                "filename": original_filename,
                "fileSize": file_size,
                "fileUrl": format!("/media/{}", relative_path),
            }))).into_response()
        }
        Ok(false) => {
            let _ = tokio::fs::remove_file(&new_file_path).await;
            (StatusCode::NOT_FOUND, Json(err("添付ファイルが見つかりません"))).into_response()
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            let _ = tokio::fs::remove_file(&new_file_path).await;
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

/// DELETE /api/v1/ai-agent/wiki-pages/{slug}/attachments/{attachment_id}/
pub async fn delete_wiki_attachment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((_slug, attachment_id)): Path<(String, i32)>,
) -> impl IntoResponse {
    if let Err((status, payload)) = authenticate_ai(&state, &headers).await {
        return (status, Json(payload)).into_response();
    }

    let existing = match crate::infrastructure::repositories::wiki_attachment_repo::find_by_id(&state.pool, attachment_id).await {
        Ok(Some(a)) => a,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(err("添付ファイルが見つかりません"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    match crate::infrastructure::repositories::wiki_attachment_repo::delete(&state.pool, attachment_id).await {
        Ok(true) => {
            let media_dir = std::path::PathBuf::from(&state.config.media_dir);
            let _ = tokio::fs::remove_file(media_dir.join(&existing.file_path)).await;
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => (StatusCode::NOT_FOUND, Json(err("添付ファイルが見つかりません"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// チケット参照リンク(リンク機能)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct AddAiTicketLinkIn {
    pub url: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
}

/// GET /api/v1/ai-agent/tickets/{ticket_key}/links/
pub async fn list_ticket_links(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(ticket_key): Path<String>,
) -> impl IntoResponse {
    if let Err((status, payload)) = authenticate_ai(&state, &headers).await {
        return (status, Json(payload)).into_response();
    }

    let ticket_id = match ticket_repo::resolve_ticket_id(&state.pool, &ticket_key).await {
        Ok(Some(id)) => id,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(err(format!("チケット '{}' が見つかりません", ticket_key)))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    match crate::infrastructure::repositories::ticket_link_repo::find_by_ticket(&state.pool, ticket_id).await {
        Ok(links) => {
            let out: Vec<_> = links.into_iter().map(|l| json!({
                "id": l.id,
                "url": l.url,
                "title": l.title,
                "createdBy": l.created_by_name,
                "createdAt": l.created_at,
            })).collect();
            (StatusCode::OK, Json(out)).into_response()
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

/// POST /api/v1/ai-agent/tickets/{ticket_key}/links/
pub async fn add_ticket_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(ticket_key): Path<String>,
    Json(body): Json<AddAiTicketLinkIn>,
) -> impl IntoResponse {
    let ai_user_id = match authenticate_ai(&state, &headers).await {
        Ok(id) => id,
        Err((status, payload)) => return (status, Json(payload)).into_response(),
    };

    let url = match &body.url {
        Some(u) if !u.trim().is_empty() => u,
        _ => return (StatusCode::BAD_REQUEST, Json(err("url は必須です"))).into_response(),
    };

    let ticket_id = match ticket_repo::resolve_ticket_id(&state.pool, &ticket_key).await {
        Ok(Some(id)) => id,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(err(format!("チケット '{}' が見つかりません", ticket_key)))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    match crate::infrastructure::repositories::ticket_link_repo::create(&state.pool, ticket_id, url, body.title.as_deref(), ai_user_id).await {
        Ok(id) => match crate::infrastructure::repositories::ticket_link_repo::find_by_id(&state.pool, id).await {
            Ok(Some(l)) => (StatusCode::CREATED, Json(json!({
                "id": l.id, "url": l.url, "title": l.title, "createdAt": l.created_at,
            }))).into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response(),
        },
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}

/// DELETE /api/v1/ai-agent/tickets/{ticket_key}/links/{link_id}/
pub async fn delete_ticket_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((ticket_key, link_id)): Path<(String, i32)>,
) -> impl IntoResponse {
    if let Err((status, payload)) = authenticate_ai(&state, &headers).await {
        return (status, Json(payload)).into_response();
    }

    let ticket_id = match ticket_repo::resolve_ticket_id(&state.pool, &ticket_key).await {
        Ok(Some(id)) => id,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(err(format!("チケット '{}' が見つかりません", ticket_key)))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    match crate::infrastructure::repositories::ticket_link_repo::delete(&state.pool, link_id, ticket_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, Json(err("見つかりません"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response()
        }
    }
}
