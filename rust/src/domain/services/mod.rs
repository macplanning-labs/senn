pub mod ticket_service;
pub mod notification_service;
pub mod wiki_service;
pub mod auth_service;
pub mod git_webhook_service;
pub mod ai_service;
pub mod jwt_service;
// totp_service / webauthn_service は Step 2 で auth-core クレートへ移行済み
// (auth_core::domain::totp / auth_core::domain::webauthn)。
// jwt_service は 2026-08-14 に django_compat 依存を終了し、WIP 専用実装に戻した。
