//! 招待とメール確認(アクセス制御の再設計 フェーズ A。詳細設計書 §3.3・§11.1)
//!
//! トークンは呼び出し側で作り、ここには SHA-256 の16進(`hash_token`)だけを渡す。
//! 受諾・確認は 1 回限り(`used_at`)・期限つき(`expires_at`)で、同じトランザクションの中で
//! 行をロックしてから使う(同時に 2 回受諾されない)。

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};

/// トークンの SHA-256(16進)。保存・照合はこの値で行う
pub fn hash_token(plain: &str) -> String {
    Sha256::digest(plain.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// 新しいトークン(32 バイトの乱数の16進)
pub fn new_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// 招待の役割
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InviteRole {
    FullMember,
    Guest,
}

impl InviteRole {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "full_member" => Some(Self::FullMember),
            "guest" => Some(Self::Guest),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FullMember => "full_member",
            Self::Guest => "guest",
        }
    }
}

/// 招待の作成の入力
#[derive(Debug, Clone)]
pub struct NewInvitation {
    pub email: String,
    pub role: InviteRole,
    pub team_id: Option<i32>,
    pub scoped_project_id: Option<i32>,
    pub end_date: Option<NaiveDate>,
    pub invited_by: i32,
    pub expires_at: DateTime<Utc>,
}

/// 招待(一覧・作成の応答)。トークンは含めない
#[derive(Debug, Clone, Serialize)]
pub struct InvitationOut {
    pub id: i64,
    pub email: String,
    pub role: String,
    #[serde(rename = "teamId")]
    pub team_id: Option<i32>,
    #[serde(rename = "scopedProjectId")]
    pub scoped_project_id: Option<i32>,
    #[serde(rename = "endDate")]
    pub end_date: Option<NaiveDate>,
    #[serde(rename = "expiresAt")]
    pub expires_at: DateTime<Utc>,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
}

/// 受諾の前に見せる招待の内容(公開ルート。メールアドレスは一部だけ)
#[derive(Debug, Clone, Serialize)]
pub struct InvitationPreview {
    #[serde(rename = "emailMasked")]
    pub email_masked: String,
    pub role: String,
    #[serde(rename = "teamName")]
    pub team_name: Option<String>,
    #[serde(rename = "projectName")]
    pub project_name: Option<String>,
    #[serde(rename = "expiresAt")]
    pub expires_at: DateTime<Utc>,
}

const OUT_COLUMNS: &str = "id, email, role, team_id::int4 AS team_id, scoped_project_id::int4 AS scoped_project_id, end_date, expires_at, created_at";

fn out_from_row(r: &sqlx::postgres::PgRow) -> InvitationOut {
    InvitationOut {
        id: r.get("id"),
        email: r.get("email"),
        role: r.get("role"),
        team_id: r.get("team_id"),
        scoped_project_id: r.get("scoped_project_id"),
        end_date: r.get("end_date"),
        expires_at: r.get("expires_at"),
        created_at: r.get("created_at"),
    }
}

/// `a***@example.com` の形にする(受諾の画面で、誰宛ての招待かだけ分かるように)
pub fn mask_email(email: &str) -> String {
    match email.split_once('@') {
        Some((local, domain)) => {
            let first: String = local.chars().take(1).collect();
            format!("{first}***@{domain}")
        }
        None => "***".to_string(),
    }
}

/// そのメールアドレスのユーザーがすでにいるか(大文字・小文字は区別しない)
pub async fn email_registered(pool: &PgPool, email: &str) -> sqlx::Result<bool> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM accounts_user WHERE lower(email) = lower($1))")
        .bind(email)
        .fetch_one(pool)
        .await
}

/// 招待を作る。同じメールアドレス・同じ招待先の未使用の招待は取り消してから作る(最新の 1 件だけ有効)
pub async fn create(
    pool: &PgPool,
    input: &NewInvitation,
    token_hash: &str,
) -> sqlx::Result<InvitationOut> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE access_invitation SET revoked_at = now()
         WHERE lower(email) = lower($1) AND team_id IS NOT DISTINCT FROM $2::int8
           AND used_at IS NULL AND revoked_at IS NULL",
    )
    .bind(&input.email)
    .bind(input.team_id.map(i64::from))
    .execute(&mut *tx)
    .await?;
    let row = sqlx::query(&format!(
        "INSERT INTO access_invitation
           (email, token_hash, role, team_id, scoped_project_id, end_date, invited_by, expires_at)
         VALUES ($1, $2, $3, $4::int8, $5::int8, $6, $7::int8, $8)
         RETURNING {OUT_COLUMNS}"
    ))
    .bind(&input.email)
    .bind(token_hash)
    .bind(input.role.as_str())
    .bind(input.team_id.map(i64::from))
    .bind(input.scoped_project_id.map(i64::from))
    .bind(input.end_date)
    .bind(i64::from(input.invited_by))
    .bind(input.expires_at)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(out_from_row(&row))
}

