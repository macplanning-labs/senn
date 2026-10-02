//! facts_repo — リソースの所属の情報を小さな問い合わせで作る
//!
//! 判定に必要なメタデータ(チーム・プロジェクト・著者など)だけを取得。
//! 本文や詳細情報は読まない。

use crate::domain::access::resource::{
    ResourceRef, SettingsPolicy, TeamFacts, Visibility, WikiScope,
};
use crate::infrastructure::access::viewer_repo;
use sqlx::{PgPool, Row};

/// チームの所属情報を取得
pub async fn facts_for_team(pool: &PgPool, team_id: i32) -> anyhow::Result<Option<TeamFacts>> {
    Ok(facts_for_teams(pool, &[team_id]).await?.into_iter().next())
}

/// 複数チームの所属情報を、1 回の SQL で取得(存在しない ID は含めない。ID 昇順)
///
/// Owner の数: 有効なユーザー(is_active)で、チーム全体の所属(scoped_project_id IS NULL)の role = 'admin' の人数
pub async fn facts_for_teams(pool: &PgPool, team_ids: &[i32]) -> anyhow::Result<Vec<TeamFacts>> {
    if team_ids.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<i64> = team_ids.iter().map(|&id| id as i64).collect();
    let rows = sqlx::query(
        "SELECT mt.id::int4 AS id, mt.visibility, mt.settings_policy,
                (SELECT COUNT(*) FROM t_team_membership tm
                   JOIN accounts_user au ON au.id = tm.user_id
                  WHERE tm.team_id = mt.id AND tm.role = 'admin'
                    AND tm.scoped_project_id IS NULL AND au.is_active)::int8 AS owner_count
         FROM m_team mt
         WHERE mt.id = ANY($1::int8[])
         ORDER BY mt.id",
    )
    .bind(&ids)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let visibility: String = row.get("visibility");
            let settings_policy: String = row.get("settings_policy");
            let owner_count: i64 = row.get("owner_count");
            TeamFacts {
                team_id: row.get("id"),
                visibility: Visibility::from_db(&visibility),
                settings_policy: SettingsPolicy::from_db(&settings_policy),
                owner_count: u32::try_from(owner_count).unwrap_or(u32::MAX),
            }
        })
        .collect())
}

/// チケットの所属情報を取得
pub async fn facts_for_ticket(
    pool: &PgPool,
    ticket_id: i32,
) -> anyhow::Result<Option<ResourceRef>> {
    let row = sqlx::query(
        "SELECT team_id, project_id, author_id FROM tickets_ticket WHERE id = $1::int8",
    )
    .bind(ticket_id as i64)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let team_id: Option<i64> = row.get("team_id");
    let project_id: Option<i64> = row.get("project_id");
    let author_id: Option<i64> = row.get("author_id");

    let team = match team_id {
        Some(tid) => facts_for_team(pool, tid as i32).await?,
        None => None,
    };

    let assignee_ids: Vec<i32> = sqlx::query_scalar(
        "SELECT user_id::int4 FROM tickets_ticket_assignees WHERE ticketmodel_id = $1::int8",
    )
    .bind(ticket_id as i64)
    .fetch_all(pool)
    .await?;

    Ok(Some(ResourceRef::Ticket {
        team,
        project_id: project_id.map(|p| p as i32),
        author_id: author_id.map(|a| a as i32),
        assignee_ids,
    }))
}

/// チケットキーからチケットの所属情報を取得
pub async fn facts_for_ticket_key(
    pool: &PgPool,
    ticket_key: &str,
) -> anyhow::Result<Option<(i32, ResourceRef)>> {
    let ticket_id: Option<i64> =
        sqlx::query_scalar("SELECT id FROM tickets_ticket WHERE ticket_key = $1")
            .bind(ticket_key)
            .fetch_optional(pool)
            .await?;

    let Some(tid) = ticket_id else {
        return Ok(None);
    };

    let tid = tid as i32;
    let facts = facts_for_ticket(pool, tid).await?;
    Ok(facts.map(|f| (tid, f)))
}

