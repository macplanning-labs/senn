/// presentation/handlers/team_archive_api.rs — チームのアーカイブ・復元
///
/// - GET /api/v1/teams/{id}/archive-check/  アーカイブ可能か事前チェック
/// - POST /api/v1/teams/{id}/archive/    アーカイブ(閲覧専用にする)
/// - POST /api/v1/teams/{id}/unarchive/  復元
///
/// 権限: システム管理者 / そのチームの管理者。設計: WIPAPPDEV-000069 第2段階。

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};

use serde::Serialize;

use crate::infrastructure::repositories::{team_archive_repo, team_repo, user_repo};
use crate::presentation::handlers::project_team_api::{error, server_error};
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::presentation::state::AppState;

const NO_PERMISSION: &str = "チームをアーカイブ・復元できるのは、システム管理者とそのチームの管理者です";

/// アーカイブを止めているプロジェクト(応答用)
#[derive(Debug, Serialize)]
pub struct BlockingProjectOut {
    pub id: i32,
    pub prefix: String,
    pub name: String,
    pub status: String,
}

/// アーカイブ可能か事前チェックの応答
#[derive(Debug, Serialize)]
pub struct TeamArchiveCheckOut {
    #[serde(rename = "canArchive")]
    pub can_archive: bool,
    #[serde(rename = "blockingProjects")]
    pub blocking_projects: Vec<BlockingProjectOut>,
}

async fn authorize(state: &AppState, auth: &AuthUser, team_id: i32) -> Result<(), Response> {
    let staff = match user_repo::find_by_id(&state.pool, auth.user_id).await {
        Ok(Some(u)) => u.is_staff,
        Ok(None) => return Err(error(StatusCode::UNAUTHORIZED, "ユーザーが見つかりません")),
        Err(e) => return Err(server_error(e)),
    };
    match team_archive_repo::can_manage(&state.pool, team_id, auth.user_id, staff).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(error(StatusCode::FORBIDDEN, NO_PERMISSION)),
        Err(e) => Err(server_error(e)),
    }
}

async fn team_response(state: &AppState, team_id: i32) -> Response {
    match team_repo::find_team_by_id(&state.pool, team_id).await {
        Ok(Some(team)) => (StatusCode::OK, Json(team)).into_response(),
        Ok(None) => error(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => server_error(e),
    }
}

/// アーカイブ可能か事前チェック GET /api/v1/teams/{id}/archive-check/
pub async fn team_archive_check(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> Response {
    // 存在しないチームは、権限の有無を教えず 404
    match team_repo::find_team_by_id(&state.pool, id).await {
        Ok(Some(_)) => {}
        Ok(None) => return error(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => return server_error(e),
    }
    if let Err(resp) = authorize(&state, &auth, id).await {
        return resp;
    }
    match team_archive_repo::blocking_projects(&state.pool, id).await {
        Ok(blockers) => {
            let can_archive = blockers.is_empty();
            let blocking_projects = blockers
                .into_iter()
                .map(|p| BlockingProjectOut {
                    id: p.id,
                    prefix: p.prefix,
                    name: p.name,
                    status: p.status,
                })
                .collect();
            (StatusCode::OK, Json(TeamArchiveCheckOut { can_archive, blocking_projects })).into_response()
        }
        Err(e) => server_error(e),
    }
}

/// チームのアーカイブ POST /api/v1/teams/{id}/archive/
pub async fn team_archive(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> Response {
    // 存在しないチームは、権限の有無を教えず 404
    match team_repo::find_team_by_id(&state.pool, id).await {
        Ok(Some(_)) => {}
        Ok(None) => return error(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => return server_error(e),
    }
    if let Err(resp) = authorize(&state, &auth, id).await {
        return resp;
    }
    match team_archive_repo::archive(&state.pool, id, auth.user_id).await {
        Ok(true) => team_response(&state, id).await,
        Ok(false) => error(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => match e.downcast_ref::<team_archive_repo::TeamArchiveBlocked>() {
            Some(blocked) => error(StatusCode::CONFLICT, &blocked.user_message()),
            None => server_error(e),
        },
    }
}

/// チームの復元 POST /api/v1/teams/{id}/unarchive/
pub async fn team_unarchive(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> Response {
    match team_repo::find_team_by_id(&state.pool, id).await {
        Ok(Some(_)) => {}
        Ok(None) => return error(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => return server_error(e),
    }
    if let Err(resp) = authorize(&state, &auth, id).await {
        return resp;
    }
    match team_archive_repo::unarchive(&state.pool, id).await {
        Ok(true) => team_response(&state, id).await,
        Ok(false) => error(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => server_error(e),
    }
}

/// チケット・サイクル等の書き込みが「アーカイブ済みチーム(閲覧専用)」で拒否されたなら、
/// 500 ではなく 409 と理由を返すためのレスポンスを作る。
pub fn archived_conflict(e: &anyhow::Error) -> Option<Response> {
    if team_archive_repo::is_team_archived_error(e.as_ref()) {
        Some(error(StatusCode::CONFLICT, team_archive_repo::ARCHIVED_READ_ONLY_MESSAGE))
    } else {
        None
    }
}
