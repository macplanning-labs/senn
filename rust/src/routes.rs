/// routes.rs — ルーター定義
///
/// アプリ処理方式設計書 §3 に基づく全ルート集約。
/// 認証必須ルートと公開ルートを分離。

use axum::{
    routing::{get, post},
    middleware as axum_middleware,
    Router,
};
use std::sync::Arc;
use tower_http::services::ServeDir;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;
// Step 2: レート制限(IPトークンバケット)は auth-core::infrastructure::rate_limit へ移行済み。
use auth_core::infrastructure::rate_limit::{ip_rate_limit_layer, IpRateLimitConfig};
use auth_core::presentation::middleware::origin_check_middleware;
use crate::presentation::{
    state::AppState,
    middleware::{auth::require_auth, jwt_auth},
    handlers::{
        auth, auth_api, password_reset_api, tickets_api,
        notification_api2,
        health, resource_api, cycle_api,
        team_api, team_rule_api, workflow_status_api, time_entry_api,
        triage_api, wiki_api, search_api, reports_api, dashboard_api, integration_api,
        external_api, ai_api, ai_agent_api, attachment_api, ticket_link_api, security_api, settings_api, api_doc,
        saved_view_api,
    },
};

pub fn create_router(state: AppState) -> Router {
    let allowed_origins = Arc::new(vec![state.config.base_url.clone()]);

    // Origin/Referer検証対象：Cookie session方式のログインルート（/auth/login, /auth/totp）
    let login_cookie_routes = Router::new()
        .route("/auth/login", get(auth::login_page).post(auth::login_submit))
        .route("/auth/totp", get(auth::totp_page).post(auth::totp_verify))
        .route("/auth/webauthn/login/begin", post(security_api::passkey_login_begin))
        .route("/auth/webauthn/login/complete", post(auth::webauthn_login_complete))
        .layer(axum_middleware::from_fn(origin_check_middleware(allowed_origins.clone())));

    // ルートのうち率制限なし（/auth/login, /auth/totp 除く）
    let basic_public_routes = Router::new()
        .route("/auth/webauthn", get(auth::webauthn_page))
        .route("/health", get(health::check))
        // REST API（APIキー認証 = セッション不要）
        .route("/api/v1/auth/token/refresh/", post(auth_api::token_refresh))
        .route("/api/v1/auth/register/", post(auth_api::register))
        .merge(login_cookie_routes);

    // JSON認証API（ログイン）: 20 requests/min per IP
    let login_routes = Router::new()
        .route("/api/v1/auth/login/", post(auth_api::login))
        .route("/api/v1/auth/login/verify/", post(auth_api::login_verify))
        .route("/api/v1/auth/passkey/login/begin/", post(security_api::passkey_login_begin))
        .route("/api/v1/auth/passkey/login/complete/", post(security_api::passkey_login_complete))
        .route(
            "/api/v1/auth/password-reset/request/",
            post(password_reset_api::password_reset_request),
        )
        .route(
            "/api/v1/auth/password-reset/confirm/",
            post(password_reset_api::password_reset_confirm),
        )
        .layer(ip_rate_limit_layer(IpRateLimitConfig::login_preset()));

    // GitHub Webhook: 60 requests/min per IP
    let webhook_routes = Router::new()
        .route("/api/v1/webhooks/github/", post(integration_api::github_webhook))
        .layer(ip_rate_limit_layer(IpRateLimitConfig::webhook_preset()));

    // 外部API: 300 requests/min per IP
    let external_routes = Router::new()
        .route("/api/v1/external/tickets/", post(external_api::create_ticket))
        .route("/api/v1/external/tickets/{ticket_key}/comments/", post(external_api::create_comment))
        .layer(ip_rate_limit_layer(IpRateLimitConfig::external_api_preset()));

    // AI専用外部API: 300 requests/min per IP
    let ai_agent_routes = Router::new()
        .route("/api/v1/ai-agent/projects/", post(ai_agent_api::create_project).get(ai_agent_api::list_projects))
        .route("/api/v1/ai-agent/tickets/{ticket_key}/comments/", post(ai_agent_api::add_comment))
        .route("/api/v1/ai-agent/tickets/{ticket_key}/", get(ai_agent_api::get_ticket).patch(ai_agent_api::patch_ticket).delete(ai_agent_api::delete_ticket))
        .route("/api/v1/ai-agent/tickets/", post(ai_agent_api::create_ticket).get(ai_agent_api::list_tickets))
        .route("/api/v1/ai-agent/tickets/{ticket_key}/dependencies/", get(ai_agent_api::list_ticket_dependencies).post(ai_agent_api::add_dependency))
        .route("/api/v1/ai-agent/tickets/{ticket_key}/dependencies/{dep_id}/", axum::routing::delete(ai_agent_api::delete_dependency))
        .route("/api/v1/ai-agent/tickets/{ticket_key}/links/", get(ai_agent_api::list_ticket_links).post(ai_agent_api::add_ticket_link))
        .route("/api/v1/ai-agent/tickets/{ticket_key}/links/{link_id}/", axum::routing::delete(ai_agent_api::delete_ticket_link))
        .route("/api/v1/ai-agent/wiki-pages/", get(ai_agent_api::list_wiki_pages))
        .route("/api/v1/ai-agent/wiki-pages/{slug}/attachments/", post(ai_agent_api::upload_wiki_attachment).get(ai_agent_api::list_wiki_attachments))
        .route("/api/v1/ai-agent/wiki-pages/{slug}/attachments/{attachment_id}/", axum::routing::put(ai_agent_api::replace_wiki_attachment).delete(ai_agent_api::delete_wiki_attachment))
        .route("/api/v1/ai-agent/projects/{project_prefix}/dependencies/", get(ai_agent_api::get_project_dependency_graph))
        .route("/api/v1/ai-agent/cycles/", post(ai_agent_api::create_cycle).get(ai_agent_api::list_cycles))
        .route("/api/v1/ai-agent/cycles/{id}/", get(ai_agent_api::get_cycle).patch(ai_agent_api::patch_cycle).delete(ai_agent_api::delete_cycle))
        .route("/api/v1/ai-agent/teams/", post(ai_agent_api::create_team).get(ai_agent_api::list_teams))
        .layer(ip_rate_limit_layer(IpRateLimitConfig::external_api_preset()));

    // 認証不要ルート（すべてのサブルーターを統合）
    let public_routes = basic_public_routes
        .merge(login_routes)
        .merge(webhook_routes)
        .merge(external_routes)
        .merge(ai_agent_routes);

    // 認証必須ルート
    let protected_routes = Router::new()
        // 認証
        .route("/auth/logout", post(auth::logout))
        .route("/auth/password", get(auth::password_page).post(auth::password_change))
        .route("/auth/mfa", get(auth::mfa_page))
        // 認証ミドルウェア適用
        .layer(axum_middleware::from_fn_with_state(state.clone(), require_auth))
        .layer(axum_middleware::from_fn(origin_check_middleware(allowed_origins.clone())));

    // JWT保護ルート
    let jwt_protected_routes = Router::new()
        .route("/api/v1/auth/me/", get(auth_api::me).patch(auth_api::update_my_profile))
        .route("/api/v1/auth/me/deactivate/", post(auth_api::deactivate_my_account))
        .route("/api/v1/auth/logout/", post(auth_api::logout))
        .route("/api/v1/users/", get(auth_api::list_users))
        .route("/api/v1/users/{id}/active/", axum::routing::patch(auth_api::set_user_active))
        .route("/api/v1/users/{id}/", axum::routing::patch(auth_api::update_user_profile))
        // JSON チケット API
        .route("/api/v1/tickets/", get(tickets_api::list).post(tickets_api::create))
        .route("/api/v1/tickets/bulk-import/", post(tickets_api::bulk_import))
        .route("/api/v1/tickets/bulk-delete/", post(tickets_api::bulk_delete))
        .route("/api/v1/tickets/{ticket_key}/", get(tickets_api::detail).put(tickets_api::update).patch(tickets_api::patch).delete(tickets_api::delete))
        .route("/api/v1/tickets/{ticket_key}/comments/", get(tickets_api::list_comments).post(tickets_api::add_comment))
        .route("/api/v1/tickets/{ticket_key}/comments/{comment_id}/", axum::routing::patch(tickets_api::update_comment).delete(tickets_api::delete_comment))
        .route("/api/v1/tickets/{ticket_key}/watch/", axum::routing::post(tickets_api::watch_ticket).delete(tickets_api::unwatch_ticket))
        .route("/api/v1/tickets/{ticket_key}/change-logs/", get(tickets_api::change_logs))
        .route("/api/v1/tickets/{ticket_key}/point-history/", get(tickets_api::point_history))
        .route("/api/v1/tickets/{ticket_key}/dependencies/", get(tickets_api::list_dependencies).post(tickets_api::add_dependency))
        .route("/api/v1/tickets/{ticket_key}/dependencies/{dep_id}/", axum::routing::delete(tickets_api::delete_dependency))
        .route("/api/v1/tickets/{ticket_key}/git-events/", get(tickets_api::git_events))
        .route("/api/v1/tickets/{ticket_key}/attachments/", post(attachment_api::upload_attachment).get(attachment_api::list_attachments))
        .route("/api/v1/tickets/{ticket_key}/attachments/{attachment_id}/", axum::routing::delete(attachment_api::delete_attachment))
        .route("/api/v1/tickets/{ticket_key}/links/", get(ticket_link_api::list_links).post(ticket_link_api::add_link))
        .route("/api/v1/tickets/{ticket_key}/links/{link_id}/", axum::routing::delete(ticket_link_api::delete_link))
        .route("/api/v1/tickets/export/csv/", get(tickets_api::export_csv))
        // JSON 通知 API (Phase 3 第二弾)
        .route("/api/v1/notifications/", get(notification_api2::list))
        .route("/api/v1/notifications/{id}/read/", post(notification_api2::mark_read))
        .route("/api/v1/notifications/read_all/", post(notification_api2::mark_all_read))
        .route("/api/v1/notifications/unread_count/", get(notification_api2::unread_count))
        // JSON サイクル API (Phase 3 第二弾)
        .route("/api/v1/cycles/", get(cycle_api::list).post(cycle_api::create))
        .route("/api/v1/cycles/velocity/", get(cycle_api::velocity))
        .route("/api/v1/cycles/{id}/", get(cycle_api::detail).put(cycle_api::update).patch(cycle_api::patch).delete(cycle_api::delete))
        .route("/api/v1/cycles/{id}/graph-position/", axum::routing::patch(cycle_api::update_graph_position))
        .route("/api/v1/cycles/{id}/progress/", get(cycle_api::progress))
        .route("/api/v1/cycles/{id}/complete/", post(cycle_api::complete))
        .route("/api/v1/cycles/{id}/burndown/", get(cycle_api::burndown))
        // JSON リソース API (Phase 3)
        .route("/api/v1/projects/", get(resource_api::project_list).post(resource_api::project_create))
        .route("/api/v1/projects/{id}/", get(resource_api::project_detail).put(resource_api::project_update).patch(resource_api::project_patch).delete(resource_api::project_delete))
        .route("/api/v1/projects/{id}/dependencies/", get(resource_api::project_dependency_graph))
        .route("/api/v1/teams/{id}/dependencies/", get(resource_api::team_dependency_graph))
        .route("/api/v1/projects/{project_id}/saved-views/", get(saved_view_api::list).post(saved_view_api::create))
        .route("/api/v1/teams/{team_id}/saved-views/", get(saved_view_api::list_by_team).post(saved_view_api::create_for_team))
        .route("/api/v1/saved-views/{id}/", axum::routing::patch(saved_view_api::update).delete(saved_view_api::delete))
        .route("/api/v1/categories/", get(resource_api::category_list).post(resource_api::category_create))
        .route("/api/v1/categories/{id}/", get(resource_api::category_detail).put(resource_api::category_update).delete(resource_api::category_delete))
        .route("/api/v1/milestones/", get(resource_api::milestone_list).post(resource_api::milestone_create))
        .route("/api/v1/milestones/{id}/", get(resource_api::milestone_detail).put(resource_api::milestone_update).delete(resource_api::milestone_delete))
        .route("/api/v1/labels/", get(resource_api::label_list).post(resource_api::label_create))
        .route("/api/v1/labels/{id}/", get(resource_api::label_detail).put(resource_api::label_update).delete(resource_api::label_delete))
        .route("/api/v1/holidays/", get(resource_api::holiday_list))
        .route("/api/v1/holidays/bulk-add/", post(resource_api::holiday_bulk_add))
        .route("/api/v1/holidays/{id}/", axum::routing::delete(resource_api::holiday_delete))

        .route("/api/v1/teams/", get(team_api::team_list).post(team_api::team_create))
        .route("/api/v1/teams/{id}/", get(team_api::team_detail).put(team_api::team_update).delete(team_api::team_delete))
        .route("/api/v1/teams/{id}/members/", get(team_api::team_members_list).post(team_api::team_members_add))
        .route("/api/v1/teams/{team_id}/members/{user_id}/", axum::routing::delete(team_api::team_members_remove))
        .route("/api/v1/teams/{id}/guests/", get(team_api::team_guests_list).post(team_api::team_guests_add))
        .route("/api/v1/teams/{team_id}/guests/{membership_id}/", axum::routing::delete(team_api::team_guests_remove))

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
        .route("/api/v1/dashboard/team/{team_slug}/summary/", get(dashboard_api::team_summary))

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

        .route("/api/v1/settings/ai/", get(settings_api::get_ai_settings).patch(settings_api::update_ai_settings))

        .route("/api/v1/ai/suggest-points/", post(ai_api::suggest_points))
        .route("/api/v1/ai/sprint-health/", post(ai_api::sprint_health))
        .route("/api/v1/ai/context-analysis/", post(ai_api::context_analysis))
        .route("/api/v1/ai/close-analysis/", post(ai_api::close_analysis))
        .route("/api/v1/ai/generate-prompt-text/", post(ai_api::generate_prompt_text))
        .route("/api/v1/ai/status/", get(ai_api::ai_status))
        .layer(axum_middleware::from_fn_with_state(state.clone(), jwt_auth::jwt_auth));

    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .merge(jwt_protected_routes)
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", api_doc::ApiDoc::openapi()))
        // 静的ファイル
        .nest_service("/static", ServeDir::new("static"))
        .nest_service("/media", ServeDir::new("media"))
        // 共有ステート
        .with_state(state)
}
