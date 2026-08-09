/// infrastructure/repositories/search_repo.rs — グローバル検索
///
/// Django apps/api/views/search.py の移植。チケット・Wiki・プロジェクトを横断検索。

use sqlx::{PgPool, Row};
use serde_json::{json, Value};

pub async fn global_search(pool: &PgPool, query: &str, limit: i64) -> anyhow::Result<Vec<Value>> {
    let pattern = format!("%{}%", query);
    let mut results = Vec::new();

    // --- チケット検索(title一致 + ticket_key一致、重複除去) ---
    let ticket_rows = sqlx::query(
        "SELECT DISTINCT t.id::int4, t.ticket_key, t.title, t.status, p.prefix
         FROM tickets_ticket t
         LEFT JOIN tickets_project p ON t.project_id = p.id
         WHERE t.title ILIKE $1 OR t.ticket_key ILIKE $1
         LIMIT $2"
    )
    .bind(&pattern)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    for row in ticket_rows {
        let ticket_key: String = row.get("ticket_key");
        let prefix: Option<String> = row.get("prefix");
        let project_key = prefix.clone().unwrap_or_default();
        let url = if prefix.is_some() {
            format!("/p/{}/tickets/{}", project_key, ticket_key)
        } else {
            format!("/tickets/{}", ticket_key)
        };
        results.push(json!({
            "type": "ticket",
            "id": row.get::<i32, _>("id"),
            "key": ticket_key,
            "title": row.get::<String, _>("title"),
            "status": row.get::<String, _>("status"),
            "projectKey": project_key,
            "url": url,
            "icon": "🎫",
        }));
    }

    // --- Wiki検索(title一致) ---
    let wiki_rows = sqlx::query(
        "SELECT w.id::int4, w.title, p.prefix
         FROM wiki_page w
         LEFT JOIN tickets_project p ON w.project_id = p.id
         WHERE w.title ILIKE $1
         LIMIT $2"
    )
    .bind(&pattern)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    for row in wiki_rows {
        let prefix: Option<String> = row.get("prefix");
        let project_key = prefix.clone().unwrap_or_default();
        let url = if prefix.is_some() {
            format!("/p/{}/wiki", project_key)
        } else {
            "/wiki".to_string()
        };
        results.push(json!({
            "type": "wiki",
            "id": row.get::<i32, _>("id"),
            "title": row.get::<String, _>("title"),
            "projectKey": project_key,
            "url": url,
            "icon": "📝",
        }));
    }

    // --- プロジェクト検索(name一致、上限5件) ---
    let project_rows = sqlx::query(
        "SELECT id::int4, name, prefix FROM tickets_project WHERE name ILIKE $1 LIMIT 5"
    )
    .bind(&pattern)
    .fetch_all(pool)
    .await?;

    for row in project_rows {
        let prefix: String = row.get("prefix");
        results.push(json!({
            "type": "project",
            "id": row.get::<i32, _>("id"),
            "title": row.get::<String, _>("name"),
            "projectKey": prefix,
            "url": format!("/p/{}/tickets", prefix),
            "icon": "📁",
        }));
    }

    results.truncate(limit as usize);
    Ok(results)
}
