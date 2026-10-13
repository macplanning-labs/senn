/// domain/services/jwt_service.rs — WIP専用JWTクレーム発行・検証
///
/// Step2④（django_compat要否の再確認）の結果、DjangoのJWT検証コードは
/// 2026-08-10に完全撤去済み（Djangoはどの環境にもデプロイされていない）
/// であることが判明し、Djangoとの相互運用を前提とした
/// `auth_core::domain::jwt::django_compat` への依存を終了することとした。
/// 本モジュールはauth-coreの汎用エンジン（encode_claims/decode_claims）を使い、
/// WIP専用の統一クレーム形状（access/refresh/MFAチャレンジをtoken_typeで
/// 判別する単一のClaims構造体）でトークンを発行・検証する。
use auth_core::domain::jwt::{decode_claims, encode_claims};
use auth_core::AuthError;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const ISSUER: &str = "wip";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenType {
    Access,
    Refresh,
    Mfa,
    /// /media(添付ファイル)の配信専用。API の認証には使えない(Access ではない)。
    Media,
    /// 管理者のパスワードログインの次の段階「パスキーの確認」専用(DEMO-000153 A-4b)。API の認証には使えない
    Passkey,
    /// 管理者の「パスキーの新規登録」専用(DEMO-000153 A-4b)。API の認証には使えない
    Enroll,
}

/// WIP統一JWTクレーム。access/refresh/MFAチャレンジの全トークン種別を
/// `token_type` で判別する単一構造体（デコード・ブラックリスト検証ロジックを
/// 一元化するため、旧django_compatのTokenClaims/MfaClaimsを統合した）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub token_type: TokenType,
    pub exp: i64,
    pub iat: i64,
    pub iss: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub jti: Option<String>,
}

impl Claims {
    /// `sub` をユーザーIDとしてパースする。発行時に必ず`user_id.to_string()`を
    /// 入れているため、署名検証済みトークンでは基本的に失敗しない。
    pub fn user_id(&self) -> Result<i32, AuthError> {
        self.sub
            .parse()
            .map_err(|_| AuthError::InvalidToken("invalid sub claim".to_string()))
    }
}

#[allow(dead_code)]
pub struct TokenPair {
    pub access: String,
    pub refresh: String,
    pub refresh_jti: String,
    pub refresh_expires_at: DateTime<Utc>,
}

fn build_claims(user_id: i32, token_type: TokenType, ttl: Duration, jti: Option<String>) -> Claims {
    let now = Utc::now();
    Claims {
        sub: user_id.to_string(),
        token_type,
        iat: now.timestamp(),
        exp: (now + ttl).timestamp(),
        iss: ISSUER.to_string(),
        jti,
    }
}

/// access/refreshトークンのペアを新規発行する。
/// 各トークンの有効期限は呼び出し側(AppConfig、env変数で上書き可能)から渡す。
pub fn issue_token_pair(
    user_id: i32,
    secret: &str,
    access_ttl_minutes: i64,
    refresh_ttl_days: i64,
) -> Result<TokenPair, AuthError> {
    let access_claims = build_claims(
        user_id,
        TokenType::Access,
        Duration::minutes(access_ttl_minutes),
        Some(Uuid::new_v4().to_string()),
    );
    let access = encode_claims(&access_claims, secret)?;

    let refresh_jti = Uuid::new_v4().to_string();
    let refresh_claims = build_claims(
        user_id,
        TokenType::Refresh,
        Duration::days(refresh_ttl_days),
        Some(refresh_jti.clone()),
    );
    let refresh_expires_at =
        DateTime::<Utc>::from_timestamp(refresh_claims.exp, 0).unwrap_or_else(Utc::now);
    let refresh = encode_claims(&refresh_claims, secret)?;

    Ok(TokenPair {
        access,
        refresh,
        refresh_jti,
        refresh_expires_at,
    })
}

