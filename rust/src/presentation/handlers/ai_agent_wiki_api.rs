/// presentation/handlers/ai_agent_wiki_api.rs — AI専用API: Wikiページの読み書き
///
/// 認証は ai_agent_api::authenticate_ai(X-AI-Api-Key)。
/// 読み取りは共有キーでも可。書き込み(作成・更新・削除)は「人ごとのキー」のみ許可し、
/// その本人の権限で画面と同じ書き込み規則(wiki_api::authorize_*)を適用する。
/// 共有キー(acting_user_id=None)では誰が書いたか分からなくなるため拒否する。
///
/// project_prefix を省略すると、全体 Wiki(どのプロジェクト・チームにも属さないページ)が対象になる。
/// 全体 Wiki の判定は画面と同じ(閲覧・書き込みは facts の Global、削除は作成者かシステム管理者)。
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::domain::access::{Action, Viewer};
use crate::domain::models::wiki_api::{WikiPageCreateIn, WikiPageUpdateIn};
use crate::infrastructure::access::facts_repo;
use crate::infrastructure::repositories::{ticket_repo, wiki_api_repo};
use crate::presentation::handlers::ai_agent_api::{
    ai_project_gate, ai_wiki_gate, project_not_found, AiCaller,
};
use crate::presentation::handlers::wiki_api::{page_gate, scope_write_gate};
use crate::presentation::state::AppState;

fn err(status: StatusCode, msg: impl Into<String>) -> Response {
    (status, Json(json!({"error": msg.into()}))).into_response()
}

fn server_error<E: std::fmt::Debug>(e: E) -> Response {
    tracing::error!("DB operation failed: {:?}", e);
    err(
        StatusCode::INTERNAL_SERVER_ERROR,
        "サーバーエラーが発生しました",
    )
}

/// 書き込みに使う本人(人ごとのキーの持ち主)とその閲覧者を返す。共有キーは書き込み不可。
#[allow(clippy::result_large_err)]
fn writer(caller: &AiCaller) -> Result<(i32, &Viewer), Response> {
    match (caller.legacy.acting_user_id, caller.viewer.as_ref()) {
        (Some(user_id), Some(viewer)) => Ok((user_id, viewer)),
        _ => Err(err(
            StatusCode::FORBIDDEN,
            "Wikiの書き込みには、人ごとのAI用APIキーが必要です(全社共有キーでは書き込めません)",
        )),
    }
}

/// 操作の対象。project_prefix があればそのプロジェクトの Wiki、無ければ全体 Wiki。
#[derive(Clone, Copy)]
enum Target {
    Project(i32),
    Global,
}

impl Target {
    fn project_id(self) -> Option<i32> {
        match self {
            Target::Project(id) => Some(id),
            Target::Global => None,
        }
    }
}

/// project_prefix から対象を解決し、閲覧できるかを判定する。
/// 空文字は省略と同じ扱い(全体 Wiki)。
async fn resolve_target(
    state: &AppState,
    caller: &AiCaller,
    prefix: &Option<String>,
    route: &'static str,
) -> Result<Target, Response> {
    let prefix = match prefix {
        Some(p) if !p.is_empty() => p,
        _ => {
            let facts = facts_repo::facts_for_wiki_scope(&state.pool, None, None, None)
                .await
                .map_err(server_error)?;
            caller.gate(&state.pool, facts.as_ref(), Action::Read, 0, route)?;
            return Ok(Target::Global);
        }
    };
    ai_project_gate(state, caller, prefix, Action::Read, route).await?;
    match ticket_repo::resolve_project_id_by_prefix(&state.pool, prefix).await {
        Ok(Some(id)) => Ok(Target::Project(id)),
        Ok(None) => Err(project_not_found(state, prefix).await),
        Err(e) => Err(server_error(e)),
    }
}

/// project_prefix(省略時は全体 Wiki)+ slug からページIDを引き、ページへの読み取りを判定する。
async fn resolve_page(
    state: &AppState,
    caller: &AiCaller,
    prefix: &Option<String>,
    slug: &str,
    route: &'static str,
) -> Result<i32, Response> {
    let found = match resolve_target(state, caller, prefix, route).await? {
        Target::Project(id) => wiki_api_repo::find_id_by_project_slug(&state.pool, id, slug).await,
        Target::Global => wiki_api_repo::find_id_by_global_slug(&state.pool, slug).await,
    };
    match found {
        Ok(Some(id)) => {
            ai_wiki_gate(state, caller, id, Action::Read, route).await?;
            Ok(id)
        }
        Ok(None) => Err(err(
            StatusCode::NOT_FOUND,
            format!("Wikiページ '{}' が見つかりません", slug),
        )),
        Err(e) => Err(server_error(e)),
    }
}

