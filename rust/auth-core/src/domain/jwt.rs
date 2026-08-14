//! domain/jwt.rs — JWT発行・検証
//!
//! ## クレーム形状の方針（2026-08-14決定）
//! 方針ドキュメント7章は `Claims { sub, roles, exp, iss, extra }` という
//! アプリ非依存の汎用クレーム形状を最終形として定義している。一方、WIPの現行実装
//! （移植元の `jwt_service.rs`）は Django（`rest_framework_simplejwt`）と
//! 同一の `SECRET_KEY` で相互検証できるよう、クレーム形状を
//! `{token_type, exp, iat, jti, user_id}` に固定し、`user_id` を文字列として
//! やり取りするなど、Djangoの実装に合わせた独自ルールを持っている。
//!
//! この2つは互換性がないため、汎用エンジン（`encode_claims` / `decode_claims`、
//! 任意のクレーム型を受け付ける）と、WIPが現在必要としているDjango互換クレーム
//! （`django_compat` サブモジュール、既存コードそのまま）を両方実装している。
//!
//! **`django_compat` は恒久仕様ではない。** Djangoは段階的に廃止する方針が
//! 決定しており（方針ドキュメント1.4節）、`django_compat` はDjangoが稼働している
//! 間だけ必要な移行期間限定のブリッジという位置付けである。Django完全廃止のタイミングで、
//! 新規発行トークンを汎用`Claims` + `TokenPolicy`ベースの実装に一本化する
//! （切替時に未失効の既存リフレッシュトークンをどう扱うかは1.4節を参照）。

use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::AuthError;

fn encoding_key(secret: &str) -> EncodingKey {
    EncodingKey::from_secret(secret.as_bytes())
}

fn decoding_key(secret: &str) -> DecodingKey {
    DecodingKey::from_secret(secret.as_bytes())
}

/// 任意のクレーム型をHS256で署名してエンコードする。
pub fn encode_claims<C: Serialize>(claims: &C, secret: &str) -> Result<String, AuthError> {
    encode(&Header::default(), claims, &encoding_key(secret))
        .map_err(|e| AuthError::InvalidToken(e.to_string()))
}

/// 任意のクレーム型としてデコードする。署名・`exp` の検証は
/// `jsonwebtoken::Validation::default()` に従う（`exp` 必須）。
pub fn decode_claims<C: DeserializeOwned>(token: &str, secret: &str) -> Result<C, AuthError> {
    decode::<C>(token, &decoding_key(secret), &Validation::default())
        .map(|data| data.claims)
        .map_err(|e| match e.kind() {
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::TokenExpired,
            _ => AuthError::InvalidToken(e.to_string()),
        })
}

// ---------------------------------------------------------------------------
// 汎用クレーム / ロール・audience別TTL（方針ドキュメント 1.1, 7章）
// ---------------------------------------------------------------------------

/// アプリ非依存の汎用JWTクレーム。`extra` にアプリ固有クレーム
/// （例: Sophiaの `can_view_all_payroll`）をフラットに埋め込む。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    #[serde(default)]
    pub roles: Vec<String>,
    pub exp: i64,
    pub iat: i64,
    pub iss: String,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// audience/ロール単位のアクセス・リフレッシュトークン寿命。
#[derive(Debug, Clone, Copy)]
pub struct TokenPolicy {
    pub access_ttl: Duration,
    pub refresh_ttl: Duration,
}

impl Default for TokenPolicy {
    /// WIPの現行既定値（アクセス30分 / リフレッシュ7日）
    fn default() -> Self {
        Self {
            access_ttl: Duration::minutes(30),
            refresh_ttl: Duration::days(7),
        }
    }
}

/// audience（もしくはロール名）ごとに `TokenPolicy` を保持するレジストリ。
/// 例: Sophiaの「社員24時間 / パートナー30日」のような役割別ポリシーを
/// 起動時設定（環境変数 or DBマスタ）から注入する。
#[derive(Debug, Clone, Default)]
pub struct TokenPolicyRegistry {
    policies: HashMap<String, TokenPolicy>,
    default_policy: TokenPolicy,
}

