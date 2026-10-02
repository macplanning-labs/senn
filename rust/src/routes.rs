/// routes.rs — ルーター定義
///
/// アプリ処理方式設計書 §3 に基づく全ルート集約。
/// 認証必須ルートと公開ルートを分離。
use axum::{
    http::{header, HeaderValue, Method},
    middleware as axum_middleware,
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::ServeDir;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;
// Step 2: レート制限(IPトークンバケット)は auth-core::infrastructure::rate_limit へ移行済み。
use crate::presentation::{
    handlers::{
        access_report_api, ai_agent_api, ai_api, api_doc, attachment_api, auth, auth_api,
        chat_integration_api, cycle_api, dashboard_api, external_api, health, integration_api,
        invitation_api, media_api, notification_api2, password_reset_api, project_activity_api,
        project_structure_api, project_team_api, reaction_api, realtime_api, reports_api,
        resource_api, roadmap_api, saved_view_api, search_api, security_api, settings_api,
        sync_api, system_admin_api, team_access_api, team_api, team_archive_api, team_rule_api,
        ticket_link_api, tickets_api, time_entry_api, triage_api, wiki_api, workflow_status_api,
    },
    middleware::{auth::require_auth, jwt_auth},
    state::AppState,
};
use auth_core::infrastructure::rate_limit::{ip_rate_limit_layer, IpRateLimitConfig};
use auth_core::presentation::middleware::origin_check_middleware;

/// リアルタイムの入口（接続用トークンの発行・WebSocket の接続）の、IP単位の制限。
/// 1秒に2回の補充（バースト120）。社内の NAT の内側に多数の利用者がいて、デプロイやネットワーク復旧の直後に
/// 全員が一斉に再接続しても弾かない広さにしてある（端末は揺らぎつきのバックオフで再接続する）。
/// 1つのIPから短時間に大量の接続試行（トークンの総当たりなど）が来たときだけ 429 にする。
fn realtime_rate_limit() -> IpRateLimitConfig {
    IpRateLimitConfig::new(std::time::Duration::from_millis(500), 120)
}

pub fn create_router(state: AppState) -> Router {
    // 許可オリジンは BASE_URL が基本。ドメイン移行中に新旧ホストを併記したい等、
    // 追加が必要な場合のみ ADDITIONAL_ALLOWED_ORIGINS(カンマ区切り)で足す。
    let mut origins = vec![state.config.base_url.clone()];
    origins.extend(state.config.additional_allowed_origins.iter().cloned());
    let allowed_origins = Arc::new(origins);

    // Origin/Referer検証対象：Cookie session方式のログインルート（/auth/login, /auth/totp）
    let login_cookie_routes = Router::new()
        .route(
            "/auth/login",
            get(auth::login_page).post(auth::login_submit),
        )
        .route("/auth/totp", get(auth::totp_page).post(auth::totp_verify))
        .route(
            "/auth/webauthn/login/begin",
            post(security_api::passkey_login_begin),
        )
        .route(
            "/auth/webauthn/login/complete",
            post(auth::webauthn_login_complete),
        )
        .layer(axum_middleware::from_fn(origin_check_middleware(
            allowed_origins.clone(),
        )));

    // ルートのうち率制限なし（/auth/login, /auth/totp 除く）
    let basic_public_routes = Router::new()
        .route("/auth/webauthn", get(auth::webauthn_page))
        .route("/health", get(health::check))
        // REST API（APIキー認証 = セッション不要）
        .route("/api/v1/auth/token/refresh/", post(auth_api::token_refresh))
        .route("/api/v1/auth/register/", post(auth_api::register))
        // リアルタイム同期の WebSocket。JWT ではなく、使い捨ての接続用トークンで認証する
        .route(
            "/api/v1/realtime/ws",
            get(realtime_api::ws).layer(ip_rate_limit_layer(realtime_rate_limit())),
        )
        .merge(login_cookie_routes);

    // JSON認証API（ログイン）: 20 requests/min per IP
    let login_routes = Router::new()
        .route("/api/v1/auth/login/", post(auth_api::login))
        .route("/api/v1/auth/login/verify/", post(auth_api::login_verify))
        .route(
            "/api/v1/auth/passkey/login/begin/",
            post(security_api::passkey_login_begin),
        )
        .route(
            "/api/v1/auth/passkey/login/complete/",
            post(security_api::passkey_login_complete),
        )
        .route(
            "/api/v1/auth/password-reset/request/",
            post(password_reset_api::password_reset_request),
        )
        .route(
            "/api/v1/auth/password-reset/confirm/",
            post(password_reset_api::password_reset_confirm),
        )
        // 招待の確認・受諾と、メール確認(アクセス制御の再設計 フェーズ A。トークンで認証する公開ルート)
        .route("/api/v1/invitations/{token}/", get(invitation_api::preview))
        .route(
            "/api/v1/invitations/{token}/accept/",
            post(invitation_api::accept),
        )
        .route(
            "/api/v1/auth/verify-email/",
            post(invitation_api::verify_email),
        )
        .layer(ip_rate_limit_layer(IpRateLimitConfig::login_preset()));

    // GitHub Webhook: 60 requests/min per IP
    let webhook_routes = Router::new()
        .route(
            "/api/v1/webhooks/github/",
            post(integration_api::github_webhook),
        )
        .layer(ip_rate_limit_layer(IpRateLimitConfig::webhook_preset()));

    // 外部API: 300 requests/min per IP
    let external_routes = Router::new()
        .route(
            "/api/v1/external/tickets/",
            post(external_api::create_ticket),
        )
        .route(
            "/api/v1/external/tickets/{ticket_key}/comments/",
            post(external_api::create_comment),
        )
        // キーごとの回数制限(F-5。on のときだけ)
        .layer(axum_middleware::from_fn(
            crate::presentation::middleware::key_rate_limit::key_rate_limit,
        ))
        .layer(ip_rate_limit_layer(IpRateLimitConfig::external_api_preset()));

    // AI専用外部API: 300 requests/min per IP
    let ai_agent_routes = Router::new()
        .route(
            "/api/v1/ai-agent/projects/",
            post(ai_agent_api::create_project).get(ai_agent_api::list_projects),
        )
        .route(
            "/api/v1/ai-agent/tickets/{ticket_key}/comments/",
            post(ai_agent_api::add_comment),
        )
        .route(
            "/api/v1/ai-agent/tickets/{ticket_key}/",
            get(ai_agent_api::get_ticket)
                .patch(ai_agent_api::patch_ticket)
                .delete(ai_agent_api::delete_ticket),
        )
        .route(
            "/api/v1/ai-agent/tickets/",
            post(ai_agent_api::create_ticket).get(ai_agent_api::list_tickets),
        )
        .route(
            "/api/v1/ai-agent/tickets/{ticket_key}/dependencies/",
            get(ai_agent_api::list_ticket_dependencies).post(ai_agent_api::add_dependency),
        )
        .route(
            "/api/v1/ai-agent/tickets/{ticket_key}/dependencies/{dep_id}/",
            axum::routing::delete(ai_agent_api::delete_dependency),
        )
        .route(
            "/api/v1/ai-agent/tickets/{ticket_key}/links/",
            get(ai_agent_api::list_ticket_links).post(ai_agent_api::add_ticket_link),
        )
        .route(
            "/api/v1/ai-agent/tickets/{ticket_key}/links/{link_id}/",
            axum::routing::delete(ai_agent_api::delete_ticket_link),
        )
        .route(
            "/api/v1/ai-agent/wiki-pages/",
            get(ai_agent_api::list_wiki_pages),
        )
        .route(
            "/api/v1/ai-agent/wiki-pages/{slug}/attachments/",
            post(ai_agent_api::upload_wiki_attachment).get(ai_agent_api::list_wiki_attachments),
        )
        .route(
            "/api/v1/ai-agent/wiki-pages/{slug}/attachments/{attachment_id}/",
            axum::routing::put(ai_agent_api::replace_wiki_attachment)
                .delete(ai_agent_api::delete_wiki_attachment),
        )
        .route(
            "/api/v1/ai-agent/projects/{project_prefix}/dependencies/",
            get(ai_agent_api::get_project_dependency_graph),
        )
        .route(
            "/api/v1/ai-agent/cycles/",
            post(ai_agent_api::create_cycle).get(ai_agent_api::list_cycles),
        )
        .route(
            "/api/v1/ai-agent/cycles/{id}/",
            get(ai_agent_api::get_cycle)
                .patch(ai_agent_api::patch_cycle)
                .delete(ai_agent_api::delete_cycle),
        )
        .route(
            "/api/v1/ai-agent/teams/",
            post(ai_agent_api::create_team).get(ai_agent_api::list_teams),
        )
        // 共有の AI キーへの廃止予定の警告(アクセス制御の再設計 F-8)
        .layer(axum_middleware::from_fn_with_state(
            state.clone(),
            ai_agent_api::shared_key_notice,
        ))
        // キーごとの回数制限(F-5。on のときだけ)
        .layer(axum_middleware::from_fn(
            crate::presentation::middleware::key_rate_limit::key_rate_limit,
        ))
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
        .route(
            "/auth/password",
            get(auth::password_page).post(auth::password_change),
        )
        .route("/auth/mfa", get(auth::mfa_page))
        // 認証ミドルウェア適用
        .layer(axum_middleware::from_fn_with_state(
            state.clone(),
            require_auth,
        ))
        .layer(axum_middleware::from_fn(origin_check_middleware(
            allowed_origins.clone(),
        )));

    // JWT保護ルート
    let jwt_protected_routes = Router::new()
        .route(
            "/api/v1/media-session/",
            axum::routing::post(media_api::issue_session),
        )
        .route(
            "/api/v1/auth/me/",
            get(auth_api::me).patch(auth_api::update_my_profile),
        )
        .route(
            "/api/v1/auth/me/notification-preferences/",
            get(auth_api::list_notification_preferences)
                .patch(auth_api::update_notification_preference),
        )
        .route(
            "/api/v1/auth/me/ai-prompt-templates/",
            get(auth_api::get_ai_prompt_templates).put(auth_api::update_ai_prompt_templates),
        )
        .route(
            "/api/v1/auth/me/deactivate/",
            post(auth_api::deactivate_my_account),
        )
        .route("/api/v1/auth/logout/", post(auth_api::logout))
        .route("/api/v1/users/", get(auth_api::list_users))
        .route(
            "/api/v1/users/{id}/active/",
            axum::routing::patch(auth_api::set_user_active),
        )
        .route(
            "/api/v1/users/{id}/",
            axum::routing::patch(auth_api::update_user_profile),
        )
        // JSON チケット API
        .route(
            "/api/v1/tickets/",
            get(tickets_api::list).post(tickets_api::create),
        )
        .route(
            "/api/v1/tickets/bulk-import/",
            post(tickets_api::bulk_import),
        )
        .route(
            "/api/v1/tickets/bulk-delete/",
            post(tickets_api::bulk_delete),
        )
        .route(
            "/api/v1/tickets/{ticket_key}/",
            get(tickets_api::detail)
                .put(tickets_api::update)
                .patch(tickets_api::patch)
                .delete(tickets_api::delete),
        )
        .route(
            "/api/v1/tickets/{ticket_key}/extras/",
            get(tickets_api::extras),
        )
        .route(
            "/api/v1/tickets/{ticket_key}/comments/",
            get(tickets_api::list_comments).post(tickets_api::add_comment),
        )
        .route(
            "/api/v1/tickets/{ticket_key}/comments/{comment_id}/",
            axum::routing::patch(tickets_api::update_comment).delete(tickets_api::delete_comment),
        )
        .route(
            "/api/v1/tickets/{ticket_key}/watch/",
            axum::routing::post(tickets_api::watch_ticket).delete(tickets_api::unwatch_ticket),
        )
        .route(
            "/api/v1/tickets/{ticket_key}/ai-prompt/",
            axum::routing::post(tickets_api::generate_ticket_ai_prompt),
        )
        .route(
            "/api/v1/tickets/{ticket_key}/change-logs/",
            get(tickets_api::change_logs),
        )
        .route(
            "/api/v1/tickets/{ticket_key}/point-history/",
            get(tickets_api::point_history),
        )
        .route(
            "/api/v1/tickets/{ticket_key}/dependencies/",
            get(tickets_api::list_dependencies).post(tickets_api::add_dependency),
        )
        .route(
            "/api/v1/tickets/{ticket_key}/dependencies/{dep_id}/",
            axum::routing::delete(tickets_api::delete_dependency),
        )
        .route(
            "/api/v1/tickets/{ticket_key}/git-events/",
            get(tickets_api::git_events),
        )
        .route(
            "/api/v1/tickets/{ticket_key}/attachments/",
            post(attachment_api::upload_attachment).get(attachment_api::list_attachments),
        )
        .route(
            "/api/v1/tickets/{ticket_key}/attachments/{attachment_id}/",
            axum::routing::delete(attachment_api::delete_attachment),
        )
        .route(
            "/api/v1/tickets/{ticket_key}/reactions/",
            get(reaction_api::list_reactions).post(reaction_api::add_reaction),
        )
        .route(
            "/api/v1/tickets/{ticket_key}/reactions/{id}/",
            axum::routing::delete(reaction_api::delete_reaction),
        )
        .route(
            "/api/v1/projects/{prefix}/custom-emojis/",
            get(reaction_api::list_custom_emojis).post(reaction_api::upload_custom_emoji),
        )
        .route(
            "/api/v1/projects/{prefix}/custom-emojis/{id}/",
            axum::routing::delete(reaction_api::delete_custom_emoji),
        )
        .route(
            "/api/v1/tickets/{ticket_key}/links/",
            get(ticket_link_api::list_links).post(ticket_link_api::add_link),
        )
        .route(
            "/api/v1/tickets/{ticket_key}/links/{link_id}/",
            axum::routing::delete(ticket_link_api::delete_link),
        )
        .route("/api/v1/tickets/export/csv/", get(tickets_api::export_csv))
        // Sync API (Local-first)
        .route("/api/v1/sync/tickets/", get(sync_api::sync_tickets))
        .route("/api/v1/sync/projects/", get(sync_api::sync_projects))
        .route("/api/v1/sync/comments/", get(sync_api::sync_comments))
        // リアルタイム同期の接続用トークン（WebSocket 本体は認証ミドルウェアの外。トークンで認証する）
        .route(
            "/api/v1/realtime/connect-token/",
            axum::routing::post(realtime_api::connect_token)
                .layer(ip_rate_limit_layer(realtime_rate_limit())),
        )
        // JSON 通知 API (Phase 3 第二弾)
        .route("/api/v1/notifications/", get(notification_api2::list))
        .route(
            "/api/v1/notifications/{id}/read/",
            post(notification_api2::mark_read),
        )
        .route(
            "/api/v1/notifications/read_all/",
            post(notification_api2::mark_all_read),
        )
        .route(
            "/api/v1/notifications/unread_count/",
            get(notification_api2::unread_count),
        )
        .route(
            "/api/v1/notifications/{id}/",
            axum::routing::delete(notification_api2::dismiss),
        )
        .route(
            "/api/v1/notifications/dismiss_read/",
            post(notification_api2::dismiss_all_read),
        )
        // JSON サイクル API (Phase 3 第二弾)
        .route(
            "/api/v1/cycles/",
            get(cycle_api::list).post(cycle_api::create),
        )
        .route("/api/v1/cycles/velocity/", get(cycle_api::velocity))
        .route(
            "/api/v1/cycles/{id}/",
            get(cycle_api::detail)
                .put(cycle_api::update)
                .patch(cycle_api::patch)
                .delete(cycle_api::delete),
        )
        .route(
            "/api/v1/cycles/{id}/graph-position/",
            axum::routing::patch(cycle_api::update_graph_position),
        )
        .route("/api/v1/cycles/{id}/progress/", get(cycle_api::progress))
        .route("/api/v1/cycles/{id}/complete/", post(cycle_api::complete))
        .route("/api/v1/cycles/{id}/burndown/", get(cycle_api::burndown))
        // JSON リソース API (Phase 3)
        .route(
            "/api/v1/projects/",
            get(resource_api::project_list).post(resource_api::project_create),
        )
        .route(
            "/api/v1/projects/{id}/",
            get(resource_api::project_detail)
                .put(resource_api::project_update)
                .patch(resource_api::project_patch)
                .delete(resource_api::project_delete),
        )
        .route(
            "/api/v1/projects/{id}/teams/",
            get(project_team_api::teams_overview).post(resource_api::project_team_add),
        )
        .route(
            "/api/v1/projects/{id}/teams/{team_id}/",
            axum::routing::delete(resource_api::project_team_remove),
        )
        .route(
            "/api/v1/projects/{id}/dependencies/",
            get(resource_api::project_dependency_graph),
        )
        .route(
            "/api/v1/projects/{id}/structure/",
            get(project_structure_api::structure),
        )
        .route(
            "/api/v1/projects/{id}/children/",
            get(project_structure_api::children),
        )
        .route(
            "/api/v1/projects/{id}/relations/",
            post(project_structure_api::add_relation),
        )
        .route(
            "/api/v1/projects/{id}/relations/{related_id}/",
            axum::routing::delete(project_structure_api::remove_relation),
        )
        .route(
            "/api/v1/projects/{id}/activity/",
            get(project_activity_api::activity_list),
        )
        .route(
            "/api/v1/teams/{id}/archive/",
            post(team_archive_api::team_archive),
        )
        .route(
            "/api/v1/teams/{id}/archive-check/",
            get(team_archive_api::team_archive_check),
        )
        .route(
            "/api/v1/teams/{id}/unarchive/",
            post(team_archive_api::team_unarchive),
        )
        .route(
            "/api/v1/roadmaps/",
            get(roadmap_api::roadmaps_list).post(roadmap_api::roadmaps_create),
        )
        .route(
            "/api/v1/roadmaps/{id}/",
            get(roadmap_api::roadmap_detail)
                .put(roadmap_api::roadmap_update)
                .delete(roadmap_api::roadmap_delete),
        )
        .route(
            "/api/v1/roadmaps/{id}/projects/",
            post(roadmap_api::roadmap_add_project),
        )
        .route(
            "/api/v1/roadmaps/{id}/projects/{project_id}/",
            axum::routing::delete(roadmap_api::roadmap_remove_project),
        )
        .route(
            "/api/v1/projects/{id}/updates/",
            get(project_activity_api::updates_list).post(project_activity_api::update_create),
        )
        .route(
            "/api/v1/projects/{id}/updates/{update_id}/",
            axum::routing::put(project_activity_api::update_edit)
                .delete(project_activity_api::update_delete),
        )
        .route(
            "/api/v1/teams/{id}/dependencies/",
            get(resource_api::team_dependency_graph),
        )
        .route(
            "/api/v1/saved-views/",
            get(saved_view_api::list_global).post(saved_view_api::create_global),
        )
        .route(
            "/api/v1/projects/{project_id}/saved-views/",
            get(saved_view_api::list).post(saved_view_api::create),
        )
        .route(
            "/api/v1/teams/{team_id}/saved-views/",
            get(saved_view_api::list_by_team).post(saved_view_api::create_for_team),
        )
        .route(
            "/api/v1/saved-views/{id}/",
            axum::routing::patch(saved_view_api::update).delete(saved_view_api::delete),
        )
        .route(
            "/api/v1/categories/",
            get(resource_api::category_list).post(resource_api::category_create),
        )
        .route(
            "/api/v1/categories/{id}/",
            get(resource_api::category_detail)
                .put(resource_api::category_update)
                .delete(resource_api::category_delete),
        )
        .route(
            "/api/v1/milestones/",
            get(resource_api::milestone_list).post(resource_api::milestone_create),
        )
        .route(
            "/api/v1/milestones/{id}/",
            get(resource_api::milestone_detail)
                .put(resource_api::milestone_update)
                .delete(resource_api::milestone_delete),
        )
        .route(
            "/api/v1/labels/",
            get(resource_api::label_list).post(resource_api::label_create),
        )
        .route(
            "/api/v1/labels/{id}/",
            get(resource_api::label_detail)
                .put(resource_api::label_update)
                .delete(resource_api::label_delete),
        )
        .route("/api/v1/holidays/", get(resource_api::holiday_list))
        .route(
            "/api/v1/holidays/bulk-add/",
            post(resource_api::holiday_bulk_add),
        )
        .route(
            "/api/v1/holidays/{id}/",
            axum::routing::delete(resource_api::holiday_delete),
        )
        .route(
            "/api/v1/teams/",
            get(team_api::team_list).post(team_api::team_create),
        )
        .route(
            "/api/v1/teams/{id}/",
            get(team_api::team_detail)
                .put(team_api::team_update)
                .delete(team_api::team_delete),
        )
        .route(
            "/api/v1/teams/{id}/members/",
            get(team_api::team_members_list).post(team_api::team_members_add),
        )
        .route(
            "/api/v1/teams/{team_id}/members/{user_id}/",
            axum::routing::delete(team_api::team_members_remove),
        )
        .route(
            "/api/v1/teams/{id}/guests/",
            get(team_api::team_guests_list).post(team_api::team_guests_add),
        )
        .route(
            "/api/v1/teams/{team_id}/guests/{membership_id}/",
            axum::routing::delete(team_api::team_guests_remove),
        )
        // アクセス制御の再設計 G-1: Join・退出・公開区分・Owner
        // 招待(アクセス制御の再設計 フェーズ A)
        .route(
            "/api/v1/invitations/",
            get(invitation_api::list).post(invitation_api::create),
        )
        .route(
            "/api/v1/invitations/{id}/revoke/",
            post(invitation_api::revoke),
        )
        .route("/api/v1/teams/{id}/join/", post(team_access_api::join))
        .route("/api/v1/teams/{id}/leave/", post(team_access_api::leave))
        .route(
            "/api/v1/teams/{id}/access/",
            axum::routing::patch(team_access_api::update_access),
        )
        .route(
            "/api/v1/teams/{team_id}/owners/{user_id}/",
            post(team_access_api::add_owner).delete(team_access_api::remove_owner),
        )
        // G-4: 孤立 Private チームの救済(システム管理者)
        .route(
            "/api/v1/system-admin/orphan-teams/",
            get(team_access_api::orphan_teams),
        )
        .route(
            "/api/v1/system-admin/orphan-teams/{id}/rescue/",
            post(team_access_api::rescue_orphan),
        )
        // G-6: 移行の確認の一覧(システム管理者)
        .route(
            "/api/v1/system-admin/access-migration-report/",
            get(access_report_api::access_migration_report),
        )
        .route(
            "/api/v1/team-rules/",
            get(team_rule_api::team_rule_list).post(team_rule_api::team_rule_create),
        )
        .route(
            "/api/v1/team-rules/{id}/",
            get(team_rule_api::team_rule_detail)
                .put(team_rule_api::team_rule_update)
                .patch(team_rule_api::team_rule_update)
                .delete(team_rule_api::team_rule_delete),
        )
        .route(
            "/api/v1/workflow-statuses/",
            get(workflow_status_api::workflow_status_list)
                .post(workflow_status_api::workflow_status_create),
        )
        .route(
            "/api/v1/workflow-statuses/reorder/",
            post(workflow_status_api::workflow_status_reorder),
        )
        .route(
            "/api/v1/workflow-statuses/{id}/",
            get(workflow_status_api::workflow_status_detail)
                .put(workflow_status_api::workflow_status_update)
                .patch(workflow_status_api::workflow_status_partial_update)
                .delete(workflow_status_api::workflow_status_delete),
        )
        .route(
            "/api/v1/time-entries/",
            get(time_entry_api::time_entry_list).post(time_entry_api::time_entry_create),
        )
        .route(
            "/api/v1/time-entries/my-today/",
            get(time_entry_api::time_entry_my_today),
        )
        .route(
            "/api/v1/time-entries/{id}/",
            axum::routing::delete(time_entry_api::time_entry_delete),
        )
        .route(
            "/api/v1/triage-requests/",
            get(triage_api::list).post(triage_api::create),
        )
        .route(
            "/api/v1/triage-requests/{id}/",
            get(triage_api::detail)
                .put(triage_api::update)
                .patch(triage_api::update)
                .delete(triage_api::delete),
        )
        .route(
            "/api/v1/triage-requests/{id}/approve/",
            post(triage_api::approve),
        )
        .route(
            "/api/v1/triage-requests/{id}/reject/",
            post(triage_api::reject),
        )
        .route("/api/v1/wiki/", get(wiki_api::list).post(wiki_api::create))
        .route(
            "/api/v1/wiki/{id}/",
            get(wiki_api::detail)
                .put(wiki_api::update)
                .patch(wiki_api::update)
                .delete(wiki_api::delete),
        )
        .route("/api/v1/wiki/{id}/revisions/", get(wiki_api::revisions))
        .route(
            "/api/v1/wiki/{id}/link-ticket/",
            post(wiki_api::link_ticket),
        )
        .route(
            "/api/v1/wiki/{id}/unlink-ticket/",
            post(wiki_api::unlink_ticket),
        )
        .route("/api/v1/search/", get(search_api::search))
        .route("/api/v1/reports/workload/", get(reports_api::workload))
        .route("/api/v1/dashboard/stats/", get(dashboard_api::stats))
        .route(
            "/api/v1/dashboard/my-tickets/",
            get(dashboard_api::my_tickets),
        )
        .route("/api/v1/dashboard/activity/", get(dashboard_api::activity))
        .route(
            "/api/v1/dashboard/default/",
            get(dashboard_api::default_dashboard),
        )
        .route(
            "/api/v1/dashboard/list/",
            get(dashboard_api::list_dashboards),
        )
        .route(
            "/api/v1/dashboard/detail/{id}/",
            get(dashboard_api::detail_dashboard),
        )
        .route(
            "/api/v1/dashboard/create/",
            post(dashboard_api::create_dashboard),
        )
        .route(
            "/api/v1/dashboard/update/",
            axum::routing::patch(dashboard_api::update_dashboard),
        )
        .route(
            "/api/v1/dashboard/delete/",
            axum::routing::delete(dashboard_api::delete_dashboard),
        )
        .route(
            "/api/v1/dashboard/add-widget/",
            post(dashboard_api::add_widget),
        )
        .route(
            "/api/v1/dashboard/remove-widget/",
            axum::routing::delete(dashboard_api::remove_widget),
        )
        .route(
            "/api/v1/dashboard/reorder-widgets/",
            post(dashboard_api::reorder_widgets),
        )
        .route(
            "/api/v1/dashboard/team/{team_slug}/summary/",
            get(dashboard_api::team_summary),
        )
        .route(
            "/api/v1/integrations/",
            get(integration_api::list).post(integration_api::create),
        )
        .route(
            "/api/v1/integrations/{id}/",
            axum::routing::patch(integration_api::update).delete(integration_api::delete),
        )
        .route(
            "/api/v1/chat-integrations/",
            get(chat_integration_api::list).post(chat_integration_api::create),
        )
        .route(
            "/api/v1/chat-integrations/{id}/",
            axum::routing::patch(chat_integration_api::update).delete(chat_integration_api::delete),
        )
        // セキュリティ設定（TOTP）
        .route(
            "/api/v1/settings/security/totp/begin/",
            post(security_api::totp_begin),
        )
        .route(
            "/api/v1/settings/security/totp/confirm/",
            post(security_api::totp_confirm),
        )
        .route(
            "/api/v1/settings/security/totp/disable/",
            post(security_api::totp_disable),
        )
        // セキュリティ設定（パスキー/WebAuthn）
        .route(
            "/api/v1/settings/security/passkey/register/begin/",
            post(security_api::passkey_register_begin),
        )
        .route(
            "/api/v1/settings/security/passkey/register/complete/",
            post(security_api::passkey_register_complete),
        )
        .route(
            "/api/v1/settings/security/passkeys/",
            get(security_api::list_passkeys),
        )
        .route(
            "/api/v1/settings/security/passkey/{id}/delete/",
            post(security_api::delete_passkey),
        )
        .route(
            "/api/v1/settings/ai/",
            get(settings_api::get_ai_settings).patch(settings_api::update_ai_settings),
        )
        .route(
            "/api/v1/system-admin/settings/",
            get(system_admin_api::get_settings).put(system_admin_api::update_settings),
        )
        .route(
            "/api/v1/system-admin/realtime/stats/",
            get(system_admin_api::get_realtime_stats),
        )
        .route(
            "/api/v1/system-admin/ai-agent-key/",
            get(system_admin_api::get_ai_agent_key).put(system_admin_api::update_ai_agent_key),
        )
        .route(
            "/api/v1/system-admin/ai-agent-personal-keys/",
            get(system_admin_api::list_ai_agent_personal_keys)
                .post(system_admin_api::create_ai_agent_personal_key),
        )
        .route(
            "/api/v1/system-admin/ai-agent-personal-keys/{id}/",
            axum::routing::delete(system_admin_api::revoke_ai_agent_personal_key),
        )
        .route(
            "/api/v1/system-admin/backup/export/",
            post(system_admin_api::backup_export),
        )
        .route("/api/v1/ai/suggest-points/", post(ai_api::suggest_points))
        .route("/api/v1/ai/sprint-health/", post(ai_api::sprint_health))
        .route(
            "/api/v1/ai/context-analysis/",
            post(ai_api::context_analysis),
        )
        .route("/api/v1/ai/close-analysis/", post(ai_api::close_analysis))
        .route(
            "/api/v1/ai/generate-prompt-text/",
            post(ai_api::generate_prompt_text),
        )
        .route("/api/v1/ai/status/", get(ai_api::ai_status))
        .layer(axum_middleware::from_fn_with_state(
            state.clone(),
            jwt_auth::jwt_auth,
        ));

    // Tauriデスクトップアプリ（macOS/Linux: tauri://localhost, Windows: http://tauri.localhost）から
    // 本番APIを利用可能にするためのCORS許可
    let desktop_cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list([
            HeaderValue::from_static("tauri://localhost"),
            HeaderValue::from_static("http://tauri.localhost"),
        ]))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]);

    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .merge(jwt_protected_routes)
        .merge(
            SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", api_doc::ApiDoc::openapi()),
        )
        // 静的ファイル
        .nest_service("/static", ServeDir::new("static"))
        // 添付ファイル・絵文字は認証付きで配信する(media_api)。ServeDir で直接配信しない
        .route("/media/{*path}", get(media_api::serve))
        // CORS許可レイヤー
        .layer(desktop_cors)
        // 共有ステート
        .with_state(state)
}

