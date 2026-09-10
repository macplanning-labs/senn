/// domain/services/git_webhook_service.rs — Git Webhook 署名検証・チケットキー抽出・ステータス遷移
///
/// Django apps/integrations/services.py の GitWebhookService の移植。

use hmac::{Hmac, Mac};
use sha2::Sha256;
use regex::Regex;
use std::sync::OnceLock;
use sqlx::PgPool;

type HmacSha256 = Hmac<Sha256>;

/// GitHub Webhook の HMAC-SHA256 署名(X-Hub-Signature-256)を検証する。
pub fn verify_github_signature(payload_body: &[u8], signature_header: &str, secret: &str) -> bool {
    if signature_header.is_empty() {
        return false;
    }

    let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(payload_body);
    let expected = format!("sha256={}", hex_encode(&mac.finalize().into_bytes()));

    constant_time_eq(expected.as_bytes(), signature_header.as_bytes())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// タイミング攻撃を避けるための定数時間比較。
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn ticket_key_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"\b([A-Z]{2,10}-\d{1,6})\b").unwrap())
}

/// テキストからチケットキーを抽出する(出現順・重複排除)。
pub fn extract_ticket_keys(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for cap in ticket_key_pattern().captures_iter(text) {
        let key = cap[1].to_string();
        if seen.insert(key.clone()) {
            result.push(key);
        }
    }
    result
}

/// ステータス遷移の結果
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StatusTransitionResult {
    Applied,    // 遷移が適用された
    Skipped,    // 遷移がスキップされた（既に同じ状態など）
    NotChanged, // 変更なし
}

/// アクター候補の優先順位
#[derive(Debug)]
pub struct GitActor {
    pub user_id: i32,
    pub username: String,
}

