/// infrastructure/repositories/integration_repo.rs — Git連携永続化
///
/// t_git_integration / t_git_event の CRUD + Webhook処理。
use sqlx::{PgPool, Row};

use crate::domain::models::integration_api::*;

const SELECT_BASE: &str = "
    SELECT
        gi.id::int4, gi.project_id::int4, gi.team_id::int4, gi.provider, gi.repository_url,
        gi.webhook_secret, gi.is_active, gi.auto_status_transition, gi.created_at,
        cb.id::int4 as cb_id, cb.username as cb_username, cb.email as cb_email, cb.display_name as cb_display_name,
        (SELECT COUNT(*) FROM t_git_event WHERE integration_id = gi.id)::int8 as event_count,
        gi.installation_id::int8, gi.last_delivery_at, gi.last_delivery_event, gi.last_delivery_result, gi.last_delivery_note
     FROM t_git_integration gi
     LEFT JOIN accounts_user cb ON gi.created_by_id = cb.id
";

fn row_to_integration(row: &sqlx::postgres::PgRow) -> GitIntegrationOut {
    let cb_id: Option<i32> = row.get("cb_id");
    GitIntegrationOut {
        id: row.get("id"),
        project: row.get("project_id"),
        team: row.get("team_id"),
        provider: row.get("provider"),
        repository_url: row.get("repository_url"),
        webhook_secret: row.get("webhook_secret"),
        is_active: row.get("is_active"),
        auto_status_transition: row.get("auto_status_transition"),
        created_by: cb_id.map(|_| UserSummaryOut {
            id: row.get("cb_id"),
            username: row.get("cb_username"),
            email: row.get("cb_email"),
            display_name: row.get("cb_display_name"),
        }),
        created_at: row.get("created_at"),
        event_count: row.get("event_count"),
        installation_id: row.get("installation_id"),
        last_delivery_at: row.get("last_delivery_at"),
        last_delivery_event: row.get("last_delivery_event"),
        last_delivery_result: row.get("last_delivery_result"),
        last_delivery_note: row.get("last_delivery_note"),
    }
}

fn scope_ok(project: Option<i32>, team: Option<i32>) -> bool {
    matches!((project, team), (Some(_), None) | (None, Some(_)))
}

pub async fn find_all(
    pool: &PgPool,
    project_id: Option<i32>,
    team_id: Option<i32>,
) -> anyhow::Result<Vec<GitIntegrationOut>> {
    let query = format!(
        "{SELECT_BASE}
         WHERE ($1::int4 IS NULL OR gi.project_id = $1)
           AND ($2::int4 IS NULL OR gi.team_id = $2)
         ORDER BY gi.created_at DESC"
    );
    let rows = sqlx::query(&query)
        .bind(project_id)
        .bind(team_id)
        .fetch_all(pool)
        .await?;
    Ok(rows.iter().map(row_to_integration).collect())
}

pub async fn find_by_id(pool: &PgPool, id: i32) -> anyhow::Result<Option<GitIntegrationOut>> {
    let query = format!("{SELECT_BASE} WHERE gi.id = $1");
    let row = sqlx::query(&query).bind(id).fetch_optional(pool).await?;
    Ok(row.map(|r| row_to_integration(&r)))
}

pub async fn find_by_url(
    pool: &PgPool,
    repository_url: &str,
) -> anyhow::Result<Option<GitIntegrationOut>> {
    let query = format!("{SELECT_BASE} WHERE gi.repository_url = $1");
    let row = sqlx::query(&query)
        .bind(repository_url)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| row_to_integration(&r)))
}