#[cfg(test)]
mod nginx_allowlist_tests {
    //! nginx(docker/nginx.conf)は、API を「許可リスト」で Rust に転送する。
    //! 新しい /api/v1/<名前>/ を足してリストに入れ忘れると、ローカル(Vite は全て転送)では動くのに、
    //! ステージング・本番だけ API が届かない(SPA の HTML が返る)。それを、ここで検出する。

    const ROUTES: &str = include_str!("routes.rs");

    /// routes.rs に書かれた /api/v1/<名前> の一覧(この tests モジュール自身の文字列は除く)
    fn route_names() -> Vec<String> {
        let code = ROUTES.split("#[cfg(test)]").next().unwrap_or(ROUTES);
        let mut names: Vec<String> = code
            .match_indices("\"/api/v1/")
            .map(|(i, _)| {
                code[i + 9..]
                    .chars()
                    .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-')
                    .collect::<String>()
            })
            .filter(|n| !n.is_empty())
            .collect();
        names.sort();
        names.dedup();
        names
    }

    fn covered(nginx: &str, name: &str) -> bool {
        // (1) 個別の location: `location /api/v1/<名前>/ {`（パスが `/api/v1/<名前>/` ちょうど）。
        //     `location /api/v1/<名前>/ws {` のように一部の下だけを転送するものは、<名前> 全体の転送とは見なさない
        //     （見なすと、ws だけ転送して connect-token が SPA に流れる漏れを見逃す: 2026-09-30 realtime）
        let exact = format!("/api/v1/{name}/");
        let direct = nginx
            .lines()
            .filter(|l| l.trim_start().starts_with("location"))
            .any(|l| l.split_whitespace().any(|tok| tok == exact));
        // (2) 正規表現の許可リスト: `^/api/v1/(a|b|c)(/|$)` の中の名前
        let in_regex = nginx.lines().any(|l| {
            l.contains("^/api/v1/(")
                && l.split("^/api/v1/(")
                    .nth(1)
                    .and_then(|r| r.split(')').next())
                    .map(|alts| alts.split('|').any(|a| a == name))
                    .unwrap_or(false)
        });
        direct || in_regex
    }

