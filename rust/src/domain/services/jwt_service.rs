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
const ACCESS_TOKEN_LIFETIME_MINUTES: i64 = 30;
const REFRESH_TOKEN_LIFETIME_DAYS: i64 = 7;
const MFA_TOKEN_LIFETIME_SECONDS: i64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenType {
    Access,
    Refresh,
    Mfa,
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

/// access/refreshトークンのペアを新規発行する
pub fn issue_token_pair(user_id: i32, secret: &str) -> Result<TokenPair, AuthError> {
    let access_claims = build_claims(
        user_id,
        TokenType::Access,
        Duration::minutes(ACCESS_TOKEN_LIFETIME_MINUTES),
        Some(Uuid::new_v4().to_string()),
    );
    let access = encode_claims(&access_claims, secret)?;

    let refresh_jti = Uuid::new_v4().to_string();
    let refresh_claims = build_claims(
        user_id,
        TokenType::Refresh,
        Duration::days(REFRESH_TOKEN_LIFETIME_DAYS),
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
pub fn issue_access_token(user_id: i32, secret: &str) -> Result<String, AuthError> {
    let claims = build_claims(
        user_id,
        TokenType::Access,
        Duration::minutes(ACCESS_TOKEN_LIFETIME_MINUTES),
        Some(Uuid::new_v4().to_string()),
    );
    encode_claims(&claims, secret)
}

/// access/refresh共通のトークンをデコードする。token_typeの妥当性チェックは
/// 呼び出し側の責務とする（旧django_compat::decode_tokenと同じ契約）。
pub fn decode_token(token: &str, secret: &str) -> Result<Claims, AuthError> {
    decode_claims::<Claims>(token, secret)
}

/// MFAチャレンジトークンを発行する（5分間有効）
pub fn issue_mfa_token(user_id: i32, secret: &str) -> Result<String, AuthError> {
    let claims = build_claims(
        user_id,
        TokenType::Mfa,
        Duration::seconds(MFA_TOKEN_LIFETIME_SECONDS),
        None,
    );
    encode_claims(&claims, secret)
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
        let pair = issue_token_pair(42, secret).unwrap();

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
        let pair = issue_token_pair(1, "correct-secret").unwrap();
        assert!(decode_token(&pair.access, "wrong-secret").is_err());
    }

    #[test]
    fn mfa_token_roundtrip() {
        let secret = "test-secret";
        let token = issue_mfa_token(7, secret).unwrap();
        let claims = decode_mfa_token(&token, secret).unwrap();
        assert_eq!(claims.token_type, TokenType::Mfa);
        assert_eq!(claims.user_id().unwrap(), 7);
        assert_eq!(claims.jti, None);
    }

    #[test]
    fn mfa_token_rejected_by_decode_token_type_check() {
        // decode_mfa_tokenはpurpose(token_type)不一致を拒否する
        let secret = "test-secret";
        let pair = issue_token_pair(1, secret).unwrap();
        assert!(decode_mfa_token(&pair.access, secret).is_err());
    }
}
