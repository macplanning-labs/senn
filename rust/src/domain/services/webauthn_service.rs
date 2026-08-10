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

// ── 認証（Authentication / ログイン）──
//
// discoverable credential(ユーザー名不要のログイン)方式を採用。
// start_passkey_authentication/finish_passkey_authentication(全ユーザーのPasskeyを
// 事前にstart時にDBから読み込んで渡す方式)は、未認証の第三者に全ユーザーの
// credential_id一覧(allowCredentials)が開示されてしまうため使わない。
// 代わりにwebauthn-rsの"conditional-ui" feature(Cargo.tomlで有効化済み)で
// 提供される start_discoverable_authentication を使う。この名前だが実体は
// 「ブラウザに認証器側で保持しているdiscoverable credentialを選ばせる」ための
// 一般的なAPIであり、フロントエンド側で navigator.credentials.get() に
// mediation: 'conditional' を明示的に指定しない限り、通常のボタン起動フロー
// (パスキー選択ダイアログ)としてそのまま使える
// (RequestChallengeResponse.mediationフィールドはpublicKeyの外側の兄弟フィールド
// であり、フロントは`rcr.publicKey`だけを取り出してnavigator.credentials.get()に
// 渡すため、mediationヒントは単に無視される)。
//
// フロー:
//   1. start_discoverable_authentication() — DBアクセスなし、全ユーザー分の
//      credential_idを列挙せずに済む
//   2. クライアントから返ってきた PublicKeyCredential に対し
//      identify_discoverable_authentication() で、検証前に(user_unique_id: Uuid,
//      credential_id) を取り出す。このUuidは登録時に
//      Uuid::from_u128(user_id as u128) で組み立てたものなので、
//      .as_u128() as i32 でWIPのuser_idに戻せる
//   3. そのuser_idのPasskeyだけをDBから取得し、DiscoverableKeyへ変換
//   4. finish_discoverable_authentication() で検証

/// パスキーログインを開始する(discoverable、ユーザー名不要、DBアクセスなし)。
pub fn start_authentication(
    webauthn: &Webauthn,
) -> anyhow::Result<(RequestChallengeResponse, DiscoverableAuthentication)> {
    webauthn.start_discoverable_authentication()
        .map_err(|e| anyhow::anyhow!("パスキー認証開始に失敗: {}", e))
}

/// クライアントから返ってきた PublicKeyCredential から、検証前に
/// (ユーザーのUuid, credential_id) を取り出す。
/// このUuidから対象ユーザーを特定し、そのユーザーのPasskeyだけをDBから読み込んで
/// finish_authentication に渡すこと(全ユーザー分を読み込まない)。
pub fn identify_authentication(
    webauthn: &Webauthn,
    credential: &PublicKeyCredential,
) -> anyhow::Result<(Uuid, Vec<u8>)> {
    webauthn.identify_discoverable_authentication(credential)
        .map(|(uuid, cred_id)| (uuid, cred_id.to_vec()))
        .map_err(|e| anyhow::anyhow!("パスキーの識別に失敗: {}", e))
}

/// パスキーログインを完了する。
/// `creds` は identify_authentication で特定した対象ユーザーの Passkey のみを渡すこと。
pub fn finish_authentication(
    webauthn: &Webauthn,
    auth_state: DiscoverableAuthentication,
    credential: &PublicKeyCredential,
    creds: &[Passkey],
) -> anyhow::Result<AuthenticationResult> {
    let discoverable_keys: Vec<DiscoverableKey> = creds.iter().map(DiscoverableKey::from).collect();
    webauthn.finish_discoverable_authentication(credential, auth_state, &discoverable_keys)
        .map_err(|e| anyhow::anyhow!("パスキー認証完了に失敗: {}", e))
}

/// DiscoverableAuthentication を JSON 文字列にシリアライズする（クライアント返却用、echo-back pattern）
pub fn authentication_state_to_json_string(state: &DiscoverableAuthentication) -> anyhow::Result<String> {
    serde_json::to_string(state)
        .map_err(|e| anyhow::anyhow!("DiscoverableAuthentication JSON シリアライズに失敗: {}", e))
}

/// JSON 文字列から DiscoverableAuthentication を復元する（クライアント echo-back用）
pub fn authentication_state_from_json_string(json_str: &str) -> anyhow::Result<DiscoverableAuthentication> {
    serde_json::from_str(json_str)
        .map_err(|e| anyhow::anyhow!("DiscoverableAuthentication JSON デシリアライズに失敗: {}", e))
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