    #[test]
    fn every_api_route_is_forwarded_by_nginx() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../docker/nginx.conf");
        let Ok(nginx) = std::fs::read_to_string(path) else {
            // Docker のビルドコンテキスト(rust/ のみ)など、リポジトリ全体が無い環境では実行しない
            eprintln!("docker/nginx.conf が無いため、突き合わせをスキップします");
            return;
        };
        let names = route_names();
        assert!(
            names.len() > 10,
            "routes.rs から API の名前を読み取れていません: {names:?}"
        );
        let missing: Vec<&String> = names.iter().filter(|n| !covered(&nginx, n)).collect();
        assert!(
            missing.is_empty(),
            "nginx の許可リストに無い API(ステージング・本番で届きません): {missing:?} — docker/nginx.conf に追加してください"
        );
    }

    #[test]
    fn covered_matches_both_location_styles() {
        let nginx = "location /api/v1/auth/ {\n}\nlocation ~ ^/api/v1/(projects|teams)(/|$) {\n}\n";
        assert!(covered(nginx, "auth"));
        assert!(covered(nginx, "projects") && covered(nginx, "teams"));
        assert!(!covered(nginx, "roadmaps"));
        assert!(!covered(nginx, "team"), "部分一致で通してはいけない");
    }

    /// 一部の下だけを転送する location は、その名前全体の転送とは見なさない
    #[test]
    fn a_location_for_only_a_sub_path_does_not_cover_the_whole_name() {
        let only_ws = "location /api/v1/realtime/ws {
}
location ~ ^/api/v1/(projects)(/|$) {
}
";
        assert!(
            !covered(only_ws, "realtime"),
            "ws だけの location では connect-token が届かない"
        );
        let whole = "location /api/v1/realtime/ {
}
location ^~ /api/v1/realtime/ws {
}
";
        assert!(covered(whole, "realtime"));
        // 完全一致の location（`=`）は、その1本のパスだけ。名前全体とは見なさない
        assert!(!covered(
            "location = /api/v1/ai/generate-prompt-text/ {
}
",
            "ai"
        ));
    }
}

