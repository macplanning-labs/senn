pub mod ticket_service;
pub mod dashboard_service;
pub mod notification_service;
pub mod wiki_service;
pub mod gantt_service;
pub mod burndown_service;
pub mod export_service;
pub mod auth_service;
pub mod git_webhook_service;
pub mod ai_service;
// jwt_service / totp_service / webauthn_service は Step 2 で auth-core クレートへ移行済み
// (auth_core::domain::jwt::django_compat / auth_core::domain::totp / auth_core::domain::webauthn)。