#[derive(Deserialize)]
pub struct PageQuery {
    pub project_prefix: Option<String>,
}

#[derive(Deserialize)]
pub struct SearchQuery {
    pub project_prefix: Option<String>,
    pub category: Option<String>,
    pub search: Option<String>,
    pub page: Option<i64>,
}

/// GET /api/v1/ai-agent/wiki-search/ — 本文を含まない軽量な検索・一覧
pub async fn search(
    State(state): State<AppState>,
    caller: AiCaller,
    Query(q): Query<SearchQuery>,
) -> Response {
    const ROUTE: &str = "GET /api/v1/ai-agent/wiki-search/";
    let target = match resolve_target(&state, &caller, &q.project_prefix, ROUTE).await {
        Ok(t) => t,
        Err(r) => return r,
    };
    let page = q.page.unwrap_or(1).max(1);
    let search = q.search.as_deref().filter(|s| !s.is_empty());
    let category = q.category.as_deref().filter(|s| !s.is_empty());
    // 新しい判定(on)の個人キーは、持ち主の見える範囲に SQL で絞る
    let scope = caller.scope_if_on();
    let (count, rows) = match target {
        Target::Project(project_id) => (
            wiki_api_repo::count_all(
                &state.pool,
                Some(project_id),
                None,
                category,
                search,
                scope.as_ref(),
            )
            .await,
            wiki_api_repo::find_all(
                &state.pool,
                Some(project_id),
                None,
                category,
                search,
                page,
                scope.as_ref(),
            )
            .await,
        ),
        Target::Global => (
            wiki_api_repo::count_all_global(&state.pool, category, search, scope.as_ref()).await,
            wiki_api_repo::find_all_global(&state.pool, category, search, page, scope.as_ref())
                .await,
        ),
    };
    match (count, rows) {
        (Ok(count), Ok(results)) => (
            StatusCode::OK,
            Json(json!({"count": count, "page": page, "results": results})),
        )
            .into_response(),
        (Err(e), _) => server_error(e),
        (_, Err(e)) => server_error(e),
    }
}

/// GET /api/v1/ai-agent/wiki-pages/{slug}/?project_prefix=(省略時は全体 Wiki)
pub async fn get_page(
    State(state): State<AppState>,
    caller: AiCaller,
    Path(slug): Path<String>,
    Query(q): Query<PageQuery>,
) -> Response {
    let id = match resolve_page(
        &state,
        &caller,
        &q.project_prefix,
        &slug,
        "GET /api/v1/ai-agent/wiki-pages/{slug}/",
    )
    .await
    {
        Ok(v) => v,
        Err(r) => return r,
    };
    match wiki_api_repo::find_by_id(&state.pool, id).await {
        Ok(Some(item)) => (StatusCode::OK, Json(item)).into_response(),
        Ok(None) => err(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => server_error(e),
    }
}

/// GET /api/v1/ai-agent/wiki-pages/{slug}/revisions/?project_prefix=(省略時は全体 Wiki)
pub async fn revisions(
    State(state): State<AppState>,
    caller: AiCaller,
    Path(slug): Path<String>,
    Query(q): Query<PageQuery>,
) -> Response {
    let id = match resolve_page(
        &state,
        &caller,
        &q.project_prefix,
        &slug,
        "GET /api/v1/ai-agent/wiki-pages/{slug}/revisions/",
    )
    .await
    {
        Ok(v) => v,
        Err(r) => return r,
    };
    match wiki_api_repo::find_revisions(&state.pool, id).await {
        Ok(items) => (StatusCode::OK, Json(items)).into_response(),
        Err(e) => server_error(e),
    }
}

#[derive(Deserialize)]
pub struct CreateBody {
    pub project_prefix: Option<String>,
    pub title: String,
    pub category: Option<String>,
    #[serde(default)]
    pub content: String,
}

/// POST /api/v1/ai-agent/wiki-pages/
pub async fn create(
    State(state): State<AppState>,
    caller: AiCaller,
    Json(body): Json<CreateBody>,
) -> Response {
    const ROUTE: &str = "POST /api/v1/ai-agent/wiki-pages/";
    let (user_id, viewer) = match writer(&caller) {
        Ok(v) => v,
        Err(r) => return r,
    };
    if body.title.trim().is_empty() {
        return err(StatusCode::BAD_REQUEST, "title は必須です");
    }
    let project_id = match resolve_target(&state, &caller, &body.project_prefix, ROUTE).await {
        Ok(t) => t.project_id(),
        Err(r) => return r,
    };
    if let Err(r) = scope_write_gate(&state, viewer, user_id, project_id, None, ROUTE).await {
        return r;
    }
    let input = WikiPageCreateIn {
        title: body.title,
        category: body.category.unwrap_or_else(|| "other".to_string()),
        project: project_id,
        team: None,
        content: body.content,
    };
    match wiki_api_repo::create(&state.pool, &input, user_id).await {
        Ok(id) => match wiki_api_repo::find_by_id(&state.pool, id).await {
            Ok(Some(item)) => (StatusCode::CREATED, Json(item)).into_response(),
            Ok(None) => err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "サーバーエラーが発生しました",
            ),
            Err(e) => server_error(e),
        },
        Err(e) => server_error(e),
    }
}

