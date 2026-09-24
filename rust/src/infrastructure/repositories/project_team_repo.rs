/// infrastructure/repositories/project_team_repo.rs — プロジェクトの担当(参加)チームの変更に関する権限・利用状況
///
/// 設計(WIPAPPDEV-000069 第1段階):
/// - 変更できる人: システム管理者 / プロジェクトのオーナー / 参加チームの管理者(role='admin')
/// - 追加できるチーム: 自分が所属しているチーム(システム管理者は全チーム)
///   (追加すると、そのチームのメンバーがプロジェクトにアクセスできるようになるため)
/// - 外せる条件: そのチームのチケット・サイクルが、そのプロジェクトに無いこと
///   (残っていると、後でそのチケットを編集した時に「チームがプロジェクトの参加者ではない」エラーになる)
///   ※最後の1チームは外せない既存ルールは resource_repo::remove_project_team が維持する

use serde::Serialize;
use sqlx::PgPool;

/// そのチームが、そのプロジェクトで持っているもの。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeamUsage {
    pub tickets: i64,
    pub cycles: i64,
}

impl TeamUsage {
    pub fn is_empty(&self) -> bool {
        self.tickets == 0 && self.cycles == 0
    }

    /// 外せない理由(利用者向け)。残っているものだけを件数つきで並べる。
    pub fn block_message(&self) -> String {
        let mut parts = Vec::new();
        if self.tickets > 0 {
            parts.push(format!("チケット{}件", self.tickets));
        }
        if self.cycles > 0 {
            parts.push(format!("サイクル{}件", self.cycles));
        }
        format!(
            "このプロジェクトに{}が残っているため、チームを外せません(チームを使わなくする場合はアーカイブを使ってください)",
            parts.join("・")
        )
    }
}

/// 画面向け: 参加チーム1件(利用状況と、外せるかどうかつき)
#[derive(Debug, Clone, Serialize)]
pub struct ParticipatingTeamOut {
    pub id: i32,
    pub name: String,
    pub slug: String,
    pub icon: String,
    pub color: String,
    #[serde(rename = "ticketCount")]
    pub ticket_count: i64,
    #[serde(rename = "cycleCount")]
    pub cycle_count: i64,
    /// アーカイブ済み(閲覧専用)のチームか
    pub archived: bool,
    pub removable: bool,
    /// 外せない場合の理由(外せる、または変更権限が無い場合は null)
    #[serde(rename = "removeBlockedReason")]
    pub remove_blocked_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AddableTeamOut {
    pub id: i32,
    pub name: String,
    pub slug: String,
    pub icon: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectTeamsOut {
    #[serde(rename = "canManage")]
    pub can_manage: bool,
    pub teams: Vec<ParticipatingTeamOut>,
    #[serde(rename = "addableTeams")]
    pub addable_teams: Vec<AddableTeamOut>,
}

/// 担当チームを変更できるか(管理者 / プロジェクトのオーナー / 参加チームの管理者)。
pub async fn can_manage(pool: &PgPool, project_id: i32, user_id: i32, is_staff: bool) -> anyhow::Result<bool> {
    if is_staff {
        return Ok(true);
    }
    let allowed: bool = sqlx::query_scalar(
        "SELECT
            EXISTS (SELECT 1 FROM tickets_project WHERE id = $1 AND owner_id = $2)
         OR EXISTS (
            SELECT 1
            FROM tickets_project_teams pt
            JOIN t_team_membership tm ON tm.team_id = pt.team_id
            WHERE pt.project_id = $1
              AND tm.user_id = $2
              AND tm.role = 'admin'
              AND tm.scoped_project_id IS NULL
         )",
    )
    .bind(project_id as i64)
    .bind(user_id as i64)
    .fetch_one(pool)
    .await?;
    Ok(allowed)
}

/// ユーザーがそのチームに(プロジェクト限定ではなく)所属しているか。
pub async fn user_in_team(pool: &PgPool, user_id: i32, team_id: i32) -> anyhow::Result<bool> {
    let found: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM t_team_membership
            WHERE user_id = $1 AND team_id = $2 AND scoped_project_id IS NULL
         )",
    )
    .bind(user_id as i64)
    .bind(team_id as i64)
    .fetch_one(pool)
    .await?;
    Ok(found)
}