/// プロジェクトの所属情報を取得
pub async fn facts_for_project(
    pool: &PgPool,
    project_id: i32,
) -> anyhow::Result<Option<ResourceRef>> {
    // プロジェクトが存在するか確認
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM tickets_project WHERE id = $1::int8)")
            .bind(project_id as i64)
            .fetch_one(pool)
            .await?;

    if !exists {
        return Ok(None);
    }

    let team_ids: Vec<i32> = sqlx::query_scalar(
        "SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1::int8 ORDER BY team_id",
    )
    .bind(project_id as i64)
    .fetch_all(pool)
    .await?;

    let teams = facts_for_teams(pool, &team_ids).await?;

    Ok(Some(ResourceRef::Project { project_id, teams }))
}

/// チーム・プロジェクト・全体のどれかに属する物(ラベル・マイルストーンなど)の所属情報(D-4)
///
/// チームがあればチーム、無ければプロジェクト、どちらも無ければ共通のマスタ(`GlobalMaster`)。
/// 指定されたチーム・プロジェクトが存在しない場合は None。
/// `scope_sql::push_label_visible`・`push_milestone_visible` と同じ優先順位にすること。
pub async fn facts_for_scoped(
    pool: &PgPool,
    team_id: Option<i32>,
    project_id: Option<i32>,
) -> anyhow::Result<Option<ResourceRef>> {
    match (team_id, project_id) {
        (Some(t), _) => Ok(facts_for_team(pool, t).await?.map(ResourceRef::Team)),
        (None, Some(p)) => facts_for_project(pool, p).await,
        (None, None) => Ok(Some(ResourceRef::GlobalMaster)),
    }
}

/// サイクルの所属情報を取得
pub async fn facts_for_cycle(pool: &PgPool, cycle_id: i32) -> anyhow::Result<Option<ResourceRef>> {
    let team_id: Option<i64> =
        sqlx::query_scalar("SELECT team_id FROM t_cycle WHERE id = $1::int8")
            .bind(cycle_id as i64)
            .fetch_optional(pool)
            .await?;

    let Some(tid) = team_id else {
        return Ok(None);
    };

    let team = facts_for_team(pool, tid as i32).await?;
    Ok(team.map(|t| ResourceRef::Cycle { team: t }))
}

/// Wiki ページの所属情報を取得
pub async fn facts_for_wiki(pool: &PgPool, page_id: i32) -> anyhow::Result<Option<ResourceRef>> {
    let row =
        sqlx::query("SELECT team_id, project_id, author_id FROM wiki_page WHERE id = $1::int8")
            .bind(page_id as i64)
            .fetch_optional(pool)
            .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let team_id: Option<i64> = row.get("team_id");
    let project_id: Option<i64> = row.get("project_id");
    let author_id: Option<i64> = row.get("author_id");

    facts_for_wiki_scope(
        pool,
        team_id.map(|t| t as i32),
        project_id.map(|p| p as i32),
        author_id.map(|a| a as i32),
    )
    .await
}

/// Wiki の所属(チーム・プロジェクト・全体)の情報。作成先・移動先の判定にも使う(D-4)
///
/// チームを優先する(`scope_sql::push_wiki_visible` と同じ)。指定されたチームが存在しない場合は None。
pub async fn facts_for_wiki_scope(
    pool: &PgPool,
    team_id: Option<i32>,
    project_id: Option<i32>,
    author_id: Option<i32>,
) -> anyhow::Result<Option<ResourceRef>> {
    let scope = match (team_id, project_id) {
        (Some(tid), _) => match facts_for_team(pool, tid).await? {
            Some(team) => WikiScope::Team(team),
            None => return Ok(None),
        },
        (None, Some(pid)) => {
            let team_ids: Vec<i32> = sqlx::query_scalar(
                "SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1::int8 ORDER BY team_id",
            )
            .bind(pid as i64)
            .fetch_all(pool)
            .await?;

            let teams = facts_for_teams(pool, &team_ids).await?;
            WikiScope::Project {
                project_id: pid,
                teams,
            }
        }
        (None, None) => WikiScope::Global,
    };

    Ok(Some(ResourceRef::Wiki { scope, author_id }))
}

