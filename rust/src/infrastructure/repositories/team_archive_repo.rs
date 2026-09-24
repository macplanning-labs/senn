/// infrastructure/repositories/team_archive_repo.rs — チームのアーカイブ・復元
///
/// 設計(WIPAPPDEV-000069 第2段階):
/// - アーカイブしたチームは閲覧専用。チケット・サイクル・コメント・添付の書き込みは、
///   DBトリガー(20260919110001_team_archive.sql)が経路によらず拒否する。
/// - アーカイブ・復元できる人: システム管理者 / そのチームの管理者(role='admin')
/// - アーカイブできない条件: そのチームを外すと、担当チームが1つも(アーカイブされていない
///   チームが)無くなる「進行中・計画中」のプロジェクトがある場合

use sqlx::PgPool;

/// アーカイブを止めているプロジェクト
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockingProject {
    pub id: i32,
    pub prefix: String,
    pub name: String,
    pub status: String,
}

/// 進行中・計画中のプロジェクトの担当チームが不在になるため、アーカイブできない。
#[derive(Debug)]
pub struct TeamArchiveBlocked {
    pub projects: Vec<BlockingProject>,
}

impl std::fmt::Display for TeamArchiveBlocked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "team archive blocked: {} project(s) would have no active team", self.projects.len())
    }
}

impl std::error::Error for TeamArchiveBlocked {}

impl TeamArchiveBlocked {
    pub fn user_message(&self) -> String {
        let names: Vec<String> = self
            .projects
            .iter()
            .take(5)
            .map(|p| format!("{}({})", p.name, p.prefix))
            .collect();
        let more = if self.projects.len() > 5 {
            format!(" ほか{}件", self.projects.len() - 5)
        } else {
            String::new()
        };
        format!(
            "進行中・計画中のプロジェクトで、このチームが最後のチームになっているためアーカイブできません: {}{}。\
             先に別のチームを追加するか、プロジェクトを完了にしてください",
            names.join("、"),
            more
        )
    }
}

/// アーカイブ・復元できるか(システム管理者 / そのチームの管理者)。
pub async fn can_manage(pool: &PgPool, team_id: i32, user_id: i32, is_staff: bool) -> anyhow::Result<bool> {
    if is_staff {
        return Ok(true);
    }
    let found: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM t_team_membership
            WHERE team_id = $1 AND user_id = $2 AND role = 'admin' AND scoped_project_id IS NULL
         )",
    )
    .bind(team_id as i64)
    .bind(user_id as i64)
    .fetch_one(pool)
    .await?;
    Ok(found)
}

/// 閲覧者が、アーカイブ・復元できるチームの id(システム管理者は全チーム / それ以外は、自分が管理者のチーム)。
pub async fn manageable_team_ids(pool: &PgPool, user_id: i32, is_staff: bool) -> anyhow::Result<std::collections::HashSet<i32>> {
    let ids: Vec<i32> = if is_staff {
        sqlx::query_scalar("SELECT id::int4 FROM m_team").fetch_all(pool).await?
    } else {
        sqlx::query_scalar(
            "SELECT DISTINCT team_id::int4 FROM t_team_membership
             WHERE user_id = $1 AND role = 'admin' AND scoped_project_id IS NULL",
        )
        .bind(user_id as i64)
        .fetch_all(pool)
        .await?
    };
    Ok(ids.into_iter().collect())
}

/// このチームをアーカイブすると、担当チームが不在になる「進行中・計画中」のプロジェクト。
pub async fn blocking_projects(pool: &PgPool, team_id: i32) -> anyhow::Result<Vec<BlockingProject>> {
    let rows = sqlx::query_as::<_, (i32, String, String, String)>(
        "SELECT p.id::int4, p.prefix, p.name, p.status
         FROM tickets_project_teams pt
         JOIN tickets_project p ON p.id = pt.project_id
         WHERE pt.team_id = $1
           AND p.status IN ('planned', 'in_progress')
           AND NOT EXISTS (
                 SELECT 1 FROM tickets_project_teams other
                 WHERE other.project_id = p.id
                   AND other.team_id <> $1
                   AND NOT team_is_archived(other.team_id))
         ORDER BY p.name",
    )
    .bind(team_id as i64)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(id, prefix, name, status)| BlockingProject { id, prefix, name, status }).collect())
}

