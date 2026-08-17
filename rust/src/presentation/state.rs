/// presentation/state.rs — 共有ステート定義
///
/// Axum の State として全ハンドラに注入される。
/// DB プール + アプリ設定 + メール送信者 を保持する。

use sqlx::PgPool;
use crate::config::AppConfig;
use crate::infrastructure::mail::MailSender;

#[derive(Clone)]
#[allow(dead_code)]
pub struct AppState {
    pub pool: PgPool,
    pub config: AppConfig,
    pub mail_sender: Option<MailSender>,
}
