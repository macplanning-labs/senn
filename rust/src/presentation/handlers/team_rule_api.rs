/// presentation/handlers/team_rule_api.rs — Team Rules JSON API
///
/// Django /api/v1/team-rules/ と挙動を一致させるハンドラー。
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::domain::access::{Action, ResourceRef, Viewer};
use crate::domain::models::team_rule_api::*;
use crate::infrastructure::access::shadow::{self, Mode, Resource};
use crate::infrastructure::repositories::team_rule_repo;
use crate::presentation::extractors::authorize;
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::presentation::state::AppState;

// =============================================================================
// リクエスト構造体
// =============================================================================

#[derive(Deserialize)]
pub struct TeamRuleListQuery {
    pub team: Option<i32>,
    pub category: Option<String>,
    pub is_active: Option<bool>,
    pub page: Option<i64>,
}

// =============================================================================
// レスポンス構造体
// =============================================================================

#[derive(Serialize)]
pub struct PaginatedTeamRulesOut {
    pub count: i64,
    pub next: Option<String>,
    pub previous: Option<String>,
    pub results: Vec<TeamRuleOut>,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub detail: String,
}

// アクセス制御の再設計(D-4)。切り替えは `ACCESS_ENFORCE_TEAM`。今の判定には確認が無い。
// 閲覧: チームのルールはチームが見える人、全体のルール(チームなし)は全員。
// 変更: チームのルールはチームの設定を管理できる人、全体のルールはシステム管理者だけ。

/// ルールの判定(`team` は判定するルールの所属)
async fn rule_gate(
    state: &AppState,
    viewer: &Viewer,
    team: Option<i32>,
    action: Action,
    id: i32,
    route: &'static str,
) -> Result<(), axum::response::Response> {
    authorize::gate_scoped(
        &state.pool,
        viewer,
        team,
        None,
        action,
        Action::ManageSettings,
        Resource::Team,
        id,
        route,
    )
    .await
}

/// 既存のルールの判定(見つからない場合、`on` では 404、それ以外は今の処理に任せる)
async fn existing_rule_gate(
    state: &AppState,
    viewer: &Viewer,
    id: i32,
    action: Action,
    route: &'static str,
) -> Result<(), axum::response::Response> {
    match team_rule_repo::find_team_rule_by_id(&state.pool, id).await {
        Ok(Some(rule)) => rule_gate(state, viewer, rule.team, action, id, route).await,
        Ok(None) if shadow::mode(Resource::Team) == Mode::On => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "見つかりません".to_string(),
            }),
        )
            .into_response()),
        Ok(None) => Ok(()),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response())
        }
    }
}

// =============================================================================
// Team Rules
// =============================================================================

/// チームルール一覧 GET /api/v1/team-rules/
pub async fn team_rule_list(
    State(state): State<AppState>,
    viewer: Viewer,
    Query(params): Query<TeamRuleListQuery>,
) -> impl IntoResponse {
    let page = params.page.unwrap_or(1).max(1);
    const PAGE_SIZE: i64 = 50;

    // 新しい判定(on)では SQL で絞る(件数・ページ送りも合わせる)
    let scope = (shadow::mode(Resource::Team) == Mode::On).then(|| viewer.scope());

    // チームルール一覧取得
    let rules = match team_rule_repo::find_all_team_rules(
        &state.pool,
        page,
        params.team,
        params.category.clone(),
        params.is_active,
        scope.as_ref(),
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response();
        }
    };

    // 試運転: 見えなくなる行を記録する
    let rules = authorize::filter_list(
        &state.pool,
        &viewer,
        Resource::Team,
        "GET /api/v1/team-rules/",
        rules,
        |r| r.id as i64,
        |r| match r.team {
            Some(t) => ResourceRef::Team(viewer.team_facts_for_read(t)),
            None => ResourceRef::GlobalMaster,
        },
    );

    // 件数取得
    let count = match team_rule_repo::count_team_rules(
        &state.pool,
        params.team,
        params.category.clone(),
        params.is_active,
        scope.as_ref(),
    )
    .await
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response();
        }
    };

    let has_next = page * PAGE_SIZE < count;
    let has_previous = page > 1;

    let mut query_params = String::new();
    if let Some(team) = params.team {
        query_params.push_str(&format!("team={}", team));
    }
    if let Some(cat) = &params.category {
        if !query_params.is_empty() {
            query_params.push('&');
        }
        query_params.push_str(&format!("category={}", cat));
    }
    if let Some(active) = params.is_active {
        if !query_params.is_empty() {
            query_params.push('&');
        }
        query_params.push_str(&format!("is_active={}", active));
    }

    let next = if has_next {
        if query_params.is_empty() {
            Some(format!("?page={}", page + 1))
        } else {
            Some(format!("?{}&page={}", query_params, page + 1))
        }
    } else {
        None
    };

    let previous = if has_previous {
        if page == 2 {
            if query_params.is_empty() {
                Some("?".to_string())
            } else {
                Some(format!("?{}", query_params))
            }
        } else {
            if query_params.is_empty() {
                Some(format!("?page={}", page - 1))
            } else {
                Some(format!("?{}&page={}", query_params, page - 1))
            }
        }
    } else {
        None
    };

    (
        StatusCode::OK,
        Json(PaginatedTeamRulesOut {
            count,
            next,
            previous,
            results: rules,
        }),
    )
        .into_response()
}

