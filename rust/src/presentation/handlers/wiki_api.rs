/// presentation/handlers/wiki_api.rs — Wiki JSON API
///
/// Django /api/v1/wiki/* と挙動を一致させるハンドラー。

use axum::{
    extract::{State, Path, Query},
    response::IntoResponse,
    http::StatusCode,
    Json,
    Extension,
};
use serde::{Deserialize, Serialize};

use crate::presentation::state::AppState;
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::infrastructure::repositories::wiki_api_repo;
use crate::domain::models::wiki_api::*;

#[derive(Deserialize)]
pub struct ListQuery {
    pub project: Option<i32>,
    pub team: Option<i32>,
    pub category: Option<String>,
    pub search: Option<String>,
    pub page: Option<i64>,
}

#[derive(Serialize)]
pub struct PaginatedWikiOut {
    pub count: i64,
    pub next: Option<String>,
    pub previous: Option<String>,
    pub results: Vec<WikiPageListOut>,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub detail: String,
}

const NO_WIKI_WRITE_PERMISSION: &str = "このWikiを変更する権限がありません";

fn forbidden() -> axum::response::Response {
    (StatusCode::FORBIDDEN, Json(ErrorResponse { detail: NO_WIKI_WRITE_PERMISSION.to_string() })).into_response()
}

fn server_error() -> axum::response::Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() }),
    )
        .into_response()
}

/// 指定した所属（プロジェクト／チーム）に書き込めるか。作成と、移動先の確認に使う。
pub(crate) async fn authorize_scope_write(
    pool: &sqlx::PgPool,
    user_id: i32,
    project: Option<i32>,
    team: Option<i32>,
) -> Result<(), axum::response::Response> {
    match wiki_api_repo::can_write_scope(pool, user_id, project, team).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(forbidden()),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            Err(server_error())
        }
    }
}

/// 既存ページへの書き込み権限（編集・紐付け・削除）。
/// ページが無ければ 404。`delete` が true のとき、どこにも属さない共有ページは staff か作成者だけ。
pub(crate) async fn authorize_page_write(
    pool: &sqlx::PgPool,
    user_id: i32,
    page_id: i32,
    delete: bool,
) -> Result<wiki_api_repo::WikiScope, axum::response::Response> {
    let scope = match wiki_api_repo::find_scope(pool, page_id).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse { detail: "見つかりません".to_string() }),
            )
                .into_response());
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return Err(server_error());
        }
    };
    authorize_scope_write(pool, user_id, scope.project, scope.team).await?;
    if delete && scope.project.is_none() && scope.team.is_none() && scope.author_id != user_id {
        let is_staff: bool = sqlx::query_scalar("SELECT is_staff FROM accounts_user WHERE id = $1")
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| {
                tracing::error!("DB operation failed: {:?}", e);
                server_error()
            })?
            .unwrap_or(false);
        if !is_staff {
            return Err(forbidden());
        }
    }
    Ok(scope)
}

/// GET /api/v1/wiki/
#[utoipa::path(
    get,
    path = "/api/v1/wiki/",
    tag = "wiki",
    responses(
        (status = 200, description = "Wikiページ一覧を返す")
    )
)]
pub async fn list(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Query(params): Query<ListQuery>,
) -> impl IntoResponse {
    let page = params.page.unwrap_or(1).max(1);
    const PAGE_SIZE: i64 = 50;

    let items = match wiki_api_repo::find_all(
        &state.pool,
        params.project,
        params.team,
        params.category.as_deref(),
        params.search.as_deref(),
        page,
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() }),
            )
                .into_response();
        }
    };

    let count = match wiki_api_repo::count_all(
        &state.pool,
        params.project,
        params.team,
        params.category.as_deref(),
        params.search.as_deref(),
    )
    .await
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() }),
            )
                .into_response();
        }
    };

    let has_next = page * PAGE_SIZE < count;
    let has_previous = page > 1;
    let next = if has_next { Some(format!("?page={}", page + 1)) } else { None };
    let previous = if has_previous { Some(format!("?page={}", page - 1)) } else { None };

    (
        StatusCode::OK,
        Json(PaginatedWikiOut { count, next, previous, results: items }),
    )
        .into_response()
}

/// GET /api/v1/wiki/{id}/
#[utoipa::path(
    get,
    path = "/api/v1/wiki/{id}/",
    tag = "wiki",
    params(
        ("id" = i32, Path, description = "WikiページID")
    ),
    responses(
        (status = 200, description = "Wikiページ詳細を返す"),
        (status = 404, description = "ページが見つからない")
    )
)]
pub async fn detail(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match wiki_api_repo::find_by_id(&state.pool, id).await {
        Ok(Some(item)) => (StatusCode::OK, Json(item)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse { detail: "見つかりません".to_string() }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() }),
            )
                .into_response()
        }
    }
}

