use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::SaltString;
/// domain/services/auth_service.rs — 認証ビジネスロジック
///
/// パスワード検証（Argon2）、TOTP 検証、Django パスワードハッシュ移行。
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
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
            tracing::info!(
                "🔄 ユーザー {} のパスワードを Argon2 に移行しました",
                user_id
            );
        }
        return Ok(valid);
    }

    // Argon2 ハッシュ検証
    // 旧Django運用時代のレコードは"argon2"プレフィックス付き(`argon2$argon2id$...`)で
    // 保存されているため、あれば取り除いてから素のPHC文字列として解釈する
    // (2026-08-14以降の新規ハッシュはプレフィックスなしで保存されるため、
    // その場合はunwrap_or(stored_hash)でそのまま使われる)。
    let phc_str = stored_hash.strip_prefix("argon2").unwrap_or(stored_hash);
    let parsed_hash =
        PasswordHash::new(phc_str).map_err(|e| anyhow::anyhow!("ハッシュ解析エラー: {}", e))?;
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

    let iterations: u32 = parts[1]
        .parse()
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

    let computed_hash =
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &derived_key);

    Ok(computed_hash == expected_hash)
}

fn current_unix_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// コードが合う区間(UNIX 時間 / 30 秒)を返す。前後 1 区間まで許容する(時計のずれに備える)。合わなければ None。
/// どの区間で合ったかを知る必要がある(使用済みのコードを断るため。DEMO-000153 A-3)。
/// 実体は、複数のアプリで共通の `auth-core`(`totp::matching_step_base32`)。
pub fn totp_matching_step(secret: &str, code: &str, now_unix: u64) -> anyhow::Result<Option<i64>> {
    auth_core::domain::totp::matching_step_base32(secret, code, now_unix)
        .map_err(|e| anyhow::anyhow!("TOTP error: {}", e))
}

/// TOTP セットアップ（QR コード URL 生成）
pub fn generate_totp_setup(username: &str) -> anyhow::Result<(String, String)> {
    use totp_rs::{Algorithm, Secret, TOTP};

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
    )
    .map_err(|e| anyhow::anyhow!("TOTP error: {}", e))?;

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

    // 1. コードを調べる前に、試行を予約する。ロック中は、コードを調べずに断る(DEMO-000153 A-1)
    if user_repo::totp_reserve_attempt(
        pool,
        user_id,
        TOTP_MAX_ATTEMPTS,
        chrono::Duration::minutes(TOTP_LOCK_WINDOW_MINUTES),
    )
    .await?
        == user_repo::TotpAttemptReservation::Locked
    {
        tracing::warn!(
            user_id,
            "二要素認証の確認コードの試行が上限に達したため、一時的に止めています"
        );
        return Ok(false);
    }

    // 2. コードが合う区間を調べる
    let Some(step) = totp_matching_step(&secret_to_verify, code, current_unix_time())? else {
        return Ok(false);
    };

    // 3. 使用済みの区間(同じ区間・古い区間)は断る。1 回の UPDATE で、同時に同じコードを送られても 1 回しか通らない(A-3)
    if !user_repo::totp_mark_step_used(pool, device.id, step).await? {
        return Ok(false);
    }

    // 4. 正しいコードだったので、数を消す
    user_repo::totp_reset_attempts(pool, user_id).await?;
    Ok(true)
}

/// 管理者のパスキー必須の、強制の開始日時を入れるシステム設定のキー(RFC 3339)。未設定なら、強制しない
pub const ADMIN_PASSKEY_ENFORCE_AFTER_KEY: &str = "sys.auth.admin_passkey_enforce_after";

/// 管理者のパスキーの状態(DEMO-000153 A-4)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminPasskeyStatus {
    /// 管理者(システム管理者・チーム Owner)か
    pub is_admin: bool,
    /// パスキーを 1 つ以上登録しているか
    pub has_passkey: bool,
    /// 強制の開始日時(未設定なら None)
    pub enforce_after: Option<chrono::DateTime<chrono::Utc>>,
}

impl AdminPasskeyStatus {
    /// 登録が必要か(管理者で、まだ登録していない)
    pub fn must_register(&self) -> bool {
        self.is_admin && !self.has_passkey
    }

