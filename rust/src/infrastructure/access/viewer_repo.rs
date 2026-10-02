//! 閲覧者(Viewer)の読み込み。1 回の SQL で、役割・有効な所属・Public チームを読む
//!
//! 「ゲストが有効か」の定義は、ここの SQL だけに置く(詳細設計書 §6.2。今ある 4 通りの定義を置き換える):
//!   end_date が無い、または end_date + プロジェクトの猶予日数(プロジェクト単位の所属のみ)>= 今日

use chrono::NaiveDate;
use sqlx::{PgPool, Row};

use std::collections::HashMap;

use crate::domain::access::{Membership, MembershipKind, Principal, ResourceRef, Role, Viewer};

/// 所属が有効か(`tm` = t_team_membership、`p` = 所属の対象プロジェクト(LEFT JOIN)、`$2` = 今日)
const MEMBERSHIP_VALID: &str =
    "(tm.end_date IS NULL OR tm.end_date + COALESCE(p.grace_period_days, 0) >= $2::date)";

/// 閲覧者を読み込む。ユーザーが存在しない・無効化されている場合は None(呼び出し側は 401 にする)。
///
/// `today` は判定の基準日。本番は `today_utc()`(DB のセッションと同じ UTC の日付)を渡す。
pub async fn load(
    pool: &PgPool,
    principal: Principal,
    today: NaiveDate,
) -> anyhow::Result<Option<Viewer>> {
    let Some(user_id) = principal.user_id() else {
        return match principal {
            Principal::Integration { integration_id } => {
                load_integration(pool, principal, integration_id).await
            }
            _ => Ok(None),
        };
    };

    let Some(user) =
        sqlx::query("SELECT is_active, is_guest, is_system_admin FROM accounts_user WHERE id = $1")
            .bind(user_id as i64)
            .fetch_optional(pool)
            .await?
    else {
        return Ok(None);
    };
    let is_active: bool = user.get("is_active");
    if !is_active {
        return Ok(None);
    }
    let is_guest: bool = user.get("is_guest");
    let is_system_admin: bool = user.get("is_system_admin");
    // CHECK 制約で両立しないが、万一のときは権限の小さい Guest に倒す
    let role = if is_guest {
        Role::Guest
    } else if is_system_admin {
        Role::SystemAdmin
    } else {
        Role::FullMember
    };

    let rows = sqlx::query(&format!(
        "SELECT tm.team_id::int4 AS team_id,
                tm.scoped_project_id::int4 AS project_id,
                (tm.role = 'admin') AS is_owner,
                CASE WHEN tm.end_date IS NULL THEN NULL
                     ELSE tm.end_date + COALESCE(p.grace_period_days, 0) END AS expires_on
         FROM t_team_membership tm
         LEFT JOIN tickets_project p ON p.id = tm.scoped_project_id
         WHERE tm.user_id = $1
           AND {MEMBERSHIP_VALID}
         ORDER BY tm.team_id, tm.scoped_project_id NULLS FIRST"
    ))
    .bind(user_id as i64)
    .bind(today)
    .fetch_all(pool)
    .await?;

    let memberships = rows
        .into_iter()
        .map(|row| {
            let team_id: i32 = row.get("team_id");
            let kind = match row.get::<Option<i32>, _>("project_id") {
                None => MembershipKind::Team,
                Some(project_id) => MembershipKind::Project { project_id },
            };
            Membership {
                team_id,
                kind,
                is_owner: row.get("is_owner"),
                expires_on: row.get("expires_on"),
            }
        })
        .collect();

    let public_team_ids: Vec<i32> =
        sqlx::query_scalar("SELECT id::int4 FROM m_team WHERE visibility = 'public' ORDER BY id")
            .fetch_all(pool)
            .await?;

    Ok(Some(Viewer {
        principal,
        role,
        memberships,
        public_team_ids,
    }))
}

