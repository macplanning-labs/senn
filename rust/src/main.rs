mod config;
mod domain;
mod infrastructure;
mod presentation;
mod routes;
#[cfg(test)]
mod test_support;

use std::net::SocketAddr;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use crate::infrastructure::mail::MailSender;
use crate::presentation::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ログ初期化
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // 環境変数読み込み
    dotenvy::dotenv().ok();

    // 設定読み込み
    let config = config::AppConfig::from_env()?;
    let port = config.port;

    // DB接続
    let pool = infrastructure::db::create_pool().await?;

    // サイクル自動アクティブ化スケジューラを起動
    infrastructure::scheduler::spawn_cycle_auto_activation(pool.clone());

    // マイグレーション実行
    //
    // ⚠ このDBはDjangoの実スキーマ(accounts_user, tickets_ticket等)を共有している。
    // rust/migrations/ 配下の各マイグレーションはDjango側のテーブルと衝突しないよう
    // 個別にレビューした上で追加する運用のため、事故防止のため明示的に
    // RUST_RUN_MIGRATIONS=trueを指定した場合のみ実行する(デフォルトはスキップ)。
    // 旧プロトタイプ時代のDjangoと無関係な並行スキーマ用マイグレーション
    // (20260623000000_init.sql、m_users等)は2026-08-08に削除済み。
    if std::env::var("RUST_RUN_MIGRATIONS").as_deref() == Ok("true") {
        tracing::warn!("RUST_RUN_MIGRATIONS=true — マイグレーションを実行します");
        sqlx::migrate!().run(&pool).await?;
    } else {
        tracing::info!("マイグレーションをスキップ(RUST_RUN_MIGRATIONS=trueで有効化)");
    }

    // メール送信者
    let mail_sender = MailSender::new(&config);

    // 共有ステート構築
    let state = AppState::new(pool, config, Some(mail_sender)).await?;

    // 期限到来/超過リマインダースケジューラを起動
    infrastructure::scheduler::spawn_due_date_reminders(state.pool.clone(), state.mail_sender.clone());

    // ルーター構築
    let app = routes::create_router(state);

    // サーバー起動
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("🚀 WIP server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    // レート制限のSmartIpKeyExtractorがX-Forwarded-For等のヘッダーを
    // 持たないリクエストに対してpeer IPへフォールバックできるよう、
    // ConnectInfoを有効化しておく。
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}