pub async fn create(
    pool: &PgPool,
    input: &GitIntegrationWriteIn,
    created_by: i32,
) -> anyhow::Result<i32> {
    if !scope_ok(input.project, input.team) {
        anyhow::bail!("exactly one of project or team is required");
    }
    let id: i32 = sqlx::query_scalar(
        "INSERT INTO t_git_integration
            (project_id, team_id, provider, repository_url, webhook_secret, is_active, auto_status_transition, created_by_id, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW())
         RETURNING id::int4"
    )
    .bind(input.project)
    .bind(input.team)
    .bind(&input.provider)
    .bind(&input.repository_url)
    .bind(&input.webhook_secret)
    .bind(input.is_active)
    .bind(input.auto_status_transition)
    .bind(created_by)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub async fn update(
    pool: &PgPool,
    id: i32,
    input: &GitIntegrationUpdateIn,
) -> anyhow::Result<bool> {
    let existing = match find_by_id(pool, id).await? {
        Some(e) => e,
        None => return Ok(false),
    };

    let project = if input.project.is_some() || input.team.is_some() {
        input.project
    } else {
        existing.project
    };
    let team = if input.project.is_some() || input.team.is_some() {
        input.team
    } else {
        existing.team
    };
    if !scope_ok(project, team) {
        anyhow::bail!("exactly one of project or team is required");
    }

    let provider = input.provider.clone().unwrap_or(existing.provider);
    let repository_url = input
        .repository_url
        .clone()
        .unwrap_or(existing.repository_url);
    let webhook_secret = input
        .webhook_secret
        .clone()
        .unwrap_or(existing.webhook_secret);
    let is_active = input.is_active.unwrap_or(existing.is_active);
    let auto_status_transition = input
        .auto_status_transition
        .unwrap_or(existing.auto_status_transition);

    let rows_affected = sqlx::query(
        "UPDATE t_git_integration
         SET project_id = $1, team_id = $2, provider = $3, repository_url = $4,
             webhook_secret = $5, is_active = $6, auto_status_transition = $7
         WHERE id = $8",
    )
    .bind(project)
    .bind(team)
    .bind(&provider)
    .bind(&repository_url)
    .bind(&webhook_secret)
    .bind(is_active)
    .bind(auto_status_transition)
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected();

    Ok(rows_affected > 0)
}

pub async fn delete(pool: &PgPool, id: i32) -> anyhow::Result<bool> {
    let mut tx = pool.begin().await?;

    sqlx::query("DELETE FROM t_git_event WHERE integration_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    let rows_affected = sqlx::query("DELETE FROM t_git_integration WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();

    tx.commit().await?;
    Ok(rows_affected > 0)
}

pub struct ActiveIntegration {
    pub id: i32,
    pub webhook_secret: String,
}

pub async fn find_active_integrations_by_repo_url(
    pool: &PgPool,
    repo_url: &str,
) -> anyhow::Result<Vec<ActiveIntegration>> {
    let rows = sqlx::query(
        "SELECT id::int4, webhook_secret FROM t_git_integration WHERE repository_url = $1 AND is_active = true"
    )
    .bind(repo_url)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| ActiveIntegration {
            id: r.get("id"),
            webhook_secret: r.get("webhook_secret"),
        })
        .collect())
}

pub async fn find_ticket_id_by_key(pool: &PgPool, ticket_key: &str) -> anyhow::Result<Option<i32>> {
    if let Some(id) =
        crate::infrastructure::repositories::ticket_repo::resolve_ticket_id(pool, ticket_key)
            .await?
    {
        return Ok(Some(id));
    }

    if let Some((prefix, num_str)) = ticket_key.split_once('-') {
        if let Ok(num) = num_str.parse::<u32>() {
            let padded_key = format!("{}-{:06}", prefix, num);
            if padded_key != ticket_key {
                return crate::infrastructure::repositories::ticket_repo::resolve_ticket_id(
                    pool,
                    &padded_key,
                )
                .await;
            }
        }
    }

    Ok(None)
}