#[cfg(test)]
mod realtime_rate_limit_tests {
    use super::realtime_rate_limit;

    /// 社内 NAT の内側の全員が一斉に再接続しても弾かない（1人あたりトークン発行+接続で2回 → 50人で100回）
    #[test]
    fn allows_an_office_wide_reconnect_storm_but_not_unbounded_bursts() {
        let cfg = realtime_rate_limit();
        assert!(
            cfg.burst_size >= 100,
            "全員の一斉再接続を許すバースト: {}",
            cfg.burst_size
        );
        assert!(
            cfg.burst_size <= 1000,
            "総当たりを許さない上限: {}",
            cfg.burst_size
        );
        let per_minute = 60_000 / cfg.period.as_millis() as u32;
        assert!(
            (60..=600).contains(&per_minute),
            "定常の補充は毎分60〜600回: {per_minute}"
        );
    }
}

#[cfg(test)]
mod access_route_tests {
    //! アクセス制御の再設計(詳細設計書 §13.2): すべてのルートのハンドラが、閲覧者(Viewer)を受け取ること。
    //! 受け取らないハンドラは、「公開ルートの一覧」か「未移行の一覧(access_pending_handlers.txt)」に載っていなければ失敗する。
    //! 未移行の一覧は、減る方向にしか変えられない(`PENDING_MAX` を超えると失敗する)。

