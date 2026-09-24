/// presentation/handlers/project_team_api.rs — プロジェクトの担当(参加)チーム
///
/// - GET /api/v1/projects/{id}/teams/ : 参加チーム(利用状況つき)・変更権限・追加できるチーム
///   追加・除外(POST/DELETE)は resource_api にあり、権限と条件のチェックは
///   このモジュールの `authorize_*` を使う。
///
/// 設計: WIPAPPDEV-000069 第1段階(docs/詳細設計書_プロジェクト詳細タブ.md)

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::Serialize;

use crate::infrastructure::repositories::{project_team_repo, resource_repo, team_archive_repo, user_repo};
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::presentation::state::AppState;

#[derive(Serialize)]
pub struct ErrorResponse {
    pub detail: String,
}

pub fn error(status: StatusCode, detail: &str) -> Response {
    (status, Json(ErrorResponse { detail: detail.to_string() })).into_response()
}

pub fn server_error(e: anyhow::Error) -> Response {
    tracing::error!("DB operation failed: {:?}", e);
    error(StatusCode::INTERNAL_SERVER_ERROR, "サーバーエラーが発生しました")
}

async fn caller_is_staff(state: &AppState, user_id: i32) -> Result<bool, Response> {
    match user_repo::find_by_id(&state.pool, user_id).await {
        Ok(Some(u)) => Ok(u.is_staff),
        Ok(None) => Err(error(StatusCode::UNAUTHORIZED, "ユーザーが見つかりません")),
        Err(e) => Err(server_error(e)),
    }
}

const NO_PERMISSION: &str =
    "担当チームを変更できるのは、管理者・プロジェクトのオーナー・参加チームの管理者です";

/// チームを追加できるか。変更権限があり、かつ自分が所属するチーム(管理者は全チーム)であること。
pub async fn authorize_add(
    state: &AppState,
    user_id: i32,
    project_id: i32,
    team_id: i32,
) -> Result<(), Response> {
    let staff = caller_is_staff(state, user_id).await?;
    match project_team_repo::can_manage(&state.pool, project_id, user_id, staff).await {
        Ok(true) => {}
        Ok(false) => return Err(error(StatusCode::FORBIDDEN, NO_PERMISSION)),
        Err(e) => return Err(server_error(e)),
    }
    // アーカイブ済みのチームは、プロジェクトに追加できない
    match team_archive_repo::is_archived(&state.pool, team_id).await {
        Ok(false) => {}
        Ok(true) => return Err(error(StatusCode::CONFLICT, "アーカイブ済みのチームは追加できません(先に復元してください)")),
        Err(e) => return Err(server_error(e)),
    }
    if staff {
        return Ok(());
    }
    match project_team_repo::user_in_team(&state.pool, user_id, team_id).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(error(
            StatusCode::FORBIDDEN,
            "自分が所属していないチームは追加できません(追加すると、そのチームのメンバーがプロジェクトにアクセスできるようになります)",
        )),
        Err(e) => Err(server_error(e)),
    }
}

/// チームを外せるか。変更権限があり、そのチームのチケット・サイクルがこのプロジェクトに無いこと。
pub async fn authorize_remove(
    state: &AppState,
    user_id: i32,
    project_id: i32,
    team_id: i32,
) -> Result<(), Response> {
    let staff = caller_is_staff(state, user_id).await?;
    match project_team_repo::can_manage(&state.pool, project_id, user_id, staff).await {
        Ok(true) => {}
        Ok(false) => return Err(error(StatusCode::FORBIDDEN, NO_PERMISSION)),
        Err(e) => return Err(server_error(e)),
    }
    match project_team_repo::team_usage(&state.pool, project_id, team_id).await {
        Ok(usage) if usage.is_empty() => Ok(()),
        Ok(usage) => Err(error(StatusCode::CONFLICT, &usage.block_message())),
        Err(e) => Err(server_error(e)),
    }
}

/// 参加チームの一覧 GET /api/v1/projects/{id}/teams/
pub async fn teams_overview(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> Response {
    match resource_repo::find_project_by_id(&state.pool, id, Some(auth.user_id)).await {
        Ok(Some(_)) => {}
        Ok(None) => return error(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => return server_error(e),
    }
    let staff = match caller_is_staff(&state, auth.user_id).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    match project_team_repo::overview(&state.pool, id, auth.user_id, staff).await {
        Ok(out) => (StatusCode::OK, Json(out)).into_response(),
        Err(e) => server_error(e),
    }
}
