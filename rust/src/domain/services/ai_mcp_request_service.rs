/// domain/services/ai_mcp_request_service.rs — AI MCP 申請のメール通知(DEMO-000192 フェーズ1b)
///
/// 申請・承認・却下時のメール送信ロジック。
use crate::domain::models::user::User;
use crate::infrastructure::mail::MailSender;
use crate::infrastructure::repositories::ai_mcp_request_repo::APPROVAL_VALID_DAYS;
use sqlx::PgPool;

/// メール送信の宛先判定。純粋な関数として、テストしやすいようにする。
/// メール通知をすべきか判定する(マスタースイッチ + メールアドレスの両方を見る)。
pub fn should_send_email_to(user: &User) -> bool {
    user.email_notifications_enabled && !user.email.is_empty()
}

/// メールに入れる画面へのリンクを作る。基準 URL が空、または localhost(開発・staging の未設定)のときは None。
pub fn build_link(base_url: &str, path: &str) -> Option<String> {
    let base = base_url.trim().trim_end_matches('/');
    if base.is_empty() || base.contains("localhost") || base.contains("127.0.0.1") {
        return None;
    }
    Some(format!("{}{}", base, path))
}

/// 管理画面(申請一覧)へのリンク
fn admin_link(base_url: &str) -> Option<String> {
    build_link(base_url, "/admin?section=ai")
}

/// 設定画面(AI タブ)へのリンク
fn settings_link(base_url: &str) -> Option<String> {
    build_link(base_url, "/settings?tab=ai")
}

fn with_link(text: String, link: Option<String>) -> String {
    match link {
        Some(l) => format!("{}\n\n{}", text, l),
        None => text,
    }
}

fn display_name(user: &User) -> String {
    if user.display_name.is_empty() {
        user.username.clone()
    } else {
        user.display_name.clone()
    }
}

pub fn request_body(requester_name: &str, base_url: &str) -> String {
    with_link(
        format!(
            "{} さんから申請があります。管理画面で承認または却下してください。",
            requester_name
        ),
        admin_link(base_url),
    )
}

pub fn reminder_body(requester_name: &str, base_url: &str) -> String {
    with_link(
        format!(
            "{} さんの申請がまだ処理されていません(3日以上経過)。管理画面で確認してください。",
            requester_name
        ),
        admin_link(base_url),
    )
}

pub fn connected_body() -> String {
    "承認されました。Claude から、そのまま使えます(キーの操作は要りません)。".to_string()
}

pub fn approval_body(base_url: &str) -> String {
    with_link(
        format!(
            "設定画面からキーを発行できます({}日以内)。",
            APPROVAL_VALID_DAYS
        ),
        settings_link(base_url),
    )
}

pub fn rejection_body(note: &str) -> String {
    if note.trim().is_empty() {
        "申請は却下されました。".to_string()
    } else {
        format!("申請は却下されました。理由: {}", note.trim())
    }
}

/// 有効なシステム管理者全員へ、同じ件名・本文のメールを送る(best-effort)
async fn send_to_admins(
    mail_sender: &Option<MailSender>,
    pool: &PgPool,
    subject: &str,
    body: &str,
) -> anyhow::Result<()> {
    let Some(sender) = mail_sender else {
        return Ok(());
    };
    let admins =
        crate::infrastructure::repositories::ai_mcp_request_repo::active_system_admin_ids(pool)
            .await?;
    for admin_id in admins {
        if let Ok(Some(admin)) =
            crate::infrastructure::repositories::user_repo::find_by_id(pool, admin_id).await
        {
            if should_send_email_to(&admin) {
                if let Err(e) = sender.send(&admin.email, subject, body).await {
                    tracing::warn!("メール送信失敗 (admin_id={}): {}", admin_id, e);
                }
            }
        }
    }
    Ok(())
}

/// 申請時: 管理者全員へメールを送る
pub async fn send_email_on_request(
    mail_sender: &Option<MailSender>,
    pool: &PgPool,
    requester: &User,
    base_url: &str,
) -> anyhow::Result<()> {
    send_to_admins(
        mail_sender,
        pool,
        "[SENN] AI Agent を利用した MCP 操作の申請",
        &request_body(&display_name(requester), base_url),
    )
    .await
}

/// 再通知時: 管理者全員へメールを送る
pub async fn send_email_on_reminder(
    mail_sender: &Option<MailSender>,
    pool: &PgPool,
    requester: &User,
    base_url: &str,
) -> anyhow::Result<()> {
    send_to_admins(
        mail_sender,
        pool,
        "[SENN] AI Agent を利用した MCP 操作の申請: 再通知",
        &reminder_body(&display_name(requester), base_url),
    )
    .await
}

