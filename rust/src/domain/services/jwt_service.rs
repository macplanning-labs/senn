/// domain/services/jwt_service.rs — JWT発行・検証
///
/// DjangoのSIMPLE_JWT(rest_framework_simplejwt)と互換のクレーム形状で
/// access/refreshトークンを発行・検証する。同じSECRET_KEY(HS256)で署名するため、
/// Rustが発行したトークンをDjangoのJWTAuthenticationがそのまま検証でき、
/// 逆にDjangoが発行したトークンもRustのjwt_authミドルウェアで検証できる。
///
/// クレーム形状はrest_framework_simplejwtのToken.__init__()が設定するものと同一:
/// token_type, exp, iat, jti, user_id (USER_ID_CLAIM デフォルト値)。

use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// config/settings.py の SIMPLE_JWT と一致させる
const ACCESS_TOKEN_LIFETIME_MINUTES: i64 = 30;
const REFRESH_TOKEN_LIFETIME_DAYS: i64 = 7;
// apps/api/views/auth.py の MFA_TOKEN_MAX_AGE と一致させる
const MFA_TOKEN_LIFETIME_SECONDS: i64 = 300;

/// djangorestframework-simplejwtの`Token.for_user()`は`user_id = str(user_id)`と
/// 無条件で文字列化してからクレームに設定するため(バージョン確認済み)、
/// user_idクレームは常にJSON文字列としてエンコードされる。Rust側もこれに合わせて
/// 文字列として送受信する(数値で来ても許容し、送信時は常に文字列化する)。
mod user_id_as_string {
    use serde::{de::Error as _, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(value: &i32, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(value)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<i32, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum StringOrInt {
            String(String),
            Int(i32),
        }
        match StringOrInt::deserialize(deserializer)? {
            StringOrInt::String(s) => s.parse::<i32>().map_err(D::Error::custom),
            StringOrInt::Int(i) => Ok(i),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenClaims {
    pub token_type: String, // "access" | "refresh"
    pub exp: i64,
    pub iat: i64,
    pub jti: String,
    #[serde(with = "user_id_as_string")]
    pub user_id: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MfaClaims {
    pub purpose: String, // "mfa" 固定
    pub exp: i64,
    pub iat: i64,
    pub user_id: i32,
}

pub struct TokenPair {
    pub access: String,
    pub refresh: String,
    /// 発行したrefreshトークンのjtiと有効期限(ログアウト/ローテーション時のブラックリスト登録用)
    pub refresh_jti: String,
    pub refresh_expires_at: chrono::DateTime<Utc>,
}

fn encoding_key(secret: &str) -> EncodingKey {
    EncodingKey::from_secret(secret.as_bytes())
}

fn decoding_key(secret: &str) -> DecodingKey {
    DecodingKey::from_secret(secret.as_bytes())
}

/// access/refreshトークンのペアを新規発行する
pub fn issue_token_pair(user_id: i32, secret: &str) -> anyhow::Result<TokenPair> {
    let now = Utc::now();

    let access_claims = TokenClaims {
        token_type: "access".to_string(),
        iat: now.timestamp(),
        exp: (now + Duration::minutes(ACCESS_TOKEN_LIFETIME_MINUTES)).timestamp(),
        jti: Uuid::new_v4().to_string(),
        user_id,
    };
    let access = encode(&Header::default(), &access_claims, &encoding_key(secret))?;

    let refresh_expires_at = now + Duration::days(REFRESH_TOKEN_LIFETIME_DAYS);
    let refresh_jti = Uuid::new_v4().to_string();
    let refresh_claims = TokenClaims {
        token_type: "refresh".to_string(),
        iat: now.timestamp(),
        exp: refresh_expires_at.timestamp(),
        jti: refresh_jti.clone(),
        user_id,
    };
    let refresh = encode(&Header::default(), &refresh_claims, &encoding_key(secret))?;

    Ok(TokenPair { access, refresh, refresh_jti, refresh_expires_at })
}

/// アクセストークンのみを新規発行する(リフレッシュ時、refreshはローテーションしない場合など)
pub fn issue_access_token(user_id: i32, secret: &str) -> anyhow::Result<String> {
    let now = Utc::now();
    let claims = TokenClaims {
        token_type: "access".to_string(),
        iat: now.timestamp(),
        exp: (now + Duration::minutes(ACCESS_TOKEN_LIFETIME_MINUTES)).timestamp(),
        jti: Uuid::new_v4().to_string(),
        user_id,
    };
    Ok(encode(&Header::default(), &claims, &encoding_key(secret))?)
}

/// access/refresh共通のトークンをデコードする。署名・有効期限のみ検証し、
/// token_typeの妥当性チェックは呼び出し側の責務とする。
pub fn decode_token(token: &str, secret: &str) -> anyhow::Result<TokenClaims> {
    let data = decode::<TokenClaims>(token, &decoding_key(secret), &Validation::default())?;
    Ok(data.claims)
}

/// MFAチャレンジトークンを発行する(Djangoのdjango.core.signing相当。5分間有効)
pub fn issue_mfa_token(user_id: i32, secret: &str) -> anyhow::Result<String> {
    let now = Utc::now();
    let claims = MfaClaims {
        purpose: "mfa".to_string(),
        iat: now.timestamp(),
        exp: (now + Duration::seconds(MFA_TOKEN_LIFETIME_SECONDS)).timestamp(),
        user_id,
    };
    Ok(encode(&Header::default(), &claims, &encoding_key(secret))?)
}

/// MFAチャレンジトークンをデコードする。purpose不一致はエラーとする。
pub fn decode_mfa_token(token: &str, secret: &str) -> anyhow::Result<MfaClaims> {
    let data = decode::<MfaClaims>(token, &decoding_key(secret), &Validation::default())?;
    if data.claims.purpose != "mfa" {
        anyhow::bail!("invalid token purpose");
    }
    Ok(data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_and_refresh_roundtrip() {
        let secret = "test-secret";
        let pair = issue_token_pair(42, secret).unwrap();

        let access = decode_token(&pair.access, secret).unwrap();
        assert_eq!(access.token_type, "access");
        assert_eq!(access.user_id, 42);

        let refresh = decode_token(&pair.refresh, secret).unwrap();
        assert_eq!(refresh.token_type, "refresh");
        assert_eq!(refresh.jti, pair.refresh_jti);
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
        assert_eq!(claims.user_id, 7);
        assert_eq!(claims.purpose, "mfa");

        // access/refreshトークンをMFAトークンとしてデコードしようとすると
        // purposeフィールドが無くデシリアライズに失敗する
        let pair = issue_token_pair(7, secret).unwrap();
        assert!(decode_mfa_token(&pair.access, secret).is_err());
    }
}
