/// infrastructure/repositories/ai_repo.rs — AI分析用データ取得
///
/// apps/api/views/ai.py の各ビューがDBから組み立てているコンテキストデータの移植。

use sqlx::{PgPool, Row};
use serde_json::{json, Value};

pub struct TicketForAi {
    pub id: i32,
    pub ticket_key: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub story_points: Option<i16>,
    pub project_id: i32,
    pub project_prefix: String,
    pub assigned_team_id: Option<i32>,
}

pub async fn find_ticket_for_ai(pool: &PgPool, ticket_id: i32) -> anyhow::Result<Option<TicketForAi>> {
    let row = sqlx::query(
        "SELECT t.id::int4, t.ticket_key, t.title, t.description, t.status, t.story_points,
            t.project_id::int4, p.prefix, t.assigned_team_id::int4
         FROM tickets_ticket t
         LEFT JOIN tickets_project p ON t.project_id = p.id
         WHERE t.id = $1"
    )
    .bind(ticket_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| TicketForAi {
        id: r.get("id"),
        ticket_key: r.get("ticket_key"),
        title: r.get("title"),
        description: r.get("description"),
        status: r.get("status"),
        story_points: r.get("story_points"),
        project_id: r.get("project_id"),
        project_prefix: r.get("prefix"),
        assigned_team_id: r.get("assigned_team_id"),
    }))
}

/// チケットに紐付くTeamRule(直接リンク + 担当チームのルール、重複除去)のテキストを
/// Django側の context_analysis_view と同じ書式("## title (category)\ncontent\n")で組み立てる。
pub async fn build_associated_rules_text(pool: &PgPool, ticket_id: i32, assigned_team_id: Option<i32>) -> anyhow::Result<String> {
    let mut text = String::new();
    let mut seen_ids: std::collections::HashSet<i32> = std::collections::HashSet::new();

    let linked_rows = sqlx::query(
        "SELECT r.id::int4, r.title, r.category, r.content
         FROM m_team_rule r
         JOIN tickets_ticket_linked_rules lr ON lr.teamrulemodel_id = r.id
         WHERE lr.ticketmodel_id = $1 AND r.is_active = true"
    )
    .bind(ticket_id)
    .fetch_all(pool)
    .await?;

    for row in &linked_rows {
        let id: i32 = row.get("id");
        seen_ids.insert(id);
        text.push_str(&format!(
            "\n## {} ({})\n{}\n",
            row.get::<String, _>("title"),
            row.get::<String, _>("category"),
            row.get::<String, _>("content"),
        ));
    }

    if let Some(team_id) = assigned_team_id {
        let team_rows = sqlx::query(
            "SELECT id::int4, title, category, content FROM m_team_rule WHERE team_id = $1 AND is_active = true"
        )
        .bind(team_id)
        .fetch_all(pool)
        .await?;

        for row in &team_rows {
            let id: i32 = row.get("id");
            if seen_ids.contains(&id) {
                continue;
            }
            text.push_str(&format!(
                "\n## [Team] {} ({})\n{}\n",
                row.get::<String, _>("title"),
                row.get::<String, _>("category"),
                row.get::<String, _>("content"),
            ));
        }
    }

    Ok(text)
}

/// チケットのコメントを時系列で結合したテキスト("### author (日時)\nbody\n")。
pub async fn build_comments_text(pool: &PgPool, ticket_id: i32) -> anyhow::Result<String> {
    let rows = sqlx::query(
        "SELECT c.body, c.created_at, COALESCE(u.username, 'Unknown') as author_name
         FROM tickets_comment c
         LEFT JOIN accounts_user u ON c.author_id = u.id
         WHERE c.ticket_id = $1
         ORDER BY c.created_at ASC"
    )
    .bind(ticket_id)
    .fetch_all(pool)
    .await?;

    let mut text = String::new();
    for row in &rows {
        let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
        text.push_str(&format!(
            "\n### {} ({})\n{}\n",
            row.get::<String, _>("author_name"),
            created_at.format("%Y-%m-%d %H:%M"),
            row.get::<String, _>("body"),
        ));
    }
    Ok(text)
}

/// プロジェクトのprefixを取得する。
pub async fn find_project_prefix(pool: &PgPool, project_id: i32) -> anyhow::Result<Option<String>> {
    let prefix: Option<String> = sqlx::query_scalar("SELECT prefix FROM tickets_project WHERE id = $1")
        .bind(project_id)
        .fetch_optional(pool)
        .await?;
    Ok(prefix)
}

/// サイクルの開始日・終了日を取得する。
pub async fn find_cycle_dates(pool: &PgPool, cycle_id: i32) -> anyhow::Result<Option<(chrono::NaiveDate, chrono::NaiveDate)>> {
    let row = sqlx::query("SELECT start_date, end_date FROM t_cycle WHERE id = $1")
        .bind(cycle_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| (r.get("start_date"), r.get("end_date"))))
}

/// プロジェクト(+任意でサイクル)配下のチケット一覧をAI分析用JSON配列として取得する。
pub async fn find_tasks_json_for_sprint_health(
    pool: &PgPool,
    project_id: i32,
    cycle_id: Option<i32>,
) -> anyhow::Result<Vec<Value>> {
    let query = if cycle_id.is_some() {
        "SELECT id::int4, title, status, priority, story_points, due_date
         FROM tickets_ticket WHERE project_id = $1 AND cycle_id = $2"
    } else {
        "SELECT id::int4, title, status, priority, story_points, due_date
         FROM tickets_ticket WHERE project_id = $1"
    };

    let rows = sqlx::query(query)
        .bind(project_id)
        .bind(cycle_id)
        .fetch_all(pool)
        .await?;

    Ok(rows
        .iter()
        .map(|row| {
            let due_date: Option<chrono::NaiveDate> = row.get("due_date");
            let story_points: Option<i16> = row.get("story_points");
            json!({
                "id": row.get::<i32, _>("id"),
                "title": row.get::<String, _>("title"),
                "status": row.get::<String, _>("status"),
                "priority": row.get::<String, _>("priority"),
                "story_points": story_points,
                "due_date": due_date.map(|d| d.format("%Y-%m-%d").to_string()),
            })
        })
        .collect())
}