    use regex::Regex;
    use std::collections::BTreeSet;

    const ROUTES: &str = include_str!("routes.rs");
    const PENDING: &str = include_str!("access_pending_handlers.txt");
    /// 未移行の一覧の件数の上限。移したら、その数だけ減らす(増やさない)
    const PENDING_MAX: usize = 0;

    /// JWT を通らない公開ルートのハンドラ(閲覧者が無い)。載せるには理由が要る
    const PUBLIC_HANDLERS: &[(&str, &str)] = &[
        ("auth::login_page", "ログイン画面(認証前)"),
        ("auth::login_submit", "ログイン(認証前)"),
        ("auth::totp_page", "二要素認証の画面(認証の途中)"),
        ("auth::totp_verify", "二要素認証(認証の途中)"),
        ("auth::webauthn_page", "パスキーの画面(認証前)"),
        (
            "auth::webauthn_login_complete",
            "パスキーでのログイン(認証前)",
        ),
        ("auth::logout", "画面のセッション(Cookie)。本人のみ"),
        ("auth::mfa_page", "画面のセッション(Cookie)。本人のみ"),
        (
            "auth::password_change",
            "画面のセッション(Cookie)。本人のみ",
        ),
        ("auth::password_page", "画面のセッション(Cookie)。本人のみ"),
        ("auth_api::login", "ログイン(認証前)"),
        ("auth_api::login_verify", "二要素認証(認証の途中)"),
        (
            "auth_api::register",
            "登録(認証前。既定は社内ドメインだけ・メール確認つき。DEMO-000085)",
        ),
        ("invitation_api::preview", "招待の確認(招待のトークン)"),
        ("invitation_api::accept", "招待の受諾(招待のトークン)"),
        ("invitation_api::verify_email", "メール確認(確認のトークン)"),
        ("auth_api::token_refresh", "リフレッシュトークンの検証"),
        ("health::check", "ヘルスチェック"),
        (
            "integration_api::github_webhook",
            "GitHub からの受信。HMAC 署名で検証",
        ),
        (
            "password_reset_api::password_reset_request",
            "パスワード再設定(認証前)",
        ),
        (
            "password_reset_api::password_reset_confirm",
            "パスワード再設定(トークン)",
        ),
        (
            "realtime_api::ws",
            "一回限りの接続トークンで検証(connect-token は閲覧者を受け取る)",
        ),
        (
            "security_api::passkey_login_begin",
            "パスキーでのログイン(認証前)",
        ),
        (
            "security_api::passkey_login_complete",
            "パスキーでのログイン(認証前)",
        ),
        (
            "media_api::serve",
            "添付の配信。メディア専用の Cookie で検証",
        ),
    ];

