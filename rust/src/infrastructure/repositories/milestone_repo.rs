/// infrastructure/repositories/milestone_repo.rs — マイルストーン永続化

use sqlx::PgPool;
use crate::domain::models::milestone::Milestone;

pub async fn find_all(pool: &PgPool, project_id: Option<i32>) -> anyhow::Result<Vec<Milestone>> {
    let rows = if let Some(pid) = project_id {
        sqlx::query_as::<_, Milestone>(
            "SELECT m.id, m.name, m.due_date, m.description, m.project_id, m.created_at,
                    (SELECT COUNT(*) FROM tickets_ticket t WHERE t.milestone_id = m.id) as total_tickets,
                    (SELECT COUNT(*) FROM tickets_ticket t WHERE t.milestone_id = m.id AND t.status = '完了') as closed_tickets
             FROM milestones_milestone m WHERE m.project_id = $1
             ORDER BY m.due_date NULLS LAST, m.name"
        ).bind(pid).fetch_all(pool).await?
    } else {
        sqlx::query_as::<_, Milestone>(
            "SELECT m.id, m.name, m.due_date, m.description, m.project_id, m.created_at,
                    (SELECT COUNT(*) FROM tickets_ticket t WHERE t.milestone_id = m.id) as total_tickets,
                    (SELECT COUNT(*) FROM tickets_ticket t WHERE t.milestone_id = m.id AND t.status = '完了') as closed_tickets
             FROM milestones_milestone m
             ORDER BY m.due_date NULLS LAST, m.name"
        ).fetch_all(pool).await?
    };
    Ok(rows)
}

pub async fn find_by_id(pool: &PgPool, id: i32) -> anyhow::Result<Option<Milestone>> {
    let row = sqlx::query_as::<_, Milestone>(
        "SELECT m.id, m.name, m.due_date, m.description, m.project_id, m.created_at,
                (SELECT COUNT(*) FROM tickets_ticket t WHERE t.milestone_id = m.id) as total_tickets,
                (SELECT COUNT(*) FROM tickets_ticket t WHERE t.milestone_id = m.id AND t.status = '完了') as closed_tickets
         FROM milestones_milestone m WHERE m.id = $1"
    ).bind(id).fetch_optional(pool).await?;
    Ok(row)
}

pub async fn create(pool: &PgPool, name: &str, due_date: Option<chrono::NaiveDate>, description: &str, project_id: Option<i32>) -> anyhow::Result<i32> {
    let id = sqlx::query_scalar::<_, i32>(
        "INSERT INTO milestones_milestone (name, due_date, description, project_id) VALUES ($1, $2, $3, $4) RETURNING id"
    ).bind(name).bind(due_date).bind(description).bind(project_id).fetch_one(pool).await?;
    Ok(id)
}

pub async fn update(pool: &PgPool, id: i32, name: &str, due_date: Option<chrono::NaiveDate>, description: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE milestones_milestone SET name=$2, due_date=$3, description=$4 WHERE id=$1")
        .bind(id).bind(name).bind(due_date).bind(description).execute(pool).await?;
    Ok(())
}

pub async fn delete(pool: &PgPool, id: i32) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM milestones_milestone WHERE id=$1").bind(id).execute(pool).await?;
    Ok(())
}