/// アクセストークンのみを新規発行する（サイレントリフレッシュ用）
pub fn issue_access_token(
    user_id: i32,
    secret: &str,
    access_ttl_minutes: i64,
) -> Result<String, AuthError> {
    let claims = build_claims(
        user_id,
        TokenType::Access,
        Duration::minutes(access_ttl_minutes),
        Some(Uuid::new_v4().to_string()),
    );
    encode_claims(&claims, secret)
}

/// access/refresh共通のトークンをデコードする。token_typeの妥当性チェックは
/// 呼び出し側の責務とする（旧django_compat::decode_tokenと同じ契約）。
pub fn decode_token(token: &str, secret: &str) -> Result<Claims, AuthError> {
    decode_claims::<Claims>(token, secret)
}

/// MFAチャレンジトークンを発行する
pub fn issue_mfa_token(user_id: i32, secret: &str, ttl_seconds: i64) -> Result<String, AuthError> {
    let claims = build_claims(
        user_id,
        TokenType::Mfa,
        Duration::seconds(ttl_seconds),
        None,
    );
    encode_claims(&claims, secret)
}

/// 「パスキーの確認」の段階のトークンを発行する(パスワード確認済みの管理者に渡す)
pub fn issue_passkey_step_token(
    user_id: i32,
    secret: &str,
    ttl_seconds: i64,
) -> Result<String, AuthError> {
    let claims = build_claims(
        user_id,
        TokenType::Passkey,
        Duration::seconds(ttl_seconds),
        None,
    );
    encode_claims(&claims, secret)
}

pub fn decode_passkey_step_token(token: &str, secret: &str) -> Result<Claims, AuthError> {
    let claims = decode_claims::<Claims>(token, secret)?;
    if claims.token_type != TokenType::Passkey {
        return Err(AuthError::InvalidToken("invalid token purpose".to_string()));
    }
    Ok(claims)
}

/// 「パスキーの新規登録」の段階のトークンを発行する(窓が開いている管理者に渡す)
pub fn issue_enroll_token(
    user_id: i32,
    secret: &str,
    ttl_seconds: i64,
) -> Result<String, AuthError> {
    let claims = build_claims(
        user_id,
        TokenType::Enroll,
        Duration::seconds(ttl_seconds),
        None,
    );
    encode_claims(&claims, secret)
}

pub fn decode_enroll_token(token: &str, secret: &str) -> Result<Claims, AuthError> {
    let claims = decode_claims::<Claims>(token, secret)?;
    if claims.token_type != TokenType::Enroll {
        return Err(AuthError::InvalidToken("invalid token purpose".to_string()));
    }
    Ok(claims)
}

/// WebAuthn の「状態」を、署名つきで預けるトークン。
///
/// 既存のパスキーの状態は、クライアントが保持して送り返す作りだが、**特定の鍵を指定する認証の状態には、
/// 公開鍵の一覧が含まれる**。クライアントが作った状態をそのまま信じると、攻撃者が自分の公開鍵を入れた状態で
/// 確認を通れてしまう。そのため、サーバーが署名した状態だけを受け付ける(用途・ユーザー・期限つき)。
#[derive(Debug, Serialize, Deserialize)]
struct WebauthnStateClaims {
    sub: String,
    purpose: String,
    state: String,
    exp: i64,
    iat: i64,
    iss: String,
}

pub fn issue_webauthn_state_token(
    user_id: i32,
    purpose: &str,
    state_json: &str,
    secret: &str,
    ttl_seconds: i64,
) -> Result<String, AuthError> {
    let now = Utc::now();
    let claims = WebauthnStateClaims {
        sub: user_id.to_string(),
        purpose: purpose.to_string(),
        state: state_json.to_string(),
        iat: now.timestamp(),
        exp: (now + Duration::seconds(ttl_seconds)).timestamp(),
        iss: ISSUER.to_string(),
    };
    encode_claims(&claims, secret)
}