pub async fn create_commit_event_if_new(
    pool: &PgPool,
    integration_id: i32,
    ticket_id: i32,
    sha: &str,
    title: &str,
    url: &str,
    branch: &str,
    author_name: &str,
) -> anyhow::Result<bool> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM t_git_event WHERE integration_id = $1 AND ticket_id = $2 AND sha = $3)"
    )
    .bind(integration_id)
    .bind(ticket_id)
    .bind(sha)
    .fetch_one(pool)
    .await?;

    if exists {
        return Ok(false);
    }

    sqlx::query(
        "INSERT INTO t_git_event
            (integration_id, ticket_id, event_type, title, url, sha, branch, author_name,
             author_avatar_url, pr_number, pr_state, created_at)
         VALUES ($1, $2, 'commit', $3, $4, $5, $6, $7, '', NULL, '', NOW())",
    )
    .bind(integration_id)
    .bind(ticket_id)
    .bind(title)
    .bind(url)
    .bind(sha)
    .bind(branch)
    .bind(author_name)
    .execute(pool)
    .await?;

    Ok(true)
}

#[allow(clippy::too_many_arguments)]
/// PR の詳細(Phase 2)。`pull_request` イベントのたびに最新の値で更新する。
#[derive(Debug, Clone, Default)]
pub struct PrDetails {
    pub is_draft: bool,
    pub reviewers: Vec<String>,
    pub additions: Option<i32>,
    pub deletions: Option<i32>,
    pub changed_files: Option<i32>,
    pub base_branch: String,
}

/// PR のイベントを保存する。`key_intent` は implicit | closing | reference。
/// 戻り値: 新しく作ったら true、既存を更新したら false。
#[allow(clippy::too_many_arguments)]
pub async fn upsert_pr_event(
    pool: &PgPool,
    integration_id: i32,
    ticket_id: i32,
    pr_number: i32,
    title: &str,
    url: &str,
    branch: &str,
    author_name: &str,
    author_avatar_url: &str,
    pr_state: &str,
    details: &PrDetails,
    key_intent: &str,
) -> anyhow::Result<bool> {
    let existing_id: Option<i32> = sqlx::query_scalar(
        "SELECT id::int4 FROM t_git_event
         WHERE integration_id = $1 AND ticket_id = $2 AND pr_number = $3 AND event_type = 'pull_request'"
    )
    .bind(integration_id)
    .bind(ticket_id)
    .bind(pr_number)
    .fetch_optional(pool)
    .await?;

    if let Some(id) = existing_id {
        sqlx::query(
            "UPDATE t_git_event SET title = $1, url = $2, branch = $3, author_name = $4,
                author_avatar_url = $5, pr_state = $6,
                is_draft = $8, reviewers = $9, additions = $10, deletions = $11,
                changed_files = $12, base_branch = $13, key_intent = $14
             WHERE id = $7",
        )
        .bind(title)
        .bind(url)
        .bind(branch)
        .bind(author_name)
        .bind(author_avatar_url)
        .bind(pr_state)
        .bind(id)
        .bind(details.is_draft)
        .bind(&details.reviewers)
        .bind(details.additions)
        .bind(details.deletions)
        .bind(details.changed_files)
        .bind(&details.base_branch)
        .bind(key_intent)
        .execute(pool)
        .await?;
        Ok(false)
    } else {
        sqlx::query(
            "INSERT INTO t_git_event
                (integration_id, ticket_id, event_type, title, url, sha, branch, author_name,
                 author_avatar_url, pr_number, pr_state, created_at,
                 is_draft, reviewers, additions, deletions, changed_files, base_branch, key_intent)
             VALUES ($1, $2, 'pull_request', $3, $4, '', $5, $6, $7, $8, $9, NOW(),
                     $10, $11, $12, $13, $14, $15, $16)",
        )
        .bind(integration_id)
        .bind(ticket_id)
        .bind(title)
        .bind(url)
        .bind(branch)
        .bind(author_name)
        .bind(author_avatar_url)
        .bind(pr_number)
        .bind(pr_state)
        .bind(details.is_draft)
        .bind(&details.reviewers)
        .bind(details.additions)
        .bind(details.deletions)
        .bind(details.changed_files)
        .bind(&details.base_branch)
        .bind(key_intent)
        .execute(pool)
        .await?;
        Ok(true)
    }
}