    /// routes.rs に登録された全ハンドラ(module::fn)
    fn route_handlers() -> BTreeSet<String> {
        let code = ROUTES.split("#[cfg(test)]").next().unwrap_or(ROUTES);
        let route = Regex::new(r#"\.route\(\s*""#).unwrap();
        let handler = Regex::new(
            r"(?:^|[^\w])(?:get|post|put|patch|delete)\(\s*((?:[a-z_0-9]+::)*[a-z_0-9]+)\s*\)",
        )
        .unwrap();
        let starts: Vec<usize> = route.find_iter(code).map(|m| m.start()).collect();
        let mut out = BTreeSet::new();
        for (i, &s) in starts.iter().enumerate() {
            let e = starts.get(i + 1).copied().unwrap_or(code.len());
            for c in handler.captures_iter(&code[s..e]) {
                out.insert(c[1].to_string());
            }
        }
        out
    }

    /// ハンドラの引数に `: Viewer`(AI エージェント API は `: AiCaller`)があるか(関数の定義を、ソースから探す)
    fn takes_viewer(handler: &str) -> bool {
        let (module, func) = handler.split_once("::").expect("module::fn の形");
        let path = format!(
            "{}/src/presentation/handlers/{module}.rs",
            env!("CARGO_MANIFEST_DIR")
        );
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("ハンドラのファイルが無い: {path}"));
        let def = Regex::new(&format!(
            r"pub async fn {}\s*(<[^>]*>)?\s*\(",
            regex::escape(func)
        ))
        .unwrap();
        let m = def
            .find(&src)
            .unwrap_or_else(|| panic!("ハンドラの定義が見つからない: {handler}"));
        let rest = &src[m.start()..];
        let end = [rest.find(") ->"), rest.find("{\n")]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or(rest.len());
        Regex::new(r":\s*(Viewer|AiCaller|ExternalCaller)\b")
            .unwrap()
            .is_match(&rest[..end])
    }

