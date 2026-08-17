/// domain/services/auth_service.rs — 認証ビジネスロジック
///
/// パスワード検証（Argon2）、TOTP 検証、Django パスワードハッシュ移行。

use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::SaltString;
use sqlx::PgPool;

use crate::infrastructure::repositories::user_repo;

/// パスワードを Argon2 でハッシュ化
///
/// 素のPHC文字列(`$argon2id$...`)をそのまま返す。旧Django運用時代は
/// `Argon2PasswordHasher.encode()`が使う`"argon2"+PHC文字列`という独自
/// フォーマットで保存する必要があったが、Django(web)は2026-08-10に
/// 完全撤去済みのためこのプレフィックスは不要になった。既存の
/// "argon2"プレフィックス付きレコードは`verify_password`が引き続き読み取れる。
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
    // 旧Django運用時代のレコードは"argon2"プレフィックス付き(`argon2$argon2id$...`)で
    // 保存されているため、あれば取り除いてから素のPHC文字列として解釈する
    // (2026-08-14以降の新規ハッシュはプレフィックスなしで保存されるため、
    // その場合はunwrap_or(stored_hash)でそのまま使われる)。
    let phc_str = stored_hash.strip_prefix("argon2").unwrap_or(stored_hash);
    let parsed_hash = PasswordHash::new(phc_str)
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
///
/// DB保存されているsecretはpyotp.random_base32()が生成するBase32エンコード済み
/// 文字列(Djangoの apps/mfa/views.py, apps/api/views/auth.py がpyotp.TOTP(device.secret)
/// と直接渡している値と同じ)。Secret::Rawだと文字列のバイト列をそのまま秘密鍵として
/// 扱ってしまいBase32デコードされず、pyotp側と異なる鍵で検証することになり必ず失敗する
/// ため、Secret::Encodedで正しくBase32デコードする。
pub fn verify_totp(secret: &str, code: &str) -> anyhow::Result<bool> {
    use totp_rs::{Algorithm, TOTP, Secret};

    let secret_bytes = Secret::Encoded(secret.to_string()).to_bytes()
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
#[allow(dead_code)]
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

/// ユーザーのTOTP検証（暗号化秘密鍵対応）
///
/// DBから秘密鍵を取得して、暗号化/平文を判定・復号・検証する共通ロジック。
/// auth_api.rs::login_verify と Web UI のハンドラで同じロジックを使えるようにする。
pub async fn verify_totp_for_user(
    pool: &PgPool,
    jwt_secret: &str,
    user_id: i32,
    code: &str,
) -> anyhow::Result<bool> {
    let device = match user_repo::find_totp(pool, user_id).await? {
        Some(d) if d.confirmed => d,
        _ => return Ok(false),
    };

    // 秘密鍵が暗号化されているか判定（Base64 blob vs Base32）
    let secret_to_verify = if device.secret.len() > 32 && !device.secret.contains('$') {
        // 暗号化された秘密鍵の可能性（Base64 blob、Base32 secretよりも長い）
        match auth_core::domain::totp::decrypt_secret(&device.secret, jwt_secret) {
            Ok(secret_bytes) => {
                // 復号化した raw bytes を Base32 に変換
                auth_core::domain::totp::secret_to_base32(&secret_bytes)
            }
            Err(_) => {
                // 復号失敗 → 平文の Base32 秘密鍵として扱う
                device.secret.clone()
            }
        }
    } else {
        // 平文の Base32 秘密鍵（pyotpやDjango由来）
        device.secret.clone()
    };

    verify_totp(&secret_to_verify, code)
}