/// この PR に紐付いたチケットと、そのキーの書かれ方(implicit | closing | reference)。
pub async fn pr_event_tickets(
    pool: &PgPool,
    integration_id: i32,
    pr_number: i32,
) -> anyhow::Result<Vec<(i32, String)>> {
    let rows: Vec<(i32, String)> = sqlx::query_as(
        "SELECT ticket_id::int4, key_intent FROM t_git_event
         WHERE integration_id = $1 AND pr_number = $2 AND event_type = 'pull_request'",
    )
    .bind(integration_id)
    .bind(pr_number)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// レビュー結果を更新する。**変わったときだけ** Some(古い状態) を返す(同じ状態の再送では None)。
pub async fn set_pr_review_state(
    pool: &PgPool,
    integration_id: i32,
    pr_number: i32,
    new_state: &str,
) -> anyhow::Result<Option<String>> {
    let old: Option<String> = sqlx::query_scalar(
        "SELECT review_state FROM t_git_event
         WHERE integration_id = $1 AND pr_number = $2 AND event_type = 'pull_request' LIMIT 1",
    )
    .bind(integration_id)
    .bind(pr_number)
    .fetch_optional(pool)
    .await?;
    let Some(old) = old else { return Ok(None) };
    if old == new_state {
        return Ok(None);
    }
    sqlx::query(
        "UPDATE t_git_event SET review_state = $3
         WHERE integration_id = $1 AND pr_number = $2 AND event_type = 'pull_request'",
    )
    .bind(integration_id)
    .bind(pr_number)
    .bind(new_state)
    .execute(pool)
    .await?;
    Ok(Some(old))
}

/// チェックごとの最新の結果から、PR の CI の状態を決める(純粋関数)。
/// 失敗が 1 つでもあれば failure、実行中があれば pending、すべて成功(または対象外)なら success、無ければ none。
/// 失敗として扱う結論: failure / timed_out / action_required。cancelled / skipped / neutral は失敗に数えない。
pub fn aggregate_ci_state(checks: &[(String, Option<String>)]) -> &'static str {
    if checks.is_empty() {
        return "none";
    }
    let failed = checks.iter().any(|(status, conclusion)| {
        status == "completed"
            && matches!(
                conclusion.as_deref(),
                Some("failure") | Some("timed_out") | Some("action_required")
            )
    });
    if failed {
        return "failure";
    }
    if checks.iter().any(|(status, _)| status != "completed") {
        return "pending";
    }
    "success"
}

/// 1 つの PR に保存するチェックの上限(CI が大量に来ても、表が増え続けないように)
const MAX_CHECKS_PER_PR: i64 = 100;

/// チェックの最新の結果を保存する。上限を超える新しいチェックは保存しない。
pub async fn upsert_check_run(
    pool: &PgPool,
    integration_id: i32,
    pr_number: i32,
    name: &str,
    status: &str,
    conclusion: Option<&str>,
) -> anyhow::Result<()> {
    let name: String = name.chars().take(200).collect();
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM t_git_check_run WHERE integration_id = $1 AND pr_number = $2 AND name <> $3",
    )
    .bind(integration_id)
    .bind(pr_number)
    .bind(&name)
    .fetch_one(pool)
    .await?;
    if count >= MAX_CHECKS_PER_PR {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO t_git_check_run (integration_id, pr_number, name, status, conclusion, updated_at)
         VALUES ($1, $2, $3, $4, $5, now())
         ON CONFLICT (integration_id, pr_number, name)
         DO UPDATE SET status = EXCLUDED.status, conclusion = EXCLUDED.conclusion, updated_at = now()",
    )
    .bind(integration_id)
    .bind(pr_number)
    .bind(&name)
    .bind(status)
    .bind(conclusion)
    .execute(pool)
    .await?;
    Ok(())
}

