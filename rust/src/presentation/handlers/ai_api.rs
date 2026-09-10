/// presentation/handlers/ai_api.rs — AI推論 JSON API
///
/// Django apps/api/views/ai.py と挙動を一致させるハンドラー。

use axum::{
    extract::State,
    response::IntoResponse,
    http::StatusCode,
    Json,
    Extension,
};
use serde::Deserialize;
use serde_json::json;

use crate::presentation::state::AppState;
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::infrastructure::repositories::ai_repo;
use crate::domain::services::ai_service;

fn err(detail: &str) -> serde_json::Value {
    json!({"error": detail})
}

/// 現時点では言語検出は日本語固定(Django側は request.LANGUAGE_CODE、
/// Rust側にAccept-Language自動判定は未実装のため既定の"ja"を使う)。
const DEFAULT_LANGUAGE: &str = "ja";

#[derive(Deserialize)]
pub struct SuggestPointsIn {
    pub title: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub team_rules: String,
}

/// POST /api/v1/ai/suggest-points/
pub async fn suggest_points(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Json(body): Json<SuggestPointsIn>,
) -> impl IntoResponse {
    let title = match &body.title {
        Some(t) if !t.is_empty() => t,
        _ => return (StatusCode::BAD_REQUEST, Json(err("title は必須です"))).into_response(),
    };

    let ai_config = state.ai_config().await;
    let result = ai_service::suggest_story_points(
        &ai_config, title, &body.description, &body.team_rules, DEFAULT_LANGUAGE,
    )
    .await;

    (StatusCode::OK, Json(result)).into_response()
}

#[derive(Deserialize)]
pub struct SprintHealthIn {
    pub project_id: Option<i32>,
    pub cycle_id: Option<i32>,
}

/// POST /api/v1/ai/sprint-health/
pub async fn sprint_health(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Json(body): Json<SprintHealthIn>,
) -> impl IntoResponse {
    let project_id = match body.project_id {
        Some(id) => id,
        None => return (StatusCode::BAD_REQUEST, Json(err("project_id は必須です"))).into_response(),
    };

    let project_prefix = match ai_repo::find_project_prefix(&state.pool, project_id).await {
        Ok(Some(p)) => p,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(err("Project not found"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    let tasks_json = match ai_repo::find_tasks_json_for_sprint_health(&state.pool, project_id, body.cycle_id).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    let (start_date, end_date) = if let Some(cid) = body.cycle_id {
        match ai_repo::find_cycle_dates(&state.pool, cid).await {
            Ok(Some((s, e))) => (s.format("%Y-%m-%d").to_string(), e.format("%Y-%m-%d").to_string()),
            Ok(None) => (String::new(), String::new()),
            Err(e) => {
                tracing::error!("DB operation failed: {:?}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
            }
        }
    } else {
        (String::new(), String::new())
    };

    let ai_config = state.ai_config().await;
    let result = ai_service::analyze_sprint_health(
        &ai_config, &project_prefix, &start_date, &end_date,
        &serde_json::Value::Array(tasks_json), DEFAULT_LANGUAGE,
    )
    .await;

    (StatusCode::OK, Json(result)).into_response()
}

/// GET /api/v1/ai/status/
pub async fn ai_status(State(state): State<AppState>, Extension(_auth): Extension<AuthUser>) -> impl IntoResponse {
    let status = ai_service::check_ai_status(&state.ai_config().await).await;
    (StatusCode::OK, Json(status)).into_response()
}

#[derive(Deserialize)]
pub struct ContextAnalysisIn {
    pub ticket_id: Option<i32>,
}

/// POST /api/v1/ai/context-analysis/
pub async fn context_analysis(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Json(body): Json<ContextAnalysisIn>,
) -> impl IntoResponse {
    let ticket_id = match body.ticket_id {
        Some(id) => id,
        None => return (StatusCode::BAD_REQUEST, Json(err("ticket_id は必須です"))).into_response(),
    };

    let ticket = match ai_repo::find_ticket_for_ai(&state.pool, ticket_id).await {
        Ok(Some(t)) => t,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(err("Ticket not found"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    let rules_text = match ai_repo::build_associated_rules_text(&state.pool, ticket_id, ticket.team_id).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    let ai_config = state.ai_config().await;
    let result = ai_service::analyze_ticket_context(
        &ai_config, ticket.id, &ticket.title, &ticket.description, &ticket.status,
        ticket.story_points.map(|p| p as i32), &ticket.project_prefix, &rules_text, DEFAULT_LANGUAGE,
    )
    .await;

    (StatusCode::OK, Json(result)).into_response()
}

#[derive(Deserialize)]
pub struct CloseAnalysisIn {
    pub ticket_id: Option<i32>,
}

/// POST /api/v1/ai/close-analysis/
pub async fn close_analysis(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Json(body): Json<CloseAnalysisIn>,
) -> impl IntoResponse {
    let ticket_id = match body.ticket_id {
        Some(id) => id,
        None => return (StatusCode::BAD_REQUEST, Json(err("ticket_id は必須です"))).into_response(),
    };

    let ticket = match ai_repo::find_ticket_for_ai(&state.pool, ticket_id).await {
        Ok(Some(t)) => t,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(err("Ticket not found"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    let comments_text = match ai_repo::build_comments_text(&state.pool, ticket_id).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    let ai_config = state.ai_config().await;
    let result = ai_service::analyze_ticket_close(
        &ai_config, &ticket.title, &ticket.description, &comments_text, DEFAULT_LANGUAGE,
    )
    .await;

    (StatusCode::OK, Json(result)).into_response()
}

#[derive(Deserialize)]
pub struct GeneratePromptTextIn {
    pub ticket_id: Option<i32>,
}

/// POST /api/v1/ai/generate-prompt-text/
pub async fn generate_prompt_text(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Json(body): Json<GeneratePromptTextIn>,
) -> impl IntoResponse {
    let ticket_id = match body.ticket_id {
        Some(id) => id,
        None => return (StatusCode::BAD_REQUEST, Json(err("ticket_id は必須です"))).into_response(),
    };

    let ticket = match ai_repo::find_ticket_for_ai(&state.pool, ticket_id).await {
        Ok(Some(t)) => t,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(err("Ticket not found"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    let comments_text = match ai_repo::build_comments_text(&state.pool, ticket_id).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    let rules_text = match ai_repo::build_associated_rules_text(&state.pool, ticket_id, ticket.team_id).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err("サーバーエラーが発生しました"))).into_response();
        }
    };

    let ai_config = state.ai_config().await;
    // ハイブリッド方式: Ollama 失敗時も Ok（テンプレート + フォールバック文言）を返す。
    // エラーは実質ほぼ発生しない（チケット不存在・DB エラー時のみ）。
    match ai_service::generate_dev_ai_prompt(
        &ai_config,
        &ticket.ticket_key,
        &ticket.project_prefix,
        &ticket.title,
        &ticket.status,
        &ticket.description,
        &comments_text,
        &rules_text,
        DEFAULT_LANGUAGE,
    )
    .await
    {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(e) => {
            tracing::error!("generate_dev_ai_prompt failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(e)).into_response()
        }
    }
}
