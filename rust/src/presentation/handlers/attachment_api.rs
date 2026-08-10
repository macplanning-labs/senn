/// presentation/handlers/attachment_api.rs — 添付ファイル API
///
/// POST   /api/v1/tickets/{ticket_key}/attachments/   → アップロード
/// GET    /api/v1/tickets/{ticket_key}/attachments/   → 一覧
/// DELETE /api/v1/tickets/{ticket_key}/attachments/{attachment_id}/ → 削除

use axum::{
    extract::{State, Path, Multipart},
    response::{IntoResponse, Response},
    http::StatusCode,
    Json,
    Extension,
};
use serde::Serialize;
use uuid::Uuid;
use std::path::PathBuf;
use tokio::fs;

use crate::presentation::state::AppState;
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::infrastructure::repositories::{ticket_repo, attachment_repo};

// =============================================================================
// レスポンス構造体
// =============================================================================

#[derive(Debug, Serialize)]
pub struct AttachmentOut {
    pub id: i32,
    pub filename: String,
    #[serde(rename = "fileSize")]
    pub file_size: i32,
    #[serde(rename = "sizeDisplay")]
    pub size_display: String,
    #[serde(rename = "isImage")]
    pub is_image: bool,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    pub uploader: UploaderInfo,
    #[serde(rename = "fileUrl")]
    pub file_url: String,
}

#[derive(Debug, Serialize)]
pub struct UploaderInfo {
    pub id: i32,
    pub username: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub detail: String,
}

// =============================================================================
// ハンドラー実装
// =============================================================================

/// POST /api/v1/tickets/{ticket_key}/attachments/ — 添付ファイルアップロード
pub async fn upload_attachment(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(ticket_key): Path<String>,
    mut multipart: Multipart,
) -> Response {
    // チケットをキーまたはIDで検索
    let ticket = match resolve_ticket(&state, &ticket_key).await {
        Ok(Some(t)) => t,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    detail: "Ticket not found".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("Failed to find ticket: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "Failed to find ticket".to_string(),
                }),
            )
                .into_response();
        }
    };

    let ticket_id = ticket.base.id;

    // マルチパートから file フィールドを抽出
    let mut file_field = None;
    let mut original_filename = String::new();
    let mut file_bytes = Vec::new();

    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some("file") {
            original_filename = field.file_name().unwrap_or("unnamed").to_string();
            match field.bytes().await {
                Ok(bytes) => {
                    file_bytes = bytes.to_vec();
                    file_field = Some(());
                }
                Err(e) => {
                    tracing::error!("Failed to read file bytes: {}", e);
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse {
                            detail: "Failed to read file".to_string(),
                        }),
                    )
                        .into_response();
                }
            }
            break;
        }
    }

    if file_field.is_none() || file_bytes.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                detail: "No file provided".to_string(),
            }),
        )
            .into_response();
    }

    let file_size = file_bytes.len() as i32;

    // ファイルを保存する場所を決定
    // 形式: media/attachments/{ticket_id}/{uuid}_{original_filename}
    let media_dir = PathBuf::from(&state.config.media_dir);
    let attachments_dir = media_dir.join("attachments").join(ticket_id.to_string());

    // ディレクトリを作成
    if let Err(e) = fs::create_dir_all(&attachments_dir).await {
        tracing::error!("Failed to create attachment directory: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                detail: "Failed to create attachment directory".to_string(),
            }),
        )
            .into_response();
    }

    // ユニークなファイル名を生成
    let uuid = Uuid::new_v4().to_string();
    let stored_filename = format!("{}_{}", uuid, original_filename);
    let file_path = attachments_dir.join(&stored_filename);

    // ファイルを書き込み
    if let Err(e) = fs::write(&file_path, &file_bytes).await {
        tracing::error!("Failed to write file: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                detail: "Failed to save file".to_string(),
            }),
        )
            .into_response();
    }

    // ファイルパス（相対パス、/app/media からの相対）
    let relative_path = format!("attachments/{}/{}", ticket_id, stored_filename);

    // DBに記録
    let attachment_id = match attachment_repo::create(
        &state.pool,
        ticket_id,
        None, // comment_id は None（チケットレベルの添付）
        auth.user_id,
        &original_filename,
        &relative_path,
        file_size,
    )
    .await
    {
        Ok(id) => id,
        Err(e) => {
            tracing::error!("Failed to create attachment record: {}", e);
            // ファイルは削除
            let _ = fs::remove_file(&file_path).await;
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "Failed to create attachment record".to_string(),
                }),
            )
                .into_response();
        }
    };

    // ユーザー情報を取得
    let username = match sqlx::query_scalar::<_, String>(
        "SELECT username FROM accounts_user WHERE id = $1"
    )
    .bind(auth.user_id)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(Some(u)) => u,
        Ok(None) => "unknown".to_string(),
        Err(e) => {
            tracing::error!("Failed to fetch uploader username: {}", e);
            "unknown".to_string()
        }
    };

    let display_name = match sqlx::query_scalar::<_, String>(
        "SELECT display_name FROM accounts_user WHERE id = $1"
    )
    .bind(auth.user_id)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(Some(d)) => d,
        Ok(None) => username.clone(),
        Err(e) => {
            tracing::error!("Failed to fetch uploader display_name: {}", e);
            username.clone()
        }
    };

    // レスポンスを作成
    let attachment = AttachmentOut {
        id: attachment_id,
        filename: original_filename.clone(),
        file_size,
        size_display: format_file_size(file_size),
        is_image: is_image_file(&original_filename),
        created_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        uploader: UploaderInfo {
            id: auth.user_id,
            username,
            display_name,
        },
        file_url: format!("/media/{}", relative_path),
    };

    (StatusCode::CREATED, Json(attachment)).into_response()
}

