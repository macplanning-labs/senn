/// domain/models/user.rs — ユーザーモデル
///
/// Djangoの accounts_user テーブルに対応する(created_at/updated_at列は存在しない)。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: i32,
    pub username: String,
    pub password_hash: String,
    pub display_name: String,
    pub email: String,
    pub first_name: String,
    pub last_name: String,
    pub is_active: bool,
    pub is_staff: bool,
    pub must_change_password: bool,
    pub email_notifications_enabled: bool,
}

#[allow(dead_code)]
impl User {
    /// 表示名（display_name が空ならユーザー名）
    pub fn display(&self) -> &str {
        if self.display_name.is_empty() {
            &self.username
        } else {
            &self.display_name
        }
    }
}

// ---------------------------------------------------------------------------
// TOTP デバイス
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TotpDevice {
    pub id: i32,
    pub user_id: i32,
    pub secret: String,
    pub confirmed: bool,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// WebAuthn クレデンシャル
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WebAuthnCredential {
    pub id: i32,
    pub user_id: i32,
    pub credential_id: Vec<u8>,
    pub public_key: Vec<u8>,
    pub sign_count: i32,
    pub name: String,
    pub created_at: DateTime<Utc>,
}
