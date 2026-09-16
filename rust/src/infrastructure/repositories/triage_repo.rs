/// infrastructure/repositories/triage_repo.rs — トリアージ依頼永続化
///
/// t_triage_request テーブルの CRUD + approve/reject 操作。

use sqlx::{PgPool, Row};

use crate::domain::models::triage_api::*;

const SELECT_BASE: &str = "
    SELECT
        tr.id::int4, tr.title, tr.description, tr.change_type, tr.change_payload,
        tr.status, tr.project_id::int4, tr.ticket_id::int4,
        tr.review_comment, tr.reviewed_at, tr.created_at,
        rb.id::int4 as rb_id, rb.username as rb_username, rb.email as rb_email, rb.display_name as rb_display_name,
        vb.id::int4 as vb_id, vb.username as vb_username, vb.email as vb_email, vb.display_name as vb_display_name,
        t.ticket_key as ticket_key
     FROM t_triage_request tr
     JOIN accounts_user rb ON tr.requested_by_id = rb.id
     LEFT JOIN accounts_user vb ON tr.reviewed_by_id = vb.id
     LEFT JOIN tickets_ticket t ON tr.ticket_id = t.id
";

fn row_to_triage(row: &sqlx::postgres::PgRow) -> TriageRequestOut {
    let requested_by = UserSummaryOut {
        id: row.get("rb_id"),
        username: row.get("rb_username"),
        email: row.get("rb_email"),
        display_name: row.get("rb_display_name"),
    };

    let vb_id: Option<i32> = row.get("vb_id");
    let reviewed_by = vb_id.map(|_| UserSummaryOut {
        id: row.get("vb_id"),
        username: row.get("vb_username"),
        email: row.get("vb_email"),
        display_name: row.get("vb_display_name"),
    });

    TriageRequestOut {
        id: row.get("id"),
        title: row.get("title"),
        description: row.get("description"),
        change_type: row.get("change_type"),
        change_payload: row.get("change_payload"),
        status: row.get("status"),
        project: row.get("project_id"),
        ticket: row.get("ticket_id"),
        ticket_key: row.get("ticket_key"),
        ticket_id: row.get("ticket_id"),
        requested_by,
        reviewed_by,
        reviewed_at: row.get("reviewed_at"),
        review_comment: row.get("review_comment"),
        created_at: row.get("created_at"),
    }
}

pub async fn find_all(
    pool: &PgPool,
    status: Option<&str>,
    change_type: Option<&str>,
    page: i64,
) -> anyhow::Result<Vec<TriageRequestOut>> {
    const PAGE_SIZE: i64 = 50;
    let page = page.max(1);
    let offset = (page - 1) * PAGE_SIZE;

    let query = format!(
        "{SELECT_BASE}
         WHERE ($1::text IS NULL OR tr.status = $1)
           AND ($2::text IS NULL OR tr.change_type = $2)
         ORDER BY tr.created_at DESC
         LIMIT $3 OFFSET $4"
    );

    let rows = sqlx::query(&query)
        .bind(status)
        .bind(change_type)
        .bind(PAGE_SIZE)
        .bind(offset)
        .fetch_all(pool)
        .await?;

    Ok(rows.iter().map(row_to_triage).collect())
}

pub async fn count_all(pool: &PgPool, status: Option<&str>, change_type: Option<&str>) -> anyhow::Result<i64> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM t_triage_request
         WHERE ($1::text IS NULL OR status = $1)
           AND ($2::text IS NULL OR change_type = $2)"
    )
    .bind(status)
    .bind(change_type)
    .fetch_one(pool)
    .await?;

    Ok(count)
}

pub async fn find_by_id(pool: &PgPool, id: i32) -> anyhow::Result<Option<TriageRequestOut>> {
    let query = format!("{SELECT_BASE} WHERE tr.id = $1");
    let row = sqlx::query(&query)
        .bind(id)
        .fetch_optional(pool)
        .await?;

    Ok(row.map(|r| row_to_triage(&r)))
}

pub async fn create(pool: &PgPool, input: &TriageRequestWriteIn, requested_by: i32) -> anyhow::Result<i32> {
    let id: i32 = sqlx::query_scalar(
        "INSERT INTO t_triage_request
            (title, description, change_type, change_payload, status, project_id, ticket_id, requested_by_id, review_comment, created_at)
         VALUES ($1, $2, $3, $4, 'pending', $5, $6, $7, '', NOW())
         RETURNING id::int4"
    )
    .bind(&input.title)
    .bind(&input.description)
    .bind(&input.change_type)
    .bind(&input.change_payload)
    .bind(input.project)
    .bind(input.ticket)
    .bind(requested_by)
    .fetch_one(pool)
    .await?;

    Ok(id)
}

pub async fn update(pool: &PgPool, id: i32, input: &TriageRequestUpdateIn) -> anyhow::Result<bool> {
    let existing = find_by_id(pool, id).await?;
    let existing = match existing {
        Some(e) => e,
        None => return Ok(false),
    };

    let title = input.title.as_ref().unwrap_or(&existing.title).clone();
    let description = input.description.as_ref().unwrap_or(&existing.description).clone();
    let change_type = input.change_type.as_ref().unwrap_or(&existing.change_type).clone();
    let change_payload = input.change_payload.as_ref().unwrap_or(&existing.change_payload).clone();
    let project = input.project.or(existing.project);
    let ticket = input.ticket.or(existing.ticket);

    let rows_affected = sqlx::query(
        "UPDATE t_triage_request
         SET title = $1, description = $2, change_type = $3, change_payload = $4,
             project_id = $5, ticket_id = $6
         WHERE id = $7"
    )
    .bind(&title)
    .bind(&description)
    .bind(&change_type)
    .bind(&change_payload)
    .bind(project)
    .bind(ticket)
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected();

    Ok(rows_affected > 0)
}

