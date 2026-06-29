/// routes.rs — ルーター定義
///
/// アプリ処理方式設計書 §3 に基づく全ルート集約。
/// 認証必須ルートと公開ルートを分離。

use axum::{
    routing::{get, post},
    middleware as axum_middleware,
    Router,
};
use tower_http::services::ServeDir;
use crate::presentation::{
    state::AppState,
    middleware::auth::require_auth,
    handlers::{
        auth, dashboard, tickets, gantt, burndown, export,
        milestones, projects, notifications, wiki, categories,
        holidays, api, health,
    },
};

pub fn create_router(state: AppState) -> Router {
    // 認証不要ルート
    let public_routes = Router::new()
        .route("/auth/login", get(auth::login_page).post(auth::login_submit))
        .route("/auth/totp", get(auth::totp_page).post(auth::totp_verify))
        .route("/auth/webauthn", get(auth::webauthn_page))
        .route("/health", get(health::check))
        // REST API（APIキー認証 = セッション不要）
        .route("/api/tickets", get(api::list_tickets).post(api::create_ticket));

    // 認証必須ルート
    let protected_routes = Router::new()
        // ダッシュボード
        .route("/", get(dashboard::index))
        .route("/dashboard/my-tickets", get(dashboard::my_tickets))
        // 認証
        .route("/auth/logout", post(auth::logout))
        .route("/auth/password", get(auth::password_page).post(auth::password_change))
        .route("/auth/mfa", get(auth::mfa_page))
        // チケット
        .route("/tickets", get(tickets::list))
        .route("/tickets/create", get(tickets::create_page).post(tickets::create_submit))
        .route("/tickets/export", get(export::ticket_excel))
        .route("/tickets/:key", get(tickets::detail))
        .route("/tickets/:id/edit", get(tickets::edit_page).post(tickets::edit_submit))
        .route("/tickets/:id/status", post(tickets::update_status))
        .route("/tickets/:id/delete", post(tickets::delete))
        .route("/tickets/:id/watch", post(tickets::toggle_watch))
        .route("/tickets/:id/comment", post(tickets::add_comment))
        .route("/comments/:id/delete", post(tickets::delete_comment))
        // ガントチャート
        .route("/tickets/gantt", get(gantt::page))
        .route("/tickets/gantt/reorder", post(gantt::reorder))
        .route("/tickets/gantt/update-dates", post(gantt::update_dates))
        .route("/tickets/gantt/export", get(export::gantt_excel))
        // バーンダウン
        .route("/tickets/burndown", get(burndown::page))
        // マイルストーン
        .route("/milestones", get(milestones::list))
        .route("/milestones/create", post(milestones::create))
        .route("/milestones/:id/edit", post(milestones::edit))
        .route("/milestones/:id/delete", post(milestones::delete))
        // プロジェクト
        .route("/tickets/projects", get(projects::list))
        .route("/tickets/projects/create", post(projects::create))
        .route("/tickets/projects/:id/edit", post(projects::edit))
        .route("/tickets/projects/:id/delete", post(projects::delete))
        .route("/tickets/projects/switch", post(projects::switch))
        // 通知
        .route("/notifications", get(notifications::list))
        .route("/notifications/read/:id", get(notifications::mark_read))
        .route("/notifications/read-all", post(notifications::mark_all_read))
        .route("/notifications/unread-count", get(notifications::unread_count))
        .route("/notifications/dropdown", get(notifications::dropdown))
        .route("/notifications/settings", post(notifications::update_settings))
        // Wiki
        .route("/wiki/shared", get(wiki::shared_list))
        .route("/wiki/shared/:slug/export", get(wiki::shared_export))
        .route("/wiki/:project_id", get(wiki::project_list))
        .route("/wiki/:project_id/new", get(wiki::create_page).post(wiki::create_submit))
        .route("/wiki/:project_id/export-all", get(wiki::export_all))
        .route("/wiki/:project_id/:slug", get(wiki::detail))
        .route("/wiki/:project_id/:slug/edit", get(wiki::edit_page).post(wiki::edit_submit))
        .route("/wiki/:project_id/:slug/delete", post(wiki::delete))
        .route("/wiki/:project_id/:slug/history", get(wiki::history))
        .route("/wiki/:project_id/:slug/revision/:rev_id", get(wiki::revision))
        .route("/wiki/:project_id/:slug/export", get(wiki::export))
        // 管理
        .route("/admin/categories", get(categories::list))
        .route("/admin/categories/create", post(categories::create))
        .route("/admin/categories/:id/edit", post(categories::edit))
        .route("/admin/categories/:id/delete", post(categories::delete))
        .route("/tickets/holidays", get(holidays::list))
        .route("/tickets/holidays/bulk-add", post(holidays::bulk_add))
        .route("/tickets/holidays/:id/delete", post(holidays::delete))
        // 認証ミドルウェア適用
        .layer(axum_middleware::from_fn(require_auth));

    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        // 静的ファイル
        .nest_service("/static", ServeDir::new("static"))
        .nest_service("/media", ServeDir::new("media"))
        // 共有ステート
        .with_state(state)
}