/// POST /api/v1/wiki/
pub async fn create(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<WikiPageCreateIn>,
) -> impl IntoResponse {
    if let Err(resp) = authorize_scope_write(&state.pool, auth.user_id, body.project, body.team).await {
        return resp;
    }
    match wiki_api_repo::create(&state.pool, &body, auth.user_id).await {
        Ok(id) => match wiki_api_repo::find_by_id(&state.pool, id).await {
            Ok(Some(item)) => (StatusCode::CREATED, Json(item)).into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() }),
            )
                .into_response(),
        },
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() }),
            )
                .into_response()
        }
    }
}

/// PUT/PATCH /api/v1/wiki/{id}/
pub async fn update(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<i32>,
    Json(body): Json<WikiPageUpdateIn>,
) -> impl IntoResponse {
    let scope = match authorize_page_write(&state.pool, auth.user_id, id, false).await {
        Ok(s) => s,
        Err(resp) => return resp,
    };
    let new_project = body.project.or(scope.project);
    let new_team = body.team.or(scope.team);
    if (new_project, new_team) != (scope.project, scope.team) {
        if let Err(resp) = authorize_scope_write(&state.pool, auth.user_id, new_project, new_team).await {
            return resp;
        }
    }
    match wiki_api_repo::update(&state.pool, id, &body, auth.user_id).await {
        Ok(true) => match wiki_api_repo::find_by_id(&state.pool, id).await {
            Ok(Some(item)) => (StatusCode::OK, Json(item)).into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() }),
            )
                .into_response(),
        },
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse { detail: "見つかりません".to_string() }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() }),
            )
                .into_response()
        }
    }
}

/// DELETE /api/v1/wiki/{id}/
pub async fn delete(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    if let Err(resp) = authorize_page_write(&state.pool, auth.user_id, id, true).await {
        return resp;
    }
    match wiki_api_repo::delete(&state.pool, id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse { detail: "見つかりません".to_string() }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() }),
            )
                .into_response()
        }
    }
}

/// GET /api/v1/wiki/{id}/revisions/
pub async fn revisions(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match wiki_api_repo::find_revisions(&state.pool, id).await {
        Ok(items) => (StatusCode::OK, Json(items)).into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() }),
            )
                .into_response()
        }
    }
}

#[derive(Serialize)]
struct LinkActionOut {
    detail: String,
    ticket_id: i32,
}

/// POST /api/v1/wiki/{id}/link-ticket/
pub async fn link_ticket(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<i32>,
    Json(body): Json<LinkTicketIn>,
) -> impl IntoResponse {
    if let Err(resp) = authorize_page_write(&state.pool, auth.user_id, id, false).await {
        return resp;
    }

    match wiki_api_repo::page_exists(&state.pool, id).await {
        Ok(true) => {}
        Ok(false) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse { detail: "見つかりません".to_string() }),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() }),
            )
                .into_response();
        }
    }

    match wiki_api_repo::ticket_exists(&state.pool, body.ticket_id).await {
        Ok(true) => {}
        Ok(false) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse { detail: "Ticket not found".to_string() }),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() }),
            )
                .into_response();
        }
    }

    match crate::infrastructure::repositories::membership_repo::check_ticket_access(&state.pool, body.ticket_id, auth.user_id).await {
        Ok(true) => {}
        Ok(false) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse { detail: "Ticket not found".to_string() }),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() }),
            )
                .into_response();
        }
    }

    match wiki_api_repo::link_ticket(&state.pool, id, body.ticket_id).await {
        Ok(()) => (
            StatusCode::OK,
            Json(LinkActionOut { detail: "linked".to_string(), ticket_id: body.ticket_id }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() }),
            )
                .into_response()
        }
    }
}