impl TokenPolicyRegistry {
    pub fn new(default_policy: TokenPolicy) -> Self {
        Self {
            policies: HashMap::new(),
            default_policy,
        }
    }

    pub fn with_policy(mut self, audience: impl Into<String>, policy: TokenPolicy) -> Self {
        self.policies.insert(audience.into(), policy);
        self
    }

    pub fn policy_for(&self, audience: &str) -> TokenPolicy {
        self.policies
            .get(audience)
            .copied()
            .unwrap_or(self.default_policy)
    }
}

/// `Claims` ベースでアクセストークンを発行する（汎用エンジン）。
pub fn issue_access_claims(
    sub: &str,
    roles: &[String],
    iss: &str,
    policy: TokenPolicy,
    extra: serde_json::Value,
) -> Claims {
    let now = Utc::now();
    Claims {
        sub: sub.to_string(),
        roles: roles.to_vec(),
        iat: now.timestamp(),
        exp: (now + policy.access_ttl).timestamp(),
        iss: iss.to_string(),
        extra,
    }
}

// ---------------------------------------------------------------------------
// トークン無効化（ブラックリスト）— 永続化はアプリ側の責務
// ---------------------------------------------------------------------------

/// リフレッシュトークンのブラックリスト（無効化）を管理するトレイト。
/// auth-coreは特定のDB（sqlx/Postgres等）に依存しないため、
/// 永続化はアプリ側の実装に委譲する（WIPなら既存の `jwt_blacklisted_token`
/// テーブルを使う `jwt_blacklist_repo` がこのトレイトを実装する想定。
/// Step 2でのつなぎ込みを参照）。
#[async_trait::async_trait]
pub trait TokenBlacklist: Send + Sync {
    async fn blacklist(&self, jti: &str, expires_at: DateTime<Utc>) -> Result<(), AuthError>;
    async fn is_blacklisted(&self, jti: &str) -> Result<bool, AuthError>;
}

// ---------------------------------------------------------------------------
// django_compat — WIP現行実装からの移植（rest_framework_simplejwt互換）
// ---------------------------------------------------------------------------

/// WIPが現在Djangoと共有しているJWTクレーム形状。
///
/// `django_compat` という名前の通り、これは恒久仕様ではなく、
/// Django側との相互運用が必要な間の互換レイヤーという位置づけ。
pub mod django_compat {
    use super::*;
    use jsonwebtoken::{decode, encode};
    use uuid::Uuid;

    // config/settings.py の SIMPLE_JWT と一致させる
    const ACCESS_TOKEN_LIFETIME_MINUTES: i64 = 30;
    const REFRESH_TOKEN_LIFETIME_DAYS: i64 = 7;
    // apps/api/views/auth.py の MFA_TOKEN_MAX_AGE と一致させる
    const MFA_TOKEN_LIFETIME_SECONDS: i64 = 300;