/// Git イベント後のステータス遷移を試みる。
///
/// 単調性ガード付き。現在が completed/cancelled なら何もしない。
pub async fn apply_git_status_transition(
    pool: &PgPool,
    ticket_id: i32,
    target_slug: &str,
    actor_user_id: i32,
    trigger: &str, // "push" | "pr_merged"
) -> anyhow::Result<StatusTransitionResult> {
    use crate::infrastructure::repositories::ticket_repo;

    // 1. 現在のチケット情報を取得
    let ticket = match ticket_repo::find_by_id(pool, ticket_id).await? {
        Some(t) => t,
        None => return Ok(StatusTransitionResult::NotChanged),
    };

    let current_status = &ticket.status;
    if current_status == target_slug {
        return Ok(StatusTransitionResult::NotChanged);
    }

    let project_id = match ticket.project_id {
        Some(id) => id,
        None => {
            tracing::error!("git auto-status: ticket {} has no project_id", ticket_id);
            return Ok(StatusTransitionResult::Skipped);
        }
    };

    // 2. 現在の status から category を取得
    let current_category: Option<String> = sqlx::query_scalar(
        "SELECT category FROM t_workflow_status WHERE project_id = $1 AND slug = $2"
    )
    .bind(project_id)
    .bind(current_status)
    .fetch_optional(pool)
    .await?;

    // 単調性ガード：completed / cancelled からは戻さない
    if let Some(category) = &current_category {
        if category == "completed" || category == "cancelled" {
            tracing::debug!("git status skipped: ticket {} already in {} category", ticket_id, category);
            return Ok(StatusTransitionResult::Skipped);
        }
    }

    // 3. status 更新（api_patch 相当）
    let mut tx = pool.begin().await?;

    // ステータス更新
    sqlx::query(
        "UPDATE tickets_ticket SET status = $1, updated_at = NOW() WHERE id = $2"
    )
    .bind(target_slug)
    .bind(ticket_id)
    .execute(&mut *tx)
    .await?;

    // ステータス履歴に記録
    sqlx::query(
        "INSERT INTO tickets_status_history (old_status, new_status, changed_by_id, changed_at, ticket_id)
         VALUES ($1, $2, $3, NOW(), $4)"
    )
    .bind(current_status)
    .bind(target_slug)
    .bind(actor_user_id)
    .bind(ticket_id)
    .execute(&mut *tx)
    .await?;

    // 4. 目標が completed カテゴリ、またはフォールバック完了 slug なら closed_at をセット
    let target_category: Option<String> = sqlx::query_scalar(
        "SELECT category FROM t_workflow_status WHERE project_id = $1 AND slug = $2"
    )
    .bind(project_id)
    .bind(target_slug)
    .fetch_optional(&mut *tx)
    .await?;

    let should_close = matches!(target_category.as_deref(), Some("completed"))
        || target_slug == "closed"
        || target_slug == "resolved";

    if should_close {
        sqlx::query("UPDATE tickets_ticket SET closed_at = NOW() WHERE id = $1")
            .bind(ticket_id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;

    tracing::info!(
        "git auto-status: ticket={} from={} to={} trigger={}",
        ticket_id, current_status, target_slug, trigger
    );

    Ok(StatusTransitionResult::Applied)
}

/// Git integration の actor を解決する。
///
/// 優先順位:
/// 1. integration.created_by_id
/// 2. project.owner_id
/// 3. project membership（accounts_user.is_staff 優先）
pub async fn resolve_git_actor(
    pool: &PgPool,
    integration_created_by_id: Option<i32>,
    project_id: i32,
) -> anyhow::Result<Option<GitActor>> {
    // 1. integration.created_by_id
    if let Some(user_id) = integration_created_by_id {
        if let Some(username) = sqlx::query_scalar::<_, String>(
            "SELECT username FROM accounts_user WHERE id = $1"
        )
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        {
            return Ok(Some(GitActor { user_id, username }));
        }
    }

    // 2. project.owner_id
    let owner: Option<(i32, String)> = sqlx::query_as(
        "SELECT p.owner_id::int4, u.username
         FROM tickets_project p
         JOIN accounts_user u ON p.owner_id = u.id
         WHERE p.id = $1 AND p.owner_id IS NOT NULL"
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await?;

    if let Some((user_id, username)) = owner {
        return Ok(Some(GitActor { user_id, username }));
    }

    // 3. project membership（is_staff 優先）
    let member: Option<(i32, String)> = sqlx::query_as(
        "SELECT m.user_id::int4, u.username
         FROM tickets_project_membership m
         JOIN accounts_user u ON u.id = m.user_id
         WHERE m.project_id = $1
         ORDER BY u.is_staff DESC, m.user_id ASC
         LIMIT 1"
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await?;

    if let Some((user_id, username)) = member {
        return Ok(Some(GitActor { user_id, username }));
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_ticket_keys_simple() {
        let text = "Implement WIP-123 feature";
        let keys = extract_ticket_keys(text);
        assert_eq!(keys, vec!["WIP-123"]);
    }

    #[test]
    fn test_extract_ticket_keys_multiple() {
        let text = "Fix WIP-123 and WIP-456 issues";
        let keys = extract_ticket_keys(text);
        assert_eq!(keys, vec!["WIP-123", "WIP-456"]);
    }

    #[test]
    fn test_extract_ticket_keys_with_branch() {
        let branch = "feature/WIP-789";
        let keys = extract_ticket_keys(branch);
        assert_eq!(keys, vec!["WIP-789"]);
    }

    #[test]
    fn test_extract_ticket_keys_dedup() {
        let text = "Fix WIP-123 in WIP-123 again";
        let keys = extract_ticket_keys(text);
        assert_eq!(keys, vec!["WIP-123"]);
    }

    #[test]
    fn test_extract_ticket_keys_empty() {
        let text = "";
        let keys = extract_ticket_keys(text);
        assert_eq!(keys, Vec::<String>::new());
    }

    #[test]
    fn test_extract_ticket_keys_no_match() {
        let text = "Just a regular commit message";
        let keys = extract_ticket_keys(text);
        assert!(keys.is_empty());
    }

    #[test]
    fn test_extract_ticket_keys_boundaries() {
        let text = "TICKET-1 and (TICKET-2) and TICKET-3x";
        let keys = extract_ticket_keys(text);
        assert_eq!(keys, vec!["TICKET-1", "TICKET-2"]);
    }

    #[test]
    fn test_extract_ticket_keys_various_prefixes() {
        let text = "A-1 AB-2 ABC-3 ABCDEFGHIJ-1 ABCDEFGHIJK-1";
        let keys = extract_ticket_keys(text);
        assert_eq!(keys, vec!["AB-2", "ABC-3", "ABCDEFGHIJ-1"]);
    }

    #[tokio::test]
    async fn test_apply_git_status_transition_completed_monotonic_guard() {
        let Some(pool) = crate::test_support::test_pool().await else {
            return; // DB 無しではスキップ
        };

        use crate::test_support::*;
        let user_id = create_test_user(&pool, "git_test_user").await;
        let project_id = create_test_project(&pool, "git_test_proj", user_id).await;

        // テスト用ワークフローステータスを作成：completed と started
        sqlx::query(
            r#"
            INSERT INTO t_workflow_status (project_id, slug, category, label, position)
            VALUES ($1, 'completed', 'completed', 'Completed', 100)
            "#,
        )
        .bind(project_id)
        .execute(&pool)
        .await
        .expect("completed status 作成失敗");

        sqlx::query(
            r#"
            INSERT INTO t_workflow_status (project_id, slug, category, label, position)
            VALUES ($1, 'started', 'started', 'Started', 10)
            "#,
        )
        .bind(project_id)
        .execute(&pool)
        .await
        .expect("started status 作成失敗");

        // テストチケットを作成し、status を completed に設定
        let ticket_id = create_test_ticket(&pool, project_id, "GITM", user_id).await;
        sqlx::query("UPDATE tickets_ticket SET status = 'completed' WHERE id = $1")
            .bind(ticket_id)
            .execute(&pool)
            .await
            .expect("チケット status 更新失敗");

        // 単調性ガード：completed から started への遷移を試みる → Skipped になるはず
        let result = apply_git_status_transition(&pool, ticket_id, "started", user_id, "push")
            .await
            .expect("apply_git_status_transition 実行失敗");

        assert_eq!(
            result,
            StatusTransitionResult::Skipped,
            "completed status のチケットへの push は Skipped になるべき"
        );

        // チケットの status が変わっていないことを確認
        let current_status: String = sqlx::query_scalar(
            "SELECT status FROM tickets_ticket WHERE id = $1"
        )
        .bind(ticket_id)
        .fetch_one(&pool)
        .await
        .expect("チケット取得失敗");

        assert_eq!(
            current_status, "completed",
            "completed status は変わらないべき"
        );
    }

    #[tokio::test]
    async fn test_apply_git_status_transition_cancelled_monotonic_guard() {
        let Some(pool) = crate::test_support::test_pool().await else {
            return; // DB 無しではスキップ
        };

        use crate::test_support::*;
        let user_id = create_test_user(&pool, "git_test_user2").await;
        let project_id = create_test_project(&pool, "git_test_proj2", user_id).await;

        // テスト用ワークフローステータスを作成：cancelled と started
        sqlx::query(
            r#"
            INSERT INTO t_workflow_status (project_id, slug, category, label, position)
            VALUES ($1, 'cancelled', 'cancelled', 'Cancelled', 100)
            "#,
        )
        .bind(project_id)
        .execute(&pool)
        .await
        .expect("cancelled status 作成失敗");

        sqlx::query(
            r#"
            INSERT INTO t_workflow_status (project_id, slug, category, label, position)
            VALUES ($1, 'started', 'started', 'Started', 10)
            "#,
        )
        .bind(project_id)
        .execute(&pool)
        .await
        .expect("started status 作成失敗");

        // テストチケットを作成し、status を cancelled に設定
        let ticket_id = create_test_ticket(&pool, project_id, "GITC", user_id).await;
        sqlx::query("UPDATE tickets_ticket SET status = 'cancelled' WHERE id = $1")
            .bind(ticket_id)
            .execute(&pool)
            .await
            .expect("チケット status 更新失敗");

        // 単調性ガード：cancelled から started への遷移を試みる → Skipped になるはず
        let result = apply_git_status_transition(&pool, ticket_id, "started", user_id, "push")
            .await
            .expect("apply_git_status_transition 実行失敗");

        assert_eq!(
            result,
            StatusTransitionResult::Skipped,
            "cancelled status のチケットへの push は Skipped になるべき"
        );

        // チケットの status が変わっていないことを確認
        let current_status: String = sqlx::query_scalar(
            "SELECT status FROM tickets_ticket WHERE id = $1"
        )
        .bind(ticket_id)
        .fetch_one(&pool)
        .await
        .expect("チケット取得失敗");

        assert_eq!(
            current_status, "cancelled",
            "cancelled status は変わらないべき"
        );
    }

    #[tokio::test]
    async fn test_apply_git_status_transition_no_change_same_slug() {
        let Some(pool) = crate::test_support::test_pool().await else {
            return; // DB 無しではスキップ
        };

        use crate::test_support::*;
        let user_id = create_test_user(&pool, "git_test_user3").await;
        let project_id = create_test_project(&pool, "git_test_proj3", user_id).await;

        // テスト用ワークフローステータスを作成
        sqlx::query(
            r#"
            INSERT INTO t_workflow_status (project_id, slug, category, label, position)
            VALUES ($1, 'started', 'started', 'Started', 10)
            "#,
        )
        .bind(project_id)
        .execute(&pool)
        .await
        .expect("started status 作成失敗");

        // テストチケットを作成し、status を started に設定
        let ticket_id = create_test_ticket(&pool, project_id, "GITN", user_id).await;
        sqlx::query("UPDATE tickets_ticket SET status = 'started' WHERE id = $1")
            .bind(ticket_id)
            .execute(&pool)
            .await
            .expect("チケット status 更新失敗");

        // 同一 slug への遷移を試みる → NotChanged になるはず
        let result = apply_git_status_transition(&pool, ticket_id, "started", user_id, "push")
            .await
            .expect("apply_git_status_transition 実行失敗");

        assert_eq!(
            result,
            StatusTransitionResult::NotChanged,
            "同一 slug への呼び出しは NotChanged になるべき"
        );
    }
}
