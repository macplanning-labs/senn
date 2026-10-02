use serde_json::{json, Value};
/// infrastructure/repositories/search_repo.rs — グローバル検索
///
/// Django apps/api/views/search.py の移植。チケット・Wiki・プロジェクトを横断検索。
///
/// 見える範囲(アクセス制御の再設計 D-6。詳細設計書 §10.5): 試運転のスイッチ `ACCESS_ENFORCE_SEARCH` に従う。
/// - `off` / `shadow`: 今の判定(チケットは所属の確認、Wiki・プロジェクトは確認なし)
/// - `on`: 新しい判定(`scope_sql` のチケット・Wiki・プロジェクト)
///
/// `shadow` では、新旧で結果が分かれる行を、裏で記録する(種類ごとに ticket / wiki / project として記録)。
use sqlx::{PgPool, Postgres, QueryBuilder, Row};

use super::ticket_repo::{push_access_expr, TicketAccessClause};
use crate::domain::access::Scope;
use crate::infrastructure::access::{
    scope_sql,
    shadow::{self, Direction, Mode, Resource},
};

const ROUTE: &str = "GET /api/v1/search/";

/// チケット詳細の画面 URL（frontend の buildTicketDetailPath と同じ形）
fn ticket_url(project_prefix: Option<&str>, team_slug: Option<&str>, ticket_key: &str) -> String {
    match (project_prefix, team_slug) {
        (Some(prefix), _) => format!("/project/{}/tickets/{}", prefix, ticket_key),
        (None, Some(slug)) => format!("/team/{}/tickets/{}", slug, ticket_key),
        (None, None) => format!("/tickets/{}", ticket_key),
    }
}

/// 検索の種類ごとの、見える範囲の条件
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Ticket,
    Wiki,
    Project,
}

impl Kind {
    fn resource(self) -> Resource {
        match self {
            Kind::Ticket => Resource::Ticket,
            Kind::Wiki => Resource::Wiki,
            Kind::Project => Resource::Project,
        }
    }

    /// 名前の一致で絞り込む、ID だけの問い合わせの頭(試運転の差分に使う)
    fn push_id_query<'a>(self, qb: &mut QueryBuilder<'a, Postgres>, pattern: &str) {
        match self {
            Kind::Ticket => qb
                .push("SELECT t.id::int8 FROM tickets_ticket t WHERE (t.title ILIKE ")
                .push_bind(pattern.to_string())
                .push(" OR t.ticket_key ILIKE ")
                .push_bind(pattern.to_string())
                .push(")"),
            Kind::Wiki => qb
                .push("SELECT w.id::int8 FROM wiki_page w WHERE w.title ILIKE ")
                .push_bind(pattern.to_string()),
            Kind::Project => qb
                .push("SELECT p.id::int8 FROM tickets_project p WHERE p.name ILIKE ")
                .push_bind(pattern.to_string()),
        };
    }
}

/// どちらの判定で絞るか
#[derive(Clone)]
enum Access {
    /// 今の判定(チケットは所属の確認。Wiki・プロジェクトは確認なし)
    Legacy(i32),
    /// 新しい判定
    Scope(Scope),
}

/// 条件の式(`Kind` の別名 t / w / p を前提)
fn push_visible(qb: &mut QueryBuilder<'_, Postgres>, kind: Kind, access: &Access) {
    match (kind, access) {
        (Kind::Ticket, Access::Legacy(uid)) => {
            push_access_expr(qb, &TicketAccessClause::Legacy(*uid));
        }
        (Kind::Ticket, Access::Scope(scope)) => scope_sql::push_ticket_visible(qb, "t", scope),
        (Kind::Wiki, Access::Scope(scope)) => scope_sql::push_wiki_visible(qb, "w", scope),
        (Kind::Project, Access::Scope(scope)) => scope_sql::push_project_visible(qb, "p.id", scope),
        (Kind::Wiki | Kind::Project, Access::Legacy(_)) => {
            qb.push("TRUE");
        }
    };
}