/// PR の CI の状態を、保存済みのチェックから決め直して保存する。
/// **変わったときだけ** Some((古い状態, 新しい状態)) を返す。
pub async fn recompute_ci_state(
    pool: &PgPool,
    integration_id: i32,
    pr_number: i32,
) -> anyhow::Result<Option<(String, String)>> {
    let checks: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT status, conclusion FROM t_git_check_run WHERE integration_id = $1 AND pr_number = $2",
    )
    .bind(integration_id)
    .bind(pr_number)
    .fetch_all(pool)
    .await?;
    let new_state = aggregate_ci_state(&checks);
    let old: Option<String> = sqlx::query_scalar(
        "SELECT ci_state FROM t_git_event
         WHERE integration_id = $1 AND pr_number = $2 AND event_type = 'pull_request' LIMIT 1",
    )
    .bind(integration_id)
    .bind(pr_number)
    .fetch_optional(pool)
    .await?;
    let Some(old) = old else { return Ok(None) };
    if old == new_state {
        return Ok(None);
    }
    sqlx::query(
        "UPDATE t_git_event SET ci_state = $3
         WHERE integration_id = $1 AND pr_number = $2 AND event_type = 'pull_request'",
    )
    .bind(integration_id)
    .bind(pr_number)
    .bind(new_state)
    .execute(pool)
    .await?;
    Ok(Some((old, new_state.to_string())))
}

pub async fn find_events_by_ticket(
    pool: &PgPool,
    ticket_id: i32,
) -> anyhow::Result<Vec<GitEventOut>> {
    let rows = sqlx::query(
        "SELECT id::int4, event_type, title, url, sha, branch, author_name,
            author_avatar_url, pr_number::int4, pr_state, created_at,
            is_draft, review_state, ci_state, reviewers, additions, deletions, changed_files, base_branch
         FROM t_git_event
         WHERE ticket_id = $1
         ORDER BY created_at DESC",
    )
    .bind(ticket_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|row| {
            let sha: String = row.get("sha");
            let sha_short: String = sha.chars().take(7).collect();
            GitEventOut {
                id: row.get("id"),
                event_type: row.get("event_type"),
                title: row.get("title"),
                url: row.get("url"),
                sha,
                sha_short,
                branch: row.get("branch"),
                author_name: row.get("author_name"),
                author_avatar_url: row.get("author_avatar_url"),
                pr_number: row.get("pr_number"),
                pr_state: row.get("pr_state"),
                created_at: row.get("created_at"),
                is_draft: row.get("is_draft"),
                review_state: row.get("review_state"),
                ci_state: row.get("ci_state"),
                reviewers: row.get("reviewers"),
                additions: row.get("additions"),
                deletions: row.get("deletions"),
                changed_files: row.get("changed_files"),
                base_branch: row.get("base_branch"),
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::aggregate_ci_state;

    fn c(status: &str, conclusion: Option<&str>) -> (String, Option<String>) {
        (status.to_string(), conclusion.map(String::from))
    }

    #[test]
    fn ci_state_aggregation() {
        assert_eq!(aggregate_ci_state(&[]), "none");
        assert_eq!(
            aggregate_ci_state(&[c("completed", Some("success"))]),
            "success"
        );
        // 失敗が 1 つでもあれば failure(実行中のものがあっても)
        assert_eq!(
            aggregate_ci_state(&[c("completed", Some("failure")), c("in_progress", None)]),
            "failure"
        );
        assert_eq!(
            aggregate_ci_state(&[c("completed", Some("timed_out"))]),
            "failure"
        );
        assert_eq!(
            aggregate_ci_state(&[c("completed", Some("action_required"))]),
            "failure"
        );
        // 実行中・待機中があれば pending
        assert_eq!(
            aggregate_ci_state(&[c("completed", Some("success")), c("queued", None)]),
            "pending"
        );
        // cancelled / skipped / neutral は失敗に数えない
        for k in ["cancelled", "skipped", "neutral"] {
            assert_eq!(
                aggregate_ci_state(&[c("completed", Some(k))]),
                "success",
                "{k}"
            );
        }
    }
}
