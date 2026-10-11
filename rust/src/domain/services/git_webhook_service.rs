use crate::infrastructure::repositories::workflow_status_repo;
/// domain/services/git_webhook_service.rs — Git Webhook 署名検証・チケットキー抽出・ステータス遷移
///
/// Django apps/integrations/services.py の GitWebhookService の移植。
use hmac::{Hmac, Mac};
use regex::Regex;
use sha2::Sha256;
use sqlx::PgPool;
use std::sync::OnceLock;

type HmacSha256 = Hmac<Sha256>;

/// GitHub Webhook の HMAC-SHA256 署名(X-Hub-Signature-256)を検証する。
pub fn verify_github_signature(payload_body: &[u8], signature_header: &str, secret: &str) -> bool {
    // 空のシークレット(App 連携の行など)で署名を通さない
    if signature_header.is_empty() || secret.is_empty() {
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

/// チケット番号を求めないブランチ名(タグは別に判定)。開発者の手元の pre-push(scripts/lib/ticket_key_check.sh)の対象外と同じ一覧。
fn is_exempt_branch(name: &str) -> bool {
    matches!(name, "main" | "develop")
        || ["release/", "deploy/", "dependabot/", "renovate/"]
            .iter()
            .any(|p| name.starts_with(p))
}

/// 番号が無いのが正常な受信(タグの push、`main` / `develop` などへの push、それらから出た PR)なら、画面に出す説明を返す。
/// 番号が見つからないときに、警告(no_ticket)ではなく「対象外」として記録するために使う(DEMO-000183)。
pub fn untracked_source_note(event_type: &str, payload: &serde_json::Value) -> Option<String> {
    match event_type {
        "push" => {
            let full = payload.get("ref").and_then(|v| v.as_str())?;
            if full.starts_with("refs/tags/") {
                return Some("タグの push は、チケット番号が無くても警告しません".to_string());
            }
            let branch = full.trim_start_matches("refs/heads/");
            is_exempt_branch(branch)
                .then(|| format!("{branch} への push は、チケット番号が無くても警告しません"))
        }
        "pull_request" => {
            let head = payload
                .get("pull_request")
                .and_then(|p| p.get("head"))
                .and_then(|h| h.get("ref"))
                .and_then(|v| v.as_str())?;
            is_exempt_branch(head)
                .then(|| format!("{head} から出た PR は、チケット番号が無くても警告しません"))
        }
        _ => None,
    }
}

/// Webhook ペイロードから、紐付けの対象になりうるチケットキー(意図つき)を集める(診断用。副作用なし)。
/// push はコミットメッセージとブランチ名、pull_request はタイトル・本文・ブランチ名を見る。
pub fn scan_payload_keys(
    event_type: &str,
    payload: &serde_json::Value,
    known_prefixes: &std::collections::HashSet<String>,
) -> Vec<crate::domain::services::git_key_scan::FoundKey> {
    use crate::domain::services::git_key_scan::{scan_keys, KeySources};
    match event_type {
        "push" => {
            let branch = payload
                .get("ref")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim_start_matches("refs/heads/");
            let messages: Vec<&str> = payload
                .get("commits")
                .and_then(|v| v.as_array())
                .map(|cs| {
                    cs.iter()
                        .filter_map(|c| c.get("message").and_then(|v| v.as_str()))
                        .collect()
                })
                .unwrap_or_default();
            scan_keys(
                &KeySources {
                    branch,
                    title: "",
                    body: "",
                    commit_messages: &messages,
                },
                known_prefixes,
            )
        }
        "pull_request" => {
            let pr = payload.get("pull_request");
            let title = pr
                .and_then(|p| p.get("title"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let body = pr
                .and_then(|p| p.get("body"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let branch = pr
                .and_then(|p| p.get("head"))
                .and_then(|h| h.get("ref"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            scan_keys(
                &KeySources {
                    branch,
                    title,
                    body,
                    commit_messages: &[],
                },
                known_prefixes,
            )
        }
        _ => Vec::new(),
    }
}

/// pull_request イベントで動かすトリガー。
/// `action`: GitHub の action、`draft`: PR が Draft か、`pr_state`: open / merged / closed。
/// 標準の規則(`builtin_rule`)と合わせて、Phase 1 までの動作(開いたらレビュー・マージで完了)を保つ。
pub fn pr_triggers(action: &str, draft: bool, pr_state: &str) -> Vec<GitTrigger> {
    let mut v = Vec::new();
    match action {
        "opened" | "reopened" => {
            v.push(if draft {
                GitTrigger::PrDraft
            } else {
                GitTrigger::PrOpened
            });
        }
        "ready_for_review" if !draft => v.push(GitTrigger::PrReady),
        "converted_to_draft" => v.push(GitTrigger::PrDraft),
        _ => {}
    }
    // マージ済みなら完了、マージされずに閉じたなら「閉じた」
    if pr_state == "merged" {
        v.push(GitTrigger::PrMerged);
    } else if action == "closed" && pr_state == "closed" {
        v.push(GitTrigger::PrClosed);
    }
    v
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

    let project_id = ticket.project_id;
    let team_id: Option<i32> =
        sqlx::query_scalar("SELECT team_id::int4 FROM tickets_ticket WHERE id = $1")
            .bind(ticket_id)
            .fetch_optional(pool)
            .await?
            .flatten();

    // 2. 現在の status から category を取得（Project → Team → ワークスペース既定）
    let current_category =
        workflow_status_repo::lookup_status_category(pool, project_id, team_id, current_status)
            .await?;

    // 単調性ガード：completed / cancelled からは戻さない
    if let Some(category) = &current_category {
        if category == "completed" || category == "cancelled" {
            tracing::debug!(
                "git status skipped: ticket {} already in {} category",
                ticket_id,
                category
            );
            return Ok(StatusTransitionResult::Skipped);
        }
    }

    // 3. status 更新（api_patch 相当）
    let mut tx = pool.begin().await?;

    // ステータス更新
    sqlx::query("UPDATE tickets_ticket SET status = $1, updated_at = NOW() WHERE id = $2")
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

    // 4. 完了日時(closed_at)をステータスに合わせる(画面・API からの変更と同じ処理)
    ticket_repo::sync_closed_at(&mut *tx, ticket_id, current_status).await?;

    tx.commit().await?;

    tracing::info!(
        "git auto-status: ticket={} from={} to={} trigger={}",
        ticket_id,
        current_status,
        target_slug,
        trigger
    );

    Ok(StatusTransitionResult::Applied)
}

/// チケットの (project_id, team_id) を取る。
pub async fn ticket_scope(
    pool: &PgPool,
    ticket_id: i32,
) -> anyhow::Result<(Option<i32>, Option<i32>)> {
    let row: Option<(Option<i32>, Option<i32>)> =
        sqlx::query_as("SELECT project_id::int4, team_id::int4 FROM tickets_ticket WHERE id = $1")
            .bind(ticket_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.unwrap_or((None, None)))
}

/// チケットスコープで category 先頭 slug を解決（Team → ワークスペース既定）。無ければ None。
pub async fn resolve_target_slug_for_ticket(
    pool: &PgPool,
    ticket_id: i32,
    category: &str,
) -> anyhow::Result<Option<String>> {
    let (project_id, team_id) = ticket_scope(pool, ticket_id).await?;
    workflow_status_repo::resolve_status_slug_by_scope(pool, project_id, team_id, category).await
}

/// チケットスコープで review 相当 slug を解決。無ければ None（別カテゴリへ飛ばさない）。
pub async fn resolve_review_slug_for_ticket(
    pool: &PgPool,
    ticket_id: i32,
) -> anyhow::Result<Option<String>> {
    let (project_id, team_id) = ticket_scope(pool, ticket_id).await?;
    workflow_status_repo::resolve_review_status_slug_by_scope(pool, project_id, team_id).await
}

/// 規則のトリガー(設計: docs/design/詳細設計_GitHub連携Phase2.md §3.1)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GitTrigger {
    BranchPushed,
    PrOpened,
    PrDraft,
    PrReady,
    PrReviewApproved,
    PrChangesRequested,
    PrCiFailed,
    PrMerged,
    PrClosed,
}

impl GitTrigger {
    pub const ALL: [GitTrigger; 9] = [
        GitTrigger::BranchPushed,
        GitTrigger::PrOpened,
        GitTrigger::PrDraft,
        GitTrigger::PrReady,
        GitTrigger::PrReviewApproved,
        GitTrigger::PrChangesRequested,
        GitTrigger::PrCiFailed,
        GitTrigger::PrMerged,
        GitTrigger::PrClosed,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            GitTrigger::BranchPushed => "branch_pushed",
            GitTrigger::PrOpened => "pr_opened",
            GitTrigger::PrDraft => "pr_draft",
            GitTrigger::PrReady => "pr_ready",
            GitTrigger::PrReviewApproved => "pr_review_approved",
            GitTrigger::PrChangesRequested => "pr_changes_requested",
            GitTrigger::PrCiFailed => "pr_ci_failed",
            GitTrigger::PrMerged => "pr_merged",
            GitTrigger::PrClosed => "pr_closed",
        }
    }

    /// 通知の文言に使う、きっかけの説明
    pub fn label_ja(self) -> &'static str {
        match self {
            GitTrigger::BranchPushed => "push",
            GitTrigger::PrOpened => "PR の作成",
            GitTrigger::PrDraft => "Draft の PR",
            GitTrigger::PrReady => "PR のレビュー準備完了",
            GitTrigger::PrReviewApproved => "PR の承認",
            GitTrigger::PrChangesRequested => "PR への変更依頼",
            GitTrigger::PrCiFailed => "CI の失敗",
            GitTrigger::PrMerged => "PR のマージ",
            GitTrigger::PrClosed => "PR のクローズ",
        }
    }

    pub fn parse(s: &str) -> Option<GitTrigger> {
        GitTrigger::ALL.into_iter().find(|t| t.as_str() == s)
    }
}

/// 規則で選べる「動かす先のカテゴリ」。`review` は「レビュー相当」(in_review 等)を探す特別扱い。
pub const VALID_TARGET_CATEGORIES: [&str; 6] = [
    "backlog",
    "unstarted",
    "started",
    "review",
    "completed",
    "cancelled",
];

/// 標準の規則の動かす先
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinTarget {
    Nothing,
    Category(&'static str),
    Review,
}

/// 標準の規則。**Phase 1 までの動作と同じ**(何も設定しない環境で、動作を変えないため)。
pub fn builtin_rule(trigger: GitTrigger) -> BuiltinTarget {
    match trigger {
        GitTrigger::BranchPushed => BuiltinTarget::Category("started"),
        GitTrigger::PrOpened | GitTrigger::PrReady => BuiltinTarget::Review,
        GitTrigger::PrMerged => BuiltinTarget::Category("completed"),
        GitTrigger::PrDraft
        | GitTrigger::PrReviewApproved
        | GitTrigger::PrChangesRequested
        | GitTrigger::PrCiFailed
        | GitTrigger::PrClosed => BuiltinTarget::Nothing,
    }
}

/// 画面表示用: 標準の規則を「保存された規則と同じ形」(action, target_category)で返す。
pub fn builtin_as_stored(trigger: GitTrigger) -> (&'static str, Option<&'static str>) {
    match builtin_rule(trigger) {
        BuiltinTarget::Nothing => ("none", None),
        BuiltinTarget::Category(c) => ("set", Some(c)),
        BuiltinTarget::Review => ("set", Some("review")),
    }
}

/// 規則を解決した結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleResolution {
    /// このスロッグへ動かす
    Set(String),
    /// 何もしない(標準で何もしない、または「何もしない」を選んだ)
    Nothing,
    /// 動かす先が、このチケットの流れに無い(カテゴリ名またはスロッグを持つ)
    MissingStatus(String),
}

async fn resolve_category_slug(
    pool: &PgPool,
    project_id: Option<i32>,
    team_id: Option<i32>,
    category: &str,
) -> anyhow::Result<RuleResolution> {
    let slug = if category == "review" {
        workflow_status_repo::resolve_review_status_slug_by_scope(pool, project_id, team_id).await?
    } else {
        workflow_status_repo::resolve_status_slug_by_scope(pool, project_id, team_id, category)
            .await?
    };
    Ok(match slug {
        Some(s) => RuleResolution::Set(s),
        None => RuleResolution::MissingStatus(category.to_string()),
    })
}

/// チケットに対して、トリガーで動かす先を決める。
/// プロジェクトの規則 → チームの規則 → 標準、の順。動かす先が流れに無ければ MissingStatus。
pub async fn resolve_trigger_target(
    pool: &PgPool,
    ticket_id: i32,
    trigger: GitTrigger,
) -> anyhow::Result<RuleResolution> {
    use crate::infrastructure::repositories::git_status_rule_repo;
    let (project_id, team_id) = ticket_scope(pool, ticket_id).await?;
    let stored =
        git_status_rule_repo::find_effective_rule(pool, project_id, team_id, trigger.as_str())
            .await?;
    match stored {
        Some(rule) if rule.action == "none" => Ok(RuleResolution::Nothing),
        Some(rule) => {
            if let Some(slug) = rule.target_slug {
                if workflow_status_repo::status_slug_exists_for_scope(
                    pool, project_id, team_id, &slug,
                )
                .await?
                {
                    Ok(RuleResolution::Set(slug))
                } else {
                    Ok(RuleResolution::MissingStatus(slug))
                }
            } else if let Some(category) = rule.target_category {
                resolve_category_slug(pool, project_id, team_id, &category).await
            } else {
                Ok(RuleResolution::Nothing)
            }
        }
        None => match builtin_rule(trigger) {
            BuiltinTarget::Nothing => Ok(RuleResolution::Nothing),
            BuiltinTarget::Category(c) => resolve_category_slug(pool, project_id, team_id, c).await,
            BuiltinTarget::Review => {
                resolve_category_slug(pool, project_id, team_id, "review").await
            }
        },
    }
}

/// Git integration の actor を解決する。
///
/// 優先順位:
/// 1. integration.created_by_id
/// 2. project.owner_id（Project 連携時）
/// 3. project / team membership（is_staff 優先）
pub async fn resolve_git_actor(
    pool: &PgPool,
    integration_created_by_id: Option<i32>,
    project_id: Option<i32>,
    team_id: Option<i32>,
) -> anyhow::Result<Option<GitActor>> {
    // 1. integration.created_by_id
    if let Some(user_id) = integration_created_by_id {
        if let Some(username) =
            sqlx::query_scalar::<_, String>("SELECT username FROM accounts_user WHERE id = $1")
                .bind(user_id)
                .fetch_optional(pool)
                .await?
        {
            return Ok(Some(GitActor { user_id, username }));
        }
    }

    // 2. project.owner_id
    if let Some(pid) = project_id {
        let owner: Option<(i32, String)> = sqlx::query_as(
            "SELECT p.owner_id::int4, u.username
             FROM tickets_project p
             JOIN accounts_user u ON p.owner_id = u.id
             WHERE p.id = $1 AND p.owner_id IS NOT NULL",
        )
        .bind(pid)
        .fetch_optional(pool)
        .await?;

        if let Some((user_id, username)) = owner {
            return Ok(Some(GitActor { user_id, username }));
        }

        // 3a. Projectゲスト(scoped_project_id一致、is_staff 優先)
        let member: Option<(i32, String)> = sqlx::query_as(
            "SELECT m.user_id::int4, u.username
             FROM t_team_membership m
             JOIN accounts_user u ON u.id = m.user_id
             WHERE m.scoped_project_id = $1
             ORDER BY u.is_staff DESC, m.user_id ASC
             LIMIT 1",
        )
        .bind(pid)
        .fetch_optional(pool)
        .await?;

        if let Some((user_id, username)) = member {
            return Ok(Some(GitActor { user_id, username }));
        }
    }

    // 3b. team membership（is_staff 優先）。L2: Projectゲスト(scoped_project_id付き)は
    // 「チーム全体メンバー」ではないため除外する
    if let Some(tid) = team_id {
        let member: Option<(i32, String)> = sqlx::query_as(
            "SELECT m.user_id::int4, u.username
             FROM t_team_membership m
             JOIN accounts_user u ON u.id = m.user_id
             WHERE m.team_id = $1 AND m.scoped_project_id IS NULL
             ORDER BY u.is_staff DESC, m.user_id ASC
             LIMIT 1",
        )
        .bind(tid)
        .fetch_optional(pool)
        .await?;

        if let Some((user_id, username)) = member {
            return Ok(Some(GitActor { user_id, username }));
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_ref(r: &str) -> serde_json::Value {
        serde_json::json!({ "ref": r })
    }

    fn pr_from(head: &str) -> serde_json::Value {
        serde_json::json!({ "pull_request": { "head": { "ref": head } } })
    }

    #[test]
    fn untracked_note_for_tag_and_integration_branches_pushes() {
        for r in [
            "refs/tags/deploy/production/20261007-140612",
            "refs/heads/main",
            "refs/heads/develop",
            "refs/heads/release/v1",
            "refs/heads/deploy/x",
            "refs/heads/dependabot/npm_and_yarn/x",
            "refs/heads/renovate/x",
        ] {
            assert!(
                untracked_source_note("push", &push_ref(r)).is_some(),
                "{r} は対象外のはず"
            );
        }
    }

    #[test]
    fn untracked_note_is_none_for_work_branches() {
        for r in [
            "refs/heads/fix/login-401-hang",
            "refs/heads/feat/github-app-default-name",
            "refs/heads/maintenance", // main で始まるだけの名前は対象外にしない
            "refs/heads/developer/x",
        ] {
            assert!(
                untracked_source_note("push", &push_ref(r)).is_none(),
                "{r} は警告の対象のはず"
            );
        }
    }

    #[test]
    fn untracked_note_for_pull_requests_from_integration_branches() {
        assert!(untracked_source_note("pull_request", &pr_from("develop")).is_some()); // リリース PR
        assert!(untracked_source_note("pull_request", &pr_from("release/x")).is_some());
        assert!(untracked_source_note("pull_request", &pr_from("fix/x")).is_none());
        // 対象外のイベントや、ペイロードに ref が無いときは None
        assert!(untracked_source_note("check_run", &push_ref("refs/heads/main")).is_none());
        assert!(untracked_source_note("push", &serde_json::json!({})).is_none());
    }

    #[test]
    fn builtin_rules_match_phase1_behavior() {
        // 何も設定しない環境で、動作が変わらないことを固定する
        use BuiltinTarget::*;
        assert_eq!(builtin_rule(GitTrigger::BranchPushed), Category("started"));
        assert_eq!(builtin_rule(GitTrigger::PrOpened), Review);
        assert_eq!(builtin_rule(GitTrigger::PrReady), Review);
        assert_eq!(builtin_rule(GitTrigger::PrMerged), Category("completed"));
        for t in [
            GitTrigger::PrDraft,
            GitTrigger::PrReviewApproved,
            GitTrigger::PrChangesRequested,
            GitTrigger::PrCiFailed,
            GitTrigger::PrClosed,
        ] {
            assert_eq!(builtin_rule(t), Nothing, "{t:?}");
        }
    }

    #[test]
    fn trigger_names_round_trip() {
        for t in GitTrigger::ALL {
            assert_eq!(GitTrigger::parse(t.as_str()), Some(t));
        }
        assert_eq!(GitTrigger::parse("nope"), None);
    }

    #[test]
    fn test_verify_signature_rejects_empty_secret() {
        // 空のシークレットで計算した署名でも通してはならない
        let body = b"{}";
        let mut mac = HmacSha256::new_from_slice(b"").unwrap();
        mac.update(body);
        let sig = format!("sha256={}", hex_encode(&mac.finalize().into_bytes()));
        assert!(!verify_github_signature(body, &sig, ""));
    }

    #[test]
    fn scan_payload_keys_push_and_pr() {
        let prefixes: std::collections::HashSet<String> = ["WAPP", "SENN", "OPS"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let push = serde_json::json!({
            "ref": "refs/heads/feat/wapp-12-x",
            "commits": [{"message": "Fix [SENN-3]"}, {"message": "no key"}]
        });
        let keys: Vec<String> = scan_payload_keys("push", &push, &prefixes)
            .into_iter()
            .map(|f| f.key)
            .collect();
        assert_eq!(keys, vec!["WAPP-12", "SENN-3"]);
        let pr = serde_json::json!({
            "pull_request": {"title": "t", "body": "Closes OPS-9", "head": {"ref": "main"}}
        });
        let found = scan_payload_keys("pull_request", &pr, &prefixes);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].key, "OPS-9");
        assert!(scan_payload_keys("ping", &pr, &prefixes).is_empty());
    }

    #[test]
    fn pr_triggers_table() {
        use GitTrigger::*;
        // (action, draft, pr_state) -> トリガー
        let cases: Vec<((&str, bool, &str), Vec<GitTrigger>)> = vec![
            (("opened", false, "open"), vec![PrOpened]),
            (("reopened", false, "open"), vec![PrOpened]),
            (("opened", true, "open"), vec![PrDraft]),
            (("ready_for_review", false, "open"), vec![PrReady]),
            (("converted_to_draft", true, "open"), vec![PrDraft]),
            (("closed", false, "merged"), vec![PrMerged]),
            (("closed", false, "closed"), vec![PrClosed]),
            (("synchronize", false, "open"), vec![]),
            (("edited", false, "open"), vec![]),
        ];
        for ((action, draft, state), want) in cases {
            assert_eq!(
                pr_triggers(action, draft, state),
                want,
                "{action} draft={draft} {state}"
            );
        }
    }

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
            INSERT INTO t_workflow_status (project_id, slug, category, name, color, position, is_default)
            VALUES ($1, 'completed', 'completed', 'Completed', '#10b981', 100, false)
            "#,
        )
        .bind(project_id)
        .execute(&pool)
        .await
        .expect("completed status 作成失敗");

        sqlx::query(
            r#"
            INSERT INTO t_workflow_status (project_id, slug, category, name, color, position, is_default)
            VALUES ($1, 'started', 'started', 'Started', '#3b82f6', 10, false)
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
        let current_status: String =
            sqlx::query_scalar("SELECT status FROM tickets_ticket WHERE id = $1")
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
            INSERT INTO t_workflow_status (project_id, slug, category, name, color, position, is_default)
            VALUES ($1, 'cancelled', 'cancelled', 'Cancelled', '#ef4444', 100, false)
            "#,
        )
        .bind(project_id)
        .execute(&pool)
        .await
        .expect("cancelled status 作成失敗");

        sqlx::query(
            r#"
            INSERT INTO t_workflow_status (project_id, slug, category, name, color, position, is_default)
            VALUES ($1, 'started', 'started', 'Started', '#3b82f6', 10, false)
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
        let current_status: String =
            sqlx::query_scalar("SELECT status FROM tickets_ticket WHERE id = $1")
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
            INSERT INTO t_workflow_status (project_id, slug, category, name, color, position, is_default)
            VALUES ($1, 'started', 'started', 'Started', '#3b82f6', 10, false)
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

    async fn insert_status(pool: &PgPool, project_id: i32, slug: &str, category: &str, pos: i32) {
        sqlx::query(
            "INSERT INTO t_workflow_status (project_id, slug, category, name, color, position, is_default)
             VALUES ($1, $2, $3, $2, '#888888', $4, false)",
        )
        .bind(project_id)
        .bind(slug)
        .bind(category)
        .bind(pos)
        .execute(pool)
        .await
        .expect("status 作成失敗");
    }

    async fn set_rule(
        pool: &PgPool,
        project: Option<i32>,
        team: Option<i32>,
        trigger: GitTrigger,
        action: &str,
        slug: Option<&str>,
        category: Option<&str>,
    ) {
        sqlx::query(
            "INSERT INTO t_git_status_rule (project_id, team_id, trigger, action, target_slug, target_category)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(project)
        .bind(team)
        .bind(trigger.as_str())
        .bind(action)
        .bind(slug)
        .bind(category)
        .execute(pool)
        .await
        .expect("規則の作成失敗");
    }

    #[tokio::test]
    async fn resolve_trigger_target_follows_project_then_team_then_builtin() {
        let Some(pool) = crate::test_support::test_pool().await else {
            return;
        };
        use crate::test_support::*;
        let user = create_test_user(&pool, "rule_user").await;
        let project = create_test_project(&pool, "RULP", user).await;
        insert_status(&pool, project, "todo", "unstarted", 0).await;
        insert_status(&pool, project, "doing", "started", 10).await;
        insert_status(&pool, project, "in_review", "started", 20).await;
        insert_status(&pool, project, "done", "completed", 30).await;
        let ticket = create_test_ticket(&pool, project, "RULT", user).await;
        let (_, team) = ticket_scope(&pool, ticket).await.unwrap();
        let team = team.expect("チケットにチームがある");

        // 1. 規則が無ければ、標準(Phase 1 までの動作)
        let r = |t| resolve_trigger_target(&pool, ticket, t);
        assert_eq!(
            r(GitTrigger::BranchPushed).await.unwrap(),
            RuleResolution::Set("doing".into())
        );
        assert_eq!(
            r(GitTrigger::PrOpened).await.unwrap(),
            RuleResolution::Set("in_review".into())
        );
        assert_eq!(
            r(GitTrigger::PrMerged).await.unwrap(),
            RuleResolution::Set("done".into())
        );
        assert_eq!(
            r(GitTrigger::PrDraft).await.unwrap(),
            RuleResolution::Nothing
        );
        assert_eq!(
            r(GitTrigger::PrClosed).await.unwrap(),
            RuleResolution::Nothing
        );

        // 2. チームの規則: Draft は「着手中」へ(スロッグ指定)、閉じたら「Todo」へ(カテゴリ指定)
        set_rule(
            &pool,
            None,
            Some(team),
            GitTrigger::PrDraft,
            "set",
            Some("doing"),
            None,
        )
        .await;
        set_rule(
            &pool,
            None,
            Some(team),
            GitTrigger::PrClosed,
            "set",
            None,
            Some("unstarted"),
        )
        .await;
        assert_eq!(
            r(GitTrigger::PrDraft).await.unwrap(),
            RuleResolution::Set("doing".into())
        );
        assert_eq!(
            r(GitTrigger::PrClosed).await.unwrap(),
            RuleResolution::Set("todo".into())
        );

        // 3. プロジェクトの規則が、チームの規則より優先される(「何もしない」も選べる)
        set_rule(
            &pool,
            Some(project),
            None,
            GitTrigger::PrDraft,
            "none",
            None,
            None,
        )
        .await;
        assert_eq!(
            r(GitTrigger::PrDraft).await.unwrap(),
            RuleResolution::Nothing
        );
        // 標準で動くものを、規則で「何もしない」にもできる
        set_rule(
            &pool,
            Some(project),
            None,
            GitTrigger::PrMerged,
            "none",
            None,
            None,
        )
        .await;
        assert_eq!(
            r(GitTrigger::PrMerged).await.unwrap(),
            RuleResolution::Nothing
        );

        // 4. 動かす先が、このチケットの流れに無ければ MissingStatus(動かさない)
        set_rule(
            &pool,
            Some(project),
            None,
            GitTrigger::PrReviewApproved,
            "set",
            Some("nope"),
            None,
        )
        .await;
        assert_eq!(
            r(GitTrigger::PrReviewApproved).await.unwrap(),
            RuleResolution::MissingStatus("nope".into())
        );

        // 5. カテゴリ review は「レビュー相当」を探す
        set_rule(
            &pool,
            Some(project),
            None,
            GitTrigger::PrChangesRequested,
            "set",
            None,
            Some("review"),
        )
        .await;
        assert_eq!(
            r(GitTrigger::PrChangesRequested).await.unwrap(),
            RuleResolution::Set("in_review".into())
        );
    }
}