/// 署名・期限・用途・ユーザーを確かめて、状態の JSON を返す
pub fn decode_webauthn_state_token(
    token: &str,
    purpose: &str,
    expected_user_id: i32,
    secret: &str,
) -> Result<String, AuthError> {
    let claims = decode_claims::<WebauthnStateClaims>(token, secret)?;
    if claims.purpose != purpose || claims.sub != expected_user_id.to_string() {
        return Err(AuthError::InvalidToken("invalid state token".to_string()));
    }
    Ok(claims.state)
}

/// /media 配信専用トークンを発行する(Cookie に入れる。ユーザーIDだけを持つ)
pub fn issue_media_token(
    user_id: i32,
    secret: &str,
    ttl_minutes: i64,
) -> Result<String, AuthError> {
    let claims = build_claims(
        user_id,
        TokenType::Media,
        Duration::minutes(ttl_minutes),
        None,
    );
    encode_claims(&claims, secret)
}

/// /media 配信専用トークンをデコードする。token_type 不一致(access/refresh など)はエラー。
pub fn decode_media_token(token: &str, secret: &str) -> Result<Claims, AuthError> {
    let claims = decode_claims::<Claims>(token, secret)?;
    if claims.token_type != TokenType::Media {
        return Err(AuthError::InvalidToken("invalid token purpose".to_string()));
    }
    Ok(claims)
}

/// MFAチャレンジトークンをデコードする。token_type不一致はエラーとする。
pub fn decode_mfa_token(token: &str, secret: &str) -> Result<Claims, AuthError> {
    let claims = decode_claims::<Claims>(token, secret)?;
    if claims.token_type != TokenType::Mfa {
        return Err(AuthError::InvalidToken("invalid token purpose".to_string()));
    }
    Ok(claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_and_refresh_roundtrip() {
        let secret = "test-secret";
        let pair = issue_token_pair(42, secret, 30, 7).unwrap();

        let access = decode_token(&pair.access, secret).unwrap();
        assert_eq!(access.token_type, TokenType::Access);
        assert_eq!(access.user_id().unwrap(), 42);

        let refresh = decode_token(&pair.refresh, secret).unwrap();
        assert_eq!(refresh.token_type, TokenType::Refresh);
        assert_eq!(refresh.jti, Some(pair.refresh_jti));
        assert_ne!(access.jti, refresh.jti);
    }

    #[test]
    fn wrong_secret_fails() {
        let pair = issue_token_pair(1, "correct-secret", 30, 7).unwrap();
        assert!(decode_token(&pair.access, "wrong-secret").is_err());
    }

    #[test]
    fn mfa_token_roundtrip() {
        let secret = "test-secret";
        let token = issue_mfa_token(7, secret, 300).unwrap();
        let claims = decode_mfa_token(&token, secret).unwrap();
        assert_eq!(claims.token_type, TokenType::Mfa);
        assert_eq!(claims.user_id().unwrap(), 7);
        assert_eq!(claims.jti, None);
    }

    #[test]
    fn mfa_token_rejected_by_decode_token_type_check() {
        // decode_mfa_tokenはpurpose(token_type)不一致を拒否する
        let secret = "test-secret";
        let pair = issue_token_pair(1, secret, 30, 7).unwrap();
        assert!(decode_mfa_token(&pair.access, secret).is_err());
    }

    #[test]
    fn media_token_is_not_an_access_token_and_vice_versa() {
        let secret = "test-secret";
        let media = issue_media_token(5, secret, 60).unwrap();
        assert_eq!(
            decode_media_token(&media, secret)
                .unwrap()
                .user_id()
                .unwrap(),
            5
        );
        // Media は Access として通らない
        assert_ne!(
            decode_token(&media, secret).unwrap().token_type,
            TokenType::Access
        );
        // Access / Refresh / MFA は Media として通らない
        let pair = issue_token_pair(5, secret, 30, 7).unwrap();
        assert!(decode_media_token(&pair.access, secret).is_err());
        assert!(decode_media_token(&pair.refresh, secret).is_err());
        assert!(decode_media_token(&issue_mfa_token(5, secret, 300).unwrap(), secret).is_err());
        // 別の鍵では通らない
        assert!(decode_media_token(&media, "other-secret").is_err());
    }
}
