/// presentation/middleware/rate_limiter.rs — レート制限設定
///
/// tower_governor(governorクレートのtower/axum向けラッパー)を使い、
/// クライアントIP単位(SmartIpKeyExtractor: X-Forwarded-For等のヘッダーを
/// 優先し、無ければpeer IPにフォールバック)でトークンバケット方式の
/// レート制限を行う。nginx側で全ての対象エンドポイントに
/// X-Forwarded-Forを設定済み(docker/nginx.conf)であることが前提。
///
/// 重要: key_extractorを明示的にSmartIpKeyExtractorにしない場合、
/// デフォルトのPeerIpKeyExtractorはリバースプロキシ(nginx)のコンテナIPしか
/// 見られず、実質「全クライアント共有のグローバル制限」になってしまう
/// (＝正規ユーザーが同時に数人ログインしただけで全員がロックアウト
/// されうる)。必ずSmartIpKeyExtractorを使うこと。

use axum::response::{IntoResponse, Response};
use governor::middleware::NoOpMiddleware;
use serde_json::json;
use std::time::Duration;
use tower_governor::{
    errors::GovernorError,
    governor::{GovernorConfig, GovernorConfigBuilder},
    key_extractor::SmartIpKeyExtractor,
};

pub type WipGovernorConfig = GovernorConfig<SmartIpKeyExtractor, NoOpMiddleware>;

/// 429応答をWIPの既存エラー規約({"detail": "..."})に合わせて整形する。
pub fn error_response(error: GovernorError) -> Response {
    let (status, detail) = match error {
        GovernorError::TooManyRequests { wait_time, .. } => (
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            format!("リクエストが多すぎます。{}秒後に再試行してください。", wait_time),
        ),
        GovernorError::UnableToExtractKey => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "レート制限の判定に失敗しました".to_string(),
        ),
        GovernorError::Other { code, msg, .. } => (
            code,
            msg.unwrap_or_else(|| "リクエストを処理できませんでした".to_string()),
        ),
    };
    let body = json!({ "detail": detail });
    (status, axum::Json(body)).into_response()
}

fn build_config(period: Duration, burst_size: u32) -> WipGovernorConfig {
    // key_extractor()はKの型そのものを差し替えるため&mut Selfではなく
    // 新しいowned builderを返す。先にkey_extractorを確定させてから、
    // period/burst_sizeを続けてチェインする。
    let mut builder = GovernorConfigBuilder::default().key_extractor(SmartIpKeyExtractor);
    builder.period(period).burst_size(burst_size);
    builder
        .finish()
        .expect("rate limiter config: burst_size/periodが不正です")
}

/// ログイン: IPごとに20回/分(バースト20、3秒に1回補充)。
/// 通常のログイン(MFA再試行込み)はこの範囲に収まり、
/// 総当たり攻撃は素早く弾かれる想定。
pub fn login_config() -> WipGovernorConfig {
    build_config(Duration::from_secs(3), 20)
}

/// GitHub Webhook: IPごとに60回/分(バースト60、1秒に1回補充)。
/// 署名検証は別途あるが、受付レベルでの多重防御として設定。
pub fn webhook_config() -> WipGovernorConfig {
    build_config(Duration::from_secs(1), 60)
}

/// 外部API(external/ai-agent): IPごとに300回/分(バースト300、200msに1回補充)。
/// B2B/AIエージェント連携でのある程度まとまったリクエストも許容しつつ、
/// 暴走・乱用は防ぐ想定。
pub fn external_api_config() -> WipGovernorConfig {
    build_config(Duration::from_millis(200), 300)
}
