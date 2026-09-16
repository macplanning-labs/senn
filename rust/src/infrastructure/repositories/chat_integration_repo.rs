/// infrastructure/repositories/chat_integration_repo.rs — チャット通知連携永続化
///
/// t_chat_integration の CRUD + イベント発火時の送信先解決。

use sqlx::{PgPool, Row};

use crate::domain::models::chat_integration_api::*;

const SELECT_BASE: &str = "
    SELECT
        ci.id::int4, ci.project_id::int4, ci.team_id::int4, ci.provider,
        ci.webhook_url, ci.api_token, ci.room_id, ci.enabled_categories, ci.is_active, ci.created_at,
        cb.id::int4 as cb_id, cb.username as cb_username, cb.email as cb_email, cb.display_name as cb_display_name
     FROM t_chat_integration ci
     LEFT JOIN accounts_user cb ON ci.created_by_id = cb.id
";

fn row_to_integration(row: &sqlx::postgres::PgRow) -> ChatIntegrationOut {
    let cb_id: Option<i32> = row.get("cb_id");
    ChatIntegrationOut {
        id: row.get("id"),
        project: row.get("project_id"),
        team: row.get("team_id"),
        provider: row.get("provider"),
        webhook_url: row.get("webhook_url"),
        api_token: row.get("api_token"),
        room_id: row.get("room_id"),
        enabled_categories: row.get("enabled_categories"),
        is_active: row.get("is_active"),
        created_by: cb_id.map(|_| UserSummaryOut {
            id: row.get("cb_id"),
            username: row.get("cb_username"),
            email: row.get("cb_email"),
            display_name: row.get("cb_display_name"),
        }),
        created_at: row.get("created_at"),
    }
}

fn scope_ok(project: Option<i32>, team: Option<i32>) -> bool {
    matches!((project, team), (Some(_), None) | (None, Some(_)))
}

/// プロバイダごとの必須フィールドを検証する。
fn validate_provider_fields(
    provider: &str,
    webhook_url: &Option<String>,
    api_token: &Option<String>,
    room_id: &Option<String>,
) -> anyhow::Result<()> {
    if !VALID_PROVIDERS.contains(&provider) {
        anyhow::bail!("invalid provider");
    }
    match provider {
        "chatwork" => {
            if api_token.as_deref().unwrap_or("").is_empty() || room_id.as_deref().unwrap_or("").is_empty() {
                anyhow::bail!("chatwork requires api_token and room_id");
            }
        }
        _ => {
            if webhook_url.as_deref().unwrap_or("").is_empty() {
                anyhow::bail!("webhook_url is required");
            }
        }
    }
    Ok(())
}

pub async fn find_all(
    pool: &PgPool,
    project_id: Option<i32>,
    team_id: Option<i32>,
) -> anyhow::Result<Vec<ChatIntegrationOut>> {
    let query = format!(
        "{SELECT_BASE}
         WHERE ($1::int4 IS NULL OR ci.project_id = $1)
           AND ($2::int4 IS NULL OR ci.team_id = $2)
         ORDER BY ci.created_at DESC"
    );
    let rows = sqlx::query(&query)
        .bind(project_id)
        .bind(team_id)
        .fetch_all(pool)
        .await?;
    Ok(rows.iter().map(row_to_integration).collect())
}

pub async fn find_by_id(pool: &PgPool, id: i32) -> anyhow::Result<Option<ChatIntegrationOut>> {
    let query = format!("{SELECT_BASE} WHERE ci.id = $1");
    let row = sqlx::query(&query).bind(id).fetch_optional(pool).await?;
    Ok(row.map(|r| row_to_integration(&r)))
}

pub async fn create(pool: &PgPool, input: &ChatIntegrationWriteIn, created_by: i32) -> anyhow::Result<i32> {
    if !scope_ok(input.project, input.team) {
        anyhow::bail!("exactly one of project or team is required");
    }
    validate_provider_fields(&input.provider, &input.webhook_url, &input.api_token, &input.room_id)?;

    let id: i32 = sqlx::query_scalar(
        "INSERT INTO t_chat_integration
            (project_id, team_id, provider, webhook_url, api_token, room_id, enabled_categories, is_active, created_by_id, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW())
         RETURNING id::int4"
    )
    .bind(input.project)
    .bind(input.team)
    .bind(&input.provider)
    .bind(&input.webhook_url)
    .bind(&input.api_token)
    .bind(&input.room_id)
    .bind(&input.enabled_categories)
    .bind(input.is_active)
    .bind(created_by)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub async fn update(pool: &PgPool, id: i32, input: &ChatIntegrationUpdateIn) -> anyhow::Result<bool> {
    let existing = match find_by_id(pool, id).await? {
        Some(e) => e,
        None => return Ok(false),
    };

    let project = if input.project.is_some() || input.team.is_some() { input.project } else { existing.project };
    let team = if input.project.is_some() || input.team.is_some() { input.team } else { existing.team };
    if !scope_ok(project, team) {
        anyhow::bail!("exactly one of project or team is required");
    }

    let provider = input.provider.clone().unwrap_or(existing.provider);
    let webhook_url = if input.webhook_url.is_some() { input.webhook_url.clone() } else { existing.webhook_url };
    let api_token = if input.api_token.is_some() { input.api_token.clone() } else { existing.api_token };
    let room_id = if input.room_id.is_some() { input.room_id.clone() } else { existing.room_id };
    let enabled_categories = input.enabled_categories.clone().unwrap_or(existing.enabled_categories);
    let is_active = input.is_active.unwrap_or(existing.is_active);

    validate_provider_fields(&provider, &webhook_url, &api_token, &room_id)?;

    let rows_affected = sqlx::query(
        "UPDATE t_chat_integration
         SET project_id = $1, team_id = $2, provider = $3, webhook_url = $4,
             api_token = $5, room_id = $6, enabled_categories = $7, is_active = $8
         WHERE id = $9"
    )
    .bind(project)
    .bind(team)
    .bind(&provider)
    .bind(&webhook_url)
    .bind(&api_token)
    .bind(&room_id)
    .bind(&enabled_categories)
    .bind(is_active)
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected();

    Ok(rows_affected > 0)
}

pub async fn delete(pool: &PgPool, id: i32) -> anyhow::Result<bool> {
    let rows_affected = sqlx::query("DELETE FROM t_chat_integration WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
    Ok(rows_affected > 0)
}

/// チケットイベント発火時、送信すべきチャット連携を解決する。
/// tickets_ticket.project_id / team_id を直接JOINして参照するため、
/// notification_service側でTicket構造体にteam_idを持たせる必要はない。
pub async fn find_active_for_ticket_category(
    pool: &PgPool,
    ticket_id: i32,
    category_db_str: &str,
) -> anyhow::Result<Vec<ChatIntegrationOut>> {
    let query = format!(
        "{SELECT_BASE}
         WHERE ci.is_active = true
           AND $1 = ANY(ci.enabled_categories)
           AND EXISTS (
               SELECT 1 FROM tickets_ticket t
               WHERE t.id = $2
                 AND ((ci.project_id IS NOT NULL AND ci.project_id = t.project_id)
                      OR (ci.team_id IS NOT NULL AND ci.team_id = t.team_id))
           )
         ORDER BY ci.created_at ASC"
    );
    let rows = sqlx::query(&query)
        .bind(category_db_str)
        .bind(ticket_id)
        .fetch_all(pool)
        .await?;
    Ok(rows.iter().map(row_to_integration).collect())
}