async fn send_to_user(
    mail_sender: &Option<MailSender>,
    user: &User,
    subject: &str,
    body: &str,
) -> anyhow::Result<()> {
    let Some(sender) = mail_sender else {
        return Ok(());
    };
    if should_send_email_to(user) {
        if let Err(e) = sender.send(&user.email, subject, body).await {
            tracing::warn!("メール送信失敗 (user_id={}): {}", user.id, e);
        }
    }
    Ok(())
}

/// 承認時: 申請者へメールを送る
pub async fn send_email_on_approval(
    mail_sender: &Option<MailSender>,
    requester: &User,
    base_url: &str,
    connected: bool,
) -> anyhow::Result<()> {
    let body = if connected {
        connected_body()
    } else {
        approval_body(base_url)
    };
    send_to_user(
        mail_sender,
        requester,
        "[SENN] AI Agent を利用した MCP 操作の申請: 承認されました",
        &body,
    )
    .await
}

/// 却下時: 申請者へメールを送る(理由も含める)
pub async fn send_email_on_rejection(
    mail_sender: &Option<MailSender>,
    requester: &User,
    note: &str,
) -> anyhow::Result<()> {
    send_to_user(
        mail_sender,
        requester,
        "[SENN] AI Agent を利用した MCP 操作の申請: 却下されました",
        &rejection_body(note),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_send_email_to_user_with_enabled_notifications_and_email() {
        // ユーザーの email_notifications_enabled が true かつメールアドレスがある場合、true を返す
        let user_enabled = User {
            id: 1,
            username: "enabled".to_string(),
            password_hash: "hash".to_string(),
            email: "enabled@example.com".to_string(),
            display_name: "Enabled User".to_string(),
            alias: None,
            first_name: "E".to_string(),
            last_name: "User".to_string(),
            is_active: true,
            is_staff: false,
            is_system_admin: false,
            is_guest: false,
            must_change_password: false,
            email_notifications_enabled: true,
        };
        assert!(should_send_email_to(&user_enabled));
    }

    #[test]
    fn should_not_send_email_to_user_with_disabled_notifications() {
        let user_disabled = User {
            id: 2,
            username: "disabled".to_string(),
            password_hash: "hash".to_string(),
            email: "disabled@example.com".to_string(),
            display_name: "Disabled User".to_string(),
            alias: None,
            first_name: "D".to_string(),
            last_name: "User".to_string(),
            is_active: true,
            is_staff: false,
            is_system_admin: false,
            is_guest: false,
            must_change_password: false,
            email_notifications_enabled: false,
        };
        assert!(!should_send_email_to(&user_disabled));
    }

    #[test]
    fn should_not_send_email_to_user_without_email() {
        let user_no_email = User {
            id: 3,
            username: "noemail".to_string(),
            password_hash: "hash".to_string(),
            email: "".to_string(),
            display_name: "No Email User".to_string(),
            alias: None,
            first_name: "N".to_string(),
            last_name: "User".to_string(),
            is_active: true,
            is_staff: false,
            is_system_admin: false,
            is_guest: false,
            must_change_password: false,
            email_notifications_enabled: true,
        };
        assert!(!should_send_email_to(&user_no_email));
    }

    #[test]
    fn link_is_built_for_a_real_base_url_and_trims_trailing_slash() {
        assert_eq!(
            build_link("https://senn.example.com/", "/admin?section=ai").as_deref(),
            Some("https://senn.example.com/admin?section=ai")
        );
    }

    #[test]
    fn link_is_omitted_when_base_url_is_empty_or_local() {
        assert_eq!(build_link("", "/x"), None);
        assert_eq!(build_link("  ", "/x"), None);
        assert_eq!(build_link("http://localhost:8150", "/x"), None);
        assert_eq!(build_link("http://127.0.0.1:8150", "/x"), None);
    }

    #[test]
    fn bodies_carry_the_right_link_and_no_secrets() {
        let base = "https://senn.example.com";
        let req = request_body("アリス", base);
        assert!(req.contains("アリス さんから申請があります"));
        assert!(req.ends_with("https://senn.example.com/admin?section=ai"));
        assert!(reminder_body("アリス", base).ends_with("/admin?section=ai"));
        let appr = approval_body(base);
        assert!(appr.contains("7日以内"));
        assert!(appr.ends_with("https://senn.example.com/settings?tab=ai"));
        // リンクが作れない環境では、文章だけ(末尾に空行やリンクを付けない)
        assert!(!request_body("アリス", "http://localhost:8150").contains("http"));
        assert!(!approval_body("").contains("http"));
    }

    #[test]
    fn rejection_body_includes_the_reason_only_when_given() {
        assert_eq!(rejection_body("  "), "申請は却下されました。");
        assert_eq!(
            rejection_body("見送り"),
            "申請は却下されました。理由: 見送り"
        );
    }
}