    fn pending() -> BTreeSet<String> {
        PENDING
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(String::from)
            .collect()
    }

    #[test]
    fn every_handler_takes_a_viewer_or_is_listed() {
        let handlers = route_handlers();
        assert!(
            handlers.len() > 200,
            "ハンドラの抽出に失敗している({} 件)",
            handlers.len()
        );
        let public: BTreeSet<&str> = PUBLIC_HANDLERS.iter().map(|(h, _)| *h).collect();
        let pending = pending();
        let mut errors = Vec::new();
        for h in &handlers {
            let has = takes_viewer(h);
            if public.contains(h.as_str()) {
                continue;
            }
            if pending.contains(h) {
                if has {
                    errors.push(format!(
                        "{h}: 閲覧者に移し終わっている。未移行の一覧から消し、PENDING_MAX を減らす"
                    ));
                }
            } else if !has {
                errors.push(format!("{h}: 閲覧者(Viewer)を受け取っていない。受け取るか、理由があれば公開ルートの一覧に載せる"));
            }
        }
        for p in &pending {
            if !handlers.contains(p) {
                errors.push(format!(
                    "{p}: 未移行の一覧にあるが、ルートに無い(一覧から消す)"
                ));
            }
        }
        for h in &public {
            if !handlers.contains(*h) {
                errors.push(format!("{h}: 公開ルートの一覧にあるが、ルートに無い"));
            }
        }
        assert!(
            errors.is_empty(),
            "アクセス制御のルート検査:\n{}",
            errors.join("\n")
        );
    }

    #[test]
    fn pending_list_only_shrinks() {
        // すべて移し終えた(フェーズ F)。一覧は空のまま保つ(フェーズ H で一覧と検査の仕組みごと片付ける)
        let n = pending().len();
        assert_eq!(
            n, PENDING_MAX,
            "未移行の一覧は空のはず({n} 件ある)。新しいハンドラは、閲覧者(Viewer)を受け取ること(一覧に足さない)"
        );
    }

