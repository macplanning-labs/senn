/// encrypted_settings.rs — system_settings 用の AES-GCM 暗号化ヘルパー
///
/// TOTP 秘密鍵と同じ auth-core の encrypt_secret / decrypt_secret を利用する。
/// 暗号鍵は JWT 秘密鍵（DJANGO_SECRET_KEY 系）から派生する。

use auth_core::domain::totp;

pub fn encrypt_value(plaintext: &str, jwt_secret: &str) -> anyhow::Result<String> {
    let encrypted = totp::encrypt_secret(plaintext.as_bytes(), jwt_secret)
        .map_err(|e| anyhow::anyhow!("設定値の暗号化に失敗: {e}"))?;
    Ok(encrypted.blob_b64)
}

pub fn decrypt_value(blob_b64: &str, jwt_secret: &str) -> anyhow::Result<String> {
    let bytes = totp::decrypt_secret(blob_b64, jwt_secret)
        .map_err(|e| anyhow::anyhow!("設定値の復号に失敗: {e}"))?;
    String::from_utf8(bytes).map_err(|e| anyhow::anyhow!("設定値のUTF-8復号に失敗: {e}"))
}

/// マスク表示用: 末尾4文字のみ（4文字未満は ****）
pub fn mask_secret_tail(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= 4 {
        return "****".to_string();
    }
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("****{tail}")
}
