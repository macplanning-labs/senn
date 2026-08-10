/// domain/services/totp_service.rs — TOTP エンロールメントサービス
///
/// 秘密鍵の生成・AES-GCM暗号化/復号・QRコード生成・コード検証。
///
/// ## 設計
/// - 秘密鍵はDB保存前に AES-256-GCM で暗号化（config.secret_key から派生）
/// - QRコードは base64 エンコードされた PNG を返す
/// - 検証時は skew=1（前後30秒のコードも許容）

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use aes_gcm::aead::generic_array::GenericArray;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use sha2::{Sha256, Digest};
use totp_rs::{Algorithm, Secret, TOTP};

/// AES-256-GCM 暗号化結果
/// DB保存時のサイズ制限（64文字）のため、ciphertext + nonce を一つのblob として保存する
pub struct EncryptedSecret {
    pub blob_b64: String,  // Base64(ciphertext + nonce)
}

/// SECRET_KEY から 32バイトの鍵を派生する（SHA-256）
fn derive_key(secret_key: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(secret_key.as_bytes());
    let result = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    key
}

// ── 秘密鍵の暗号化/復号 ──

/// TOTP 秘密鍵を AES-256-GCM で暗号化する
/// 戻り値: Base64(ciphertext + nonce を連結したblob)
/// サイズ: 36 bytes ciphertext + 12 bytes nonce = 48 bytes → 64 chars base64 (DB制限に適合)
pub fn encrypt_secret(plaintext: &[u8], secret_key: &str) -> anyhow::Result<EncryptedSecret> {
    let key_bytes = derive_key(secret_key);
    let key = GenericArray::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);

    let nonce_bytes: [u8; 12] = rand::random();
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher.encrypt(nonce, plaintext)
        .map_err(|e| anyhow::anyhow!("TOTP秘密鍵の暗号化に失敗: {}", e))?;

    // ciphertext + nonce を一つのblob として結合
    let mut blob = Vec::with_capacity(ciphertext.len() + nonce_bytes.len());
    blob.extend_from_slice(&ciphertext);
    blob.extend_from_slice(&nonce_bytes);

    Ok(EncryptedSecret {
        blob_b64: BASE64.encode(&blob),
    })
}

/// AES-256-GCM で暗号化された TOTP 秘密鍵を復号する
/// 入力: Base64エンコードされたblob (ciphertext + nonce を連結したもの)
pub fn decrypt_secret(blob_b64: &str, secret_key: &str) -> anyhow::Result<Vec<u8>> {
    let key_bytes = derive_key(secret_key);
    let key = GenericArray::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);

    let blob = BASE64.decode(blob_b64)?;
    if blob.len() < 12 {
        return Err(anyhow::anyhow!("TOTP秘密鍵blobが短すぎます"));
    }

    // blob の最後の12バイトが nonce
    let ciphertext = &blob[..blob.len() - 12];
    let nonce_bytes = &blob[blob.len() - 12..];
    let nonce = Nonce::from_slice(nonce_bytes);

    cipher.decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("TOTP秘密鍵の復号に失敗: {}", e))
}

// ── TOTP 操作 ──

/// 新しい TOTP 秘密鍵を生成する（バイト列、20バイト）
pub fn generate_secret() -> Vec<u8> {
    use rand::RngCore;
    let mut secret_bytes = vec![0u8; 20];
    rand::thread_rng().fill_bytes(&mut secret_bytes);
    secret_bytes
}

/// TOTP インスタンスを構築する
fn build_totp(secret_bytes: &[u8], username: &str) -> anyhow::Result<TOTP> {
    TOTP::new(
        Algorithm::SHA1,  // Google Authenticator 互換
        6,                // 6桁コード
        1,                // skew: 前後1ステップ許容
        30,               // 30秒ステップ
        secret_bytes.to_vec(),
        Some("WIP".to_string()),
        username.to_string(),
    )
    .map_err(|e| anyhow::anyhow!("TOTPの構築に失敗: {}", e))
}

/// QR コードを base64 エンコードされた PNG として返す
/// フロントエンドで `<img src="data:image/png;base64,{qr_base64}">` で表示
pub fn generate_qr_base64(secret_bytes: &[u8], username: &str) -> anyhow::Result<String> {
    let totp = build_totp(secret_bytes, username)?;
    totp.get_qr_base64()
        .map_err(|e| anyhow::anyhow!("QRコードの生成に失敗: {}", e))
}

/// TOTP コードを検証する
/// 現在時刻の前後 ±30秒（skew=1）のコードも許容
pub fn verify_code(secret_bytes: &[u8], username: &str, code: &str) -> anyhow::Result<bool> {
    let totp = build_totp(secret_bytes, username)?;
    Ok(totp.check_current(code).unwrap_or(false))
}

/// 秘密鍵の Base32 表現を返す（手動入力用のバックアップコード表示）
pub fn secret_to_base32(secret_bytes: &[u8]) -> String {
    Secret::Raw(secret_bytes.to_vec()).to_encoded().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let secret_key = "test-secret-key-for-wip";
        let original = b"test-totp-secret-bytes-here";

        let encrypted = encrypt_secret(original, secret_key).unwrap();
        let decrypted = decrypt_secret(&encrypted.blob_b64, secret_key).unwrap();

        assert_eq!(original.to_vec(), decrypted);
    }

    #[test]
    fn test_blob_size_fits_in_db() {
        let secret_key = "test-secret-key";
        let secret = generate_secret();
        let encrypted = encrypt_secret(&secret, secret_key).unwrap();
        // DB column は max 64 chars
        assert!(encrypted.blob_b64.len() <= 64);
    }

    #[test]
    fn test_generate_secret_is_not_empty() {
        let secret = generate_secret();
        assert!(!secret.is_empty());
    }
}
