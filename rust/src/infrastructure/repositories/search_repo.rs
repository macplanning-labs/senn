/// infrastructure/repositories/search_repo.rs — グローバル検索
///
/// Django apps/api/views/search.py の移植。チケット・Wiki・プロジェクトを横断検索。

use sqlx::{PgPool, Row};
use serde_json::{json, Value};

/// チケット詳細の画面 URL（frontend の buildTicketDetailPath と同じ形）
fn ticket_url(project_prefix: Option<&str>, team_slug: Option<&str>, ticket_key: &str) -> String {
    match (project_prefix, team_slug) {
        (Some(prefix), _) => format!("/project/{}/tickets/{}", prefix, ticket_key),
        (None, Some(slug)) => format!("/team/{}/tickets/{}", slug, ticket_key),
        (None, None) => format!("/tickets/{}", ticket_key),
    }
}

pub async fn global_search(pool: &PgPool, user_id: i32, query: &str, limit: i64) -> anyhow::Result<Vec<Value>> {
    let pattern = format!("%{}%", query);
    let mut results = Vec::new();

    // --- チケット検索(title一致 + ticket_key一致、重複除去) ---
    let mut sql = String::from(
        "SELECT DISTINCT t.id::int4, t.ticket_key, t.title, t.status, p.prefix, team_m.slug AS team_slug
         FROM tickets_ticket t
         LEFT JOIN tickets_project p ON t.project_id = p.id
         LEFT JOIN m_team team_m ON t.team_id = team_m.id
         WHERE (t.title ILIKE $1 OR t.ticket_key ILIKE $1)",
    );
    let mut param_count = 2usize; // $1 = pattern
    super::ticket_repo::push_ticket_access_sql(&mut sql, &mut param_count); // $2, $3 = user_id
    sql.push_str(&format!(" LIMIT ${}", param_count)); // $4
    let ticket_rows = sqlx::query(&sql)
        .bind(&pattern)
        .bind(user_id)
        .bind(user_id)
        .bind(limit)
        .fetch_all(pool)
        .await?;

    for row in ticket_rows {
        let ticket_key: String = row.get("ticket_key");
        let prefix: Option<String> = row.get("prefix");
        let project_key = prefix.clone().unwrap_or_default();
        let team_slug: Option<String> = row.get("team_slug");
        let url = ticket_url(prefix.as_deref(), team_slug.as_deref(), &ticket_key);
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
            format!("/project/{}/wiki", project_key)
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
            "url": format!("/project/{}/tickets", prefix),
            "icon": "📁",
        }));
    }

    results.truncate(limit as usize);
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::ticket_url;
    use super::global_search;
    use crate::test_support;

    #[test]
    fn ticket_url_matches_frontend_routes() {
        assert_eq!(ticket_url(Some("PRS"), Some("t"), "DEMO-000001"), "/project/PRS/tickets/DEMO-000001");
        assert_eq!(ticket_url(None, Some("team-1"), "TT-000001"), "/team/team-1/tickets/TT-000001");
        assert_eq!(ticket_url(None, None, "X-000001"), "/tickets/X-000001");
    }

    #[tokio::test]
    async fn search_s1_team_member_finds_team_ticket_by_title() {
        // S1: チームAのチケットを、チームAのメンバーが検索（タイトル）
        let Some(pool) = test_support::test_pool().await else { return; };
        let member = test_support::create_test_user(&pool, "s1m").await;
        let project = test_support::create_test_project(&pool, "S1", member).await;

        // プロジェクトのチームを取得
        let team = sqlx::query_scalar::<_, i32>("SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1")
            .bind(project as i64).fetch_one(&pool).await.unwrap();

        // メンバーをチームに追加
        let suffix = test_support::unique_suffix();
        let title = format!("S1-ticket-{}", suffix);
        sqlx::query("INSERT INTO t_team_membership (team_id, user_id, role, joined_at) VALUES ($1, $2, 'member', NOW())")
            .bind(team as i64)
            .bind(member as i64)
            .execute(&pool)
            .await
            .unwrap();

        // チケットを作成（チーム紐付け）
        let ticket_id = sqlx::query_scalar::<_, i32>(
            "INSERT INTO tickets_ticket (ticket_key, title, description, status, priority, ticket_type, project_id, author_id, team_id, created_at, updated_at, gantt_order) VALUES ($1, $2, '', 'open', 'medium', 'task', $3, $4, $5, NOW(), NOW(), 0) RETURNING id::int4"
        )
        .bind(format!("S1-{}", suffix))
        .bind(&title)
        .bind(project)
        .bind(member)
        .bind(team)
        .fetch_one(&pool)
        .await
        .unwrap();

        // 検索
        let results = global_search(&pool, member, &suffix, 10).await.unwrap();
        let found = results.iter().any(|r| r["type"] == "ticket" && r["id"] == ticket_id);
        assert!(found, "チームメンバーがチームのチケットを見つけられるべき");
    }

    #[tokio::test]
    async fn search_s2_non_member_cannot_find_team_ticket_by_title() {
        // S2: 同じチケットを、チームAに所属しない人が検索（タイトル）
        let Some(pool) = test_support::test_pool().await else { return; };
        let member = test_support::create_test_user(&pool, "s2m").await;
        let outsider = test_support::create_test_user(&pool, "s2o").await;
        let project = test_support::create_test_project(&pool, "S2", member).await;

        let team = sqlx::query_scalar::<_, i32>("SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1")
            .bind(project as i64).fetch_one(&pool).await.unwrap();

        let suffix = test_support::unique_suffix();
        let title = format!("S2-ticket-{}", suffix);

        sqlx::query("INSERT INTO t_team_membership (team_id, user_id, role, joined_at) VALUES ($1, $2, 'member', NOW())")
            .bind(team as i64)
            .bind(member as i64)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query_scalar::<_, i32>(
            "INSERT INTO tickets_ticket (ticket_key, title, description, status, priority, ticket_type, project_id, author_id, team_id, created_at, updated_at, gantt_order) VALUES ($1, $2, '', 'open', 'medium', 'task', $3, $4, $5, NOW(), NOW(), 0) RETURNING id::int4"
        )
        .bind(format!("S2-{}", suffix))
        .bind(&title)
        .bind(project)
        .bind(member)
        .bind(team)
        .fetch_one(&pool)
        .await
        .unwrap();

        // 所属していない人が検索
        let results = global_search(&pool, outsider, &suffix, 10).await.unwrap();
        let found = results.iter().any(|r| r["type"] == "ticket" && r["title"] == title);
        assert!(!found, "チームに所属していない人はチケットを見つけられないべき");
    }

    #[tokio::test]
    async fn search_s3_non_member_cannot_find_team_ticket_by_key() {
        // S3: 同じチケットを、所属しない人がチケット番号で検索
        let Some(pool) = test_support::test_pool().await else { return; };
        let member = test_support::create_test_user(&pool, "s3m").await;
        let outsider = test_support::create_test_user(&pool, "s3o").await;
        let project = test_support::create_test_project(&pool, "S3", member).await;

        let team = sqlx::query_scalar::<_, i32>("SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1")
            .bind(project as i64).fetch_one(&pool).await.unwrap();

        let suffix = test_support::unique_suffix();
        let key = format!("S3-{}", suffix);

        sqlx::query("INSERT INTO t_team_membership (team_id, user_id, role, joined_at) VALUES ($1, $2, 'member', NOW())")
            .bind(team as i64)
            .bind(member as i64)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query_scalar::<_, i32>(
            "INSERT INTO tickets_ticket (ticket_key, title, description, status, priority, ticket_type, project_id, author_id, team_id, created_at, updated_at, gantt_order) VALUES ($1, $2, '', 'open', 'medium', 'task', $3, $4, $5, NOW(), NOW(), 0) RETURNING id::int4"
        )
        .bind(&key)
        .bind(&key)
        .bind(project)
        .bind(member)
        .bind(team)
        .fetch_one(&pool)
        .await
        .unwrap();

        // チケット番号で検索
        let results = global_search(&pool, outsider, &suffix, 10).await.unwrap();
        let found = results.iter().any(|r| r["type"] == "ticket" && r["key"] == key);
        assert!(!found, "チームに所属していない人はチケット番号でも見つけられないべき");
    }

    #[tokio::test]
    async fn search_s4_staff_can_find_team_ticket() {
        // S4: 同じチケットを、staff が検索
        let Some(pool) = test_support::test_pool().await else { return; };
        let member = test_support::create_test_user(&pool, "s4m").await;
        let project = test_support::create_test_project(&pool, "S4", member).await;

        let team = sqlx::query_scalar::<_, i32>("SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1")
            .bind(project as i64).fetch_one(&pool).await.unwrap();

        // Staff ユーザーを作成
        let suffix = test_support::unique_suffix();
        let staff_user = sqlx::query_scalar::<_, i32>(
            "INSERT INTO accounts_user (password, is_superuser, username, first_name, last_name, email, is_staff, is_active, date_joined, display_name, must_change_password, email_notifications_enabled) VALUES ('!', false, $1, '', '', $1 || '@test.local', true, true, NOW(), $1, false, false) RETURNING id::int4"
        )
        .bind(format!("s4staff-{}", suffix))
        .fetch_one(&pool)
        .await
        .unwrap();

        let title = format!("S4-ticket-{}", suffix);

        sqlx::query("INSERT INTO t_team_membership (team_id, user_id, role, joined_at) VALUES ($1, $2, 'member', NOW())")
            .bind(team as i64)
            .bind(member as i64)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query_scalar::<_, i32>(
            "INSERT INTO tickets_ticket (ticket_key, title, description, status, priority, ticket_type, project_id, author_id, team_id, created_at, updated_at, gantt_order) VALUES ($1, $2, '', 'open', 'medium', 'task', $3, $4, $5, NOW(), NOW(), 0) RETURNING id::int4"
        )
        .bind(format!("S4-{}", suffix))
        .bind(&title)
        .bind(project)
        .bind(member)
        .bind(team)
        .fetch_one(&pool)
        .await
        .unwrap();

        // Staff が検索
        let results = global_search(&pool, staff_user, &suffix, 10).await.unwrap();
        let found = results.iter().any(|r| r["type"] == "ticket" && r["title"] == title);
        assert!(found, "Staff はすべてのチケットを見つけられるべき");
    }

    #[tokio::test]
    async fn search_s5_teamless_ticket_is_visible_to_staff_only() {
        // S5: チームにもプロジェクトにも紐付かないチケットは、staff でない人には出ず、staff には出る
        let Some(pool) = test_support::test_pool().await else { return; };
        let suffix = test_support::unique_suffix();
        let author = test_support::create_test_user(&pool, "s5a").await;
        let other = test_support::create_test_user(&pool, "s5o").await;
        let staff_user = sqlx::query_scalar::<_, i32>(
            "INSERT INTO accounts_user (password, is_superuser, username, first_name, last_name, email, is_staff, is_active, date_joined, display_name, must_change_password, email_notifications_enabled) VALUES ('!', false, $1, '', '', $1 || '@test.local', true, true, NOW(), $1, false, false) RETURNING id::int4"
        )
        .bind(format!("s5staff-{}", suffix))
        .fetch_one(&pool)
        .await
        .unwrap();

        let title = format!("S5-ticket-{}", suffix);
        sqlx::query(
            "INSERT INTO tickets_ticket (ticket_key, title, description, status, priority, ticket_type, author_id, created_at, updated_at, gantt_order) VALUES ($1, $2, '', 'open', 'medium', 'task', $3, NOW(), NOW(), 0)"
        )
        .bind(format!("S5-{}", suffix))
        .bind(&title)
        .bind(author)
        .execute(&pool)
        .await
        .unwrap();

        let by_staff = global_search(&pool, staff_user, &suffix, 10).await.unwrap();
        assert!(
            by_staff.iter().any(|r| r["type"] == "ticket" && r["title"] == title),
            "staff にはチームなしチケットも出るべき"
        );
        for viewer in [author, other] {
            let results = global_search(&pool, viewer, &suffix, 10).await.unwrap();
            assert!(
                !results.iter().any(|r| r["type"] == "ticket" && r["title"] == title),
                "staff でない人（作成者本人を含む）にはチームなしチケットは出ない"
            );
        }
    }

    #[tokio::test]
    async fn search_s6_project_guest_can_find_ticket_within_validity() {
        // S6: Project ゲストが、その Project のチケットを検索。期限内
        let Some(pool) = test_support::test_pool().await else { return; };
        let owner = test_support::create_test_user(&pool, "s6o").await;
        let project = test_support::create_test_project(&pool, "S6", owner).await;

        let team = sqlx::query_scalar::<_, i32>("SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1")
            .bind(project as i64).fetch_one(&pool).await.unwrap();

        // Project ゲストを作成（scoped_project_id付き）
        let suffix = test_support::unique_suffix();
        let guest = test_support::create_test_user(&pool, "s6g").await;
        let tomorrow = crate::test_support::db_today().succ_opt().unwrap();
        sqlx::query("INSERT INTO t_team_membership (team_id, user_id, role, scoped_project_id, joined_at, end_date) VALUES ($1, $2, 'member', $3, NOW(), $4)")
            .bind(team as i64)
            .bind(guest as i64)
            .bind(project as i64)
            .bind(tomorrow)
            .execute(&pool)
            .await
            .unwrap();

        let title = format!("S6-ticket-{}", suffix);

        sqlx::query("INSERT INTO t_team_membership (team_id, user_id, role, joined_at) VALUES ($1, $2, 'member', NOW())")
            .bind(team as i64)
            .bind(owner as i64)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query_scalar::<_, i32>(
            "INSERT INTO tickets_ticket (ticket_key, title, description, status, priority, ticket_type, project_id, author_id, team_id, created_at, updated_at, gantt_order) VALUES ($1, $2, '', 'open', 'medium', 'task', $3, $4, $5, NOW(), NOW(), 0) RETURNING id::int4"
        )
        .bind(format!("S6-{}", suffix))
        .bind(&title)
        .bind(project)
        .bind(owner)
        .bind(team)
        .fetch_one(&pool)
        .await
        .unwrap();

        // Project ゲストが検索
        let results = global_search(&pool, guest, &suffix, 10).await.unwrap();
        let found = results.iter().any(|r| r["type"] == "ticket" && r["title"] == title);
        assert!(found, "期限内の Project ゲストはチケットを見つけられるべき");
    }

    #[tokio::test]
    async fn search_s7_project_guest_cannot_find_ticket_after_validity() {
        // S7: Project ゲスト、期限切れ
        let Some(pool) = test_support::test_pool().await else { return; };
        let owner = test_support::create_test_user(&pool, "s7o").await;
        let project = test_support::create_test_project(&pool, "S7", owner).await;

        let team = sqlx::query_scalar::<_, i32>("SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1")
            .bind(project as i64).fetch_one(&pool).await.unwrap();

        // Project ゲストを作成（期限切れ）
        let suffix = test_support::unique_suffix();
        let guest = test_support::create_test_user(&pool, "s7g").await;
        let yesterday = crate::test_support::db_today().pred_opt().unwrap();
        sqlx::query("INSERT INTO t_team_membership (team_id, user_id, role, scoped_project_id, joined_at, end_date) VALUES ($1, $2, 'member', $3, NOW(), $4)")
            .bind(team as i64)
            .bind(guest as i64)
            .bind(project as i64)
            .bind(yesterday)
            .execute(&pool)
            .await
            .unwrap();

        let title = format!("S7-ticket-{}", suffix);

        sqlx::query("INSERT INTO t_team_membership (team_id, user_id, role, joined_at) VALUES ($1, $2, 'member', NOW())")
            .bind(team as i64)
            .bind(owner as i64)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query_scalar::<_, i32>(
            "INSERT INTO tickets_ticket (ticket_key, title, description, status, priority, ticket_type, project_id, author_id, team_id, created_at, updated_at, gantt_order) VALUES ($1, $2, '', 'open', 'medium', 'task', $3, $4, $5, NOW(), NOW(), 0) RETURNING id::int4"
        )
        .bind(format!("S7-{}", suffix))
        .bind(&title)
        .bind(project)
        .bind(owner)
        .bind(team)
        .fetch_one(&pool)
        .await
        .unwrap();

        // Project ゲストが検索（期限切れ）
        let results = global_search(&pool, guest, &suffix, 10).await.unwrap();
        let found = results.iter().any(|r| r["type"] == "ticket" && r["title"] == title);
        assert!(!found, "期限切れの Project ゲストはチケットを見つけられないべき");
    }
}