pub async fn delete(pool: &PgPool, id: i32) -> anyhow::Result<bool> {
    let rows_affected = sqlx::query("DELETE FROM t_triage_request WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();

    Ok(rows_affected > 0)
}

pub async fn get_status(pool: &PgPool, id: i32) -> anyhow::Result<Option<String>> {
    let status: Option<String> = sqlx::query_scalar("SELECT status FROM t_triage_request WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?;

    Ok(status)
}

/// 却下: ステータスをrejectedにし、レビュー情報を記録する。
pub async fn reject(pool: &PgPool, id: i32, reviewed_by: i32, comment: &str) -> anyhow::Result<bool> {
    let rows_affected = sqlx::query(
        "UPDATE t_triage_request
         SET status = 'rejected', reviewed_by_id = $1, reviewed_at = NOW(), review_comment = $2
         WHERE id = $3"
    )
    .bind(reviewed_by)
    .bind(comment)
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected();

    Ok(rows_affected > 0)
}

/// 承認: レビュー情報を記録し、必要であればチケットを自動生成して紐付ける。
/// ticket_id/project_idは既にセットされている場合(ticketが既存)は変更しない。
pub struct ApproveResult {
    pub created_ticket_id: Option<i32>,
}

pub async fn approve(
    pool: &PgPool,
    id: i32,
    reviewed_by: i32,
    comment: &str,
    body_project_id: Option<i32>,
) -> anyhow::Result<Option<ApproveResult>> {
    let mut tx = pool.begin().await?;

    let row = sqlx::query(
        "SELECT title, description, change_type, project_id::int4, ticket_id::int4
         FROM t_triage_request WHERE id = $1 FOR UPDATE"
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;

    let row = match row {
        Some(r) => r,
        None => return Ok(None),
    };

    let title: String = row.get("title");
    let description: String = row.get("description");
    let change_type: String = row.get("change_type");
    let existing_ticket_id: Option<i32> = row.get("ticket_id");

    let existing_project_id: Option<i32> = row.get("project_id");
    let resolved_project_id = body_project_id.or(existing_project_id);

    let mut created_ticket_id: Option<i32> = None;
    let mut final_ticket_id = existing_ticket_id;
    let mut final_project_id: Option<i32> = existing_project_id;

    if existing_ticket_id.is_none() {
        if let Some(project_id) = resolved_project_id {
            let requested_by_username: Option<String> = sqlx::query_scalar(
                "SELECT rb.username FROM t_triage_request tr
                 JOIN accounts_user rb ON tr.requested_by_id = rb.id
                 WHERE tr.id = $1"
            )
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;

            let change_type_label = match change_type.as_str() {
                "master_change" => "マスタ変更",
                _ => "テキスト依頼",
            };

            let ticket_description = format!(
                "**トリアージ依頼から自動生成**\n\n依頼者: {}\n種別: {}\n\n---\n\n{}",
                requested_by_username.unwrap_or_default(),
                change_type_label,
                description,
            );

            let gantt_order: i32 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(gantt_order), 0) + 1 FROM tickets_ticket WHERE project_id = $1"
            )
            .bind(project_id)
            .fetch_one(&mut *tx)
            .await?;

            // project に参加チームが1つだけなら補完。2つ以上ならエラー
            let participating_teams: Vec<i32> = sqlx::query_scalar(
                "SELECT team_id FROM tickets_project_teams WHERE project_id = $1 ORDER BY team_id"
            )
            .bind(project_id)
            .fetch_all(&mut *tx)
            .await?;

            let team_id = match participating_teams.as_slice() {
                [single_team] => *single_team,
                [] => return Err(anyhow::anyhow!("project has no participating teams")),
                _ => return Err(anyhow::anyhow!("teamId is required when project has multiple teams")),
            };

            let ticket_key = crate::infrastructure::repositories::ticket_repo::api_generate_ticket_key(&mut tx, team_id).await?;

            let new_ticket_id: i32 = sqlx::query_scalar(
                "INSERT INTO tickets_ticket
                    (ticket_key, title, description, status, priority, ticket_type,
                     author_id, project_id, gantt_order, team_id, created_at, updated_at)
                 VALUES ($1, $2, $3, 'open', 'medium', 'issue', $4, $5, $6, $7, NOW(), NOW())
                 RETURNING id::int4"
            )
            .bind(&ticket_key)
            .bind(format!("[Triage] {}", title))
            .bind(&ticket_description)
            .bind(reviewed_by)
            .bind(project_id)
            .bind(gantt_order)
            .bind(team_id)
            .fetch_one(&mut *tx)
            .await?;

            sqlx::query(
                "INSERT INTO tickets_ticket_assignees (ticketmodel_id, user_id) VALUES ($1, $2)"
            )
            .bind(new_ticket_id)
            .bind(reviewed_by)
            .execute(&mut *tx)
            .await?;

            created_ticket_id = Some(new_ticket_id);
            final_ticket_id = Some(new_ticket_id);
            final_project_id = Some(project_id);
        }
    }

    sqlx::query(
        "UPDATE t_triage_request
         SET status = 'approved', reviewed_by_id = $1, reviewed_at = NOW(), review_comment = $2,
             ticket_id = $3, project_id = $4
         WHERE id = $5"
    )
    .bind(reviewed_by)
    .bind(comment)
    .bind(final_ticket_id)
    .bind(final_project_id)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Some(ApproveResult { created_ticket_id }))
}
