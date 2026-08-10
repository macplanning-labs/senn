/// domain/services/webauthn_service.rs — WebAuthn（パスキー）サービス
///
/// webauthn-rs を使用したパスキーの登録・認証。
/// Sophia と異なり、WIP は stateless (JWT のみ) なため、
/// 登録のセッション状態（PasskeyRegistration）はクライアントに返して
/// echoes back させる（TOTP方式を踏襲）。

use axum::http::HeaderMap;
use url::Url;
use webauthn_rs::prelude::*;
use webauthn_rs::WebauthnBuilder;

/// WebAuthn設定を初期化して Webauthn インスタンスを返す
///
/// # Arguments
/// - `rp_id` — Relying Party ID（ドメイン名、例: "localhost", "wip.example.com"）
/// - `rp_origin` — Relying Party Origin（例: "http://localhost:3000"）
pub fn create_webauthn(rp_id: &str, rp_origin: &str) -> anyhow::Result<Webauthn> {
    let origin = Url::parse(rp_origin)
        .map_err(|e| anyhow::anyhow!("無効な RP Origin '{}': {}", rp_origin, e))?;

    let builder = WebauthnBuilder::new(rp_id, &origin)
        .map_err(|e| anyhow::anyhow!("WebAuthn設定エラー: {}", e))?;

    let webauthn = builder
        .rp_name("WIP — プロジェクト管理ツール")
        .build()
        .map_err(|e| anyhow::anyhow!("WebAuthn構築エラー: {}", e))?;

    Ok(webauthn)
}

/// リクエストヘッダーから RP ID / RP Origin をその場で解決し、Webauthn インスタンスを
/// 都度構築する（Sophia の `create_webauthn_from_headers` を踏襲）。
///
/// 固定の WEBAUTHN_RP_ID/RP_ORIGIN 環境変数に頼ると、環境ごとにズレやすい。
/// アクセスされた Host ヘッダーから その場で組み立てることで、環境ごとの設定ミスを無くす。
pub fn create_webauthn_from_headers(headers: &HeaderMap) -> anyhow::Result<Webauthn> {
    let host = headers
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .unwrap_or("localhost");
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .unwrap_or("http");

    let rp_id = host.split(':').next().unwrap_or(host);
    let rp_origin = format!("{scheme}://{host}");

    create_webauthn(rp_id, &rp_origin)
}

// ── 登録（Registration）──

/// パスキー登録を開始する
///
/// フロントエンドに CreationChallengeResponse（JSON）を返し、
/// PasskeyRegistration はクライアントに返す（stateless/echo-back pattern）。
pub fn start_registration(
    webauthn: &Webauthn,
    user_id: i32,
    username: &str,
    display_name: &str,
    existing_credentials: Option<Vec<Passkey>>,
) -> anyhow::Result<(CreationChallengeResponse, PasskeyRegistration)> {
    let exclude = existing_credentials
        .as_ref()
        .map(|creds| creds.iter().map(|c| c.cred_id().clone()).collect::<Vec<_>>());

    webauthn.start_passkey_registration(
        Uuid::from_u128(user_id as u128),
        username,
        display_name,
        exclude,
    )
    .map_err(|e| anyhow::anyhow!("パスキー登録開始に失敗: {}", e))
}

/// パスキー登録を完了する
///
/// ブラウザから返却された RegisterPublicKeyCredential を検証し、
/// 成功すれば Passkey を返す（DB に保存する）。
pub fn finish_registration(
    webauthn: &Webauthn,
    reg_state: &PasskeyRegistration,
    credential: &RegisterPublicKeyCredential,
) -> anyhow::Result<Passkey> {
    webauthn.finish_passkey_registration(credential, reg_state)
        .map_err(|e| anyhow::anyhow!("パスキー登録完了に失敗: {}", e))
}

// ── ユーティリティ ──

/// Passkey の credential_id をバイナリの状態で返す（DB保存用）
pub fn credential_id_from_passkey(passkey: &Passkey) -> Vec<u8> {
    passkey.cred_id().to_vec()
}

// NOTE: Passkey の public_key は webauthn-rs の Passkey 型から直接アクセスできません。
// 代わりに完全な Passkey JSON を DB に保存します。

/// Passkey 型を JSON 文字列にシリアライズする（クライアント返却用）
pub fn passkey_to_json_string(passkey: &Passkey) -> anyhow::Result<String> {
    serde_json::to_string(passkey)
        .map_err(|e| anyhow::anyhow!("Passkey JSON シリアライズに失敗: {}", e))
}

/// JSON 文字列から Passkey 型を復元する（DB読み込み用）
pub fn passkey_from_json_string(json_str: &str) -> anyhow::Result<Passkey> {
    serde_json::from_str(json_str)
        .map_err(|e| anyhow::anyhow!("Passkey JSON デシリアライズに失敗: {}", e))
}

/// PasskeyRegistration を JSON 文字列にシリアライズする（クライアント返却用）
pub fn registration_state_to_json_string(state: &PasskeyRegistration) -> anyhow::Result<String> {
    serde_json::to_string(state)
        .map_err(|e| anyhow::anyhow!("PasskeyRegistration JSON シリアライズに失敗: {}", e))
}

/// JSON 文字列から PasskeyRegistration を復元する（クライアント echo-back用）
pub fn registration_state_from_json_string(json_str: &str) -> anyhow::Result<PasskeyRegistration> {
    serde_json::from_str(json_str)
        .map_err(|e| anyhow::anyhow!("PasskeyRegistration JSON デシリアライズに失敗: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_webauthn_localhost() {
        let result = create_webauthn("localhost", "http://localhost:3000");
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_webauthn_invalid_origin() {
        let result = create_webauthn("example.com", "not-a-url");
        assert!(result.is_err());
    }
}