/// 外部連携の閲覧者(F-7)。許可されたチームだけを、チーム全体の所属として持つ
/// (Public でも、許可の無いチームは見えない。Full Member ではないので Public チームの一覧は持たない)。
/// 無効化された連携は None
async fn load_integration(
    pool: &PgPool,
    principal: Principal,
    integration_id: i64,
) -> anyhow::Result<Option<Viewer>> {
    let active: Option<bool> =
        sqlx::query_scalar("SELECT is_active FROM access_integration WHERE id = $1::int8")
            .bind(integration_id)
            .fetch_optional(pool)
            .await?;
    if active != Some(true) {
        return Ok(None);
    }
    let team_ids: Vec<i32> = sqlx::query_scalar(
        "SELECT team_id::int4 FROM access_integration_team WHERE integration_id = $1::int8 ORDER BY team_id",
    )
    .bind(integration_id)
    .fetch_all(pool)
    .await?;
    Ok(Some(Viewer {
        principal,
        role: Role::Integration,
        memberships: team_ids
            .into_iter()
            .map(|team_id| Membership {
                team_id,
                kind: MembershipKind::Team,
                is_owner: false,
                expires_on: None,
            })
            .collect(),
        public_team_ids: vec![],
    }))
}

/// 複数のユーザーの、閲覧の判定に使う情報(`ResourceRef::User`)をまとめて読む(ユーザー一覧用。1 回の SQL)
///
/// 無効化されたユーザー・存在しないユーザーは含めない。有効な所属の定義は `load` と同じ。
pub async fn load_user_facts(
    pool: &PgPool,
    user_ids: &[i32],
    today: NaiveDate,
) -> anyhow::Result<HashMap<i32, ResourceRef>> {
    let ids: Vec<i64> = user_ids.iter().map(|&u| u as i64).collect();
    let rows = sqlx::query(&format!(
        "SELECT u.id::int4 AS user_id, u.is_guest, m.team_id, m.project_id
         FROM accounts_user u
         LEFT JOIN (
             SELECT tm.user_id, tm.team_id::int4 AS team_id, tm.scoped_project_id::int4 AS project_id
             FROM t_team_membership tm
             LEFT JOIN tickets_project p ON p.id = tm.scoped_project_id
             WHERE {MEMBERSHIP_VALID}
         ) m ON m.user_id = u.id
         WHERE u.id = ANY($1::int8[]) AND u.is_active"
    ))
    .bind(&ids)
    .bind(today)
    .fetch_all(pool)
    .await?;

    let mut out: HashMap<i32, ResourceRef> = HashMap::new();
    for row in rows {
        let user_id: i32 = row.get("user_id");
        let entry = out.entry(user_id).or_insert_with(|| ResourceRef::User {
            user_id,
            is_guest: row.get("is_guest"),
            team_ids: vec![],
            project_grants: vec![],
        });
        let ResourceRef::User {
            team_ids,
            project_grants,
            ..
        } = entry
        else {
            unreachable!()
        };
        match (
            row.get::<Option<i32>, _>("team_id"),
            row.get::<Option<i32>, _>("project_id"),
        ) {
            (Some(t), None) => team_ids.push(t),
            (Some(t), Some(p)) => project_grants.push((t, p)),
            _ => {}
        }
    }
    Ok(out)
}

