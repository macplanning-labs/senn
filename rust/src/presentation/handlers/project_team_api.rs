/// presentation/handlers/project_team_api.rs — プロジェクトの担当(参加)チーム
///
/// - GET /api/v1/projects/{id}/teams/ : 参加チーム(利用状況つき)・変更権限・追加できるチーム
///   追加・除外(POST/DELETE)は resource_api にあり、権限と条件のチェックは
///   このモジュールの `authorize_*` を使う。
///
/// 設計: DEMO-000069 第1段階(docs/design/詳細設計書_プロジェクト詳細タブ.md)
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

use crate::domain::access::{sees_team, Action, Viewer};
use crate::infrastructure::access::shadow::{self, Mode, Resource};
use crate::infrastructure::repositories::{
    project_team_repo, resource_repo, team_archive_repo, user_repo,
};
use crate::presentation::extractors::authorize;
use crate::presentation::state::AppState;

#[derive(Serialize)]
pub struct ErrorResponse {
    pub detail: String,
}

pub fn error(status: StatusCode, detail: &str) -> Response {
    (
        status,
        Json(ErrorResponse {
            detail: detail.to_string(),
        }),
    )
        .into_response()
}

pub fn server_error(e: anyhow::Error) -> Response {
    tracing::error!("DB operation failed: {:?}", e);
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "サーバーエラーが発生しました",
    )
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

/// 変更権限(今の判定): 管理者・プロジェクトのオーナー・参加チームの管理者
async fn legacy_can_manage(
    state: &AppState,
    project_id: i32,
    user_id: i32,
    staff: bool,
) -> Result<Result<(), Response>, Response> {
    match project_team_repo::can_manage(&state.pool, project_id, user_id, staff).await {
        Ok(true) => Ok(Ok(())),
        Ok(false) => Ok(Err(error(StatusCode::FORBIDDEN, NO_PERMISSION))),
        Err(e) => Err(server_error(e)),
    }
}

/// チームを追加できるか。変更権限があり、かつ自分が所属するチーム(管理者は全チーム)であること。
///
/// アクセス制御の再設計(D-3)の新しい判定: プロジェクトの設定を管理でき(ManageSettings)、
/// 追加するチームが見えること(Read)。アーカイブ済みのチームを断る規則は、どちらの判定でも適用する。
pub async fn authorize_add(
    state: &AppState,
    viewer: &Viewer,
    project_id: i32,
    team_id: i32,
    route: &'static str,
) -> Result<(), Response> {
    let user_id = viewer.require_user_id()?;
    let staff = caller_is_staff(state, user_id).await?;
    let legacy = legacy_can_manage(state, project_id, user_id, staff).await?;
    authorize::gate_project(
        &state.pool,
        viewer,
        project_id,
        Action::ManageSettings,
        legacy,
        route,
    )
    .await?;
    // アーカイブ済みのチームは、プロジェクトに追加できない
    match team_archive_repo::is_archived(&state.pool, team_id).await {
        Ok(false) => {}
        Ok(true) => {
            return Err(error(
                StatusCode::CONFLICT,
                "アーカイブ済みのチームは追加できません(先に復元してください)",
            ))
        }
        Err(e) => return Err(server_error(e)),
    }
    let legacy = if staff {
        Ok(())
    } else {
        match project_team_repo::user_in_team(&state.pool, user_id, team_id).await {
            Ok(true) => Ok(()),
            Ok(false) => Err(error(
                StatusCode::FORBIDDEN,
                "自分が所属していないチームは追加できません(追加すると、そのチームのメンバーがプロジェクトにアクセスできるようになります)",
            )),
            Err(e) => return Err(server_error(e)),
        }
    };
    authorize::gate_team(&state.pool, viewer, team_id, Action::Read, legacy, route).await
}

/// チームを外せるか。変更権限があり、そのチームのチケット・サイクルがこのプロジェクトに無いこと。
///
/// 新しい判定(D-3): プロジェクトの設定を管理できること(ManageSettings)。使用中のチームを断る規則は、
/// どちらの判定でも適用する。
pub async fn authorize_remove(
    state: &AppState,
    viewer: &Viewer,
    project_id: i32,
    team_id: i32,
    route: &'static str,
) -> Result<(), Response> {
    let user_id = viewer.require_user_id()?;
    let staff = caller_is_staff(state, user_id).await?;
    let legacy = legacy_can_manage(state, project_id, user_id, staff).await?;
    authorize::gate_project(
        &state.pool,
        viewer,
        project_id,
        Action::ManageSettings,
        legacy,
        route,
    )
    .await?;
    match project_team_repo::team_usage(&state.pool, project_id, team_id).await {
        Ok(usage) if usage.is_empty() => Ok(()),
        Ok(usage) => Err(error(StatusCode::CONFLICT, &usage.block_message())),
        Err(e) => Err(server_error(e)),
    }
}

/// 参加チームの一覧 GET /api/v1/projects/{id}/teams/
pub async fn teams_overview(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
) -> Response {
    let user_id = match viewer.require_user_id() {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    let legacy = match resource_repo::find_project_by_id(&state.pool, id, Some(user_id)).await {
        Ok(Some(_)) => Ok(()),
        Ok(None) => Err(error(StatusCode::NOT_FOUND, "見つかりません")),
        Err(e) => return server_error(e),
    };
    if let Err(resp) = authorize::gate_project(
        &state.pool,
        &viewer,
        id,
        Action::Read,
        legacy,
        "GET /api/v1/projects/{id}/teams/",
    )
    .await
    {
        return resp;
    }
    let staff = match caller_is_staff(&state, user_id).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    match project_team_repo::overview(&state.pool, id, user_id, staff).await {
        Ok(mut out) => {
            // 新しい判定(`on`)では、見えない参加チームは名前を出さずに数だけ返す。
            // 追加できるチームも、見えるチームに限る
            if shadow::mode(Resource::Project) == Mode::On {
                let before = out.teams.len();
                out.teams
                    .retain(|t| sees_team(&viewer, &viewer.team_facts_for_read(t.id)));
                out.hidden_team_count = (before - out.teams.len()) as i64;
                out.addable_teams
                    .retain(|t| sees_team(&viewer, &viewer.team_facts_for_read(t.id)));
            }
            (StatusCode::OK, Json(out)).into_response()
        }
        Err(e) => server_error(e),
    }
}