/// そのチームが、そのプロジェクトで持っているチケット・サイクルの件数。
pub async fn team_usage(pool: &PgPool, project_id: i32, team_id: i32) -> anyhow::Result<TeamUsage> {
    let tickets: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tickets_ticket WHERE project_id = $1 AND team_id = $2",
    )
    .bind(project_id as i64)
    .bind(team_id as i64)
    .fetch_one(pool)
    .await?;
    let cycles: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM t_cycle WHERE project_id = $1 AND team_id = $2",
    )
    .bind(project_id as i64)
    .bind(team_id as i64)
    .fetch_one(pool)
    .await?;
    Ok(TeamUsage { tickets, cycles })
}

/// 画面用: 参加チーム(利用状況つき)・変更権限・追加できるチームをまとめて返す。
pub async fn overview(
    pool: &PgPool,
    project_id: i32,
    user_id: i32,
    is_staff: bool,
) -> anyhow::Result<ProjectTeamsOut> {
    let can_manage = can_manage(pool, project_id, user_id, is_staff).await?;

    let rows = sqlx::query_as::<_, (i32, String, String, String, String, i64, i64, bool)>(
        "SELECT t.id::int4, t.name, t.slug, t.icon, t.color,
                (SELECT COUNT(*) FROM tickets_ticket tk WHERE tk.project_id = pt.project_id AND tk.team_id = t.id),
                (SELECT COUNT(*) FROM t_cycle cy WHERE cy.project_id = pt.project_id AND cy.team_id = t.id),
                (t.archived_at IS NOT NULL)
         FROM tickets_project_teams pt
         JOIN m_team t ON t.id = pt.team_id
         WHERE pt.project_id = $1
         ORDER BY pt.joined_at, t.id",
    )
    .bind(project_id as i64)
    .fetch_all(pool)
    .await?;

    let only_one = rows.len() <= 1;
    let teams = rows
        .into_iter()
        .map(|(id, name, slug, icon, color, tickets, cycles, archived)| {
            let usage = TeamUsage { tickets, cycles };
            let reason = if !can_manage {
                None
            } else if only_one {
                Some("プロジェクトには1つ以上のチームが必要です(先に別のチームを追加してください)".to_string())
            } else if !usage.is_empty() {
                Some(usage.block_message())
            } else {
                None
            };
            ParticipatingTeamOut {
                id,
                name,
                slug,
                icon,
                color,
                ticket_count: tickets,
                cycle_count: cycles,
                archived,
                removable: can_manage && reason.is_none(),
                remove_blocked_reason: reason,
            }
        })
        .collect();

    let addable_teams = if can_manage {
        sqlx::query_as::<_, AddableTeamOut>(
            "SELECT t.id::int4 AS id, t.name, t.slug, t.icon, t.color
             FROM m_team t
             WHERE t.archived_at IS NULL
               AND NOT EXISTS (
                     SELECT 1 FROM tickets_project_teams pt WHERE pt.project_id = $1 AND pt.team_id = t.id)
               AND ($3
                    OR EXISTS (
                        SELECT 1 FROM t_team_membership tm
                        WHERE tm.team_id = t.id AND tm.user_id = $2 AND tm.scoped_project_id IS NULL))
             ORDER BY t.name",
        )
        .bind(project_id as i64)
        .bind(user_id as i64)
        .bind(is_staff)
        .fetch_all(pool)
        .await?
    } else {
        Vec::new()
    };

    Ok(ProjectTeamsOut { can_manage, teams, addable_teams })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;

    async fn set_role(pool: &PgPool, team_id: i32, user_id: i32, role: &str) {
        sqlx::query(
            "INSERT INTO t_team_membership (team_id, user_id, role, joined_at) VALUES ($1, $2, $3, NOW())",
        )
        .bind(team_id as i64)
        .bind(user_id as i64)
        .bind(role)
        .execute(pool)
        .await
        .unwrap();
    }

    #[test]
    fn block_message_lists_only_remaining_items() {
        let m = TeamUsage { tickets: 3, cycles: 0 }.block_message();
        assert!(m.contains("チケット3件") && !m.contains("サイクル"));
        let m = TeamUsage { tickets: 1, cycles: 2 }.block_message();
        assert!(m.contains("チケット1件・サイクル2件"));
        assert!(TeamUsage { tickets: 0, cycles: 0 }.is_empty());
    }

    #[tokio::test]
    async fn can_manage_allows_staff_owner_and_team_admin_only() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let owner = test_support::create_test_user(&pool, "own").await;
        let project = test_support::create_test_project(&pool, "PT1", owner).await;
        sqlx::query("UPDATE tickets_project SET owner_id = $2 WHERE id = $1")
            .bind(project as i64).bind(owner as i64).execute(&pool).await.unwrap();
        let team = sqlx::query_scalar::<_, i32>("SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1")
            .bind(project as i64).fetch_one(&pool).await.unwrap();

        let admin = test_support::create_test_user(&pool, "adm").await;
        let member = test_support::create_test_user(&pool, "mem").await;
        let outsider = test_support::create_test_user(&pool, "out").await;
        set_role(&pool, team, admin, "admin").await;
        set_role(&pool, team, member, "member").await;

        assert!(can_manage(&pool, project, owner, false).await.unwrap(), "オーナー");
        assert!(can_manage(&pool, project, admin, false).await.unwrap(), "参加チームの管理者");
        assert!(can_manage(&pool, project, outsider, true).await.unwrap(), "システム管理者");
        assert!(!can_manage(&pool, project, member, false).await.unwrap(), "一般メンバーは不可");
        assert!(!can_manage(&pool, project, outsider, false).await.unwrap(), "無関係な人は不可");
    }

    #[tokio::test]
    async fn overview_lists_addable_teams_by_membership_and_blocks_removal_with_work() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let owner = test_support::create_test_user(&pool, "ov").await;
        let project = test_support::create_test_project(&pool, "PT2", owner).await;
        sqlx::query("UPDATE tickets_project SET owner_id = $2 WHERE id = $1")
            .bind(project as i64).bind(owner as i64).execute(&pool).await.unwrap();
        let team_a = sqlx::query_scalar::<_, i32>("SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1")
            .bind(project as i64).fetch_one(&pool).await.unwrap();

        // オーナーが所属するチームB(追加可能)と、所属しないチームC(追加不可)
        let team_b = test_support::create_test_team(&pool, "addB").await;
        let team_c = test_support::create_test_team(&pool, "addC").await;
        set_role(&pool, team_b, owner, "member").await;

        let out = overview(&pool, project, owner, false).await.unwrap();
        assert!(out.can_manage);
        let addable: Vec<i32> = out.addable_teams.iter().map(|t| t.id).collect();
        assert!(addable.contains(&team_b), "所属チームは追加できる");
        assert!(!addable.contains(&team_c), "所属しないチームは追加できない");
        assert!(!addable.contains(&team_a), "参加済みのチームは出ない");
        // 参加チームが1つだけなら、外せない
        assert_eq!(out.teams.len(), 1);
        assert!(!out.teams[0].removable);
        assert!(out.teams[0].remove_blocked_reason.as_deref().unwrap().contains("1つ以上"));

        // システム管理者には、所属していないチームも出る
        let staff_out = overview(&pool, project, owner, true).await.unwrap();
        assert!(staff_out.addable_teams.iter().any(|t| t.id == team_c));

        // Bを追加し、Aにチケットがあると、Aは外せず、Bは外せる
        crate::infrastructure::repositories::resource_repo::add_project_team(&pool, project, team_b).await.unwrap();
        test_support::create_test_ticket(&pool, project, "PT2", owner).await;
        let out = overview(&pool, project, owner, false).await.unwrap();
        assert_eq!(out.teams.len(), 2);
        let a = out.teams.iter().find(|t| t.id == team_a).unwrap();
        let b = out.teams.iter().find(|t| t.id == team_b).unwrap();
        assert!(a.ticket_count >= 1 && !a.removable);
        assert!(a.remove_blocked_reason.as_deref().unwrap().contains("チケット"));
        assert!(b.removable && b.remove_blocked_reason.is_none());
    }

    #[tokio::test]
    async fn overview_hides_controls_without_permission() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let owner = test_support::create_test_user(&pool, "np").await;
        let project = test_support::create_test_project(&pool, "PT3", owner).await;
        let member = test_support::create_test_user(&pool, "npm").await;
        let team = sqlx::query_scalar::<_, i32>("SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1")
            .bind(project as i64).fetch_one(&pool).await.unwrap();
        set_role(&pool, team, member, "member").await;

        let out = overview(&pool, project, member, false).await.unwrap();
        assert!(!out.can_manage);
        assert!(out.addable_teams.is_empty());
        assert!(out.teams.iter().all(|t| !t.removable));
    }
}