/// チームルール詳細 GET /api/v1/team-rules/{id}/
pub async fn team_rule_detail(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match team_rule_repo::find_team_rule_by_id(&state.pool, id).await {
        Ok(Some(rule)) => {
            if let Err(resp) = rule_gate(
                &state,
                &viewer,
                rule.team,
                Action::Read,
                id,
                "GET /api/v1/team-rules/{id}/",
            )
            .await
            {
                return resp;
            }
            (StatusCode::OK, Json(rule)).into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "見つかりません".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// チームルール作成 POST /api/v1/team-rules/
pub async fn team_rule_create(
    State(state): State<AppState>,
    viewer: Viewer,
    Json(body): Json<TeamRuleWriteIn>,
) -> impl IntoResponse {
    let auth = AuthUser {
        user_id: match viewer.require_user_id() {
            Ok(id) => id,
            Err(resp) => return resp,
        },
    };
    if let Err(resp) = rule_gate(
        &state,
        &viewer,
        body.team,
        Action::Create,
        0,
        "POST /api/v1/team-rules/",
    )
    .await
    {
        return resp;
    }
    match team_rule_repo::create_team_rule(&state.pool, &body, auth.user_id).await {
        Ok(rule_id) => {
            // 作成したチームルールを返す
            match team_rule_repo::find_team_rule_by_id(&state.pool, rule_id).await {
                Ok(Some(rule)) => (StatusCode::CREATED, Json(rule)).into_response(),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response(),
            }
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// チームルール更新 PATCH /api/v1/team-rules/{id}/
pub async fn team_rule_update(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
    Json(body): Json<crate::domain::models::team_rule_api::TeamRuleUpdateIn>,
) -> impl IntoResponse {
    const ROUTE: &str = "PATCH /api/v1/team-rules/{id}/";
    if let Err(resp) = existing_rule_gate(&state, &viewer, id, Action::Write, ROUTE).await {
        return resp;
    }
    // 別のチームへ移す場合は、移し先でも判定する
    if body.team.is_some() {
        if let Err(resp) = rule_gate(&state, &viewer, body.team, Action::Write, id, ROUTE).await {
            return resp;
        }
    }
    match team_rule_repo::partial_update_team_rule(&state.pool, id, &body).await {
        Ok(true) => {
            // 更新後のチームルールを返す
            match team_rule_repo::find_team_rule_by_id(&state.pool, id).await {
                Ok(Some(rule)) => (StatusCode::OK, Json(rule)).into_response(),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response(),
            }
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "見つかりません".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// チームルール削除 DELETE /api/v1/team-rules/{id}/
pub async fn team_rule_delete(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    if let Err(resp) = existing_rule_gate(
        &state,
        &viewer,
        id,
        Action::Delete,
        "DELETE /api/v1/team-rules/{id}/",
    )
    .await
    {
        return resp;
    }
    match team_rule_repo::delete_team_rule(&state.pool, id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "見つかりません".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}
