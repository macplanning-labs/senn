/// presentation/handlers/chat_integration_api.rs — チャット通知連携 JSON API
///
/// chat-integrations CRUD(JWT認証必須)。
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::domain::access::Viewer;
use crate::domain::models::chat_integration_api::*;
use crate::infrastructure::repositories::chat_integration_repo;
use crate::presentation::state::AppState;

fn err(detail: &str) -> serde_json::Value {
    json!({"detail": detail})
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub project: Option<i32>,
    pub team: Option<i32>,
}

/// GET /api/v1/chat-integrations/?project=<id> または ?team=<id>
pub async fn list(
    State(state): State<AppState>,
    viewer: Viewer,
    Query(params): Query<ListQuery>,
) -> impl IntoResponse {
    if let Err(r) = crate::presentation::handlers::access_guard::require_scope_manager_v(
        &state.pool,
        &viewer,
        params.project,
        params.team,
        "GET /api/v1/chat-integrations/",
    )
    .await
    {
        return r;
    }

    match chat_integration_repo::find_all(&state.pool, params.project, params.team).await {
        Ok(items) => (StatusCode::OK, Json(items)).into_response(),
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

/// POST /api/v1/chat-integrations/
pub async fn create(
    State(state): State<AppState>,
    viewer: Viewer,
    Json(body): Json<ChatIntegrationWriteIn>,
) -> impl IntoResponse {
    if let Err(r) = crate::presentation::handlers::access_guard::require_scope_manager_v(
        &state.pool,
        &viewer,
        body.project,
        body.team,
        "POST /api/v1/chat-integrations/",
    )
    .await
    {
        return r;
    }

    match chat_integration_repo::create(
        &state.pool,
        &body,
        match viewer.require_user_id() {
            Ok(id) => id,
            Err(resp) => return resp,
        },
    )
    .await
    {
        Ok(id) => match chat_integration_repo::find_by_id(&state.pool, id).await {
            Ok(Some(item)) => (StatusCode::CREATED, Json(item)).into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response(),
        },
        Err(e) if e.to_string().contains("exactly one of project or team") => (
            StatusCode::BAD_REQUEST,
            Json(err("project か team のどちらか一方が必要です")),
        )
            .into_response(),
        Err(e) if e.to_string().contains("invalid provider") => {
            (StatusCode::BAD_REQUEST, Json(err("provider が不正です"))).into_response()
        }
        Err(e)
            if e.to_string().contains("requires")
                || e.to_string().contains("webhook_url is required") =>
        {
            (StatusCode::BAD_REQUEST, Json(err(&e.to_string()))).into_response()
        }
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

/// PATCH /api/v1/chat-integrations/{id}/
pub async fn update(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
    Json(body): Json<ChatIntegrationUpdateIn>,
) -> impl IntoResponse {
    let existing = match chat_integration_repo::find_by_id(&state.pool, id).await {
        Ok(Some(item)) => item,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(err("見つかりません"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
    };
    if let Err(r) = crate::presentation::handlers::access_guard::require_scope_manager_v(
        &state.pool,
        &viewer,
        existing.project,
        existing.team,
        "PATCH /api/v1/chat-integrations/{id}/",
    )
    .await
    {
        return r;
    }
    // 付け先を変えるときは、変更先も管理できること(管理していない別チームへ移させない。DEMO-000169)。
    // project / team のどちらかが指定されたら、付け先は (body.project, body.team) になる(chat_integration_repo::update)
    if body.project.is_some() || body.team.is_some() {
        let target = (body.project, body.team);
        if target != (existing.project, existing.team) {
            if let Err(r) = crate::presentation::handlers::access_guard::require_scope_manager_v(
                &state.pool,
                &viewer,
                target.0,
                target.1,
                "PATCH /api/v1/chat-integrations/{id}/",
            )
            .await
            {
                return r;
            }
        }
    }

    match chat_integration_repo::update(&state.pool, id, &body).await {
        Ok(true) => match chat_integration_repo::find_by_id(&state.pool, id).await {
            Ok(Some(item)) => (StatusCode::OK, Json(item)).into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response(),
        },
        Ok(false) => (StatusCode::NOT_FOUND, Json(err("見つかりません"))).into_response(),
        Err(e) if e.to_string().contains("exactly one of project or team") => (
            StatusCode::BAD_REQUEST,
            Json(err("project か team のどちらか一方が必要です")),
        )
            .into_response(),
        Err(e) if e.to_string().contains("invalid provider") => {
            (StatusCode::BAD_REQUEST, Json(err("provider が不正です"))).into_response()
        }
        Err(e)
            if e.to_string().contains("requires")
                || e.to_string().contains("webhook_url is required") =>
        {
            (StatusCode::BAD_REQUEST, Json(err(&e.to_string()))).into_response()
        }
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

/// DELETE /api/v1/chat-integrations/{id}/
pub async fn delete(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    let existing = match chat_integration_repo::find_by_id(&state.pool, id).await {
        Ok(Some(item)) => item,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(err("見つかりません"))).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
    };
    if let Err(r) = crate::presentation::handlers::access_guard::require_scope_manager_v(
        &state.pool,
        &viewer,
        existing.project,
        existing.team,
        "DELETE /api/v1/chat-integrations/{id}/",
    )
    .await
    {
        return r;
    }

    match chat_integration_repo::delete(&state.pool, id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, Json(err("見つかりません"))).into_response(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        add_test_team_member, create_test_team, create_test_user, test_pool, test_state,
        test_viewer,
    };

    /// 管理しているチームのチャット連携を、管理していない別チームへ移せない(DEMO-000169)
    #[tokio::test]
    async fn cannot_move_chat_integration_to_unmanaged_team() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let me = create_test_user(&pool, "ci-me").await;
        let other_owner = create_test_user(&pool, "ci-oo").await;
        let mine = create_test_team(&pool, "ci-a").await;
        let theirs = create_test_team(&pool, "ci-b").await;
        add_test_team_member(&pool, mine, me, "admin").await;
        add_test_team_member(&pool, theirs, other_owner, "admin").await;
        let id: i32 = sqlx::query_scalar(
            "INSERT INTO t_chat_integration (project_id, team_id, provider, webhook_url, enabled_categories)
             VALUES (NULL, $1::int4, 'slack', 'https://example.invalid/hook', '{assigned}') RETURNING id::int4",
        )
        .bind(mine)
        .fetch_one(&pool)
        .await
        .unwrap();

        let patch = |team: i32| {
            let pool = pool.clone();
            async move {
                let body: ChatIntegrationUpdateIn =
                    serde_json::from_value(json!({ "team": team })).unwrap();
                update(
                    State(test_state(&pool).await),
                    test_viewer(&pool, me).await,
                    Path(id),
                    Json(body),
                )
                .await
                .into_response()
                .status()
            }
        };
        assert_eq!(patch(theirs).await, StatusCode::FORBIDDEN);
        let team: Option<i32> =
            sqlx::query_scalar("SELECT team_id::int4 FROM t_chat_integration WHERE id = $1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(team, Some(mine));
        // 付け先を変えない更新は、今までどおり通る
        assert_eq!(patch(mine).await, StatusCode::OK);
    }
}
