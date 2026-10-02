//! policy(Rust の判定)と scope_sql(SQL の絞り込み)が、同じ答えを出すことの検査(詳細設計書 §7.3)
//!
//! 実 DB に、Public / Private のチーム・プロジェクト・チケット・Wiki を作り、閲覧者の種類ごとに
//! 「policy::can(Read) の答え」と「scope_sql で絞った SELECT に行が含まれるか」を 1 行ずつ比べる。

use sqlx::{PgPool, QueryBuilder};

use crate::domain::access::{
    can, Action, Decision, Principal, ResourceRef, SettingsPolicy, TeamFacts, Visibility, WikiScope,
};
use crate::infrastructure::access::{scope_sql, viewer_repo};
use crate::test_support::{
    create_test_project, create_test_team, create_test_ticket, create_test_user, db_today,
    test_pool,
};

async fn run_sql(pool: &PgPool, sql: &str, binds: &[i64]) {
    let mut q = sqlx::query(sql);
    for b in binds {
        q = q.bind(*b);
    }
    q.execute(pool).await.unwrap();
}

async fn team_of(pool: &PgPool, project: i32) -> i32 {
    sqlx::query_scalar("SELECT pt.team_id::int4 FROM tickets_project_teams pt WHERE pt.project_id = $1::int8 ORDER BY pt.team_id LIMIT 1")
        .bind(project as i64)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn facts(team_id: i32, private: bool) -> TeamFacts {
    TeamFacts {
        team_id,
        visibility: if private {
            Visibility::Private
        } else {
            Visibility::Public
        },
        settings_policy: SettingsPolicy::Members,
        owner_count: 0,
    }
}

async fn label(pool: &PgPool, team: Option<i32>, project: Option<i32>) -> i32 {
    sqlx::query_scalar(
        "INSERT INTO m_label (name, color, created_at, project_id, team_id, description, category, is_ai_enabled)
         VALUES ($3, '#000', NOW(), $1, $2, NULL, NULL, false) RETURNING id::int4",
    )
    .bind(project.map(|p| p as i64))
    .bind(team.map(|t| t as i64))
    .bind(format!("par{}", crate::test_support::unique_suffix()))
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn milestone(pool: &PgPool, project: Option<i32>) -> i32 {
    sqlx::query_scalar(
        "INSERT INTO milestones_milestone (name, due_date, description, created_at, project_id)
         VALUES ('par', NULL, '', NOW(), $1) RETURNING id::int4",
    )
    .bind(project.map(|p| p as i64))
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn workflow_status(
    pool: &PgPool,
    team: Option<i32>,
    project: Option<i32>,
    slug: &str,
) -> i32 {
    sqlx::query_scalar(
        "INSERT INTO t_workflow_status (slug, name, category, color, position, is_default, team_id, project_id)
         VALUES ($1, 'par', 'unstarted', '#000', 0, false, $2, $3) RETURNING id::int4",
    )
    .bind(slug)
    .bind(team.map(|t| t as i64))
    .bind(project.map(|p| p as i64))
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn team_rule(pool: &PgPool, author: i32, team: Option<i32>) -> i32 {
    sqlx::query_scalar(
        "INSERT INTO m_team_rule (title, content, category, sort_order, is_active, created_by_id, team_id, created_at, updated_at)
         VALUES ('par', '', 'general', 0, true, $1, $2, NOW(), NOW()) RETURNING id::int4",
    )
    .bind(author as i64)
    .bind(team.map(|t| t as i64))
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn wiki(pool: &PgPool, author: i32, team: Option<i32>, project: Option<i32>) -> i32 {
    let slug = format!("w{}", crate::test_support::unique_suffix());
    sqlx::query_scalar(
        "INSERT INTO wiki_page (project_id, team_id, title, slug, category, content, author_id, created_at, updated_at)
         VALUES ($1, $2, $3, $3, 'general', '', $4, NOW(), NOW()) RETURNING id::int4",
    )
    .bind(project.map(|p| p as i64))
    .bind(team.map(|t| t as i64))
    .bind(slug)
    .bind(author as i64)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn policy_and_scope_sql_agree() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let today = db_today();

    // ── チームとプロジェクト ──
    let author = create_test_user(&pool, "par-a").await;
    let p_priv = create_test_project(&pool, "PARV", author).await; // Private チームのプロジェクト
    let t_priv = team_of(&pool, p_priv).await;
    let p_pub = create_test_project(&pool, "PARB", author).await; // Public チームのプロジェクト
    let t_pub = team_of(&pool, p_pub).await;
    let p_both = create_test_project(&pool, "PARM", author).await; // Public + Private
    let t_both_pub = team_of(&pool, p_both).await;
    run_sql(&pool, "INSERT INTO tickets_project_teams (project_id, team_id, joined_at) VALUES ($1::int8, $2::int8, NOW())", &[p_both as i64, t_priv as i64]).await;
    run_sql(
        &pool,
        "UPDATE m_team SET visibility = 'private' WHERE id = $1::int8",
        &[t_priv as i64],
    )
    .await;
    let t_other_priv = create_test_team(&pool, "par-op").await;
    run_sql(
        &pool,
        "UPDATE m_team SET visibility = 'private' WHERE id = $1::int8",
        &[t_other_priv as i64],
    )
    .await;

    // ── 閲覧者 ──
    let fm_none = create_test_user(&pool, "par-fm").await; // Full Member・未所属
    let fm_priv = create_test_user(&pool, "par-fmp").await; // Private チームのメンバー
    let admin = create_test_user(&pool, "par-adm").await; // システム管理者・未所属
    let guest_pub = create_test_user(&pool, "par-gp").await; // Public チームの Guest
    let pj_guest = create_test_user(&pool, "par-pg").await; // Private チームのプロジェクトの Guest
    run_sql(&pool, "INSERT INTO t_team_membership (team_id, user_id, role, joined_at) VALUES ($1::int8, $2::int8, 'member', NOW())", &[t_priv as i64, fm_priv as i64]).await;
    run_sql(
        &pool,
        "UPDATE accounts_user SET is_system_admin = true WHERE id = $1::int8",
        &[admin as i64],
    )
    .await;
    run_sql(
        &pool,
        "UPDATE accounts_user SET is_guest = true WHERE id = ANY(ARRAY[$1::int8, $2::int8])",
        &[guest_pub as i64, pj_guest as i64],
    )
    .await;
    run_sql(&pool, "INSERT INTO t_team_membership (team_id, user_id, role, joined_at) VALUES ($1::int8, $2::int8, 'member', NOW())", &[t_pub as i64, guest_pub as i64]).await;
    run_sql(&pool, "INSERT INTO t_team_membership (team_id, user_id, role, scoped_project_id, joined_at) VALUES ($1::int8, $2::int8, 'member', $3::int8, NOW())", &[t_priv as i64, pj_guest as i64, p_priv as i64]).await;

    // ── チケット ──
    let k_pub = create_test_ticket(&pool, p_pub, "PK", author).await;
    let k_priv_p = create_test_ticket(&pool, p_priv, "PK", author).await; // Private・プロジェクトあり
    let k_priv = create_test_ticket(&pool, p_priv, "PK", author).await;
    run_sql(
        &pool,
        "UPDATE tickets_ticket SET project_id = NULL WHERE id = $1::int8",
        &[k_priv as i64],
    )
    .await; // Private・プロジェクトなし
    let k_none = create_test_ticket(&pool, p_pub, "PK", fm_none).await;
    run_sql(
        &pool,
        "UPDATE tickets_ticket SET project_id = NULL, team_id = NULL WHERE id = $1::int8",
        &[k_none as i64],
    )
    .await; // チームなし・作成者 fm_none
    let k_other = create_test_ticket(&pool, p_pub, "PK", author).await;
    run_sql(
        &pool,
        "UPDATE tickets_ticket SET project_id = NULL, team_id = $1::int8 WHERE id = $2::int8",
        &[t_other_priv as i64, k_other as i64],
    )
    .await;

    let tickets: Vec<(i32, ResourceRef)> = vec![
        (
            k_pub,
            ResourceRef::Ticket {
                team: Some(facts(t_pub, false)),
                project_id: Some(p_pub),
                author_id: Some(author),
                assignee_ids: vec![],
            },
        ),
        (
            k_priv_p,
            ResourceRef::Ticket {
                team: Some(facts(t_priv, true)),
                project_id: Some(p_priv),
                author_id: Some(author),
                assignee_ids: vec![],
            },
        ),
        (
            k_priv,
            ResourceRef::Ticket {
                team: Some(facts(t_priv, true)),
                project_id: None,
                author_id: Some(author),
                assignee_ids: vec![],
            },
        ),
        (
            k_none,
            ResourceRef::Ticket {
                team: None,
                project_id: None,
                author_id: Some(fm_none),
                assignee_ids: vec![],
            },
        ),
        (
            k_other,
            ResourceRef::Ticket {
                team: Some(facts(t_other_priv, true)),
                project_id: None,
                author_id: Some(author),
                assignee_ids: vec![],
            },
        ),
    ];
    let projects: Vec<(i32, ResourceRef)> = vec![
        (
            p_pub,
            ResourceRef::Project {
                project_id: p_pub,
                teams: vec![facts(t_pub, false)],
            },
        ),
        (
            p_priv,
            ResourceRef::Project {
                project_id: p_priv,
                teams: vec![facts(t_priv, true)],
            },
        ),
        (
            p_both,
            ResourceRef::Project {
                project_id: p_both,
                teams: vec![facts(t_both_pub, false), facts(t_priv, true)],
            },
        ),
    ];
    let w_team_priv = wiki(&pool, author, Some(t_priv), None).await;
    let w_proj_priv = wiki(&pool, author, None, Some(p_priv)).await;
    let w_team_pub = wiki(&pool, author, Some(t_pub), None).await;
    let w_global = wiki(&pool, author, None, None).await;
    let wikis: Vec<(i32, ResourceRef)> = vec![
        (
            w_team_priv,
            ResourceRef::Wiki {
                scope: WikiScope::Team(facts(t_priv, true)),
                author_id: Some(author),
            },
        ),
        (
            w_proj_priv,
            ResourceRef::Wiki {
                scope: WikiScope::Project {
                    project_id: p_priv,
                    teams: vec![facts(t_priv, true)],
                },
                author_id: Some(author),
            },
        ),
        (
            w_team_pub,
            ResourceRef::Wiki {
                scope: WikiScope::Team(facts(t_pub, false)),
                author_id: Some(author),
            },
        ),
        (
            w_global,
            ResourceRef::Wiki {
                scope: WikiScope::Global,
                author_id: Some(author),
            },
        ),
    ];

    let l_team_priv = label(&pool, Some(t_priv), None).await;
    let l_proj_priv = label(&pool, None, Some(p_priv)).await;
    let l_team_pub = label(&pool, Some(t_pub), None).await;
    let l_global = label(&pool, None, None).await;
    let labels: Vec<(i32, ResourceRef)> = vec![
        (l_team_priv, ResourceRef::Team(facts(t_priv, true))),
        (
            l_proj_priv,
            ResourceRef::Project {
                project_id: p_priv,
                teams: vec![facts(t_priv, true)],
            },
        ),
        (l_team_pub, ResourceRef::Team(facts(t_pub, false))),
        (l_global, ResourceRef::GlobalMaster),
    ];
    let m_priv = milestone(&pool, Some(p_priv)).await;
    let m_pub = milestone(&pool, Some(p_pub)).await;
    let m_global = milestone(&pool, None).await;
    let milestones: Vec<(i32, ResourceRef)> = vec![
        (
            m_priv,
            ResourceRef::Project {
                project_id: p_priv,
                teams: vec![facts(t_priv, true)],
            },
        ),
        (
            m_pub,
            ResourceRef::Project {
                project_id: p_pub,
                teams: vec![facts(t_pub, false)],
            },
        ),
        (m_global, ResourceRef::GlobalMaster),
    ];
    // 全体のラベルは、Guest には「見えるチケットで使われている物」だけ(§7.4。SQL だけの規則)。
    // Public チームのチケットに付けておき、Guest_Public には見え、PJGuest には見えないことを、別に確かめる
    run_sql(
        &pool,
        "INSERT INTO tickets_ticket_labels (ticketmodel_id, labelmodel_id) VALUES ($1::int8, $2::int8)",
        &[k_pub as i64, l_global as i64],
    )
    .await;

    // ワークフローの状態: 全体の状態は、ラベルと同じく Guest には使用範囲だけ(k_pub の状態にする)
    let slug = format!("par{}", crate::test_support::unique_suffix());
    let s_team_priv = workflow_status(&pool, Some(t_priv), None, &slug).await;
    let s_proj_priv = workflow_status(&pool, None, Some(p_priv), &slug).await;
    let s_global = workflow_status(&pool, None, None, &slug).await;
    run_sql(
        &pool,
        "UPDATE tickets_ticket SET status = (SELECT slug FROM t_workflow_status WHERE id = $1::int8) WHERE id = $2::int8",
        &[s_global as i64, k_pub as i64],
    )
    .await;
    let statuses: Vec<(i32, ResourceRef)> = vec![
        (s_team_priv, ResourceRef::Team(facts(t_priv, true))),
        (
            s_proj_priv,
            ResourceRef::Project {
                project_id: p_priv,
                teams: vec![facts(t_priv, true)],
            },
        ),
        (s_global, ResourceRef::GlobalMaster),
    ];
    let r_priv = team_rule(&pool, author, Some(t_priv)).await;
    let r_pub = team_rule(&pool, author, Some(t_pub)).await;
    let r_global = team_rule(&pool, author, None).await;
    let rules: Vec<(i32, ResourceRef)> = vec![
        (r_priv, ResourceRef::Team(facts(t_priv, true))),
        (r_pub, ResourceRef::Team(facts(t_pub, false))),
        (r_global, ResourceRef::GlobalMaster),
    ];

    let mut mismatches = Vec::new();
    let mut compared = 0;
    for (name, uid) in [
        ("FM未所属", fm_none),
        ("FM_Private所属", fm_priv),
        ("管理者", admin),
        ("Guest_Public", guest_pub),
        ("PJGuest", pj_guest),
    ] {
        let v = viewer_repo::load(&pool, Principal::Human { user_id: uid }, today)
            .await
            .unwrap()
            .unwrap();
        let scope = v.scope();
        for (kind, rows) in [
            ("ticket", &tickets),
            ("project", &projects),
            ("wiki", &wikis),
            ("label", &labels),
            ("milestone", &milestones),
            ("status", &statuses),
            ("rule", &rules),
        ] {
            for (id, res) in rows.iter() {
                let mut by_policy = can(&v, Action::Read, res) == Decision::Allow;
                let global_in_use =
                    (kind == "label" && *id == l_global) || (kind == "status" && *id == s_global);
                if global_in_use && !v.is_full_member() {
                    // §7.4: Guest は、使われているチケットが見える場合だけ(k_pub が見えるか)
                    by_policy = can(&v, Action::Read, &tickets[0].1) == Decision::Allow;
                }
                let mut qb = QueryBuilder::new(match kind {
                    "ticket" => "SELECT count(*) FROM tickets_ticket t WHERE t.id = ",
                    "project" => "SELECT count(*) FROM tickets_project p WHERE p.id = ",
                    "label" => "SELECT count(*) FROM m_label l WHERE l.id = ",
                    "milestone" => "SELECT count(*) FROM milestones_milestone m WHERE m.id = ",
                    "status" => "SELECT count(*) FROM t_workflow_status ws WHERE ws.id = ",
                    "rule" => "SELECT count(*) FROM m_team_rule r WHERE r.id = ",
                    _ => "SELECT count(*) FROM wiki_page w WHERE w.id = ",
                });
                qb.push_bind(*id as i64).push(" AND ");
                match kind {
                    "ticket" => scope_sql::push_ticket_visible(&mut qb, "t", &scope),
                    "project" => scope_sql::push_project_visible(&mut qb, "p.id", &scope),
                    "label" => scope_sql::push_label_visible(&mut qb, "l", &scope),
                    "milestone" => scope_sql::push_milestone_visible(&mut qb, "m", &scope),
                    "status" => scope_sql::push_workflow_status_visible(&mut qb, "ws", &scope),
                    "rule" => scope_sql::push_team_or_global_visible(&mut qb, "r.team_id", &scope),
                    _ => scope_sql::push_wiki_visible(&mut qb, "w", &scope),
                }
                let n: i64 = qb.build_query_scalar().fetch_one(&pool).await.unwrap();
                compared += 1;
                if by_policy != (n == 1) {
                    mismatches.push(format!(
                        "{name} / {kind} {id}: policy={by_policy} sql={}",
                        n == 1
                    ));
                }
            }
        }
    }
    assert_eq!(compared, 5 * (5 + 3 + 4 + 4 + 3 + 3 + 3), "比較した件数");
    assert!(
        mismatches.is_empty(),
        "policy と scope_sql が食い違う:\n{}",
        mismatches.join("\n")
    );
}