/// 未使用・期限内・取り消されていない招待の一覧(`team_id` が None なら、チームの無い招待)
pub async fn list_pending(pool: &PgPool, team_id: Option<i32>) -> sqlx::Result<Vec<InvitationOut>> {
    let rows = sqlx::query(&format!(
        "SELECT {OUT_COLUMNS} FROM access_invitation
         WHERE team_id IS NOT DISTINCT FROM $1::int8
           AND used_at IS NULL AND revoked_at IS NULL AND expires_at > now()
         ORDER BY created_at DESC"
    ))
    .bind(team_id.map(i64::from))
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(out_from_row).collect())
}

/// 招待の招待先(取り消しの権限の確認用)。無ければ None
pub async fn find_target(pool: &PgPool, id: i64) -> sqlx::Result<Option<(Option<i32>, bool)>> {
    let row = sqlx::query(
        "SELECT team_id::int4 AS team_id, (used_at IS NULL AND revoked_at IS NULL) AS pending
         FROM access_invitation WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| (r.get("team_id"), r.get("pending"))))
}

/// 招待を取り消す(未使用のものだけ)
pub async fn revoke(pool: &PgPool, id: i64) -> sqlx::Result<bool> {
    let n = sqlx::query(
        "UPDATE access_invitation SET revoked_at = now()
         WHERE id = $1 AND used_at IS NULL AND revoked_at IS NULL",
    )
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(n > 0)
}

/// 受諾の前の確認(公開ルート)。使えない招待(使用済み・取り消し・期限切れ・存在しない)は None
pub async fn preview(pool: &PgPool, token_hash: &str) -> sqlx::Result<Option<InvitationPreview>> {
    let row = sqlx::query(
        "SELECT i.email, i.role, i.expires_at, t.name AS team_name, p.name AS project_name
         FROM access_invitation i
         LEFT JOIN m_team t ON t.id = i.team_id
         LEFT JOIN tickets_project p ON p.id = i.scoped_project_id
         WHERE i.token_hash = $1 AND i.used_at IS NULL AND i.revoked_at IS NULL AND i.expires_at > now()",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| InvitationPreview {
        email_masked: mask_email(&r.get::<String, _>("email")),
        role: r.get("role"),
        team_name: r.get("team_name"),
        project_name: r.get("project_name"),
        expires_at: r.get("expires_at"),
    }))
}

/// 受諾の結果
#[derive(Debug)]
pub enum AcceptOutcome {
    /// アカウントを作った(新しいユーザーの ID)
    Created { user_id: i32 },
    /// 使えない招待(使用済み・取り消し・期限切れ・存在しない)
    Invalid,
    /// そのメールアドレスのユーザーがすでにいる
    EmailTaken,
    /// ユーザー名が使われている
    UsernameTaken,
}

/// 受諾の入力
pub struct AcceptInput<'a> {
    pub username: &'a str,
    pub password_hash: &'a str,
    pub display_name: &'a str,
}