/// DB のセッションと同じ暦の「今日」(UTC)。sqlx は接続のタイムゾーンを UTC にするため、
/// 既存の判定(`NOW()::date`)と日付がずれないように UTC で揃える。
pub fn today_utc() -> NaiveDate {
    chrono::Utc::now().date_naive()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        create_test_project, create_test_team, create_test_user, db_today, test_pool,
    };

    async fn add(
        pool: &PgPool,
        team: i32,
        user: i32,
        role: &str,
        project: Option<i32>,
        end: Option<NaiveDate>,
    ) {
        sqlx::query(
            "INSERT INTO t_team_membership (team_id, user_id, role, scoped_project_id, end_date, joined_at)
             VALUES ($1, $2, $3, $4, $5, NOW())",
        )
        .bind(team as i64)
        .bind(user as i64)
        .bind(role)
        .bind(project.map(|p| p as i64))
        .bind(end)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn loads_role_memberships_and_applies_one_expiry_rule() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let today = db_today();
        let team = create_test_team(&pool, "vr").await;
        let user = create_test_user(&pool, "vr").await;
        let project = create_test_project(&pool, "VR", user).await;
        sqlx::query("UPDATE tickets_project SET grace_period_days = 3 WHERE id = $1")
            .bind(project as i64)
            .execute(&pool)
            .await
            .unwrap();
        add(&pool, team, user, "admin", None, None).await;
        // 期限切れだが猶予期間内(2 日前 + 猶予 3 日)→ 有効
        add(
            &pool,
            team,
            user,
            "member",
            Some(project),
            Some(today - chrono::Duration::days(2)),
        )
        .await;

        let v = load(&pool, Principal::Human { user_id: user }, today)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(v.role, Role::FullMember);
        assert!(v.team_membership(team).is_some_and(|m| m.is_owner));
        assert!(v.has_project_membership(team, project));
        assert!(
            v.public_team_ids.contains(&team),
            "既存チームは Public(既定値)"
        );

        // 猶予期間も過ぎた(5 日前 + 3 日)→ 無効
        sqlx::query("UPDATE t_team_membership SET end_date = $1 WHERE user_id = $2 AND scoped_project_id IS NOT NULL")
            .bind(today - chrono::Duration::days(5))
            .bind(user as i64)
            .execute(&pool)
            .await
            .unwrap();
        let v = load(&pool, Principal::Human { user_id: user }, today)
            .await
            .unwrap()
            .unwrap();
        assert!(!v.has_project_membership(team, project));

        // Guest・システム管理者・Private
        sqlx::query("UPDATE accounts_user SET is_guest = true WHERE id = $1")
            .bind(user as i64)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE m_team SET visibility = 'private' WHERE id = $1")
            .bind(team as i64)
            .execute(&pool)
            .await
            .unwrap();
        let v = load(&pool, Principal::Human { user_id: user }, today)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(v.role, Role::Guest);
        assert!(!v.public_team_ids.contains(&team));
    }

    #[tokio::test]
    async fn inactive_or_missing_user_has_no_viewer() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = create_test_user(&pool, "vr-off").await;
        sqlx::query("UPDATE accounts_user SET is_active = false WHERE id = $1")
            .bind(user as i64)
            .execute(&pool)
            .await
            .unwrap();
        assert!(load(&pool, Principal::Human { user_id: user }, db_today())
            .await
            .unwrap()
            .is_none());
        assert!(
            load(&pool, Principal::Human { user_id: i32::MAX }, db_today())
                .await
                .unwrap()
                .is_none()
        );
        // 存在しない連携も None
        assert!(load(
            &pool,
            Principal::Integration {
                integration_id: i64::MAX
            },
            db_today()
        )
        .await
        .unwrap()
        .is_none());
    }

    #[tokio::test]
    async fn system_admin_follows_is_staff_during_migration() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = create_test_user(&pool, "vr-adm").await;
        // 移行の間は、is_staff の変更がトリガーで is_system_admin に写る
        sqlx::query("UPDATE accounts_user SET is_staff = true WHERE id = $1")
            .bind(user as i64)
            .execute(&pool)
            .await
            .unwrap();
        let v = load(&pool, Principal::Human { user_id: user }, db_today())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(v.role, Role::SystemAdmin);
        sqlx::query("UPDATE accounts_user SET is_staff = false WHERE id = $1")
            .bind(user as i64)
            .execute(&pool)
            .await
            .unwrap();
        let v = load(&pool, Principal::Human { user_id: user }, db_today())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(v.role, Role::FullMember);
    }

    /// ユーザー一覧用の一括読み込み: `load` と同じ期限の規則。無効化されたユーザーは含めない
    #[tokio::test]
    async fn load_user_facts_matches_load() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let today = db_today();
        let team = create_test_team(&pool, "vuf").await;
        let member = create_test_user(&pool, "vuf-m").await;
        let guest = create_test_user(&pool, "vuf-g").await;
        let loner = create_test_user(&pool, "vuf-l").await;
        let gone = create_test_user(&pool, "vuf-x").await;
        let project = create_test_project(&pool, "VUF", member).await;
        let expired = create_test_project(&pool, "VUX", member).await;
        add(&pool, team, member, "member", None, None).await;
        // 期限切れ(猶予 0 日)の所属は含めない。有効な所属は含める
        add(
            &pool,
            team,
            guest,
            "member",
            Some(expired),
            Some(today - chrono::Duration::days(1)),
        )
        .await;
        add(&pool, team, guest, "member", Some(project), Some(today)).await;
        sqlx::query("UPDATE accounts_user SET is_guest = true WHERE id = $1::int8")
            .bind(guest as i64)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE accounts_user SET is_active = false WHERE id = $1::int8")
            .bind(gone as i64)
            .execute(&pool)
            .await
            .unwrap();

        let facts = load_user_facts(&pool, &[member, guest, loner, gone], today)
            .await
            .unwrap();
        assert_eq!(facts.len(), 3, "無効化されたユーザーは含めない");
        for uid in [member, guest, loner] {
            let v = load(&pool, Principal::Human { user_id: uid }, today)
                .await
                .unwrap()
                .unwrap();
            let Some(ResourceRef::User {
                is_guest,
                team_ids,
                project_grants,
                ..
            }) = facts.get(&uid)
            else {
                panic!("ユーザーの情報がない: {uid}");
            };
            assert_eq!(*is_guest, v.role == Role::Guest);
            let mut expected_teams: Vec<i32> = v
                .memberships
                .iter()
                .filter(|m| m.kind == MembershipKind::Team)
                .map(|m| m.team_id)
                .collect();
            expected_teams.sort_unstable();
            let mut got = team_ids.clone();
            got.sort_unstable();
            assert_eq!(got, expected_teams);
            let mut got = project_grants.clone();
            got.sort_unstable();
            assert_eq!(got, v.project_grants());
        }
        let Some(ResourceRef::User { project_grants, .. }) = facts.get(&guest) else {
            unreachable!()
        };
        assert_eq!(
            project_grants,
            &vec![(team, project)],
            "期限切れの所属は含めない"
        );
    }

    /// 外部連携(F-7): 許可したチームだけが見える(Public でも、許可の無いチームは見えない)。無効化は None
    #[tokio::test]
    async fn integration_sees_only_allowed_teams() {
        use crate::domain::access::{
            can, Action, Decision, ResourceRef, SettingsPolicy, TeamFacts, Visibility,
        };
        let Some(pool) = test_pool().await else {
            return;
        };
        let today = db_today();
        let admin = create_test_user(&pool, "vint").await;
        let allowed = create_test_team(&pool, "vint-a").await;
        let other_public = create_test_team(&pool, "vint-o").await;
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO access_integration (name, key_hash, created_by) VALUES ('t', $1, $2::int8) RETURNING id",
        )
        .bind(format!("hash-{}", crate::test_support::unique_suffix()))
        .bind(admin as i64)
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO access_integration_team (integration_id, team_id) VALUES ($1, $2::int8)",
        )
        .bind(id)
        .bind(allowed as i64)
        .execute(&pool)
        .await
        .unwrap();

        let v = load(&pool, Principal::Integration { integration_id: id }, today)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(v.role, Role::Integration);
        assert_eq!(v.user_id(), None);
        let team = |t: i32| {
            ResourceRef::Team(TeamFacts {
                team_id: t,
                visibility: Visibility::Public,
                settings_policy: SettingsPolicy::Members,
                owner_count: 0,
            })
        };
        assert_eq!(can(&v, Action::Read, &team(allowed)), Decision::Allow);
        assert_eq!(
            can(&v, Action::Read, &team(other_public)),
            Decision::NotFound,
            "許可の無い Public チームは見えない"
        );

        sqlx::query("UPDATE access_integration SET is_active = false WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            load(&pool, Principal::Integration { integration_id: id }, today)
                .await
                .unwrap()
                .is_none()
        );
    }
}