    /// すでに強制の期間か(開始日時を過ぎている)。開始日時が未設定なら、強制しない
    pub fn is_enforced(&self, now: chrono::DateTime<chrono::Utc>) -> bool {
        self.enforce_after.is_some_and(|t| now >= t)
    }
}

/// 管理者がパスワードでログインしたあと、どう進めるか(DEMO-000153 A-4b)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminLoginPolicy {
    /// 今までどおり(管理者でない / 猶予の間で未登録)
    Normal,
    /// パスキーの確認が必須(TOTP の段階には進まない)
    RequirePasskey,
    /// 強制の期間で、パスキーが未登録。新しく登録するには、別の管理者が開けた窓が要る
    EnrollRequired,
}

pub fn admin_login_policy(
    st: &AdminPasskeyStatus,
    now: chrono::DateTime<chrono::Utc>,
) -> AdminLoginPolicy {
    if !st.is_admin {
        AdminLoginPolicy::Normal
    } else if st.has_passkey {
        AdminLoginPolicy::RequirePasskey
    } else if st.is_enforced(now) {
        AdminLoginPolicy::EnrollRequired
    } else {
        AdminLoginPolicy::Normal
    }
}

pub async fn admin_passkey_status(
    pool: &PgPool,
    user_id: i32,
) -> anyhow::Result<AdminPasskeyStatus> {
    let is_admin = user_repo::is_admin_account(pool, user_id).await?;
    // 管理者でなければ、パスキーの数は調べない
    let has_passkey = is_admin && user_repo::count_passkeys(pool, user_id).await? > 0;
    let enforce_after = crate::infrastructure::repositories::system_settings_repo::get(
        pool,
        ADMIN_PASSKEY_ENFORCE_AFTER_KEY,
    )
    .await?
    .and_then(|v| chrono::DateTime::parse_from_rfc3339(v.trim()).ok())
    .map(|t| t.with_timezone(&chrono::Utc));
    Ok(AdminPasskeyStatus {
        is_admin,
        has_passkey,
        enforce_after,
    })
}

/// 確認コードの試行の上限(時間窓の間に調べてよい回数)と、時間窓・ロックの長さ(分)。運用を見て調整する。
pub const TOTP_MAX_ATTEMPTS: i32 = 10;
pub const TOTP_LOCK_WINDOW_MINUTES: i64 = 15;

#[cfg(test)]
mod totp_guard_tests {
    use super::*;
    use crate::test_support::{create_test_user, test_pool};
    use totp_rs::{Algorithm, Secret, TOTP};

    const JWT: &str = "test-jwt-secret-not-used-for-plain-totp";