/// 招待を受諾し、アカウントを作る(1 つのトランザクション)。
/// - メールアドレスは招待のもの(受け取った人がメールを読めた = 確認済みとして `email_verified_at` を入れる)
/// - Guest の招待は `is_guest = true`
/// - 招待先のチームがあれば所属させる(Guest のプロジェクト単位の招待は、そのプロジェクトだけ)
pub async fn accept(
    pool: &PgPool,
    token_hash: &str,
    input: &AcceptInput<'_>,
) -> sqlx::Result<AcceptOutcome> {
    let mut tx: Transaction<'_, Postgres> = pool.begin().await?;
    let Some(inv) = sqlx::query(
        "SELECT id, email, role, team_id, scoped_project_id, end_date
         FROM access_invitation
         WHERE token_hash = $1 AND used_at IS NULL AND revoked_at IS NULL AND expires_at > now()
         FOR UPDATE",
    )
    .bind(token_hash)
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok(AcceptOutcome::Invalid);
    };
    let inv_id: i64 = inv.get("id");
    let email: String = inv.get("email");
    let is_guest = inv.get::<String, _>("role") == "guest";
    let team_id: Option<i64> = inv.get("team_id");
    let scoped_project_id: Option<i64> = inv.get("scoped_project_id");
    let end_date: Option<NaiveDate> = inv.get("end_date");

    let email_taken: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM accounts_user WHERE lower(email) = lower($1))",
    )
    .bind(&email)
    .fetch_one(&mut *tx)
    .await?;
    if email_taken {
        return Ok(AcceptOutcome::EmailTaken);
    }
    let username_taken: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM accounts_user WHERE username = $1)")
            .bind(input.username)
            .fetch_one(&mut *tx)
            .await?;
    if username_taken {
        return Ok(AcceptOutcome::UsernameTaken);
    }

    // 列は user_repo::create_user と同じ(Django の accounts_user の必須列)
    let user_id: i32 = sqlx::query_scalar(
        "INSERT INTO accounts_user
            (password, is_superuser, username, first_name, last_name, email,
             is_staff, is_active, date_joined, display_name,
             must_change_password, email_notifications_enabled,
             is_guest, is_system_admin, email_verified_at)
         VALUES ($1, false, $2, '', '', $3, false, true, NOW(), $4, false, true, $5, false, NOW())
         RETURNING id::int4",
    )
    .bind(input.password_hash)
    .bind(input.username)
    .bind(&email)
    .bind(input.display_name)
    .bind(is_guest)
    .fetch_one(&mut *tx)
    .await?;

    if let Some(team_id) = team_id {
        sqlx::query(
            "INSERT INTO t_team_membership (team_id, user_id, role, scoped_project_id, end_date, joined_at)
             VALUES ($1, $2::int8, 'member', $3, $4, NOW())",
        )
        .bind(team_id)
        .bind(i64::from(user_id))
        .bind(scoped_project_id)
        .bind(end_date)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query("UPDATE access_invitation SET used_at = now() WHERE id = $1")
        .bind(inv_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO access_audit_log (actor_user_id, actor_kind, action, team_id, target_user_id, detail)
         VALUES ($1::int8, 'human', 'invitation_accepted', $2, $1::int8, $3)",
    )
    .bind(i64::from(user_id))
    .bind(team_id)
    .bind(serde_json::json!({"invitation_id": inv_id, "guest": is_guest}))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(AcceptOutcome::Created { user_id })
}

/// メール確認のトークンを作る(同じユーザーの古いトークンは消す)
pub async fn create_email_verification(
    pool: &PgPool,
    user_id: i32,
    token_hash: &str,
    expires_at: DateTime<Utc>,
) -> sqlx::Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "DELETE FROM access_email_verification WHERE user_id = $1::int8 AND used_at IS NULL",
    )
    .bind(i64::from(user_id))
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO access_email_verification (user_id, token_hash, expires_at) VALUES ($1::int8, $2, $3)",
    )
    .bind(i64::from(user_id))
    .bind(token_hash)
    .bind(expires_at)
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

/// メールを確認する。確認できたら、ユーザーを有効にして `email_verified_at` を入れる。
/// 使えないトークン(使用済み・期限切れ・存在しない)は None
pub async fn verify_email(pool: &PgPool, token_hash: &str) -> sqlx::Result<Option<i32>> {
    let mut tx = pool.begin().await?;
    let Some(user_id) = sqlx::query_scalar::<_, i32>(
        "SELECT user_id::int4 FROM access_email_verification
         WHERE token_hash = $1 AND used_at IS NULL AND expires_at > now()
         FOR UPDATE",
    )
    .bind(token_hash)
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok(None);
    };
    sqlx::query("UPDATE access_email_verification SET used_at = now() WHERE token_hash = $1")
        .bind(token_hash)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE accounts_user SET is_active = true, email_verified_at = now() WHERE id = $1::int8",
    )
    .bind(i64::from(user_id))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Some(user_id))
}

