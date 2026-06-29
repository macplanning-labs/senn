/// domain/services/auth_service.rs — 認証ビジネスロジック
///
/// パスワード検証（Argon2）、TOTP 検証、Django パスワードハッシュ移行。

use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::SaltString;
use sqlx::PgPool;

use crate::infrastructure::repositories::user_repo;

/// パスワードを Argon2 でハッシュ化
pub fn hash_password(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("パスワードハッシュエラー: {}", e))?;
    Ok(hash.to_string())
}

/// パスワード検証
///
/// Argon2 ハッシュとの照合を行う。
/// Django の PBKDF2 ハッシュの場合は移行処理を行う。
pub async fn verify_password(
    pool: &PgPool,
    user_id: i32,
    password: &str,
    stored_hash: &str,
) -> anyhow::Result<bool> {
    // Django PBKDF2 ハッシュの場合（pbkdf2_sha256$ で始まる）
    if stored_hash.starts_with("pbkdf2_sha256$") {
        let valid = verify_django_pbkdf2(password, stored_hash)?;
        if valid {
            // Argon2 に再ハッシュして更新
            let new_hash = hash_password(password)?;
            user_repo::update_password(pool, user_id, &new_hash).await?;
            tracing::info!("🔄 ユーザー {} のパスワードを Argon2 に移行しました", user_id);
        }
        return Ok(valid);
    }

    // Argon2 ハッシュ検証
    let parsed_hash = PasswordHash::new(stored_hash)
        .map_err(|e| anyhow::anyhow!("ハッシュ解析エラー: {}", e))?;
    let result = Argon2::default().verify_password(password.as_bytes(), &parsed_hash);
    Ok(result.is_ok())
}

/// Django PBKDF2-SHA256 ハッシュ検証
///
/// フォーマット: pbkdf2_sha256$<iterations>$<salt>$<hash>
fn verify_django_pbkdf2(password: &str, stored: &str) -> anyhow::Result<bool> {
    let parts: Vec<&str> = stored.split('$').collect();
    if parts.len() != 4 || parts[0] != "pbkdf2_sha256" {
        return Ok(false);
    }

    let iterations: u32 = parts[1].parse()
        .map_err(|_| anyhow::anyhow!("PBKDF2 iterations parse error"))?;
    let salt = parts[2];
    let expected_hash = parts[3];

    // PBKDF2-HMAC-SHA256
    use hmac::Hmac;
    use sha2::Sha256;

    let mut derived_key = vec![0u8; 32];
    pbkdf2::pbkdf2::<Hmac<Sha256>>(
        password.as_bytes(),
        salt.as_bytes(),
        iterations,
        &mut derived_key,
    )
    .map_err(|e| anyhow::anyhow!("PBKDF2 error: {}", e))?;

    let computed_hash = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        &derived_key,
    );

    Ok(computed_hash == expected_hash)
}

/// TOTP コード検証
pub fn verify_totp(secret: &str, code: &str) -> anyhow::Result<bool> {
    use totp_rs::{Algorithm, TOTP, Secret};

    let secret_bytes = Secret::Raw(secret.as_bytes().to_vec()).to_bytes()
        .map_err(|e| anyhow::anyhow!("TOTP secret error: {}", e))?;

    let totp = TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret_bytes,
        Some("WIP".to_string()),
        String::new(),
    ).map_err(|e| anyhow::anyhow!("TOTP error: {}", e))?;

    Ok(totp.check_current(code).unwrap_or(false))
}

/// TOTP セットアップ（QR コード URL 生成）
pub fn generate_totp_setup(username: &str) -> anyhow::Result<(String, String)> {
    use totp_rs::{Algorithm, TOTP, Secret};

    // ランダムシークレット生成
    let mut secret_bytes = vec![0u8; 20];
    use rand::RngCore;
    rand::thread_rng().fill_bytes(&mut secret_bytes);

    let secret = Secret::Raw(secret_bytes.clone());
    let secret_base32 = secret.to_encoded().to_string();

    let totp = TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret_bytes,
        Some("WIP".to_string()),
        username.to_string(),
    ).map_err(|e| anyhow::anyhow!("TOTP error: {}", e))?;

    let uri = totp.get_url();

    Ok((secret_base32, uri))
}