/// チームをアーカイブする。すでにアーカイブ済みなら何もしない(冪等)。
/// 条件を満たさなければ `TeamArchiveBlocked`。チームが無ければ `Ok(false)`。
pub async fn archive(pool: &PgPool, team_id: i32, actor_id: i32) -> anyhow::Result<bool> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM m_team WHERE id = $1)")
        .bind(team_id as i64)
        .fetch_one(pool)
        .await?;
    if !exists {
        return Ok(false);
    }
    let blockers = blocking_projects(pool, team_id).await?;
    if !blockers.is_empty() {
        return Err(TeamArchiveBlocked { projects: blockers }.into());
    }
    sqlx::query(
        "UPDATE m_team SET archived_at = NOW(), archived_by = $2
         WHERE id = $1 AND archived_at IS NULL",
    )
    .bind(team_id as i64)
    .bind(actor_id as i64)
    .execute(pool)
    .await?;
    Ok(true)
}

/// チームを復元する(冪等)。チームが無ければ `Ok(false)`。
pub async fn unarchive(pool: &PgPool, team_id: i32) -> anyhow::Result<bool> {
    let affected = sqlx::query(
        "UPDATE m_team SET archived_at = NULL, archived_by = NULL WHERE id = $1",
    )
    .bind(team_id as i64)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(affected > 0)
}

/// 指定したチームのうち、アーカイブ済みのものがあるか。
pub async fn any_archived(pool: &PgPool, team_ids: &[i32]) -> anyhow::Result<bool> {
    if team_ids.is_empty() {
        return Ok(false);
    }
    let ids: Vec<i64> = team_ids.iter().map(|v| *v as i64).collect();
    let found: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM m_team WHERE id = ANY($1) AND archived_at IS NOT NULL)",
    )
    .bind(&ids)
    .fetch_one(pool)
    .await?;
    Ok(found)
}

/// チームがアーカイブ済みか。
pub async fn is_archived(pool: &PgPool, team_id: i32) -> anyhow::Result<bool> {
    any_archived(pool, &[team_id]).await
}

