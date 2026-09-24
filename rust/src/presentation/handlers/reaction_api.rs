/// presentation/handlers/reaction_api.rs — チケットリアクション & カスタム絵文字 API

use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::Serialize;
use std::path::PathBuf;
use tokio::fs;
use uuid::Uuid;

use crate::domain::models::reaction::{CustomEmojiOut, ReactionIn, ReactionOut, ReactionRow};
use crate::domain::models::ticket::Ticket;
use crate::domain::models::ticket_api::UserSummaryOut;
use crate::domain::models::project::Project;
use crate::infrastructure::repositories::{
    membership_repo, project_repo, project_team_repo, reaction_repo, team_repo, ticket_repo,
    user_repo,
};
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::presentation::state::AppState;

// =============================================================================
// レスポンス
// =============================================================================

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub detail: String,
}

fn err(status: StatusCode, detail: &str) -> Response {
    (status, Json(ErrorResponse { detail: detail.to_string() })).into_response()
}

fn server_err(e: impl std::fmt::Debug) -> Response {
    tracing::error!("Reaction API error: {:?}", e);
    err(StatusCode::INTERNAL_SERVER_ERROR, "サーバーエラーが発生しました")
}

// =============================================================================
// アクセス制御
// =============================================================================

async fn caller_is_staff(state: &AppState, user_id: i32) -> bool {
    match user_repo::find_by_id(&state.pool, user_id).await {
        Ok(Some(u)) => u.is_staff,
        _ => false,
    }
}

async fn deny_unless_ticket_access(
    state: &AppState,
    ticket_id: i32,
    user_id: i32,
) -> Option<Response> {
    match membership_repo::check_ticket_access(&state.pool, ticket_id, user_id).await {
        Ok(true) => None,
        Ok(false) => Some(err(StatusCode::NOT_FOUND, "見つかりません")),
        Err(e) => Some(server_err(e)),
    }
}

async fn deny_unless_project_access(
    state: &AppState,
    project_id: i32,
    user_id: i32,
) -> Option<Response> {
    if caller_is_staff(state, user_id).await {
        return None;
    }
    match team_repo::check_project_access(&state.pool, project_id, user_id).await {
        Ok(true) => None,
        Ok(false) => Some(err(StatusCode::NOT_FOUND, "見つかりません")),
        Err(e) => Some(server_err(e)),
    }
}

// =============================================================================
// ユーティリティ
// =============================================================================

async fn resolve_ticket(state: &AppState, ticket_key: &str) -> anyhow::Result<Option<Ticket>> {
    if let Ok(id) = ticket_key.parse::<i32>() {
        return ticket_repo::find_by_id(&state.pool, id).await;
    }
    ticket_repo::find_by_key(&state.pool, ticket_key).await
}

async fn resolve_project(state: &AppState, prefix: &str) -> anyhow::Result<Option<Project>> {
    project_repo::find_by_prefix(&state.pool, prefix).await
}

fn media_url(image_path: &str) -> String {
    format!("/media/{}", image_path)
}

fn is_allowed_unicode(s: &str) -> bool {
    const ALLOWED: &[&str] = &["👍", "👀", "👏", "❤️", "✅", "🙇", "🙏", "🔥", "🎉", "😇", "🤔"];
    ALLOWED.contains(&s) || s == "❤"
}

fn normalize_unicode(s: &str) -> String {
    if s == "❤" {
        "❤️".to_string()
    } else {
        s.to_string()
    }
}

fn valid_slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 32
        && s.chars().all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_'))
}

fn ext_from_mime(mime: &str) -> Option<&'static str> {
    match mime {
        "image/jpeg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/gif" => Some("gif"),
        _ => None,
    }
}

fn mime_from_ext(ext: &str) -> Option<&'static str> {
    match ext.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "gif" => Some("image/gif"),
        _ => None,
    }
}

fn is_unique_violation(e: &anyhow::Error) -> bool {
    let msg = e.to_string().to_lowercase();
    msg.contains("unique") || msg.contains("23505") || msg.contains("duplicate key")
}

async fn custom_image_url(state: &AppState, emoji_kind: &str, emoji_value: &str) -> Option<String> {
    if emoji_kind != "custom" {
        return None;
    }
    let emoji_id = emoji_value.parse::<i64>().ok()?;
    match reaction_repo::find_emoji_by_id(&state.pool, emoji_id).await {
        Ok(Some((row, _, _, _, _))) => Some(media_url(&row.image_path)),
        _ => None,
    }
}