/// 試運転: 名前の一致した行のうち、新旧で見え方が分かれる行(最大 200 件ずつ)を、裏で記録する
fn spawn_shadow(pool: &PgPool, kind: Kind, pattern: &str, user_id: i32, scope: &Scope) {
    let pool = pool.clone();
    let pattern = pattern.to_string();
    let legacy = Access::Legacy(user_id);
    let new = Access::Scope(scope.clone());
    tokio::spawn(async move {
        for (dir, a, b) in [
            (Direction::NewlyHidden, &legacy, &new),
            (Direction::NewlyVisible, &new, &legacy),
        ] {
            let mut qb = QueryBuilder::new("");
            kind.push_id_query(&mut qb, &pattern);
            // NULL を偽として扱う(NOT NULL が NULL になり、行が落ちるのを防ぐ)
            qb.push(" AND COALESCE(");
            push_visible(&mut qb, kind, a);
            qb.push(", FALSE) AND NOT COALESCE(");
            push_visible(&mut qb, kind, b);
            qb.push(", FALSE) LIMIT 200");
            match qb.build_query_scalar::<i64>().fetch_all(&pool).await {
                Ok(ids) => shadow::record(
                    &pool,
                    kind.resource(),
                    Some(user_id),
                    ROUTE,
                    ids.into_iter().map(|id| (dir, id)).collect(),
                ),
                Err(e) => {
                    tracing::warn!("[認可/試運転] 検索の差分の計算に失敗(検索は続行): {:?}", e);
                    return;
                }
            }
        }
    });
}

/// グローバル検索
///
/// `user_id`: 今の判定に使う利用者。`scope`: 新しい判定の見える範囲(`Viewer::scope()`)
pub async fn global_search(
    pool: &PgPool,
    user_id: i32,
    scope: &Scope,
    query: &str,
    limit: i64,
) -> anyhow::Result<Vec<Value>> {
    let pattern = format!("%{}%", query);
    let mode = shadow::mode(Resource::Search);
    let access = if mode == Mode::On {
        Access::Scope(scope.clone())
    } else {
        Access::Legacy(user_id)
    };
    if mode == Mode::Shadow {
        for kind in [Kind::Ticket, Kind::Wiki, Kind::Project] {
            spawn_shadow(pool, kind, &pattern, user_id, scope);
        }
    }
    search_with(pool, &access, &pattern, limit).await
}