#[derive(Deserialize)]
pub struct UpdateBody {
    pub title: Option<String>,
    pub category: Option<String>,
    pub content: Option<String>,
    #[serde(default)]
    pub revision_comment: String,
}

/// PATCH /api/v1/ai-agent/wiki-pages/{slug}/?project_prefix=(省略時は全体 Wiki)
/// 所属(project/team)の移動はAI経由では行わない。
pub async fn update(
    State(state): State<AppState>,
    caller: AiCaller,
    Path(slug): Path<String>,
    Query(q): Query<PageQuery>,
    Json(body): Json<UpdateBody>,
) -> Response {
    const ROUTE: &str = "PATCH /api/v1/ai-agent/wiki-pages/{slug}/";
    let (user_id, viewer) = match writer(&caller) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let id = match resolve_page(&state, &caller, &q.project_prefix, &slug, ROUTE).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    if let Err(r) = page_gate(&state, viewer, user_id, id, Action::Write, ROUTE).await {
        return r;
    }
    let input = WikiPageUpdateIn {
        title: body.title,
        category: body.category,
        project: None,
        team: None,
        content: body.content,
        revision_comment: body.revision_comment,
    };
    match wiki_api_repo::update(&state.pool, id, &input, user_id).await {
        Ok(true) => match wiki_api_repo::find_by_id(&state.pool, id).await {
            Ok(Some(item)) => (StatusCode::OK, Json(item)).into_response(),
            Ok(None) => err(StatusCode::NOT_FOUND, "見つかりません"),
            Err(e) => server_error(e),
        },
        Ok(false) => err(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => server_error(e),
    }
}

/// DELETE /api/v1/ai-agent/wiki-pages/{slug}/?project_prefix=(省略時は全体 Wiki)
pub async fn delete(
    State(state): State<AppState>,
    caller: AiCaller,
    Path(slug): Path<String>,
    Query(q): Query<PageQuery>,
) -> Response {
    const ROUTE: &str = "DELETE /api/v1/ai-agent/wiki-pages/{slug}/";
    let (user_id, viewer) = match writer(&caller) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let id = match resolve_page(&state, &caller, &q.project_prefix, &slug, ROUTE).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    if let Err(r) = page_gate(&state, viewer, user_id, id, Action::Delete, ROUTE).await {
        return r;
    }
    match wiki_api_repo::delete(&state.pool, id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => err(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => server_error(e),
    }
}

#[cfg(test)]
mod http_tests {
    //! ルーターを通した HTTP テスト(全体 Wiki = project_prefix 省略時)。
    use crate::infrastructure::repositories::ai_agent_key_repo;
    use crate::test_support::{create_test_project, create_test_user, test_config, test_pool};
    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use serde_json::{json, Value};
    use sqlx::PgPool;
    use tower::ServiceExt;
    use uuid::Uuid;

    const SHARED_KEY: &str = "test-ai-wiki-shared-key-0123456789";

    async fn app(pool: &PgPool) -> axum::Router {
        let mut cfg = test_config();
        cfg.wip_ai_api_key = Some(SHARED_KEY.to_string());
        let state = crate::presentation::state::AppState::new(pool.clone(), cfg, None)
            .await
            .expect("AppState");
        crate::routes::create_router(state)
    }

    /// 人ごとのキーを発行して、平文を返す
    async fn personal_key(pool: &PgPool, user_id: i32) -> String {
        let plain = ai_agent_key_repo::generate_plain_key();
        ai_agent_key_repo::create(
            pool,
            user_id,
            &ai_agent_key_repo::hash_key(&plain),
            &ai_agent_key_repo::display_prefix(&plain),
            Some("test"),
            user_id,
        )
        .await
        .unwrap();
        plain
    }

    async fn call(
        app: &axum::Router,
        method: &str,
        uri: &str,
        key: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let req = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .header("X-AI-Api-Key", key);
        let mut req = req
            .body(body.map_or_else(Body::empty, |b| Body::from(b.to_string())))
            .unwrap();
        // レート制限の層が接続元 IP を見るので、実サーバーと同じく ConnectInfo を渡す
        req.extensions_mut().insert(axum::extract::ConnectInfo(
            "127.0.0.1:40000".parse::<std::net::SocketAddr>().unwrap(),
        ));
        let resp = app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    #[tokio::test]
    async fn global_wiki_create_get_search_delete_without_project_prefix() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let app = app(&pool).await;
        let author = create_test_user(&pool, "gw-author").await;
        let other = create_test_user(&pool, "gw-other").await;
        let author_key = personal_key(&pool, author).await;
        let other_key = personal_key(&pool, other).await;
        let title = format!("gw-{}", Uuid::new_v4().simple());

        // 作成: project_prefix なし → 所属なし(全体 Wiki)
        let (s, page) = call(
            &app,
            "POST",
            "/api/v1/ai-agent/wiki-pages/",
            &author_key,
            Some(json!({"title": title, "category": "manual", "content": "# 本文"})),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED, "{page}");
        assert_eq!(page["project"], Value::Null);
        assert_eq!(page["team"], Value::Null);
        assert_eq!(page["category"], "manual");
        let id = page["id"].as_i64().unwrap();
        let slug = page["slug"].as_str().unwrap().to_string();

        // 同じ slug のプロジェクトのページがあっても、全体 Wiki のページを引く
        let project = create_test_project(&pool, "GWP", author).await;
        sqlx::query(
            "INSERT INTO wiki_page (project_id, team_id, title, slug, category, content, author_id, last_editor_id, created_at, updated_at)
             VALUES ($1, NULL, $2, $3, 'other', '', $4, $4, NOW(), NOW())",
        )
        .bind(project as i64)
        .bind(&title)
        .bind(&slug)
        .bind(author as i64)
        .execute(&pool)
        .await
        .unwrap();

        // 取得
        let uri = format!("/api/v1/ai-agent/wiki-pages/{slug}/");
        let (s, got) = call(&app, "GET", &uri, &author_key, None).await;
        assert_eq!(s, StatusCode::OK, "{got}");
        assert_eq!(got["id"].as_i64(), Some(id));
        assert_eq!(got["content"], "# 本文");

        // 検索: 全体 Wiki のページだけ(プロジェクトの同名ページは入らない)
        let (s, found) = call(
            &app,
            "GET",
            &format!("/api/v1/ai-agent/wiki-search/?search={title}"),
            &author_key,
            None,
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{found}");
        assert_eq!(found["count"], 1);
        assert_eq!(found["results"][0]["id"].as_i64(), Some(id));

        // プロジェクトにしか無い slug は、全体 Wiki としては見つからない
        let (s, _) = call(
            &app,
            "GET",
            "/api/v1/ai-agent/wiki-pages/no-such-global-page-xyz/",
            &author_key,
            None,
        )
        .await;
        assert_eq!(s, StatusCode::NOT_FOUND);

        // 共有キーでは書き込めない
        let (s, _) = call(
            &app,
            "POST",
            "/api/v1/ai-agent/wiki-pages/",
            SHARED_KEY,
            Some(json!({"title": format!("{title}-shared")})),
        )
        .await;
        assert_eq!(s, StatusCode::FORBIDDEN);

        // 削除: 作成者でもシステム管理者でもない人は不可、作成者は可
        let (s, _) = call(&app, "DELETE", &uri, &other_key, None).await;
        assert_eq!(s, StatusCode::FORBIDDEN);
        let (s, _) = call(&app, "DELETE", &uri, &author_key, None).await;
        assert_eq!(s, StatusCode::NO_CONTENT);
        let (s, _) = call(&app, "GET", &uri, &author_key, None).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }
}