/// GET /api/v1/tickets/{ticket_key}/attachments/ — 添付ファイル一覧
pub async fn list_attachments(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(ticket_key): Path<String>,
) -> Response {
    // チケットをキーまたはIDで検索
    let ticket = match resolve_ticket(&state, &ticket_key).await {
        Ok(Some(t)) => t,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    detail: "Ticket not found".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("Failed to find ticket: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "Failed to find ticket".to_string(),
                }),
            )
                .into_response();
        }
    };

    // チケットの添付ファイルを取得
    let attachments = match attachment_repo::find_by_ticket(&state.pool, ticket.base.id).await {
        Ok(atts) => atts,
        Err(e) => {
            tracing::error!("Failed to find attachments: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "Failed to find attachments".to_string(),
                }),
            )
                .into_response();
        }
    };

    // ユーザー情報を取得（キャッシュするか？ここでは単純に取得）
    let mut result = Vec::new();
    for att in attachments {
        // ユーザー情報を取得
        let username = match sqlx::query_scalar::<_, String>(
            "SELECT username FROM accounts_user WHERE id = $1"
        )
        .bind(att.uploader_id)
        .fetch_optional(&state.pool)
        .await
        {
            Ok(Some(u)) => u,
            Ok(None) => "unknown".to_string(),
            Err(e) => {
                tracing::error!("Failed to fetch uploader username: {}", e);
                "unknown".to_string()
            }
        };

        let display_name = match sqlx::query_scalar::<_, String>(
            "SELECT display_name FROM accounts_user WHERE id = $1"
        )
        .bind(att.uploader_id)
        .fetch_optional(&state.pool)
        .await
        {
            Ok(Some(d)) => d,
            Ok(None) => username.clone(),
            Err(e) => {
                tracing::error!("Failed to fetch uploader display_name: {}", e);
                username.clone()
            }
        };

        result.push(AttachmentOut {
            id: att.id,
            filename: att.filename.clone(),
            file_size: att.file_size,
            size_display: format_file_size(att.file_size),
            is_image: att.is_image(),
            created_at: att.created_at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            uploader: UploaderInfo {
                id: att.uploader_id,
                username,
                display_name,
            },
            file_url: format!("/media/{}", att.file_path),
        });
    }

    Json(result).into_response()
}

/// DELETE /api/v1/tickets/{ticket_key}/attachments/{attachment_id}/ — 添付ファイル削除
pub async fn delete_attachment(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path((ticket_key, attachment_id)): Path<(String, i32)>,
) -> Response {
    // チケットをキーまたはIDで検索
    let ticket = match resolve_ticket(&state, &ticket_key).await {
        Ok(Some(t)) => t,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    detail: "Ticket not found".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("Failed to find ticket: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "Failed to find ticket".to_string(),
                }),
            )
                .into_response();
        }
    };

    // 添付ファイルを検索
    let attachment = match attachment_repo::find_by_id(&state.pool, attachment_id).await {
        Ok(Some(att)) => att,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    detail: "Attachment not found".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("Failed to find attachment: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "Failed to find attachment".to_string(),
                }),
            )
                .into_response();
        }
    };

    // チケットIDが一致するか確認
    if attachment.ticket_id != ticket.base.id {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                detail: "Attachment does not belong to this ticket".to_string(),
            }),
        )
            .into_response();
    }

    // ファイルを削除（ベストエフォート）
    let media_dir = PathBuf::from(&state.config.media_dir);
    let file_path = media_dir.join(&attachment.file_path);
    if let Err(e) = fs::remove_file(&file_path).await {
        tracing::warn!("Failed to delete attachment file {}: {}", file_path.display(), e);
        // ファイルが存在しない場合は警告のみ、エラーではない
    }

    // DBから削除
    if let Err(e) = attachment_repo::delete(&state.pool, attachment_id).await {
        tracing::error!("Failed to delete attachment record: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                detail: "Failed to delete attachment".to_string(),
            }),
        )
            .into_response();
    }

    StatusCode::NO_CONTENT.into_response()
}

// =============================================================================
// ヘルパー関数
// =============================================================================

/// チケットをキーまたはIDで検索
/// 最初にキーで試し、見つからなければ数値IDで試す
async fn resolve_ticket(
    state: &AppState,
    ticket_key_or_id: &str,
) -> anyhow::Result<Option<crate::domain::models::ticket_api::TicketDetailOut>> {
    // 最初にキーで検索
    if let Some(ticket) = ticket_repo::api_find_by_key(&state.pool, ticket_key_or_id).await? {
        return Ok(Some(ticket));
    }

    // キーでの検索に失敗した場合、数値IDで検索
    if let Ok(id) = ticket_key_or_id.parse::<i32>() {
        // IDから ticket_key を取得
        if let Some(ticket_key) = sqlx::query_scalar::<_, String>(
            "SELECT ticket_key FROM tickets_ticket WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        {
            // ticket_key で検索
            return ticket_repo::api_find_by_key(&state.pool, &ticket_key).await;
        }
    }

    Ok(None)
}

fn format_file_size(size: i32) -> String {
    let size_f = size as f64;
    if size_f < 1024.0 {
        format!("{} B", size)
    } else if size_f < 1024.0 * 1024.0 {
        format!("{:.1} KB", size_f / 1024.0)
    } else {
        format!("{:.1} MB", size_f / 1024.0 / 1024.0)
    }
}

fn is_image_file(filename: &str) -> bool {
    let lower = filename.to_lowercase();
    lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".gif")
        || lower.ends_with(".webp")
        || lower.ends_with(".svg")
}