/// 決めた判定で検索する(`pattern` は ILIKE の形)
async fn search_with(
    pool: &PgPool,
    access: &Access,
    pattern: &str,
    limit: i64,
) -> anyhow::Result<Vec<Value>> {
    let mut results = Vec::new();

    // --- チケット検索(title一致 + ticket_key一致、重複除去) ---
    let mut qb = QueryBuilder::new(
        "SELECT DISTINCT t.id::int4, t.ticket_key, t.title, t.status, p.prefix, team_m.slug AS team_slug
         FROM tickets_ticket t
         LEFT JOIN tickets_project p ON t.project_id = p.id
         LEFT JOIN m_team team_m ON t.team_id = team_m.id
         WHERE (t.title ILIKE ",
    );
    qb.push_bind(pattern.to_string())
        .push(" OR t.ticket_key ILIKE ")
        .push_bind(pattern.to_string())
        .push(") AND ");
    push_visible(&mut qb, Kind::Ticket, access);
    qb.push(" LIMIT ").push_bind(limit);
    let ticket_rows = qb.build().fetch_all(pool).await?;

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
    let mut qb = QueryBuilder::new(
        "SELECT w.id::int4, w.title, p.prefix
         FROM wiki_page w
         LEFT JOIN tickets_project p ON w.project_id = p.id
         WHERE w.title ILIKE ",
    );
    qb.push_bind(pattern.to_string()).push(" AND ");
    push_visible(&mut qb, Kind::Wiki, access);
    qb.push(" LIMIT ").push_bind(limit);
    let wiki_rows = qb.build().fetch_all(pool).await?;

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
    let mut qb = QueryBuilder::new(
        "SELECT p.id::int4, p.name, p.prefix FROM tickets_project p WHERE p.name ILIKE ",
    );
    qb.push_bind(pattern.to_string()).push(" AND ");
    push_visible(&mut qb, Kind::Project, access);
    qb.push(" LIMIT 5");
    let project_rows = qb.build().fetch_all(pool).await?;

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
    use super::{global_search, search_with, Access};
    use crate::domain::access::Principal;
    use crate::infrastructure::access::viewer_repo;
    use crate::test_support;

    /// 今の判定の経路で検索する(既定の試運転と同じ応答)
    async fn search_as(
        pool: &sqlx::PgPool,
        user_id: i32,
        q: &str,
    ) -> anyhow::Result<Vec<serde_json::Value>> {
        let viewer =
            viewer_repo::load(pool, Principal::Human { user_id }, viewer_repo::today_utc())
                .await?
                .expect("有効なユーザー");
        global_search(pool, user_id, &viewer.scope(), q, 10).await
    }

    /// 新しい判定(on)の経路で検索する
    async fn search_new(pool: &sqlx::PgPool, user_id: i32, q: &str) -> Vec<serde_json::Value> {
        let viewer =
            viewer_repo::load(pool, Principal::Human { user_id }, viewer_repo::today_utc())
                .await
                .unwrap()
                .expect("有効なユーザー");
        search_with(
            pool,
            &Access::Scope(viewer.scope()),
            &format!("%{}%", q),
            10,
        )
        .await
        .unwrap()
    }

    #[test]
    fn ticket_url_matches_frontend_routes() {
        assert_eq!(
            ticket_url(Some("PRS"), Some("t"), "DEMO-000001"),
            "/project/PRS/tickets/DEMO-000001"
        );
        assert_eq!(
            ticket_url(None, Some("team-1"), "TT-000001"),
            "/team/team-1/tickets/TT-000001"
        );
        assert_eq!(ticket_url(None, None, "X-000001"), "/tickets/X-000001");
    }

    #[tokio::test]
    async fn search_s1_team_member_finds_team_ticket_by_title() {
        // S1: チームAのチケットを、チームAのメンバーが検索（タイトル）
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let member = test_support::create_test_user(&pool, "s1m").await;
        let project = test_support::create_test_project(&pool, "S1", member).await;

        // プロジェクトのチームを取得
        let team = sqlx::query_scalar::<_, i32>(
            "SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1",
        )
        .bind(project as i64)
        .fetch_one(&pool)
        .await
        .unwrap();

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
        let results = search_as(&pool, member, &suffix).await.unwrap();
        let found = results
            .iter()
            .any(|r| r["type"] == "ticket" && r["id"] == ticket_id);
        assert!(found, "チームメンバーがチームのチケットを見つけられるべき");
    }

    #[tokio::test]
    async fn search_s2_non_member_cannot_find_team_ticket_by_title() {
        // S2: 同じチケットを、チームAに所属しない人が検索（タイトル）
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let member = test_support::create_test_user(&pool, "s2m").await;
        let outsider = test_support::create_test_user(&pool, "s2o").await;
        let project = test_support::create_test_project(&pool, "S2", member).await;

        let team = sqlx::query_scalar::<_, i32>(
            "SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1",
        )
        .bind(project as i64)
        .fetch_one(&pool)
        .await
        .unwrap();

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
        let results = search_as(&pool, outsider, &suffix).await.unwrap();
        let found = results
            .iter()
            .any(|r| r["type"] == "ticket" && r["title"] == title);
        assert!(
            !found,
            "チームに所属していない人はチケットを見つけられないべき"
        );
    }

    #[tokio::test]
    async fn search_s3_non_member_cannot_find_team_ticket_by_key() {
        // S3: 同じチケットを、所属しない人がチケット番号で検索
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let member = test_support::create_test_user(&pool, "s3m").await;
        let outsider = test_support::create_test_user(&pool, "s3o").await;
        let project = test_support::create_test_project(&pool, "S3", member).await;

        let team = sqlx::query_scalar::<_, i32>(
            "SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1",
        )
        .bind(project as i64)
        .fetch_one(&pool)
        .await
        .unwrap();

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
        let results = search_as(&pool, outsider, &suffix).await.unwrap();
        let found = results
            .iter()
            .any(|r| r["type"] == "ticket" && r["key"] == key);
        assert!(
            !found,
            "チームに所属していない人はチケット番号でも見つけられないべき"
        );
    }

    #[tokio::test]
    async fn search_s4_staff_can_find_team_ticket() {
        // S4: 同じチケットを、staff が検索
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let member = test_support::create_test_user(&pool, "s4m").await;
        let project = test_support::create_test_project(&pool, "S4", member).await;

        let team = sqlx::query_scalar::<_, i32>(
            "SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1",
        )
        .bind(project as i64)
        .fetch_one(&pool)
        .await
        .unwrap();

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
        let results = search_as(&pool, staff_user, &suffix).await.unwrap();
        let found = results
            .iter()
            .any(|r| r["type"] == "ticket" && r["title"] == title);
        assert!(found, "Staff はすべてのチケットを見つけられるべき");
    }

    #[tokio::test]
    async fn search_s5_teamless_ticket_is_visible_to_staff_only() {
        // S5: チームにもプロジェクトにも紐付かないチケットは、staff でない人には出ず、staff には出る
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
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

        let by_staff = search_as(&pool, staff_user, &suffix).await.unwrap();
        assert!(
            by_staff
                .iter()
                .any(|r| r["type"] == "ticket" && r["title"] == title),
            "staff にはチームなしチケットも出るべき"
        );
        for viewer in [author, other] {
            let results = search_as(&pool, viewer, &suffix).await.unwrap();
            assert!(
                !results
                    .iter()
                    .any(|r| r["type"] == "ticket" && r["title"] == title),
                "staff でない人（作成者本人を含む）にはチームなしチケットは出ない"
            );
        }
    }

    #[tokio::test]
    async fn search_s6_project_guest_can_find_ticket_within_validity() {
        // S6: Project ゲストが、その Project のチケットを検索。期限内
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let owner = test_support::create_test_user(&pool, "s6o").await;
        let project = test_support::create_test_project(&pool, "S6", owner).await;

        let team = sqlx::query_scalar::<_, i32>(
            "SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1",
        )
        .bind(project as i64)
        .fetch_one(&pool)
        .await
        .unwrap();

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
        let results = search_as(&pool, guest, &suffix).await.unwrap();
        let found = results
            .iter()
            .any(|r| r["type"] == "ticket" && r["title"] == title);
        assert!(found, "期限内の Project ゲストはチケットを見つけられるべき");
    }

    #[tokio::test]
    async fn search_s7_project_guest_cannot_find_ticket_after_validity() {
        // S7: Project ゲスト、期限切れ
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let owner = test_support::create_test_user(&pool, "s7o").await;
        let project = test_support::create_test_project(&pool, "S7", owner).await;

        let team = sqlx::query_scalar::<_, i32>(
            "SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1",
        )
        .bind(project as i64)
        .fetch_one(&pool)
        .await
        .unwrap();

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
        let results = search_as(&pool, guest, &suffix).await.unwrap();
        let found = results
            .iter()
            .any(|r| r["type"] == "ticket" && r["title"] == title);
        assert!(
            !found,
            "期限切れの Project ゲストはチケットを見つけられないべき"
        );
    }

    /// 新しい判定(D-6): Public チームは Full Member なら所属なしで見つかり、Private にすると見つからない。
    /// チケット・Wiki・プロジェクトの 3 つとも同じ
    #[tokio::test]
    async fn search_new_rule_follows_team_visibility() {
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let author = test_support::create_test_user(&pool, "snr-a").await;
        let outsider = test_support::create_test_user(&pool, "snr-o").await;
        let base = format!("SN{}", &test_support::unique_suffix()[..6]);
        let project = test_support::create_test_project(&pool, &base, author).await;
        let ticket = test_support::create_test_ticket(&pool, project, &base, author).await;
        let team: i32 = sqlx::query_scalar(
            "SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1::int8",
        )
        .bind(project as i64)
        .fetch_one(&pool)
        .await
        .unwrap();
        let wiki: i32 = sqlx::query_scalar(
            "INSERT INTO wiki_page (team_id, title, slug, category, content, author_id, created_at, updated_at)
             VALUES ($1, $2, $3, 'general', '', $4, NOW(), NOW()) RETURNING id::int4",
        )
        .bind(team as i64)
        .bind(format!("{base}-wiki"))
        .bind(format!("snr-{}", test_support::unique_suffix()))
        .bind(author as i64)
        .fetch_one(&pool)
        .await
        .unwrap();

        let has = |rs: &[serde_json::Value], ty: &str, id: i32| {
            rs.iter().any(|r| r["type"] == ty && r["id"] == id)
        };

        // Public(既定): 所属していない Full Member にも見える
        let rs = search_new(&pool, outsider, &base).await;
        assert!(has(&rs, "ticket", ticket), "Public のチケット");
        assert!(has(&rs, "wiki", wiki), "Public のチームの Wiki");
        assert!(
            has(&rs, "project", project),
            "Public のチームのプロジェクト"
        );
        // 今の判定では、所属していない人にチケットは見えない(Wiki・プロジェクトは確認なし)
        let rs = search_as(&pool, outsider, &base).await.unwrap();
        assert!(!has(&rs, "ticket", ticket));
        assert!(has(&rs, "wiki", wiki) && has(&rs, "project", project));

        // Private: 所属していない人には、3 つとも見えない。作成者(所属なし)にも見えない
        sqlx::query("UPDATE m_team SET visibility = 'private' WHERE id = $1::int8")
            .bind(team as i64)
            .execute(&pool)
            .await
            .unwrap();
        let rs = search_new(&pool, outsider, &base).await;
        assert!(!has(&rs, "ticket", ticket));
        assert!(!has(&rs, "wiki", wiki));
        assert!(!has(&rs, "project", project));

        // Private のメンバーには見える
        sqlx::query("INSERT INTO t_team_membership (team_id, user_id, role, joined_at) VALUES ($1::int8, $2::int8, 'member', NOW())")
            .bind(team as i64)
            .bind(author as i64)
            .execute(&pool)
            .await
            .unwrap();
        let rs = search_new(&pool, author, &base).await;
        assert!(has(&rs, "ticket", ticket));
        assert!(has(&rs, "wiki", wiki));
        assert!(has(&rs, "project", project));
    }

    /// 試運転: 今の判定と新しい判定で分かれる行を記録する(Public のチケットは、所属なしの人に新しく見える)
    #[tokio::test]
    async fn search_shadow_records_differences() {
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let author = test_support::create_test_user(&pool, "sns-a").await;
        let outsider = test_support::create_test_user(&pool, "sns-o").await;
        let base = format!("SS{}", &test_support::unique_suffix()[..6]);
        let project = test_support::create_test_project(&pool, &base, author).await;
        let ticket = test_support::create_test_ticket(&pool, project, &base, author).await;
        let viewer = viewer_repo::load(
            &pool,
            Principal::Human { user_id: outsider },
            viewer_repo::today_utc(),
        )
        .await
        .unwrap()
        .unwrap();
        super::spawn_shadow(
            &pool,
            super::Kind::Ticket,
            &format!("%{base}%"),
            outsider,
            &viewer.scope(),
        );
        let mut n = 0i64;
        for _ in 0..100 {
            n = sqlx::query_scalar(
                "SELECT count(*) FROM access_shadow_diff WHERE resource = 'ticket' AND direction = 'newly_visible' AND resource_id = $1::int8 AND user_id = $2::int8",
            )
            .bind(ticket as i64)
            .bind(outsider as i64)
            .fetch_one(&pool)
            .await
            .unwrap();
            if n > 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert_eq!(n, 1, "新しく見えるチケットとして記録される");
    }
}