    fn new_secret() -> String {
        let mut raw = vec![0u8; 20];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut raw);
        Secret::Raw(raw).to_encoded().to_string()
    }

    fn code_at(secret: &str, unix: u64) -> String {
        let bytes = Secret::Encoded(secret.to_string()).to_bytes().unwrap();
        TOTP::new(Algorithm::SHA1, 6, 1, 30, bytes, None, String::new())
            .unwrap()
            .generate(unix)
    }

    fn wrong_code(secret: &str, unix: u64) -> String {
        // 前後 1 区間のどれとも違う 6 桁
        let ok: Vec<String> = [0u64, 30, 60]
            .iter()
            .flat_map(|d| {
                [
                    code_at(secret, unix + d),
                    code_at(secret, unix.saturating_sub(*d)),
                ]
            })
            .collect();
        (0..1_000_000u32)
            .map(|n| format!("{n:06}"))
            .find(|c| !ok.contains(c))
            .unwrap()
    }

    async fn user_with_totp(pool: &PgPool) -> (i32, i32, String) {
        let user_id = create_test_user(pool, "totp-guard").await;
        let secret = new_secret();
        let device_id: i64 = sqlx::query_scalar(
            "INSERT INTO mfa_totp_device (secret, confirmed, created_at, user_id) VALUES ($1, true, now(), $2) RETURNING id",
        )
        .bind(&secret)
        .bind(user_id as i64)
        .fetch_one(pool)
        .await
        .unwrap();
        (user_id, device_id as i32, secret)
    }

    #[test]
    fn matching_step_accepts_current_and_adjacent_but_not_further() {
        let secret = new_secret();
        let now = 1_800_000_000u64;
        let step = (now / 30) as i64;
        assert_eq!(
            totp_matching_step(&secret, &code_at(&secret, now), now).unwrap(),
            Some(step)
        );
        assert_eq!(
            totp_matching_step(&secret, &code_at(&secret, now - 30), now).unwrap(),
            Some(step - 1)
        );
        assert_eq!(
            totp_matching_step(&secret, &code_at(&secret, now + 30), now).unwrap(),
            Some(step + 1)
        );
        assert_eq!(
            totp_matching_step(&secret, &code_at(&secret, now - 90), now).unwrap(),
            None
        );
        assert_eq!(
            totp_matching_step(&secret, &wrong_code(&secret, now), now).unwrap(),
            None
        );
        assert_eq!(totp_matching_step(&secret, "12345", now).unwrap(), None);
    }

    #[tokio::test]
    async fn same_code_cannot_be_used_twice_and_older_code_is_refused() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let (user_id, _dev, secret) = user_with_totp(&pool).await;
        let now = current_unix_time();
        let cur = code_at(&secret, now);
        let prev = code_at(&secret, now - 30);
        assert!(verify_totp_for_user(&pool, JWT, user_id, &cur)
            .await
            .unwrap());
        // 同じコードの 2 回目は断る
        assert!(!verify_totp_for_user(&pool, JWT, user_id, &cur)
            .await
            .unwrap());
        // 使った区間より古い区間のコードも断る
        assert!(!verify_totp_for_user(&pool, JWT, user_id, &prev)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn locks_after_max_attempts_even_for_the_correct_code_and_survives_new_calls() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let (user_id, _dev, secret) = user_with_totp(&pool).await;
        let now = current_unix_time();
        let bad = wrong_code(&secret, now);
        // 上限までは、調べられる(間違い)
        for _ in 0..TOTP_MAX_ATTEMPTS {
            assert!(!verify_totp_for_user(&pool, JWT, user_id, &bad)
                .await
                .unwrap());
        }
        // 上限を超えたら、正しいコードでも断る(ログインし直しても、数は DB にあるので引き継がれる)
        let good = code_at(&secret, now);
        assert!(!verify_totp_for_user(&pool, JWT, user_id, &good)
            .await
            .unwrap());
        let locked: Option<chrono::DateTime<chrono::Utc>> =
            sqlx::query_scalar("SELECT locked_until FROM mfa_totp_attempt WHERE user_id = $1")
                .bind(user_id as i64)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(locked.is_some_and(|t| t > chrono::Utc::now()));

        // ロックが明けたら、正しいコードで入れて、数が消える
        sqlx::query("UPDATE mfa_totp_attempt SET locked_until = now() - interval '1 second' WHERE user_id = $1")
            .bind(user_id as i64)
            .execute(&pool)
            .await
            .unwrap();
        assert!(verify_totp_for_user(&pool, JWT, user_id, &good)
            .await
            .unwrap());
        let rows: i64 =
            sqlx::query_scalar("SELECT count(*) FROM mfa_totp_attempt WHERE user_id = $1")
                .bind(user_id as i64)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(rows, 0);
    }

    #[tokio::test]
    async fn other_users_are_not_locked() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let (a, _, sa) = user_with_totp(&pool).await;
        let (b, _, sb) = user_with_totp(&pool).await;
        let now = current_unix_time();
        for _ in 0..=TOTP_MAX_ATTEMPTS {
            let _ = verify_totp_for_user(&pool, JWT, a, &wrong_code(&sa, now)).await;
        }
        assert!(verify_totp_for_user(&pool, JWT, b, &code_at(&sb, now))
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn parallel_attempts_never_exceed_the_limit() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let (user_id, _dev, _secret) = user_with_totp(&pool).await;
        // 同時に 40 回送っても、コードを調べるのは上限の回数まで(残りはロックで断られる)
        let mut handles = Vec::new();
        for _ in 0..40 {
            let pool = pool.clone();
            handles.push(tokio::spawn(async move {
                user_repo::totp_reserve_attempt(
                    &pool,
                    user_id,
                    TOTP_MAX_ATTEMPTS,
                    chrono::Duration::minutes(15),
                )
                .await
                .unwrap()
            }));
        }
        let mut allowed = 0;
        for h in handles {
            if h.await.unwrap() == user_repo::TotpAttemptReservation::Allowed {
                allowed += 1;
            }
        }
        assert_eq!(allowed, TOTP_MAX_ATTEMPTS);
    }
}