async fn reaction_row_to_out(
    state: &AppState,
    row: ReactionRow,
    uid: i32,
    username: String,
    email: String,
    display_name: String,
) -> ReactionOut {
    let image_url = custom_image_url(state, &row.emoji_kind, &row.emoji_value).await;
    ReactionOut {
        id: row.id,
        ticket_id: row.ticket_id,
        user_id: row.user_id,
        user: UserSummaryOut {
            id: uid,
            username,
            email,
            display_name,
        },
        emoji_kind: row.emoji_kind,
        emoji_value: row.emoji_value,
        image_url,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn user_summary(uid: i32, username: String, email: String, display_name: String) -> UserSummaryOut {
    UserSummaryOut {
        id: uid,
        username,
        email,
        display_name,
    }
}

// =============================================================================
// Reaction ハンドラー
// =============================================================================

/// GET /api/v1/tickets/{ticket_key}/reactions/
pub async fn list_reactions(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(ticket_key): Path<String>,
) -> Response {
    let ticket = match resolve_ticket(&state, &ticket_key).await {
        Ok(Some(t)) => t,
        Ok(None) => return err(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => return server_err(e),
    };

    if let Some(resp) = deny_unless_ticket_access(&state, ticket.id, auth.user_id).await {
        return resp;
    }

    match reaction_repo::find_by_ticket(&state.pool, ticket.id as i64).await {
        Ok(reactions) => {
            let mut result = Vec::with_capacity(reactions.len());
            for (row, uid, username, email, display_name) in reactions {
                result.push(
                    reaction_row_to_out(&state, row, uid, username, email, display_name).await,
                );
            }
            Json(result).into_response()
        }
        Err(e) => server_err(e),
    }
}

/// POST /api/v1/tickets/{ticket_key}/reactions/
pub async fn add_reaction(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(ticket_key): Path<String>,
    Json(payload): Json<ReactionIn>,
) -> Response {
    let ticket = match resolve_ticket(&state, &ticket_key).await {
        Ok(Some(t)) => t,
        Ok(None) => return err(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => return server_err(e),
    };

    if let Some(resp) = deny_unless_ticket_access(&state, ticket.id, auth.user_id).await {
        return resp;
    }

    if payload.emoji_kind != "unicode" && payload.emoji_kind != "custom" {
        return err(
            StatusCode::BAD_REQUEST,
            "Invalid emoji_kind. Must be 'unicode' or 'custom'",
        );
    }

    let emoji_value = if payload.emoji_kind == "unicode" {
        if !is_allowed_unicode(&payload.emoji_value) {
            return err(StatusCode::BAD_REQUEST, "Invalid unicode emoji");
        }
        normalize_unicode(&payload.emoji_value)
    } else if let Ok(emoji_id) = payload.emoji_value.parse::<i64>() {
        match reaction_repo::find_emoji_by_id(&state.pool, emoji_id).await {
            Ok(Some((emoji, _, _, _, _))) => {
                let Some(project_id) = ticket.project_id else {
                    return err(StatusCode::BAD_REQUEST, "Ticket has no project");
                };
                if emoji.project_id != project_id as i64 {
                    return err(
                        StatusCode::BAD_REQUEST,
                        "Custom emoji does not belong to this ticket's project",
                    );
                }
                payload.emoji_value.clone()
            }
            Ok(None) => return err(StatusCode::BAD_REQUEST, "Custom emoji not found"),
            Err(e) => return server_err(e),
        }
    } else {
        return err(StatusCode::BAD_REQUEST, "Invalid custom emoji ID");
    };

    let ticket_id = ticket.id as i64;
    let user_id = auth.user_id as i64;

    match reaction_repo::create(
        &state.pool,
        ticket_id,
        user_id,
        &payload.emoji_kind,
        &emoji_value,
    )
    .await
    {
        Ok(id) => match reaction_repo::find_by_id(&state.pool, id).await {
            Ok(Some((row, uid, username, email, display_name))) => {
                let reaction =
                    reaction_row_to_out(&state, row, uid, username, email, display_name).await;
                (StatusCode::CREATED, Json(reaction)).into_response()
            }
            _ => err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to retrieve created reaction",
            ),
        },
        Err(e) if is_unique_violation(&e) => {
            match reaction_repo::find_by_ticket_user_emoji(
                &state.pool,
                ticket_id,
                user_id,
                &payload.emoji_kind,
                &emoji_value,
            )
            .await
            {
                Ok(Some((row, uid, username, email, display_name))) => {
                    let reaction =
                        reaction_row_to_out(&state, row, uid, username, email, display_name)
                            .await;
                    (StatusCode::OK, Json(reaction)).into_response()
                }
                Ok(None) => err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to retrieve existing reaction",
                ),
                Err(e) => server_err(e),
            }
        }
        Err(e) => server_err(e),
    }
}

/// DELETE /api/v1/tickets/{ticket_key}/reactions/{id}/
pub async fn delete_reaction(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((ticket_key, reaction_id)): Path<(String, i64)>,
) -> Response {
    let ticket = match resolve_ticket(&state, &ticket_key).await {
        Ok(Some(t)) => t,
        Ok(None) => return err(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => return server_err(e),
    };

    if let Some(resp) = deny_unless_ticket_access(&state, ticket.id, auth.user_id).await {
        return resp;
    }

    let staff = caller_is_staff(&state, auth.user_id).await;

    match reaction_repo::find_by_id(&state.pool, reaction_id).await {
        Ok(Some((row, _, _, _, _))) => {
            if row.ticket_id != ticket.id as i64 {
                return err(StatusCode::NOT_FOUND, "見つかりません");
            }
            if row.user_id != auth.user_id as i64 && !staff {
                return err(StatusCode::FORBIDDEN, "Only the reaction owner can delete");
            }
            match reaction_repo::delete(&state.pool, reaction_id).await {
                Ok(_) => StatusCode::NO_CONTENT.into_response(),
                Err(e) => server_err(e),
            }
        }
        Ok(None) => err(StatusCode::NOT_FOUND, "Reaction not found"),
        Err(e) => server_err(e),
    }
}

// =============================================================================
// Custom Emoji ハンドラー
// =============================================================================

/// GET /api/v1/projects/{prefix}/custom-emojis/
pub async fn list_custom_emojis(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(prefix): Path<String>,
) -> Response {
    let project = match resolve_project(&state, &prefix).await {
        Ok(Some(p)) => p,
        Ok(None) => return err(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => return server_err(e),
    };

    if let Some(resp) = deny_unless_project_access(&state, project.id, auth.user_id).await {
        return resp;
    }

    match reaction_repo::find_emoji_by_project(&state.pool, project.id as i64).await {
        Ok(emojis) => {
            let result: Vec<CustomEmojiOut> = emojis
                .into_iter()
                .map(|(row, uid, username, email, display_name)| CustomEmojiOut {
                    id: row.id,
                    project_id: row.project_id,
                    slug: row.slug,
                    name: row.name,
                    image_url: media_url(&row.image_path),
                    uploaded_by: user_summary(uid, username, email, display_name),
                    created_at: row.created_at,
                })
                .collect();
            Json(result).into_response()
        }
        Err(e) => server_err(e),
    }
}

/// POST /api/v1/projects/{prefix}/custom-emojis/
pub async fn upload_custom_emoji(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(prefix): Path<String>,
    mut multipart: Multipart,
) -> Response {
    let project = match resolve_project(&state, &prefix).await {
        Ok(Some(p)) => p,
        Ok(None) => return err(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => return server_err(e),
    };

    if let Some(resp) = deny_unless_project_access(&state, project.id, auth.user_id).await {
        return resp;
    }

    let mut slug = String::new();
    let mut name = String::new();
    let mut file_bytes = Vec::new();
    let mut mime_type = String::new();
    let mut original_filename = String::new();

    while let Ok(Some(field)) = multipart.next_field().await {
        match field.name() {
            Some("file") => {
                mime_type = field.content_type().unwrap_or("").trim().to_string();
                original_filename = field.file_name().unwrap_or("").to_string();
                match field.bytes().await {
                    Ok(bytes) => file_bytes = bytes.to_vec(),
                    Err(e) => {
                        tracing::error!("Failed to read file bytes: {}", e);
                        return err(StatusCode::BAD_REQUEST, "Failed to read file");
                    }
                }
            }
            Some("slug") => {
                if let Ok(s) = field.text().await {
                    slug = s;
                }
            }
            Some("name") => {
                if let Ok(n) = field.text().await {
                    name = n;
                }
            }
            _ => {}
        }
    }

    if file_bytes.is_empty() {
        return err(StatusCode::BAD_REQUEST, "No file provided");
    }

    if !matches!(mime_type.as_str(), "image/jpeg" | "image/png" | "image/gif") {
        return err(
            StatusCode::BAD_REQUEST,
            "Only JPEG, PNG, and GIF images are allowed",
        );
    }

    if let Some(ext) = PathBuf::from(&original_filename)
        .extension()
        .and_then(|e| e.to_str())
    {
        if let Some(expected_mime) = mime_from_ext(ext) {
            if expected_mime != mime_type.as_str() {
                return err(
                    StatusCode::BAD_REQUEST,
                    "File extension does not match content type",
                );
            }
        }
    }

    if file_bytes.len() > state.config.max_upload_size {
        return err(StatusCode::PAYLOAD_TOO_LARGE, "File size exceeds maximum allowed");
    }

    if !valid_slug(&slug) {
        return err(
            StatusCode::BAD_REQUEST,
            "Invalid slug format. Must be 1-32 lowercase alphanumeric characters or underscore",
        );
    }

    if name.is_empty() {
        return err(StatusCode::BAD_REQUEST, "Name is required");
    }

    match reaction_repo::find_emoji_by_project_slug(&state.pool, project.id as i64, &slug).await {
        Ok(Some(_)) => {
            return err(
                StatusCode::BAD_REQUEST,
                "Slug already exists for this project",
            );
        }
        Ok(None) => {}
        Err(e) => return server_err(e),
    }

    let ext = match ext_from_mime(&mime_type) {
        Some(e) => e,
        None => {
            return err(StatusCode::BAD_REQUEST, "Invalid image format");
        }
    };

    let media_dir = PathBuf::from(&state.config.media_dir);
    let emoji_dir = media_dir.join(format!("custom-emojis/{}", project.id));

    if let Err(e) = fs::create_dir_all(&emoji_dir).await {
        tracing::error!("Failed to create emoji directory: {}", e);
        return err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to create storage directory",
        );
    }

    let filename = format!("{}.{}", Uuid::new_v4(), ext);
    let file_path = emoji_dir.join(&filename);
    let relative_path = format!("custom-emojis/{}/{}", project.id, filename);

    if let Err(e) = fs::write(&file_path, &file_bytes).await {
        tracing::error!("Failed to write emoji file: {}", e);
        return err(StatusCode::INTERNAL_SERVER_ERROR, "Failed to save file");
    }

    match reaction_repo::create_emoji(
        &state.pool,
        project.id as i64,
        &slug,
        &name,
        &relative_path,
        auth.user_id as i64,
    )
    .await
    {
        Ok(id) => match reaction_repo::find_emoji_by_id(&state.pool, id).await {
            Ok(Some((row, uid, username, email, display_name))) => {
                let emoji = CustomEmojiOut {
                    id: row.id,
                    project_id: row.project_id,
                    slug: row.slug,
                    name: row.name,
                    image_url: media_url(&row.image_path),
                    uploaded_by: user_summary(uid, username, email, display_name),
                    created_at: row.created_at,
                };
                (StatusCode::CREATED, Json(emoji)).into_response()
            }
            _ => {
                let _ = fs::remove_file(&file_path).await;
                err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to retrieve created emoji",
                )
            }
        },
        Err(e) => {
            let _ = fs::remove_file(&file_path).await;
            server_err(e)
        }
    }
}

/// DELETE /api/v1/projects/{prefix}/custom-emojis/{id}/
pub async fn delete_custom_emoji(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((prefix, emoji_id)): Path<(String, i64)>,
) -> Response {
    let project = match resolve_project(&state, &prefix).await {
        Ok(Some(p)) => p,
        Ok(None) => return err(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => return server_err(e),
    };

    if let Some(resp) = deny_unless_project_access(&state, project.id, auth.user_id).await {
        return resp;
    }

    let staff = caller_is_staff(&state, auth.user_id).await;

    match reaction_repo::find_emoji_by_id(&state.pool, emoji_id).await {
        Ok(Some((row, _, _, _, _))) => {
            if row.project_id != project.id as i64 {
                return err(StatusCode::NOT_FOUND, "見つかりません");
            }

            let can_manage = match project_team_repo::can_manage(
                &state.pool,
                project.id,
                auth.user_id,
                staff,
            )
            .await
            {
                Ok(v) => v,
                Err(e) => return server_err(e),
            };

            if row.uploaded_by != auth.user_id as i64 && !staff && !can_manage {
                return err(StatusCode::FORBIDDEN, "Only the uploader can delete this emoji");
            }

            let media_dir = PathBuf::from(&state.config.media_dir);
            let file_path = media_dir.join(&row.image_path);
            if let Err(e) = fs::remove_file(&file_path).await {
                tracing::warn!("Failed to delete emoji file {}: {}", file_path.display(), e);
            }

            match reaction_repo::delete_emoji(&state.pool, emoji_id).await {
                Ok(_) => StatusCode::NO_CONTENT.into_response(),
                Err(e) => server_err(e),
            }
        }
        Ok(None) => err(StatusCode::NOT_FOUND, "Custom emoji not found"),
        Err(e) => server_err(e),
    }
}