    /// djangorestframework-simplejwtの`Token.for_user()`は`user_id = str(user_id)`と
    /// 無条件で文字列化してからクレームに設定するため、user_idクレームは常に
    /// JSON文字列としてエンコードされる。Rust側もこれに合わせて文字列として
    /// 送受信する（数値で来ても許容し、送信時は常に文字列化する）。
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
        /// 発行したrefreshトークンのjtiと有効期限（ログアウト/ローテーション時のブラックリスト登録用）
        pub refresh_jti: String,
        pub refresh_expires_at: chrono::DateTime<Utc>,
    }

    /// access/refreshトークンのペアを新規発行する
    pub fn issue_token_pair(user_id: i32, secret: &str) -> Result<TokenPair, AuthError> {
        let now = Utc::now();

        let access_claims = TokenClaims {
            token_type: "access".to_string(),
            iat: now.timestamp(),
            exp: (now + Duration::minutes(ACCESS_TOKEN_LIFETIME_MINUTES)).timestamp(),
            jti: Uuid::new_v4().to_string(),
            user_id,
        };
        let access = encode(&Header::default(), &access_claims, &encoding_key(secret))
            .map_err(|e| AuthError::InvalidToken(e.to_string()))?;

        let refresh_expires_at = now + Duration::days(REFRESH_TOKEN_LIFETIME_DAYS);
        let refresh_jti = Uuid::new_v4().to_string();
        let refresh_claims = TokenClaims {
            token_type: "refresh".to_string(),
            iat: now.timestamp(),
            exp: refresh_expires_at.timestamp(),
            jti: refresh_jti.clone(),
            user_id,
        };
        let refresh = encode(&Header::default(), &refresh_claims, &encoding_key(secret))
            .map_err(|e| AuthError::InvalidToken(e.to_string()))?;

        Ok(TokenPair {
            access,
            refresh,
            refresh_jti,
            refresh_expires_at,
        })
    }

    /// アクセストークンのみを新規発行する（リフレッシュ時など）
    pub fn issue_access_token(user_id: i32, secret: &str) -> Result<String, AuthError> {
        let now = Utc::now();
        let claims = TokenClaims {
            token_type: "access".to_string(),
            iat: now.timestamp(),
            exp: (now + Duration::minutes(ACCESS_TOKEN_LIFETIME_MINUTES)).timestamp(),
            jti: Uuid::new_v4().to_string(),
            user_id,
        };
        encode(&Header::default(), &claims, &encoding_key(secret))
            .map_err(|e| AuthError::InvalidToken(e.to_string()))
    }

    /// access/refresh共通のトークンをデコードする。署名・有効期限のみ検証し、
    /// token_typeの妥当性チェックは呼び出し側の責務とする。
    pub fn decode_token(token: &str, secret: &str) -> Result<TokenClaims, AuthError> {
        decode::<TokenClaims>(token, &decoding_key(secret), &Validation::default())
            .map(|data| data.claims)
            .map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::TokenExpired,
                _ => AuthError::InvalidToken(e.to_string()),
            })
    }

    /// MFAチャレンジトークンを発行する（5分間有効）
    pub fn issue_mfa_token(user_id: i32, secret: &str) -> Result<String, AuthError> {
        let now = Utc::now();
        let claims = MfaClaims {
            purpose: "mfa".to_string(),
            iat: now.timestamp(),
            exp: (now + Duration::seconds(MFA_TOKEN_LIFETIME_SECONDS)).timestamp(),
            user_id,
        };
        encode(&Header::default(), &claims, &encoding_key(secret))
            .map_err(|e| AuthError::InvalidToken(e.to_string()))
    }

    /// MFAチャレンジトークンをデコードする。purpose不一致はエラーとする。
    pub fn decode_mfa_token(token: &str, secret: &str) -> Result<MfaClaims, AuthError> {
        let data = decode::<MfaClaims>(token, &decoding_key(secret), &Validation::default())
            .map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::TokenExpired,
                _ => AuthError::InvalidToken(e.to_string()),
            })?;
        if data.claims.purpose != "mfa" {
            return Err(AuthError::InvalidToken("invalid token purpose".to_string()));
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct DummyClaims {
        sub: String,
        exp: i64,
    }

    #[test]
    fn generic_roundtrip() {
        let claims = DummyClaims {
            sub: "user-1".to_string(),
            exp: (Utc::now() + Duration::minutes(5)).timestamp(),
        };
        let token = encode_claims(&claims, "secret").unwrap();
        let decoded: DummyClaims = decode_claims(&token, "secret").unwrap();
        assert_eq!(claims, decoded);
    }

    #[test]
    fn generic_expired_token_is_rejected() {
        let claims = DummyClaims {
            sub: "user-1".to_string(),
            exp: (Utc::now() - Duration::minutes(5)).timestamp(),
        };
        let token = encode_claims(&claims, "secret").unwrap();
        let err = decode_claims::<DummyClaims>(&token, "secret").unwrap_err();
        assert!(matches!(err, AuthError::TokenExpired));
    }

    #[test]
    fn token_policy_registry_falls_back_to_default() {
        let registry = TokenPolicyRegistry::new(TokenPolicy::default()).with_policy(
            "partner",
            TokenPolicy {
                access_ttl: Duration::hours(1),
                refresh_ttl: Duration::days(30),
            },
        );

        assert_eq!(
            registry.policy_for("employee").refresh_ttl,
            Duration::days(7)
        );
        assert_eq!(
            registry.policy_for("partner").refresh_ttl,
            Duration::days(30)
        );
    }
}