/// DBトリガーが「アーカイブ済みチームへの書き込み」を拒否したエラーか。
pub fn is_team_archived_error(e: &(dyn std::error::Error + 'static)) -> bool {
    let mut cur: Option<&(dyn std::error::Error + 'static)> = Some(e);
    while let Some(err) = cur {
        if err.to_string().contains("team_archived") {
            return true;
        }
        cur = err.source();
    }
    false
}

/// 利用者向けの理由(ticket/cycle の書き込みを拒否した時)。
pub const ARCHIVED_READ_ONLY_MESSAGE: &str =
    "このチームはアーカイブされているため、閲覧専用です(変更するには、チームを復元してください)";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;

    async fn project_of(pool: &PgPool, project_id: i32) -> i32 {
        sqlx::query_scalar::<_, i32>("SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1")
            .bind(project_id as i64).fetch_one(pool).await.unwrap()
    }

    #[test]
    fn blocked_message_lists_projects_and_truncates() {
        let blocked = TeamArchiveBlocked {
            projects: (1..=7).map(|i| BlockingProject { id: i as i32, prefix: format!("P{i}"), name: format!("案件{i}"), status: "in_progress".to_string() }).collect(),
        };
        let m = blocked.user_message();
        assert!(m.contains("案件1(P1)") && m.contains("案件5(P5)"));
        assert!(!m.contains("案件6(P6)"));
        assert!(m.contains("ほか2件"));
    }

    #[test]
    fn detects_the_trigger_error_message() {
        let e = anyhow::anyhow!("error returned from database: team_archived: team_id=3");
        assert!(is_team_archived_error(e.as_ref()));
        let other = anyhow::anyhow!("duplicate key");
        assert!(!is_team_archived_error(other.as_ref()));
    }

    #[tokio::test]
    async fn archive_is_blocked_when_team_is_the_last_of_an_active_project() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let user = test_support::create_test_user(&pool, "ar1").await;
        let project = test_support::create_test_project(&pool, "AR1", user).await;
        sqlx::query("UPDATE tickets_project SET status = 'in_progress' WHERE id = $1")
            .bind(project as i64).execute(&pool).await.unwrap();
        let team = project_of(&pool, project).await;

        let err = archive(&pool, team, user).await.expect_err("blocked");
        let blocked = err.downcast_ref::<TeamArchiveBlocked>().expect("TeamArchiveBlocked");
        assert!(blocked.projects.iter().any(|p| p.prefix.starts_with("AR1")));
        assert!(blocked.projects.iter().any(|p| p.status == "in_progress"));
        assert!(!is_archived(&pool, team).await.unwrap());

        // 別のチームを追加すれば、アーカイブできる
        let other = test_support::create_test_team(&pool, "ar1b").await;
        crate::infrastructure::repositories::resource_repo::add_project_team(&pool, project, other).await.unwrap();
        assert!(archive(&pool, team, user).await.unwrap());
        assert!(is_archived(&pool, team).await.unwrap());
        // 冪等
        assert!(archive(&pool, team, user).await.unwrap());

        // 「残っているチーム」がアーカイブ済みだと、今度はそちらがアーカイブできない
        assert!(archive(&pool, other, user).await.unwrap_err().downcast_ref::<TeamArchiveBlocked>().is_some());

        // 復元すれば戻る
        assert!(unarchive(&pool, team).await.unwrap());
        assert!(!is_archived(&pool, team).await.unwrap());
    }

    #[tokio::test]
    async fn archive_is_not_blocked_by_completed_or_paused_projects() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let user = test_support::create_test_user(&pool, "ar2").await;
        let project = test_support::create_test_project(&pool, "AR2", user).await;
        let team = project_of(&pool, project).await;
        for status in ["completed", "paused"] {
            sqlx::query("UPDATE tickets_project SET status = $2 WHERE id = $1")
                .bind(project as i64).bind(status).execute(&pool).await.unwrap();
            assert!(blocking_projects(&pool, team).await.unwrap().is_empty(), "{status} は止めない");
        }
        sqlx::query("UPDATE tickets_project SET status = 'planned' WHERE id = $1")
            .bind(project as i64).execute(&pool).await.unwrap();
        assert_eq!(blocking_projects(&pool, team).await.unwrap().len(), 1, "計画中は止める");
    }

    /// 閲覧専用: アーカイブ済みチームのチケット・サイクル・コメント・添付の書き込みを、DBが拒否する
    #[tokio::test]
    async fn archived_team_is_read_only_at_db_level_and_restorable() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let user = test_support::create_test_user(&pool, "ro").await;
        let project = test_support::create_test_project(&pool, "RO", user).await;
        let team = project_of(&pool, project).await;
        let ticket = test_support::create_test_ticket(&pool, project, "RO", user).await;
        // 別チームを足して、アーカイブできる状態にする
        let other = test_support::create_test_team(&pool, "rob").await;
        crate::infrastructure::repositories::resource_repo::add_project_team(&pool, project, other).await.unwrap();

        sqlx::query("INSERT INTO tickets_comment (ticket_id, author_id, body, created_at, updated_at) VALUES ($1, $2, 'before', NOW(), NOW())")
            .bind(ticket as i64).bind(user as i64).execute(&pool).await.expect("アーカイブ前は書ける");

        // (アーカイブ後は、そのチームでチケットを作れないため、別チームへ移す用のチケットを先に作る)
        let other_ticket = test_support::create_test_ticket(&pool, project, "RO2", user).await;
        assert!(archive(&pool, team, user).await.unwrap());

        let blocked = |r: Result<sqlx::postgres::PgQueryResult, sqlx::Error>, what: &str| {
            let err = r.expect_err(what);
            assert!(is_team_archived_error(&err), "{what}: {err}");
        };
        blocked(sqlx::query("UPDATE tickets_ticket SET title = 'x' WHERE id = $1").bind(ticket as i64).execute(&pool).await, "チケット更新");
        blocked(sqlx::query("DELETE FROM tickets_ticket WHERE id = $1").bind(ticket as i64).execute(&pool).await, "チケット削除");
        blocked(
            sqlx::query("INSERT INTO tickets_comment (ticket_id, author_id, body, created_at, updated_at) VALUES ($1, $2, 'after', NOW(), NOW())")
                .bind(ticket as i64).bind(user as i64).execute(&pool).await,
            "コメント追加",
        );
        blocked(
            sqlx::query("INSERT INTO t_cycle (project_id, team_id, name, number, description, start_date, end_date, status, created_at) VALUES ($1, $2, 'c', 1, '', CURRENT_DATE, CURRENT_DATE + 7, 'planned', NOW())")
                .bind(project as i64).bind(team as i64).execute(&pool).await,
            "サイクル作成",
        );
        // 他のチームのチケットは、引き続き書ける(新しいチケットを、アーカイブされていないチームで作る)
        let live_ticket: i64 = sqlx::query_scalar(
            "INSERT INTO tickets_ticket (ticket_key, title, description, status, priority, ticket_type, project_id, team_id, author_id, gantt_order, created_at, updated_at)
             VALUES ($4, 'live', '', 'open', 'medium', 'task', $1, $2, $3, 99, NOW(), NOW()) RETURNING id")
            .bind(project as i64).bind(other as i64).bind(user as i64)
            .bind(format!("RO3-{}", test_support::unique_suffix()))
            .fetch_one(&pool).await
            .expect("アーカイブされていないチームでは作成できる");
        sqlx::query("UPDATE tickets_ticket SET title = 'live2' WHERE id = $1").bind(live_ticket).execute(&pool).await
            .expect("アーカイブされていないチームのチケットは更新できる");
        // ただし、アーカイブ済みチームのチケットを、別の(アーカイブされていない)チームへ移すことも、元が閲覧専用なので拒否される
        blocked(sqlx::query("UPDATE tickets_ticket SET team_id = $2 WHERE id = $1").bind(other_ticket as i64).bind(other as i64).execute(&pool).await, "アーカイブ済みチームのチケットの付け替え");
        // アーカイブ済みのチームへチケットを移す操作も拒否
        blocked(sqlx::query("UPDATE tickets_ticket SET team_id = $2 WHERE id = $1").bind(live_ticket).bind(team as i64).execute(&pool).await, "アーカイブ済みチームへの移動");

        // 復元すれば、また書ける
        assert!(unarchive(&pool, team).await.unwrap());
        sqlx::query("UPDATE tickets_ticket SET title = 'restored' WHERE id = $1").bind(ticket as i64).execute(&pool).await.expect("復元後は更新できる");
    }

    #[tokio::test]
    async fn can_manage_allows_staff_and_team_admin_only() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let team = test_support::create_test_team(&pool, "cm").await;
        let admin = test_support::create_test_user(&pool, "cma").await;
        let member = test_support::create_test_user(&pool, "cmm").await;
        for (u, role) in [(admin, "admin"), (member, "member")] {
            sqlx::query("INSERT INTO t_team_membership (team_id, user_id, role, joined_at) VALUES ($1, $2, $3, NOW())")
                .bind(team as i64).bind(u as i64).bind(role).execute(&pool).await.unwrap();
        }
        assert!(can_manage(&pool, team, admin, false).await.unwrap());
        assert!(can_manage(&pool, team, member, true).await.unwrap(), "システム管理者");
        assert!(!can_manage(&pool, team, member, false).await.unwrap());
    }

    #[tokio::test]
    async fn manageable_team_ids_follow_staff_and_team_admin() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let mine = test_support::create_test_team(&pool, "mgA").await;
        let other = test_support::create_test_team(&pool, "mgB").await;
        let admin = test_support::create_test_user(&pool, "mga").await;
        let member = test_support::create_test_user(&pool, "mgm").await;
        for (t, u, role) in [(mine, admin, "admin"), (mine, member, "member"), (other, member, "member")] {
            sqlx::query("INSERT INTO t_team_membership (team_id, user_id, role, joined_at) VALUES ($1, $2, $3, NOW())")
                .bind(t as i64).bind(u as i64).bind(role).execute(&pool).await.unwrap();
        }
        let admin_ids = manageable_team_ids(&pool, admin, false).await.unwrap();
        assert!(admin_ids.contains(&mine) && !admin_ids.contains(&other), "管理者は自分のチームだけ");
        assert!(manageable_team_ids(&pool, member, false).await.unwrap().is_empty(), "一般メンバーは無し");
        let staff_ids = manageable_team_ids(&pool, member, true).await.unwrap();
        assert!(staff_ids.contains(&mine) && staff_ids.contains(&other), "システム管理者は全チーム");
    }

    /// プロジェクトの担当チームに、アーカイブ済みかどうかが付く(Overview のバッジ用)
    #[tokio::test]
    async fn project_teams_carry_the_archived_flag() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let user = test_support::create_test_user(&pool, "pf").await;
        let project = test_support::create_test_project(&pool, "PF", user).await;
        sqlx::query("UPDATE tickets_project SET status = 'in_progress' WHERE id = $1").bind(project as i64).execute(&pool).await.unwrap();
        let team = project_of(&pool, project).await;
        let other = test_support::create_test_team(&pool, "pfb").await;
        crate::infrastructure::repositories::resource_repo::add_project_team(&pool, project, other).await.unwrap();

        let before = crate::infrastructure::repositories::resource_repo::find_project_by_id(&pool, project, None).await.unwrap().unwrap();
        assert!(before.teams.iter().all(|t| !t.archived));
        assert!(archive(&pool, team, user).await.unwrap());
        let after = crate::infrastructure::repositories::resource_repo::find_project_by_id(&pool, project, None).await.unwrap().unwrap();
        assert!(after.teams.iter().find(|t| t.id == team).unwrap().archived);
        assert!(!after.teams.iter().find(|t| t.id == other).unwrap().archived);
    }
}
