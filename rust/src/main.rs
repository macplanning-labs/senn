mod config;
mod domain;
mod infrastructure;
mod presentation;
mod routes;

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

    // マイグレーション実行
    //
    // ⚠ このDBはDjangoの実スキーマ(accounts_user, tickets_ticket等)を共有している。
    // rust/migrations/ 配下のマイグレーションはDjangoスキーマとは無関係な旧プロトタイプ用
    // テーブル(m_users等)を作るものなので、明示的にRUST_RUN_MIGRATIONS=trueを指定した
    // 場合のみ実行する(デフォルトはスキップ)。
    if std::env::var("RUST_RUN_MIGRATIONS").as_deref() == Ok("true") {
        tracing::warn!("RUST_RUN_MIGRATIONS=true — マイグレーションを実行します");
        sqlx::migrate!().run(&pool).await?;
    } else {
        tracing::info!("マイグレーションをスキップ(RUST_RUN_MIGRATIONS=trueで有効化)");
    }

    // メール送信者
    let mail_sender = MailSender::new(&config);

    // 共有ステート構築
    let state = AppState {
        pool,
        config,
        mail_sender: Some(mail_sender),
    };

    // ルーター構築
    let app = routes::create_router(state);

    // サーバー起動
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("🚀 WIP server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