/// POST /api/v1/wiki/{id}/unlink-ticket/
pub async fn unlink_ticket(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<i32>,
    Json(body): Json<LinkTicketIn>,
) -> impl IntoResponse {
    if let Err(resp) = authorize_page_write(&state.pool, auth.user_id, id, false).await {
        return resp;
    }
    match wiki_api_repo::unlink_ticket(&state.pool, id, body.ticket_id).await {
        Ok(()) => (
            StatusCode::OK,
            Json(LinkActionOut { detail: "unlinked".to_string(), ticket_id: body.ticket_id }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("DB operation failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { detail: "サーバーエラーが発生しました".to_string() }),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;

    async fn set_role(pool: &sqlx::PgPool, team_id: i32, user_id: i32, role: &str) {
        sqlx::query(
            "INSERT INTO t_team_membership (team_id, user_id, role, joined_at) VALUES ($1, $2, $3, NOW())",
        )
        .bind(team_id as i64)
        .bind(user_id as i64)
        .bind(role)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn set_staff(pool: &sqlx::PgPool, user_id: i32) {
        sqlx::query("UPDATE accounts_user SET is_staff = true WHERE id = $1")
            .bind(user_id as i64)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn create_wiki_page(
        pool: &sqlx::PgPool,
        project: Option<i32>,
        team: Option<i32>,
        author_id: i32,
    ) -> i32 {
        let suffix = test_support::unique_suffix();
        let title = format!("wiki_{}", suffix);
        let slug = format!("wiki-{}", suffix);
        sqlx::query_scalar::<_, i32>(
            "INSERT INTO wiki_page (project_id, team_id, title, slug, category, content, author_id, last_editor_id, created_at, updated_at)
             VALUES ($1, $2, $3, $4, 'general', 'content', $5, $5, NOW(), NOW())
             RETURNING id::int4"
        )
        .bind(project.map(|p| p as i64))
        .bind(team.map(|t| t as i64))
        .bind(&title)
        .bind(&slug)
        .bind(author_id as i64)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn a1_authorize_page_write_non_member_returns_403() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let owner = test_support::create_test_user(&pool, "a1o").await;
        let outsider = test_support::create_test_user(&pool, "a1x").await;
        let project = test_support::create_test_project(&pool, "A1", owner).await;

        let page_id = create_wiki_page(&pool, Some(project), None, owner).await;

        let result = authorize_page_write(&pool, outsider, page_id, false).await;
        assert!(result.is_err(), "権限なしは Err を返すべき");
        if let Err(response) = result {
            let status_code = response.status();
            assert_eq!(status_code, StatusCode::FORBIDDEN, "403 Forbidden が返されるべき");
        }
    }

    #[tokio::test]
    async fn a2_authorize_page_write_team_member_returns_ok() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let owner = test_support::create_test_user(&pool, "a2o").await;
        let member = test_support::create_test_user(&pool, "a2m").await;
        let project = test_support::create_test_project(&pool, "A2", owner).await;
        let team = sqlx::query_scalar::<_, i32>(
            "SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1"
        )
        .bind(project as i64)
        .fetch_one(&pool)
        .await
        .unwrap();
        set_role(&pool, team, member, "member").await;

        let page_id = create_wiki_page(&pool, Some(project), None, owner).await;

        let result = authorize_page_write(&pool, member, page_id, false).await;
        assert!(result.is_ok(), "参加チームメンバーは Ok を返すべき");
    }

    #[tokio::test]
    async fn a3_authorize_page_write_nonexistent_page_returns_404() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let user = test_support::create_test_user(&pool, "a3").await;

        let result = authorize_page_write(&pool, user, i32::MAX, false).await;
        assert!(result.is_err(), "存在しないページは Err を返すべき");
        if let Err(response) = result {
            let status_code = response.status();
            assert_eq!(status_code, StatusCode::NOT_FOUND, "404 Not Found が返されるべき");
        }
    }

    #[tokio::test]
    async fn a4_delete_shared_page_non_author_non_staff_returns_403() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let author = test_support::create_test_user(&pool, "a4a").await;
        let other = test_support::create_test_user(&pool, "a4o").await;

        let page_id = create_wiki_page(&pool, None, None, author).await;

        let result = authorize_page_write(&pool, other, page_id, true).await;
        assert!(result.is_err(), "共有ページ削除は作成者でない非staff は Err を返すべき");
        if let Err(response) = result {
            let status_code = response.status();
            assert_eq!(status_code, StatusCode::FORBIDDEN, "403 Forbidden が返されるべき");
        }
    }

    #[tokio::test]
    async fn a5_delete_shared_page_author_returns_ok() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let author = test_support::create_test_user(&pool, "a5a").await;

        let page_id = create_wiki_page(&pool, None, None, author).await;

        let result = authorize_page_write(&pool, author, page_id, true).await;
        assert!(result.is_ok(), "共有ページ削除は作成者は Ok を返すべき");
    }

    #[tokio::test]
    async fn a6_delete_shared_page_staff_returns_ok() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let author = test_support::create_test_user(&pool, "a6a").await;
        let staff = test_support::create_test_user(&pool, "a6s").await;
        set_staff(&pool, staff).await;

        let page_id = create_wiki_page(&pool, None, None, author).await;

        let result = authorize_page_write(&pool, staff, page_id, true).await;
        assert!(result.is_ok(), "共有ページ削除は staff は Ok を返すべき");
    }

    #[tokio::test]
    async fn a7_delete_project_page_team_member_returns_ok() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let owner = test_support::create_test_user(&pool, "a7o").await;
        let member = test_support::create_test_user(&pool, "a7m").await;
        let project = test_support::create_test_project(&pool, "A7", owner).await;
        let team = sqlx::query_scalar::<_, i32>(
            "SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1"
        )
        .bind(project as i64)
        .fetch_one(&pool)
        .await
        .unwrap();
        set_role(&pool, team, member, "member").await;

        let page_id = create_wiki_page(&pool, Some(project), None, owner).await;

        let result = authorize_page_write(&pool, member, page_id, true).await;
        assert!(result.is_ok(), "プロジェクト所属ページ削除は所属チームメンバーは Ok を返すべき");
    }

    #[tokio::test]
    async fn a8_authorize_scope_write_non_member_project_returns_403() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let owner = test_support::create_test_user(&pool, "a8o").await;
        let outsider = test_support::create_test_user(&pool, "a8x").await;
        let project = test_support::create_test_project(&pool, "A8", owner).await;

        let result = authorize_scope_write(&pool, outsider, Some(project), None).await;
        assert!(result.is_err(), "権限なしプロジェクトは Err を返すべき");
        if let Err(response) = result {
            let status_code = response.status();
            assert_eq!(status_code, StatusCode::FORBIDDEN, "403 Forbidden が返されるべき");
        }
    }
}
