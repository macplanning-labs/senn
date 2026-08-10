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
use tower_governor::GovernorLayer;
use crate::presentation::{
    state::AppState,
    middleware::{auth::require_auth, jwt_auth, rate_limiter},
    handlers::{
        auth, auth_api, dashboard, tickets, tickets_api, gantt, burndown, export,
        milestones, projects, notifications, notification_api2, wiki, categories,
        holidays, api, health, resource_api, cycle_api,
        team_api, membership_api, team_rule_api, workflow_status_api, time_entry_api,
        triage_api, wiki_api, search_api, reports_api, dashboard_api, integration_api,
        external_api, ai_api, ai_agent_api, attachment_api, security_api,
    },
};

pub fn create_router(state: AppState) -> Router {
    // ルートのうち率制限なし
    let basic_public_routes = Router::new()
        .route("/auth/login", get(auth::login_page).post(auth::login_submit))
        .route("/auth/totp", get(auth::totp_page).post(auth::totp_verify))
        .route("/auth/webauthn", get(auth::webauthn_page))
        .route("/health", get(health::check))
        // REST API（APIキー認証 = セッション不要）
        .route("/api/tickets", get(api::list_tickets).post(api::create_ticket))
        .route("/api/v1/auth/token/refresh/", post(auth_api::token_refresh))
        .route("/api/v1/auth/register/", post(auth_api::register));

    // JSON認証API（ログイン）: 20 requests/min per IP
    let login_routes = Router::new()
        .route("/api/v1/auth/login/", post(auth_api::login))
        .route("/api/v1/auth/login/verify/", post(auth_api::login_verify))
        .layer(GovernorLayer::new(rate_limiter::login_config()).error_handler(rate_limiter::error_response));

    // GitHub Webhook: 60 requests/min per IP
    let webhook_routes = Router::new()
        .route("/api/v1/webhooks/github/", post(integration_api::github_webhook))
        .layer(GovernorLayer::new(rate_limiter::webhook_config()).error_handler(rate_limiter::error_response));

    // 外部API: 300 requests/min per IP
    let external_routes = Router::new()
        .route("/api/v1/external/tickets/", post(external_api::create_ticket))
        .route("/api/v1/external/tickets/{ticket_key}/comments/", post(external_api::create_comment))
        .layer(GovernorLayer::new(rate_limiter::external_api_config()).error_handler(rate_limiter::error_response));

    // AI専用外部API: 300 requests/min per IP
    let ai_agent_routes = Router::new()
        .route("/api/v1/ai-agent/projects/", post(ai_agent_api::create_project).get(ai_agent_api::list_projects))
        .route("/api/v1/ai-agent/tickets/{ticket_key}/comments/", post(ai_agent_api::add_comment))
        .route("/api/v1/ai-agent/tickets/{ticket_key}/", axum::routing::patch(ai_agent_api::patch_ticket))
        .route("/api/v1/ai-agent/tickets/", post(ai_agent_api::create_ticket).get(ai_agent_api::list_tickets))
        .route("/api/v1/ai-agent/wiki-pages/", get(ai_agent_api::list_wiki_pages))
        .layer(GovernorLayer::new(rate_limiter::external_api_config()).error_handler(rate_limiter::error_response));

    // 認証不要ルート（すべてのサブルーターを統合）
    let public_routes = basic_public_routes
        .merge(login_routes)
        .merge(webhook_routes)
        .merge(external_routes)
        .merge(ai_agent_routes);

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
        .route("/tickets/{key}", get(tickets::detail))
        .route("/tickets/{id}/edit", get(tickets::edit_page).post(tickets::edit_submit))
        .route("/tickets/{id}/status", post(tickets::update_status))
        .route("/tickets/{id}/delete", post(tickets::delete))
        .route("/tickets/{id}/watch", post(tickets::toggle_watch))
        .route("/tickets/{id}/comment", post(tickets::add_comment))
        .route("/comments/{id}/delete", post(tickets::delete_comment))
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
        .route("/milestones/{id}/edit", post(milestones::edit))
        .route("/milestones/{id}/delete", post(milestones::delete))
        // プロジェクト
        .route("/tickets/projects", get(projects::list))
        .route("/tickets/projects/create", post(projects::create))
        .route("/tickets/projects/{id}/edit", post(projects::edit))
        .route("/tickets/projects/{id}/delete", post(projects::delete))
        .route("/tickets/projects/switch", post(projects::switch))
        // 通知
        .route("/notifications", get(notifications::list))
        .route("/notifications/read/{id}", get(notifications::mark_read))
        .route("/notifications/read-all", post(notifications::mark_all_read))
        .route("/notifications/unread-count", get(notifications::unread_count))
        .route("/notifications/dropdown", get(notifications::dropdown))
        .route("/notifications/settings", post(notifications::update_settings))
        // Wiki
        .route("/wiki/shared", get(wiki::shared_list))
        .route("/wiki/shared/{slug}/export", get(wiki::shared_export))
        .route("/wiki/{project_id}", get(wiki::project_list))
        .route("/wiki/{project_id}/new", get(wiki::create_page).post(wiki::create_submit))
        .route("/wiki/{project_id}/export-all", get(wiki::export_all))
        .route("/wiki/{project_id}/{slug}", get(wiki::detail))
        .route("/wiki/{project_id}/{slug}/edit", get(wiki::edit_page).post(wiki::edit_submit))
        .route("/wiki/{project_id}/{slug}/delete", post(wiki::delete))
        .route("/wiki/{project_id}/{slug}/history", get(wiki::history))
        .route("/wiki/{project_id}/{slug}/revision/{rev_id}", get(wiki::revision))
        .route("/wiki/{project_id}/{slug}/export", get(wiki::export))
        // 管理
        .route("/admin/categories", get(categories::list))
        .route("/admin/categories/create", post(categories::create))
        .route("/admin/categories/{id}/edit", post(categories::edit))
        .route("/admin/categories/{id}/delete", post(categories::delete))
        .route("/tickets/holidays", get(holidays::list))
        .route("/tickets/holidays/bulk-add", post(holidays::bulk_add))
        .route("/tickets/holidays/{id}/delete", post(holidays::delete))
        // 認証ミドルウェア適用
        .layer(axum_middleware::from_fn(require_auth));

    // JWT保護ルート
    let jwt_protected_routes = Router::new()
        .route("/api/v1/auth/me/", get(auth_api::me))
        .route("/api/v1/auth/logout/", post(auth_api::logout))
        .route("/api/v1/users/", get(auth_api::list_users))
        .route("/api/v1/users/{id}/active/", axum::routing::patch(auth_api::set_user_active))
        .route("/api/v1/users/{id}/", axum::routing::patch(auth_api::update_user_profile))
        // JSON チケット API
        .route("/api/v1/tickets/", get(tickets_api::list).post(tickets_api::create))
        .route("/api/v1/tickets/bulk-import/", post(tickets_api::bulk_import))
        .route("/api/v1/tickets/{ticket_key}/", get(tickets_api::detail).put(tickets_api::update).patch(tickets_api::patch).delete(tickets_api::delete))
        .route("/api/v1/tickets/{ticket_key}/comments/", get(tickets_api::list_comments).post(tickets_api::add_comment))
        .route("/api/v1/tickets/{ticket_key}/change-logs/", get(tickets_api::change_logs))
        .route("/api/v1/tickets/{ticket_key}/point-history/", get(tickets_api::point_history))
        .route("/api/v1/tickets/{ticket_key}/dependencies/", get(tickets_api::list_dependencies).post(tickets_api::add_dependency))
        .route("/api/v1/tickets/{ticket_key}/dependencies/{dep_id}/", axum::routing::delete(tickets_api::delete_dependency))
        .route("/api/v1/tickets/{ticket_key}/git-events/", get(tickets_api::git_events))
        .route("/api/v1/tickets/{ticket_key}/attachments/", post(attachment_api::upload_attachment).get(attachment_api::list_attachments))
        .route("/api/v1/tickets/{ticket_key}/attachments/{attachment_id}/", axum::routing::delete(attachment_api::delete_attachment))
        .route("/api/v1/tickets/export/csv/", get(tickets_api::export_csv))
        // JSON 通知 API (Phase 3 第二弾)
        .route("/api/v1/notifications/", get(notification_api2::list))
        .route("/api/v1/notifications/{id}/read/", post(notification_api2::mark_read))
        .route("/api/v1/notifications/read_all/", post(notification_api2::mark_all_read))
        .route("/api/v1/notifications/unread_count/", get(notification_api2::unread_count))
        // JSON サイクル API (Phase 3 第二弾)
        .route("/api/v1/cycles/", get(cycle_api::list).post(cycle_api::create))
        .route("/api/v1/cycles/velocity/", get(cycle_api::velocity))
        .route("/api/v1/cycles/{id}/", get(cycle_api::detail).put(cycle_api::update).delete(cycle_api::delete))
        .route("/api/v1/cycles/{id}/progress/", get(cycle_api::progress))
        .route("/api/v1/cycles/{id}/complete/", post(cycle_api::complete))
        .route("/api/v1/cycles/{id}/burndown/", get(cycle_api::burndown))
        // JSON リソース API (Phase 3)
        .route("/api/v1/projects/", get(resource_api::project_list).post(resource_api::project_create))
        .route("/api/v1/projects/{id}/", get(resource_api::project_detail).put(resource_api::project_update).delete(resource_api::project_delete))
        .route("/api/v1/categories/", get(resource_api::category_list).post(resource_api::category_create))
        .route("/api/v1/categories/{id}/", get(resource_api::category_detail).put(resource_api::category_update).delete(resource_api::category_delete))
        .route("/api/v1/milestones/", get(resource_api::milestone_list).post(resource_api::milestone_create))
        .route("/api/v1/milestones/{id}/", get(resource_api::milestone_detail).put(resource_api::milestone_update).delete(resource_api::milestone_delete))
        .route("/api/v1/labels/", get(resource_api::label_list).post(resource_api::label_create))
        .route("/api/v1/labels/{id}/", get(resource_api::label_detail).put(resource_api::label_update).delete(resource_api::label_delete))

        .route("/api/v1/teams/", get(team_api::team_list).post(team_api::team_create))
        .route("/api/v1/teams/{id}/", get(team_api::team_detail).put(team_api::team_update).delete(team_api::team_delete))
        .route("/api/v1/teams/{id}/members/", get(team_api::team_members_list).post(team_api::team_members_add))
        .route("/api/v1/teams/{team_id}/members/{user_id}/", axum::routing::delete(team_api::team_members_remove))

        .route("/api/v1/memberships/", get(membership_api::membership_list).post(membership_api::membership_create))
        .route("/api/v1/memberships/{id}/", get(membership_api::membership_detail).patch(membership_api::membership_update).delete(membership_api::membership_delete))

        .route("/api/v1/team-rules/", get(team_rule_api::team_rule_list).post(team_rule_api::team_rule_create))
        .route("/api/v1/team-rules/{id}/", get(team_rule_api::team_rule_detail).put(team_rule_api::team_rule_update).patch(team_rule_api::team_rule_update).delete(team_rule_api::team_rule_delete))

        .route("/api/v1/workflow-statuses/", get(workflow_status_api::workflow_status_list).post(workflow_status_api::workflow_status_create))
        .route("/api/v1/workflow-statuses/reorder/", post(workflow_status_api::workflow_status_reorder))
        .route("/api/v1/workflow-statuses/{id}/", get(workflow_status_api::workflow_status_detail).put(workflow_status_api::workflow_status_update).patch(workflow_status_api::workflow_status_partial_update).delete(workflow_status_api::workflow_status_delete))

        .route("/api/v1/time-entries/", get(time_entry_api::time_entry_list).post(time_entry_api::time_entry_create))
        .route("/api/v1/time-entries/my-today/", get(time_entry_api::time_entry_my_today))
        .route("/api/v1/time-entries/{id}/", axum::routing::delete(time_entry_api::time_entry_delete))

        .route("/api/v1/triage-requests/", get(triage_api::list).post(triage_api::create))
        .route("/api/v1/triage-requests/{id}/", get(triage_api::detail).put(triage_api::update).patch(triage_api::update).delete(triage_api::delete))
        .route("/api/v1/triage-requests/{id}/approve/", post(triage_api::approve))
        .route("/api/v1/triage-requests/{id}/reject/", post(triage_api::reject))

        .route("/api/v1/wiki/", get(wiki_api::list).post(wiki_api::create))
        .route("/api/v1/wiki/{id}/", get(wiki_api::detail).put(wiki_api::update).patch(wiki_api::update).delete(wiki_api::delete))
        .route("/api/v1/wiki/{id}/revisions/", get(wiki_api::revisions))
        .route("/api/v1/wiki/{id}/link-ticket/", post(wiki_api::link_ticket))
        .route("/api/v1/wiki/{id}/unlink-ticket/", post(wiki_api::unlink_ticket))

        .route("/api/v1/search/", get(search_api::search))
        .route("/api/v1/reports/workload/", get(reports_api::workload))

        .route("/api/v1/dashboard/stats/", get(dashboard_api::stats))
        .route("/api/v1/dashboard/my-tickets/", get(dashboard_api::my_tickets))
        .route("/api/v1/dashboard/activity/", get(dashboard_api::activity))
        .route("/api/v1/dashboard/default/", get(dashboard_api::default_dashboard))
        .route("/api/v1/dashboard/list/", get(dashboard_api::list_dashboards))
        .route("/api/v1/dashboard/detail/{id}/", get(dashboard_api::detail_dashboard))
        .route("/api/v1/dashboard/create/", post(dashboard_api::create_dashboard))
        .route("/api/v1/dashboard/update/", axum::routing::patch(dashboard_api::update_dashboard))
        .route("/api/v1/dashboard/delete/", axum::routing::delete(dashboard_api::delete_dashboard))
        .route("/api/v1/dashboard/add-widget/", post(dashboard_api::add_widget))
        .route("/api/v1/dashboard/remove-widget/", axum::routing::delete(dashboard_api::remove_widget))
        .route("/api/v1/dashboard/reorder-widgets/", post(dashboard_api::reorder_widgets))

        .route("/api/v1/integrations/", get(integration_api::list).post(integration_api::create))
        .route("/api/v1/integrations/{id}/", axum::routing::patch(integration_api::update).delete(integration_api::delete))

        // セキュリティ設定（TOTP）
        .route("/api/v1/settings/security/totp/begin/", post(security_api::totp_begin))
        .route("/api/v1/settings/security/totp/confirm/", post(security_api::totp_confirm))
        .route("/api/v1/settings/security/totp/disable/", post(security_api::totp_disable))
        // セキュリティ設定（パスキー/WebAuthn）
        .route("/api/v1/settings/security/passkey/register/begin/", post(security_api::passkey_register_begin))
        .route("/api/v1/settings/security/passkey/register/complete/", post(security_api::passkey_register_complete))
        .route("/api/v1/settings/security/passkeys/", get(security_api::list_passkeys))
        .route("/api/v1/settings/security/passkey/{id}/delete/", post(security_api::delete_passkey))

        .route("/api/v1/ai/suggest-points/", post(ai_api::suggest_points))
        .route("/api/v1/ai/sprint-health/", post(ai_api::sprint_health))
        .route("/api/v1/ai/context-analysis/", post(ai_api::context_analysis))
        .route("/api/v1/ai/close-analysis/", post(ai_api::close_analysis))
        .route("/api/v1/ai/status/", get(ai_api::ai_status))
        .layer(axum_middleware::from_fn_with_state(state.clone(), jwt_auth::jwt_auth));

    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .merge(jwt_protected_routes)
        // 静的ファイル
        .nest_service("/static", ServeDir::new("static"))
        .nest_service("/media", ServeDir::new("media"))
        // 共有ステート
        .with_state(state)
}
