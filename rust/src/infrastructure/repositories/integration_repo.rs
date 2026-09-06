/// infrastructure/repositories/integration_repo.rs — Git連携永続化
///
/// t_git_integration / t_git_event の CRUD + Webhook処理。

use sqlx::{PgPool, Row};

use crate::domain::models::integration_api::*;

const SELECT_BASE: &str = "
    SELECT
        gi.id::int4, gi.project_id::int4, gi.provider, gi.repository_url,
        gi.webhook_secret, gi.is_active, gi.auto_status_transition, gi.created_at,
        cb.id::int4 as cb_id, cb.username as cb_username, cb.email as cb_email, cb.display_name as cb_display_name,
        (SELECT COUNT(*) FROM t_git_event WHERE integration_id = gi.id)::int8 as event_count
     FROM t_git_integration gi
     LEFT JOIN accounts_user cb ON gi.created_by_id = cb.id
";

fn row_to_integration(row: &sqlx::postgres::PgRow) -> GitIntegrationOut {
    let cb_id: Option<i32> = row.get("cb_id");
    GitIntegrationOut {
        id: row.get("id"),
        project: row.get("project_id"),
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
    }
}

pub async fn find_all(pool: &PgPool, project_id: Option<i32>) -> anyhow::Result<Vec<GitIntegrationOut>> {
    let query = format!("{SELECT_BASE} WHERE ($1::int4 IS NULL OR gi.project_id = $1) ORDER BY gi.created_at DESC");
    let rows = sqlx::query(&query).bind(project_id).fetch_all(pool).await?;
    Ok(rows.iter().map(row_to_integration).collect())
}

pub async fn find_by_id(pool: &PgPool, id: i32) -> anyhow::Result<Option<GitIntegrationOut>> {
    let query = format!("{SELECT_BASE} WHERE gi.id = $1");
    let row = sqlx::query(&query).bind(id).fetch_optional(pool).await?;
    Ok(row.map(|r| row_to_integration(&r)))
}

pub async fn create(pool: &PgPool, input: &GitIntegrationWriteIn, created_by: i32) -> anyhow::Result<i32> {
    let id: i32 = sqlx::query_scalar(
        "INSERT INTO t_git_integration (project_id, provider, repository_url, webhook_secret, is_active, auto_status_transition, created_by_id, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())
         RETURNING id::int4"
    )
    .bind(input.project)
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

pub async fn update(pool: &PgPool, id: i32, input: &GitIntegrationUpdateIn) -> anyhow::Result<bool> {
    let existing = find_by_id(pool, id).await?;
    let existing = match existing {
        Some(e) => e,
        None => return Ok(false),
    };

    let project = input.project.unwrap_or(existing.project);
    let provider = input.provider.clone().unwrap_or(existing.provider);
    let repository_url = input.repository_url.clone().unwrap_or(existing.repository_url);
    let webhook_secret = input.webhook_secret.clone().unwrap_or(existing.webhook_secret);
    let is_active = input.is_active.unwrap_or(existing.is_active);
    let auto_status_transition = input.auto_status_transition.unwrap_or(existing.auto_status_transition);

    let rows_affected = sqlx::query(
        "UPDATE t_git_integration
         SET project_id = $1, provider = $2, repository_url = $3, webhook_secret = $4, is_active = $5, auto_status_transition = $6
         WHERE id = $7"
    )
    .bind(project)
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

    // t_git_event.integration は on_delete=CASCADE
    sqlx::query("DELETE FROM t_git_event WHERE integration_id = $1").bind(id).execute(&mut *tx).await?;

    let rows_affected = sqlx::query("DELETE FROM t_git_integration WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();

    tx.commit().await?;
    Ok(rows_affected > 0)
}

// =============================================================================
// Webhook処理(GitWebhookService の移植)
// =============================================================================

pub struct ActiveIntegration {
    pub id: i32,
    pub webhook_secret: String,
}

pub async fn find_active_integrations_by_repo_url(pool: &PgPool, repo_url: &str) -> anyhow::Result<Vec<ActiveIntegration>> {
    let rows = sqlx::query(
        "SELECT id::int4, webhook_secret FROM t_git_integration WHERE repository_url = $1 AND is_active = true"
    )
    .bind(repo_url)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| ActiveIntegration { id: r.get("id"), webhook_secret: r.get("webhook_secret") })
        .collect())
}

/// チケットキーからticket_idを検索する。見つからない場合、prefix+ゼロパディングで再検索する
/// (例: WIP-123 → WIP-000123)。Djangoの GitWebhookService._find_ticket と同じロジック。
pub async fn find_ticket_id_by_key(pool: &PgPool, ticket_key: &str) -> anyhow::Result<Option<i32>> {
    if let Some(id) = crate::infrastructure::repositories::ticket_repo::resolve_ticket_id(pool, ticket_key).await? {
        return Ok(Some(id));
    }

    if let Some((prefix, num_str)) = ticket_key.split_once('-') {
        if let Ok(num) = num_str.parse::<u32>() {
            let padded_key = format!("{}-{:06}", prefix, num);
            if padded_key != ticket_key {
                return crate::infrastructure::repositories::ticket_repo::resolve_ticket_id(pool, &padded_key).await;
            }
        }
    }

    Ok(None)
}

/// コミットイベントを作成する(get_or_create相当)。既存なら false(未作成)を返す。
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
         VALUES ($1, $2, 'commit', $3, $4, $5, $6, $7, '', NULL, '', NOW())"
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

/// PRイベントをupsertする(update_or_create相当)。
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
                author_avatar_url = $5, pr_state = $6 WHERE id = $7"
        )
        .bind(title)
        .bind(url)
        .bind(branch)
        .bind(author_name)
        .bind(author_avatar_url)
        .bind(pr_state)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(false)
    } else {
        sqlx::query(
            "INSERT INTO t_git_event
                (integration_id, ticket_id, event_type, title, url, sha, branch, author_name,
                 author_avatar_url, pr_number, pr_state, created_at)
             VALUES ($1, $2, 'pull_request', $3, $4, '', $5, $6, $7, $8, $9, NOW())"
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
        .execute(pool)
        .await?;
        Ok(true)
    }
}

/// 指定チケットのGitイベント一覧(新しい順)。
pub async fn find_events_by_ticket(pool: &PgPool, ticket_id: i32) -> anyhow::Result<Vec<GitEventOut>> {
    let rows = sqlx::query(
        "SELECT id::int4, event_type, title, url, sha, branch, author_name,
            author_avatar_url, pr_number::int4, pr_state, created_at
         FROM t_git_event
         WHERE ticket_id = $1
         ORDER BY created_at DESC"
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
            }
        })
        .collect())
}