    #[test]
    fn handlers_do_not_create_system_context() {
        let dir = format!("{}/src/presentation/handlers", env!("CARGO_MANIFEST_DIR"));
        let mut hits = Vec::new();
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let src = std::fs::read_to_string(&path).unwrap_or_default();
            if src.contains("SystemContext::for_background_job") {
                hits.push(path.display().to_string());
            }
        }
        assert!(
            hits.is_empty(),
            "ハンドラ(利用者の操作)で SystemContext を作っている: {hits:?}"
        );
    }
}

#[cfg(test)]
mod enforced_handler_tests {
    //! 試運転(shadow)を通さず、すぐに判定を適用するハンドラの一覧(DEMO-000085・DEMO-000169)。
    //!
    //! 試運転中は今の判定が結果を決めるので、新しい規則の穴は本番に出ない。ところが、すぐに適用する
    //! ハンドラ(参加・Owner・招待など)は、新しい規則の穴がそのまま本番に出る(10/01 に、参加しただけで
    //! Owner になれた)。そこで、すぐに適用する判定を呼ぶハンドラは、この一覧に載せ、攻撃の手順を確かめる
    //! テスト(DB テスト)の名前を書くことを必須にする。
    //! - ハンドラのソースから、下の目印を呼んでいる `pub async fn` を見つける
    //! - 見つかったハンドラが一覧に無い / 一覧の物が目印を呼んでいない / 書いたテストが無い → 失敗

    use std::collections::BTreeSet;

    /// すぐに判定を適用する呼び出し(試運転のスイッチを通らない)
    const MARKERS: &[&str] = &[
        "team_and_check(",
        "check_invite(",
        "require_owner_operation(",
        "guard_owner_removal(",
        "require_system_admin(",
    ];

    /// (ハンドラ, そのハンドラへの攻撃の手順を確かめるテスト関数の名前。同じファイルにあること)
    const ENFORCED: &[(&str, &[&str])] = &[
        ("team_access_api::join", &["joiner_cannot_take_over_team"]),
        (
            "team_access_api::leave",
            &["last_owner_and_last_private_member_cannot_leave"],
        ),
        (
            "team_access_api::update_access",
            &["joiner_cannot_take_over_team"],
        ),
        (
            "team_access_api::add_owner",
            &[
                "joiner_cannot_take_over_team",
                "ownerless_team_needs_system_admin_to_nominate",
            ],
        ),
        (
            "team_access_api::remove_owner",
            &["joiner_cannot_take_over_team"],
        ),
        (
            "team_access_api::orphan_teams",
            &["rescue_is_for_system_admins_only"],
        ),
        (
            "team_access_api::rescue_orphan",
            &[
                "rescue_is_for_system_admins_only",
                "rescue_private_team_without_owner_from_members_only",
            ],
        ),
        ("invitation_api::create", &["joiner_cannot_invite"]),
        (
            "invitation_api::list",
            &["joiner_cannot_list_or_revoke_invitations"],
        ),
        (
            "invitation_api::revoke",
            &["joiner_cannot_list_or_revoke_invitations"],
        ),
        (
            "team_api::team_members_add",
            &[
                "member_of_ownerless_team_cannot_grant_owner",
                "guest_cannot_be_owner",
            ],
        ),
        (
            "team_api::team_members_remove",
            &[
                "last_owner_cannot_be_removed",
                "member_cannot_remove_other_owner_even_under_new_rules",
            ],
        ),
        (
            "team_api::team_delete",
            &["member_of_ownerless_team_cannot_delete_team"],
        ),
        (
            "team_api::team_guests_add",
            &["member_of_ownerless_team_cannot_add_project_guest"],
        ),
    ];

    fn handler_sources() -> Vec<(String, String)> {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/presentation/handlers");
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&dir).expect("handlers のディレクトリを読めない")
        {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let module = path.file_stem().unwrap().to_string_lossy().to_string();
            out.push((module, std::fs::read_to_string(&path).unwrap()));
        }
        out
    }

    /// 目印を呼んでいる `pub async fn`(module::fn)
    fn handlers_with_markers() -> BTreeSet<String> {
        let mut found = BTreeSet::new();
        for (module, src) in handler_sources() {
            if module == "access_guard" {
                continue; // 目印の定義そのもの
            }
            let code = src.split("#[cfg(test)]").next().unwrap_or(&src);
            for chunk in code.split("\npub async fn ").skip(1) {
                let name: String = chunk
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                // 本文は、次の関数(`fn `)の手前まで
                let body = chunk.split("\nfn ").next().unwrap_or(chunk);
                let body = body.split("\nasync fn ").next().unwrap_or(body);
                if MARKERS.iter().any(|m| body.contains(m)) {
                    found.insert(format!("{module}::{name}"));
                }
            }
        }
        found
    }

    #[test]
    fn every_enforced_handler_is_listed_with_attack_tests() {
        let found = handlers_with_markers();
        let listed: BTreeSet<String> = ENFORCED.iter().map(|(h, _)| h.to_string()).collect();
        let missing: Vec<_> = found.difference(&listed).collect();
        assert!(
            missing.is_empty(),
            "すぐに判定を適用するハンドラが一覧(ENFORCED)にありません。攻撃の手順を確かめる DB テストを書き、一覧に足してください: {missing:?}"
        );
        let stale: Vec<_> = listed.difference(&found).collect();
        assert!(
            stale.is_empty(),
            "一覧(ENFORCED)にあるが、すぐに適用する判定を呼んでいないハンドラ(一覧を直す): {stale:?}"
        );
    }

    #[test]
    fn listed_attack_tests_exist() {
        let sources: std::collections::BTreeMap<String, String> =
            handler_sources().into_iter().collect();
        for (handler, tests) in ENFORCED {
            assert!(
                !tests.is_empty(),
                "{handler}: テストが 1 つも書かれていない"
            );
            let module = handler.split("::").next().unwrap();
            let src = sources
                .get(module)
                .unwrap_or_else(|| panic!("{module}.rs が無い"));
            for t in *tests {
                assert!(
                    src.contains(&format!("async fn {t}(")) || src.contains(&format!("fn {t}(")),
                    "{handler}: テスト `{t}` が {module}.rs にありません"
                );
            }
        }
    }
}
