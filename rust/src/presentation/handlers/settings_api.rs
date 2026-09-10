/// presentation/handlers/settings_api.rs — インスタンス設定 API

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::domain::services::ai_service;
use crate::infrastructure::repositories::{system_settings_repo, user_repo};
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::presentation::state::AppState;

async fn caller_is_staff(state: &AppState, auth: &AuthUser) -> Result<bool, StatusCode> {
    match user_repo::find_by_id(&state.pool, auth.user_id).await {
        Ok(Some(user)) => Ok(user.is_staff),
        Ok(None) => Err(StatusCode::UNAUTHORIZED),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Serialize)]
pub struct AiSettingsOut {
    #[serde(rename = "ollamaModel")]
    pub ollama_model: String,
    #[serde(rename = "envDefaultModel")]
    pub env_default_model: String,
    #[serde(rename = "ollamaTimeoutSecs")]
    pub ollama_timeout_secs: u64,
    #[serde(rename = "envDefaultTimeoutSecs")]
    pub env_default_timeout_secs: u64,
    #[serde(rename = "ollamaConnected")]
    pub ollama_connected: bool,
    #[serde(rename = "availableModels")]
    pub available_models: Vec<String>,
    #[serde(rename = "canEdit")]
    pub can_edit: bool,
}

#[derive(Deserialize)]
pub struct UpdateAiSettingsIn {
    #[serde(rename = "ollamaModel")]
    pub ollama_model: Option<String>,
    #[serde(rename = "ollamaTimeoutSecs")]
    pub ollama_timeout_secs: Option<u64>,
    #[serde(rename = "resetOllamaTimeout")]
    pub reset_ollama_timeout: Option<bool>,
}

fn normalize_model_input(model: Option<String>) -> Result<Option<String>, &'static str> {
    match model {
        None => Ok(None),
        Some(m) if m.trim().is_empty() => Ok(None),
        Some(m) => Ok(Some(m.trim().to_string())),
    }
}

fn err(detail: &str) -> serde_json::Value {
    json!({ "error": detail })
}

/// GET /api/v1/settings/ai/
pub async fn get_ai_settings(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
) -> impl IntoResponse {
    let can_edit = match caller_is_staff(&state, &auth).await {
        Ok(v) => v,
        Err(status) => return (status, Json(err("認証エラー"))).into_response(),
    };

    let ai_config = state.ai_config().await;
    let status = ai_service::check_ai_status(&ai_config).await;

    (
        StatusCode::OK,
        Json(AiSettingsOut {
            ollama_model: ai_config.ollama_model,
            env_default_model: state.config.ollama_model.clone(),
            ollama_timeout_secs: ai_config.ollama_timeout_secs,
            env_default_timeout_secs: state.config.ollama_timeout_secs,
            ollama_connected: status.ollama.connected,
            available_models: status.ollama.available_models,
            can_edit,
        }),
    )
        .into_response()
}

const MIN_OLLAMA_TIMEOUT_SECS: u64 = 30;
const MAX_OLLAMA_TIMEOUT_SECS: u64 = 300;

/// PATCH /api/v1/settings/ai/
pub async fn update_ai_settings(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<UpdateAiSettingsIn>,
) -> impl IntoResponse {
    let can_edit = match caller_is_staff(&state, &auth).await {
        Ok(v) => v,
        Err(status) => return (status, Json(err("認証エラー"))).into_response(),
    };
    if !can_edit {
        return (
            StatusCode::FORBIDDEN,
            Json(err("管理者のみ変更できます")),
        )
            .into_response();
    }

    if let Some(true) = body.reset_ollama_timeout {
        if let Err(e) = system_settings_repo::delete(&state.pool, system_settings_repo::KEY_OLLAMA_TIMEOUT).await {
            tracing::error!("system_settings delete failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("タイムアウトのリセットに失敗しました")),
            )
                .into_response();
        }
        state.set_ollama_timeout_override(None).await;
    } else if let Some(timeout_secs) = body.ollama_timeout_secs {
        if timeout_secs < MIN_OLLAMA_TIMEOUT_SECS || timeout_secs > MAX_OLLAMA_TIMEOUT_SECS {
            return (
                StatusCode::BAD_REQUEST,
                Json(err(&format!(
                    "タイムアウトは {}〜{}秒である必要があります",
                    MIN_OLLAMA_TIMEOUT_SECS, MAX_OLLAMA_TIMEOUT_SECS
                ))),
            )
                .into_response();
        }
        if let Err(e) = system_settings_repo::set(
            &state.pool,
            system_settings_repo::KEY_OLLAMA_TIMEOUT,
            &timeout_secs.to_string(),
        )
        .await
        {
            tracing::error!("system_settings save failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("タイムアウトの保存に失敗しました")),
            )
                .into_response();
        }
        state.set_ollama_timeout_override(Some(timeout_secs)).await;
    }

    if body.ollama_model.is_some() {
        match normalize_model_input(body.ollama_model) {
            Ok(Some(model)) => {
                let ai_config = state.ai_config().await;
                let status = ai_service::check_ai_status(&ai_config).await;
                if !status.ollama.connected {
                    return (
                        StatusCode::SERVICE_UNAVAILABLE,
                        Json(err("Ollama に接続できません。モデルを変更できません。")),
                    )
                        .into_response();
                }
                if !status.ollama.available_models.contains(&model) {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(err(&format!(
                            "モデル「{}」は Ollama にインストールされていません",
                            model
                        ))),
                    )
                        .into_response();
                }

                if let Err(e) = system_settings_repo::set(&state.pool, system_settings_repo::KEY_OLLAMA_MODEL, &model).await
                {
                    tracing::error!("system_settings save failed: {:?}", e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(err("モデルの保存に失敗しました")),
                    )
                        .into_response();
                }
                state.set_ollama_model_override(Some(model)).await;
            }
            Ok(None) => {
                if let Err(e) =
                    system_settings_repo::delete(&state.pool, system_settings_repo::KEY_OLLAMA_MODEL).await
                {
                    tracing::error!("system_settings delete failed: {:?}", e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(err("モデル設定のリセットに失敗しました")),
                    )
                        .into_response();
                }
                state.set_ollama_model_override(None).await;
            }
            Err(msg) => {
                return (StatusCode::BAD_REQUEST, Json(err(msg))).into_response();
            }
        }
    }

    let ai_config = state.ai_config().await;
    let status = ai_service::check_ai_status(&ai_config).await;

    (
        StatusCode::OK,
        Json(AiSettingsOut {
            ollama_model: ai_config.ollama_model,
            env_default_model: state.config.ollama_model.clone(),
            ollama_timeout_secs: ai_config.ollama_timeout_secs,
            env_default_timeout_secs: state.config.ollama_timeout_secs,
            ollama_connected: status.ollama.connected,
            available_models: status.ollama.available_models,
            can_edit: true,
        }),
    )
        .into_response()
}