/// 自己登録(メール確認待ち)のユーザーを作る。確認が済むまで無効(ログインできない・Full Member にしない)
pub async fn create_unverified_user(
    pool: &PgPool,
    username: &str,
    email: &str,
    password_hash: &str,
    first_name: &str,
    last_name: &str,
) -> sqlx::Result<i32> {
    sqlx::query_scalar(
        "INSERT INTO accounts_user
            (password, is_superuser, username, first_name, last_name, email,
             is_staff, is_active, date_joined, display_name,
             must_change_password, email_notifications_enabled)
         VALUES ($1, false, $2, $3, $4, $5, false, false, NOW(), '', false, true)
         RETURNING id::int4",
    )
    .bind(password_hash)
    .bind(username)
    .bind(first_name)
    .bind(last_name)
    .bind(email)
    .fetch_one(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;

    #[test]
    fn masks_email() {
        assert_eq!(mask_email("taro@example.com"), "t***@example.com");
        assert_eq!(mask_email("broken"), "***");
    }

    #[test]
    fn hashes_and_creates_tokens() {
        let t = new_token();
        assert_eq!(t.len(), 64);
        assert_ne!(t, new_token());
        assert_eq!(hash_token("abc").len(), 64);
        assert_ne!(hash_token(&t), t, "保存するのはハッシュ");
    }

    /// 受諾は 1 回限り。Guest の招待はゲストとしてチームに所属し、期限切れ・取り消しは使えない
    #[tokio::test]
    async fn accept_once_creates_member_and_rejects_reuse() {
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let inviter = test_support::create_test_user(&pool, "inviter").await;
        let team = test_support::create_test_team(&pool, "inv").await;
        let email = format!("guest-{}@outside.example", test_support::unique_suffix());
        let token = new_token();
        create(
            &pool,
            &NewInvitation {
                email: email.clone(),
                role: InviteRole::Guest,
                team_id: Some(team),
                scoped_project_id: None,
                end_date: None,
                invited_by: inviter,
                expires_at: Utc::now() + chrono::Duration::days(7),
            },
            &hash_token(&token),
        )
        .await
        .unwrap();

        let shown = preview(&pool, &hash_token(&token)).await.unwrap().unwrap();
        assert_eq!(shown.role, "guest");
        assert!(shown.email_masked.starts_with("g***@"));

        let username = format!("g{}", test_support::unique_suffix());
        let input = AcceptInput {
            username: &username,
            password_hash: "x",
            display_name: "ゲスト",
        };
        let user_id = match accept(&pool, &hash_token(&token), &input).await.unwrap() {
            AcceptOutcome::Created { user_id } => user_id,
            other => panic!("作成されるはず: {other:?}"),
        };
        let (is_guest, verified): (bool, bool) = sqlx::query_as(
            "SELECT is_guest, email_verified_at IS NOT NULL FROM accounts_user WHERE id = $1::int8",
        )
        .bind(i64::from(user_id))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(is_guest && verified);
        let member: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM t_team_membership WHERE team_id = $1::int8 AND user_id = $2::int8)",
        )
        .bind(i64::from(team))
        .bind(i64::from(user_id))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(member, "招待先のチームに所属する");

        // 2 回目は使えない
        assert!(matches!(
            accept(&pool, &hash_token(&token), &input).await.unwrap(),
            AcceptOutcome::Invalid
        ));
        assert!(preview(&pool, &hash_token(&token)).await.unwrap().is_none());

        // 同じメールアドレスへの新しい招待は、受諾時に「登録済み」になる
        let token2 = new_token();
        create(
            &pool,
            &NewInvitation {
                email: email.clone(),
                role: InviteRole::Guest,
                team_id: Some(team),
                scoped_project_id: None,
                end_date: None,
                invited_by: inviter,
                expires_at: Utc::now() + chrono::Duration::days(7),
            },
            &hash_token(&token2),
        )
        .await
        .unwrap();
        let other = AcceptInput {
            username: "unused-name-for-test",
            password_hash: "x",
            display_name: "",
        };
        assert!(matches!(
            accept(&pool, &hash_token(&token2), &other).await.unwrap(),
            AcceptOutcome::EmailTaken
        ));

        // 期限切れは使えない
        let token3 = new_token();
        create(
            &pool,
            &NewInvitation {
                email: format!("late-{}@outside.example", test_support::unique_suffix()),
                role: InviteRole::FullMember,
                team_id: None,
                scoped_project_id: None,
                end_date: None,
                invited_by: inviter,
                expires_at: Utc::now() - chrono::Duration::minutes(1),
            },
            &hash_token(&token3),
        )
        .await
        .unwrap();
        assert!(preview(&pool, &hash_token(&token3))
            .await
            .unwrap()
            .is_none());
    }

    /// メール確認: 確認前は無効、確認でき有効になる。2 回目は使えない
    #[tokio::test]
    async fn verify_email_activates_once() {
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let username = format!("self{}", test_support::unique_suffix());
        let user_id = create_unverified_user(
            &pool,
            &username,
            &format!("{username}@corp.example"),
            "x",
            "",
            "",
        )
        .await
        .unwrap();
        let active: bool =
            sqlx::query_scalar("SELECT is_active FROM accounts_user WHERE id = $1::int8")
                .bind(i64::from(user_id))
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(!active, "確認前は無効");
        let token = new_token();
        create_email_verification(
            &pool,
            user_id,
            &hash_token(&token),
            Utc::now() + chrono::Duration::hours(24),
        )
        .await
        .unwrap();
        assert_eq!(
            verify_email(&pool, &hash_token(&token)).await.unwrap(),
            Some(user_id)
        );
        let (active, verified): (bool, bool) = sqlx::query_as(
            "SELECT is_active, email_verified_at IS NOT NULL FROM accounts_user WHERE id = $1::int8",
        )
        .bind(i64::from(user_id))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(active && verified);
        assert_eq!(
            verify_email(&pool, &hash_token(&token)).await.unwrap(),
            None
        );
    }
}
