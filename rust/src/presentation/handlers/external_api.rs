/// presentation/handlers/external_api.rs — 外部API(X-API-Keyヘッダー認証)
///
/// Django apps/api/views/external.py, apps/api/auth.py の移植。
/// JWT/セッション認証を使わず、X-API-Keyヘッダーの値をWIP_API_KEYと比較する。
/// 認証成功時はWIP_API_USER(デフォルト"管理者")のユーザーとして操作する。
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::infrastructure::repositories::{ticket_repo, user_repo};
use crate::presentation::state::AppState;

/// APIキーを検証し、成功時は操作主体のuser_idを返す。
async fn authenticate(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<i32, (StatusCode, serde_json::Value)> {
    let api_key = headers
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let expected_key = match &state.config.wip_api_key {
        Some(k) => k,
        None => {
            return Err((
                StatusCode::UNAUTHORIZED,
                json!({"error": "WIP_API_KEY が設定されていません"}),
            ));
        }
    };

    if api_key.is_empty() || !constant_time_eq(api_key.as_bytes(), expected_key.as_bytes()) {
        return Err((
            StatusCode::UNAUTHORIZED,
            json!({"error": "APIキーが無効です"}),
        ));
    }

    let user = user_repo::find_by_username(&state.pool, &state.config.wip_api_user)
        .await
        .map_err(|e| {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"error": "サーバーエラーが発生しました"}),
            )
        })?;

    let user = match user {
        Some(u) => u,
        None => {
            let fallback = user_repo::find_first_staff_user(&state.pool)
                .await
                .map_err(|e| {
                    tracing::error!("DB operation failed: {:?}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        json!({"error": "サーバーエラーが発生しました"}),
                    )
                })?;
            match fallback {
                Some(u) => u,
                None => {
                    return Err((
                        StatusCode::UNAUTHORIZED,
                        json!({"error": "APIユーザーが見つかりません"}),
                    ));
                }
            }
        }
    };

    Ok(user.id)
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

#[derive(Deserialize)]
pub struct CreateExternalTicketIn {
    pub project_prefix: Option<String>,
    pub title: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_priority")]
    pub priority: String,
    #[serde(default = "default_ticket_type")]
    pub ticket_type: String,
    pub due_date: Option<chrono::NaiveDate>,
}
fn default_priority() -> String {
    "medium".to_string()
}
fn default_ticket_type() -> String {
    "task".to_string()
}

/// POST /api/v1/external/tickets/
pub async fn create_ticket(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateExternalTicketIn>,
) -> impl IntoResponse {
    let author_id = match authenticate(&state, &headers).await {
        Ok(id) => id,
        Err((status, payload)) => return (status, Json(payload)).into_response(),
    };

    let project_prefix = match &body.project_prefix {
        Some(p) if !p.is_empty() => p,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "project_prefix と title は必須です"})),
            )
                .into_response();
        }
    };
    let title = match &body.title {
        Some(t) if !t.is_empty() => t,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "project_prefix と title は必須です"})),
            )
                .into_response();
        }
    };

    match ticket_repo::api_create_external(
        &state.pool,
        project_prefix,
        title,
        &body.description,
        &body.priority,
        &body.ticket_type,
        body.due_date,
        author_id,
    )
    .await
    {
        Ok(Some(r)) => (
            StatusCode::CREATED,
            Json(json!({
                "id": r.id,
                "ticket_key": r.ticket_key,
                "title": title,
                "project": r.project_name,
                "status": r.status,
                "url": format!("/tickets/{}/", r.ticket_key),
            })),
        )
            .into_response(),
        Ok(None) => {
            let available = ticket_repo::list_project_prefixes(&state.pool)
                .await
                .unwrap_or_default();
            (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": format!("プロジェクト '{}' が見つかりません", project_prefix),
                    "available_projects": available.into_iter().map(|(p, n)| json!({"prefix": p, "name": n})).collect::<Vec<_>>(),
                })),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "サーバーエラーが発生しました"})),
            )
                .into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct CreateExternalCommentIn {
    pub comment: Option<String>,
}

/// POST /api/v1/external/tickets/{ticket_key}/comments/
pub async fn create_comment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(ticket_key): Path<String>,
    Json(body): Json<CreateExternalCommentIn>,
) -> impl IntoResponse {
    let author_id = match authenticate(&state, &headers).await {
        Ok(id) => id,
        Err((status, payload)) => return (status, Json(payload)).into_response(),
    };

    let comment_body = match &body.comment {
        Some(c) if !c.is_empty() => c,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "comment は必須です"})),
            )
                .into_response();
        }
    };

    let ticket_id = match ticket_repo::resolve_ticket_id(&state.pool, &ticket_key).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": format!("チケット '{}' が見つかりません", ticket_key)})),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "サーバーエラーが発生しました"})),
            )
                .into_response();
        }
    };

    match ticket_repo::api_add_comment(
        &state.pool,
        ticket_id,
        author_id,
        comment_body,
        None,
        None,
        None,
    )
    .await
    {
        Ok(comment) => (
            StatusCode::CREATED,
            Json(json!({
                "id": comment.id,
                "ticket_key": ticket_key,
                "created_at": comment.created_at.to_rfc3339(),
                "url": format!("/tickets/{}/", ticket_key),
            })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "サーバーエラーが発生しました"})),
            )
                .into_response()
        }
    }
}
