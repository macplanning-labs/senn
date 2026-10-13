mod config;
mod domain;
mod infrastructure;
mod presentation;
mod routes;
#[cfg(test)]
mod test_support;

use crate::infrastructure::mail::MailSender;
use crate::presentation::state::AppState;
use std::net::SocketAddr;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// ビルド時に実行ファイルへ焼き込んだ、コミットの SHA(Dockerfile の `ARG GIT_SHA` → `ENV SENN_BUILD_SHA`)。
/// 無ければ "unknown"(手元の `cargo build` など)。反映のスクリプトが `--build-sha` で読み、反映したコミットと
/// 一致するかを確かめる(古い実行ファイルが使い回される事故を防ぐ。DEMO-000143)。
const BUILD_SHA: &str = match option_env!("SENN_BUILD_SHA") {
    Some(sha) => sha,
    None => "unknown",
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // `senn --build-sha`: サーバーを起動せず、焼き込んだ SHA だけを表示して終わる(DB・設定は読まない)
    if std::env::args().nth(1).as_deref() == Some("--build-sha") {
        println!("{BUILD_SHA}");
        return Ok(());
    }

    // `senn --reset-passkeys <ユーザー名>`: サーバーを起動せず、そのユーザーのパスキーをすべて削除して終わる
    // (パスキーを失くした管理者の復旧用。サーバー上で、DB につながる環境(root など)から実行する。DEMO-000153 A-4)
    if std::env::args().nth(1).as_deref() == Some("--reset-passkeys") {
        return reset_passkeys_cli(std::env::args().nth(2)).await;
    }

    // ログ初期化
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
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
    infrastructure::scheduler::spawn_mfa_attempt_purge(pool.clone());

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

    // メール送信者（送信時に system_settings を参照）
    let mail_sender = MailSender::new(pool.clone(), config.clone());

    // 共有ステート構築
    let state = AppState::new(pool, config, Some(mail_sender)).await?;

    // 設定のモデルが Ollama に無ければ、警告を 1 回ログに出す(起動を待たせない。DEMO-000146)
    {
        let state = state.clone();
        tokio::spawn(async move {
            let cfg = state.ai_config().await;
            domain::services::ai_service::warn_if_ollama_model_missing(&cfg).await;
        });
    }

    // 期限到来/超過リマインダースケジューラを起動
    infrastructure::scheduler::spawn_due_date_reminders(
        state.pool.clone(),
        state.mail_sender.clone(),
    );

    // AI実行履歴見張りジョブを起動
    infrastructure::scheduler::spawn_ai_run_watchdog(
        state.pool.clone(),
        state.config.wip_ai_api_user.clone(),
    );

    // AI MCP 申請の再通知スケジューラを起動
    infrastructure::scheduler::spawn_ai_mcp_request_reminder(
        state.pool.clone(),
        state.mail_sender.clone(),
        state.config.base_url.clone(),
    );

    // リアルタイム同期の配信役（DB の変更通知 → 部屋ごとの差分パケット）
    if state.realtime_enabled {
        infrastructure::realtime::dispatcher::spawn(
            state.pool.clone(),
            state.realtime.clone(),
            state.config.wip_ai_api_user.clone(),
        );
    } else {
        tracing::warn!("REALTIME_ENABLED=false — リアルタイム同期は無効です");
    }

    // ルーター構築
    let app = routes::create_router(state);

    // サーバー起動
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!(
        "🚀 SENN server listening on {} (build_sha={})",
        addr,
        BUILD_SHA
    );

    let listener = tokio::net::TcpListener::bind(addr).await?;
    // レート制限の送り元IPの判定(auth_core::infrastructure::client_ip)は、
    // 接続元(peer)が信用する中継かどうかを見るため、ConnectInfoが必須。
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}

/// `--reset-passkeys` の本体。実行した事実を、標準出力とログに残す。
async fn reset_passkeys_cli(username: Option<String>) -> anyhow::Result<()> {
    let Some(username) = username.filter(|u| !u.is_empty()) else {
        anyhow::bail!("使い方: senn --reset-passkeys <ユーザー名>");
    };
    dotenvy::dotenv().ok();
    let pool = infrastructure::db::create_pool().await?;
    let Some(user) =
        infrastructure::repositories::user_repo::find_by_username(&pool, &username).await?
    else {
        anyhow::bail!("ユーザーが見つかりません: {username}");
    };
    let deleted =
        infrastructure::repositories::user_repo::delete_all_passkeys(&pool, user.id).await?;
    infrastructure::repositories::user_repo::open_passkey_enroll_window(
        &pool,
        user.id,
        presentation::handlers::system_admin_api::PASSKEY_ENROLL_WINDOW_HOURS,
    )
    .await?;
    println!("パスキーを {deleted} 件削除しました(ユーザー: {username})。本人は、パスワードでログインして、パスキーを登録し直してください。");
    eprintln!(
        "event=admin_passkey_reset_cli target_user_id={} deleted_count={deleted}",
        user.id
    );
    Ok(())
}

#[cfg(test)]
mod build_sha_tests {
    use super::BUILD_SHA;

    #[test]
    fn build_sha_is_never_empty() {
        // 焼き込みが無い手元のビルドでも "unknown" になり、空にならない(スクリプトが「報告なし」と区別できる)
        assert!(!BUILD_SHA.trim().is_empty());
        assert!(!BUILD_SHA.contains(char::is_whitespace));
    }
}