/// コメントの所属情報を取得
pub async fn facts_for_comment(
    pool: &PgPool,
    comment_id: i32,
) -> anyhow::Result<Option<ResourceRef>> {
    let ticket_id: Option<i64> =
        sqlx::query_scalar("SELECT ticket_id FROM tickets_comment WHERE id = $1::int8")
            .bind(comment_id as i64)
            .fetch_optional(pool)
            .await?;

    let Some(tid) = ticket_id else {
        return Ok(None);
    };

    facts_for_ticket(pool, tid as i32).await
}

/// 添付ファイルの所属情報を取得
pub async fn facts_for_attachment(
    pool: &PgPool,
    attachment_id: i32,
) -> anyhow::Result<Option<ResourceRef>> {
    let ticket_id: Option<i64> =
        sqlx::query_scalar("SELECT ticket_id FROM tickets_attachment WHERE id = $1::int8")
            .bind(attachment_id as i64)
            .fetch_optional(pool)
            .await?;

    let Some(tid) = ticket_id else {
        return Ok(None);
    };

    facts_for_ticket(pool, tid as i32).await
}

/// ユーザーの所属情報を取得
pub async fn facts_for_user(pool: &PgPool, user_id: i32) -> anyhow::Result<Option<ResourceRef>> {
    // viewer_repo と同じ判定(無効化されたユーザーは None)
    let viewer = viewer_repo::load(
        pool,
        crate::domain::access::viewer::Principal::Human { user_id },
        viewer_repo::today_utc(),
    )
    .await?;

    let Some(viewer) = viewer else {
        return Ok(None);
    };

    let is_guest = viewer.role == crate::domain::access::viewer::Role::Guest;
    let team_ids = viewer
        .memberships
        .iter()
        .filter(|m| m.kind == crate::domain::access::viewer::MembershipKind::Team)
        .map(|m| m.team_id)
        .collect();
    let project_grants = viewer.project_grants();

    Ok(Some(ResourceRef::User {
        user_id,
        is_guest,
        team_ids,
        project_grants,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        create_test_project, create_test_team, create_test_ticket, create_test_user, test_pool,
    };

    #[tokio::test]
    async fn facts_for_team_loads_basic_info() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let team = create_test_team(&pool, "fr-team").await;

        let facts = facts_for_team(&pool, team).await.unwrap();
        assert!(facts.is_some());
        let facts = facts.unwrap();
        assert_eq!(facts.team_id, team);
        assert_eq!(facts.visibility, Visibility::Public); // default
        assert_eq!(facts.settings_policy, SettingsPolicy::Members); // default
    }

    #[tokio::test]
    async fn facts_for_team_counts_active_owners() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let team = create_test_team(&pool, "fr-owner").await;
        let user1 = create_test_user(&pool, "fr-owner-1").await;
        let user2 = create_test_user(&pool, "fr-owner-2").await;

        // Add two owners
        sqlx::query(
            "INSERT INTO t_team_membership (team_id, user_id, role, joined_at) VALUES ($1, $2, 'admin', NOW())",
        )
        .bind(team as i64)
        .bind(user1 as i64)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO t_team_membership (team_id, user_id, role, joined_at) VALUES ($1, $2, 'admin', NOW())",
        )
        .bind(team as i64)
        .bind(user2 as i64)
        .execute(&pool)
        .await
        .unwrap();

        let facts = facts_for_team(&pool, team).await.unwrap().unwrap();
        assert_eq!(facts.owner_count, 2);

        // Deactivate one user
        sqlx::query("UPDATE accounts_user SET is_active = false WHERE id = $1")
            .bind(user1 as i64)
            .execute(&pool)
            .await
            .unwrap();

        let facts = facts_for_team(&pool, team).await.unwrap().unwrap();
        assert_eq!(facts.owner_count, 1);
    }

    #[tokio::test]
    async fn facts_for_team_returns_none_for_missing_team() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let result = facts_for_team(&pool, i32::MAX).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn facts_for_teams_loads_multiple_and_ordered() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let team1 = create_test_team(&pool, "fr-multi-1").await;
        let team2 = create_test_team(&pool, "fr-multi-2").await;
        let team3 = create_test_team(&pool, "fr-multi-3").await;

        let facts = facts_for_teams(&pool, &[team3, team1, team2])
            .await
            .unwrap();
        assert_eq!(facts.len(), 3);
        assert_eq!(facts[0].team_id, team1); // ordered
        assert_eq!(facts[1].team_id, team2);
        assert_eq!(facts[2].team_id, team3);
    }

    #[tokio::test]
    async fn facts_for_teams_skips_missing() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let team = create_test_team(&pool, "fr-exist").await;
        let facts = facts_for_teams(&pool, &[team, i32::MAX]).await.unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].team_id, team);
    }

    #[tokio::test]
    async fn facts_for_ticket_includes_team_and_assignees() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = create_test_user(&pool, "fr-ticket").await;
        let project = create_test_project(&pool, "FRT", user).await;
        let ticket = create_test_ticket(&pool, project, "FRT", user).await;
        let assignee = create_test_user(&pool, "fr-ticket-assignee").await;

        // Add assignee
        sqlx::query(
            "INSERT INTO tickets_ticket_assignees (ticketmodel_id, user_id) VALUES ($1, $2)",
        )
        .bind(ticket as i64)
        .bind(assignee as i64)
        .execute(&pool)
        .await
        .unwrap();

        let res = facts_for_ticket(&pool, ticket).await.unwrap().unwrap();
        if let ResourceRef::Ticket {
            team,
            project_id,
            author_id,
            assignee_ids,
        } = res
        {
            assert!(team.is_some());
            assert_eq!(project_id, Some(project));
            assert_eq!(author_id, Some(user));
            assert!(assignee_ids.contains(&assignee));
        } else {
            panic!("Expected Ticket variant");
        }
    }

    #[tokio::test]
    async fn facts_for_ticket_without_team() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = create_test_user(&pool, "fr-no-team").await;

        // Create ticket without team
        let ticket_key = format!("NOTI-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
        let ticket_id: i32 = sqlx::query_scalar(
            "INSERT INTO tickets_ticket (ticket_key, title, description, status, priority, ticket_type, created_at, updated_at, gantt_order, author_id)
             VALUES ($1, $1, '', 'open', 'medium', 'task', NOW(), NOW(), 0, $2)
             RETURNING id::int4",
        )
        .bind(&ticket_key)
        .bind(user as i64)
        .fetch_one(&pool)
        .await
        .unwrap();

        let res = facts_for_ticket(&pool, ticket_id).await.unwrap().unwrap();
        if let ResourceRef::Ticket { team, .. } = res {
            assert!(team.is_none());
        } else {
            panic!("Expected Ticket variant");
        }
    }

    #[tokio::test]
    async fn facts_for_ticket_key_resolves_and_returns_id() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = create_test_user(&pool, "fr-key").await;
        let project = create_test_project(&pool, "FRK", user).await;
        let ticket = create_test_ticket(&pool, project, "FRK", user).await;

        let key: String = sqlx::query_scalar(
            "SELECT tt.ticket_key FROM tickets_ticket tt WHERE tt.id = $1::int8",
        )
        .bind(ticket as i64)
        .fetch_one(&pool)
        .await
        .unwrap();
        let (id, res) = facts_for_ticket_key(&pool, &key)
            .await
            .unwrap()
            .expect("Should find ticket");
        assert_eq!(id, ticket);
        assert!(matches!(res, ResourceRef::Ticket { .. }));
    }

    #[tokio::test]
    async fn facts_for_ticket_key_returns_none_for_missing() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let result = facts_for_ticket_key(&pool, "NOTEXIST-123").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn facts_for_project_loads_teams() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = create_test_user(&pool, "fr-proj").await;
        let project = create_test_project(&pool, "FRP", user).await;

        let res = facts_for_project(&pool, project).await.unwrap().unwrap();
        if let ResourceRef::Project { project_id, teams } = res {
            assert_eq!(project_id, project);
            assert!(!teams.is_empty());
        } else {
            panic!("Expected Project variant");
        }
    }

    #[tokio::test]
    async fn facts_for_project_returns_none_for_missing() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let result = facts_for_project(&pool, i32::MAX).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn facts_for_cycle_includes_team() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let team = create_test_team(&pool, "fr-cycle").await;

        let cycle_id: i32 = sqlx::query_scalar(
            "INSERT INTO t_cycle (team_id, name, number, status, start_date, end_date, created_at)
             VALUES ($1::int8, 'Test Cycle', 1, 'planned', CURRENT_DATE, CURRENT_DATE + 7, NOW()) RETURNING id::int4",
        )
        .bind(team as i64)
        .fetch_one(&pool)
        .await
        .unwrap();

        let res = facts_for_cycle(&pool, cycle_id).await.unwrap().unwrap();
        if let ResourceRef::Cycle { team: t } = res {
            assert_eq!(t.team_id, team);
        } else {
            panic!("Expected Cycle variant");
        }
    }

    #[tokio::test]
    async fn facts_for_wiki_team_scope() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let team = create_test_team(&pool, "fr-wiki-team").await;
        let user = create_test_user(&pool, "fr-wiki").await;

        let page_id: i32 = sqlx::query_scalar(
            "INSERT INTO wiki_page (team_id, title, slug, category, content, author_id, created_at, updated_at)
             VALUES ($1, 'Test', $2, 'general', '', $3, NOW(), NOW()) RETURNING id::int4",
        )
        .bind(team as i64)
        .bind(format!("test-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]))
        .bind(user as i64)
        .fetch_one(&pool)
        .await
        .unwrap();

        let res = facts_for_wiki(&pool, page_id).await.unwrap().unwrap();
        if let ResourceRef::Wiki { scope, author_id } = res {
            assert!(matches!(scope, WikiScope::Team(_)));
            assert_eq!(author_id, Some(user));
        } else {
            panic!("Expected Wiki variant");
        }
    }

    #[tokio::test]
    async fn facts_for_wiki_project_scope() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = create_test_user(&pool, "fr-wiki-proj").await;
        let project = create_test_project(&pool, "FWP", user).await;

        let page_id: i32 = sqlx::query_scalar(
            "INSERT INTO wiki_page (project_id, title, slug, category, content, author_id, created_at, updated_at)
             VALUES ($1, 'Test', $2, 'general', '', $3, NOW(), NOW()) RETURNING id::int4",
        )
        .bind(project as i64)
        .bind(format!("test-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]))
        .bind(user as i64)
        .fetch_one(&pool)
        .await
        .unwrap();

        let res = facts_for_wiki(&pool, page_id).await.unwrap().unwrap();
        if let ResourceRef::Wiki { scope, .. } = res {
            assert!(matches!(scope, WikiScope::Project { .. }));
        } else {
            panic!("Expected Wiki variant");
        }
    }

    #[tokio::test]
    async fn facts_for_wiki_global_scope() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = create_test_user(&pool, "fr-wiki-global").await;

        let page_id: i32 = sqlx::query_scalar(
            "INSERT INTO wiki_page (title, slug, category, content, author_id, created_at, updated_at)
             VALUES ('Global', $1, 'general', '', $2, NOW(), NOW()) RETURNING id::int4",
        )
        .bind(format!("global-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]))
        .bind(user as i64)
        .fetch_one(&pool)
        .await
        .unwrap();

        let res = facts_for_wiki(&pool, page_id).await.unwrap().unwrap();
        if let ResourceRef::Wiki { scope, .. } = res {
            assert!(matches!(scope, WikiScope::Global));
        } else {
            panic!("Expected Wiki variant");
        }
    }

    #[tokio::test]
    async fn facts_for_comment_returns_ticket_facts() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = create_test_user(&pool, "fr-comment").await;
        let project = create_test_project(&pool, "FRC", user).await;
        let ticket = create_test_ticket(&pool, project, "FRC", user).await;

        let comment_id: i32 = sqlx::query_scalar(
            "INSERT INTO tickets_comment (ticket_id, author_id, body, created_at) VALUES ($1::int8, $2::int8, 'test', NOW()) RETURNING id::int4",
        )
        .bind(ticket as i64)
        .bind(user as i64)
        .fetch_one(&pool)
        .await
        .unwrap();

        let res = facts_for_comment(&pool, comment_id).await.unwrap().unwrap();
        assert!(matches!(res, ResourceRef::Ticket { .. }));
    }

    #[tokio::test]
    async fn facts_for_attachment_returns_ticket_facts() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = create_test_user(&pool, "fr-attachment").await;
        let project = create_test_project(&pool, "FRA", user).await;
        let ticket = create_test_ticket(&pool, project, "FRA", user).await;

        let att_id: i32 = sqlx::query_scalar(
            "INSERT INTO tickets_attachment (ticket_id, uploader_id, filename, file, file_size, created_at)
             VALUES ($1::int8, $2::int8, 'test.txt', 'attachments/test.txt', 0, NOW()) RETURNING id::int4",
        )
        .bind(ticket as i64)
        .bind(user as i64)
        .fetch_one(&pool)
        .await
        .unwrap();

        let res = facts_for_attachment(&pool, att_id).await.unwrap().unwrap();
        assert!(matches!(res, ResourceRef::Ticket { .. }));
    }

    #[tokio::test]
    async fn facts_for_user_returns_active_user_with_memberships() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = create_test_user(&pool, "fr-user").await;
        let team = create_test_team(&pool, "fr-user-team").await;

        sqlx::query("INSERT INTO t_team_membership (team_id, user_id, role, joined_at) VALUES ($1, $2, 'member', NOW())")
            .bind(team as i64)
            .bind(user as i64)
            .execute(&pool)
            .await
            .unwrap();

        let res = facts_for_user(&pool, user).await.unwrap().unwrap();
        if let ResourceRef::User {
            user_id,
            is_guest,
            team_ids,
            ..
        } = res
        {
            assert_eq!(user_id, user);
            assert!(!is_guest);
            assert!(team_ids.contains(&team));
        } else {
            panic!("Expected User variant");
        }
    }

    #[tokio::test]
    async fn facts_for_user_guest_flag() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = create_test_user(&pool, "fr-user-guest").await;

        sqlx::query("UPDATE accounts_user SET is_guest = true WHERE id = $1")
            .bind(user as i64)
            .execute(&pool)
            .await
            .unwrap();

        let res = facts_for_user(&pool, user).await.unwrap().unwrap();
        if let ResourceRef::User { is_guest, .. } = res {
            assert!(is_guest);
        } else {
            panic!("Expected User variant");
        }
    }

    #[tokio::test]
    async fn facts_for_user_inactive_returns_none() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = create_test_user(&pool, "fr-user-inactive").await;

        sqlx::query("UPDATE accounts_user SET is_active = false WHERE id = $1")
            .bind(user as i64)
            .execute(&pool)
            .await
            .unwrap();

        let result = facts_for_user(&pool, user).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn facts_for_user_missing_returns_none() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let result = facts_for_user(&pool, i32::MAX).await.unwrap();
        assert!(result.is_none());
    }
}
