/// presentation/handlers/access_guard.rs — 管理操作の認可(チーム・プロジェクトの管理者だけが通れる)
///
/// チームの設定・メンバー管理と、チャット/Git連携の設定は、ログインしているだけでは操作できない。
/// 「システム管理者(is_staff)」か「そのチーム(プロジェクト)の管理者」だけが通る。
/// 各ハンドラはここの関数だけを呼び、判定を自前で書かない(書き漏らしを防ぐ)。
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use sqlx::PgPool;

use crate::domain::models::team_api::TeamOut;
use crate::infrastructure::repositories::{project_team_repo, team_archive_repo, user_repo};

const TEAM_ONLY: &str = "チームの管理者のみ操作できます";
const PROJECT_ONLY: &str = "プロジェクトの管理者のみ操作できます";
const SCOPE_REQUIRED: &str = "project か team のどちらか一方が必要です";
const SERVER_ERROR: &str = "サーバーエラーが発生しました";

fn reply(status: StatusCode, detail: &str) -> Response {
    (status, Json(json!({ "detail": detail }))).into_response()
}

async fn is_staff(pool: &PgPool, user_id: i32) -> anyhow::Result<bool> {
    Ok(user_repo::find_by_id(pool, user_id)
        .await?
        .map(|u| u.is_staff)
        .unwrap_or(false))
}

/// そのチームを管理できるか(システム管理者 / チームの管理者 / 管理者不在のチームの一般ユーザー)
pub async fn can_manage_team(pool: &PgPool, user_id: i32, team_id: i32) -> anyhow::Result<bool> {
    let staff = is_staff(pool, user_id).await?;
    if team_archive_repo::can_manage(pool, team_id, user_id, staff).await? {
        return Ok(true);
    }
    team_has_no_admin_for(pool, team_id, user_id).await
}

/// 管理者がひとりもいないチームを、そのチームのメンバーに開いておく(救済規則)。
/// 管理者がいないと、システム管理者以外の誰もメンバーを追加できず、チームが詰まるため。
/// 管理者が1人でもいれば適用されない。対象は、そのチームのチーム全体の所属を持つ Guest 以外の人だけ
/// (以前はメンバーでない人にも開いていて、自分を管理者として追加できた。DEMO-000169)。
/// Projectゲスト(scoped_project_id 付きの所属がある人)には適用しない。
/// Owner(admin)を付ける・外す操作には及ばない(`require_owner_operation` が別に止める。設計書 §4.3)。
async fn team_has_no_admin_for(pool: &PgPool, team_id: i32, user_id: i32) -> anyhow::Result<bool> {
    let open: bool = sqlx::query_scalar(
        "SELECT NOT EXISTS (
            SELECT 1 FROM t_team_membership
            WHERE team_id = $1 AND role = 'admin' AND scoped_project_id IS NULL
         ) AND NOT EXISTS (
            SELECT 1 FROM t_team_membership
            WHERE team_id = $1 AND user_id = $2 AND scoped_project_id IS NOT NULL
         ) AND EXISTS (
            SELECT 1 FROM t_team_membership tm JOIN accounts_user u ON u.id = tm.user_id
            WHERE tm.team_id = $1 AND tm.user_id = $2 AND tm.scoped_project_id IS NULL
              AND NOT u.is_guest
         )",
    )
    .bind(team_id as i64)
    .bind(user_id as i64)
    .fetch_one(pool)
    .await?;
    Ok(open)
}

/// そのプロジェクトを管理できるか(project_team_repo::can_manage に従う)
pub async fn can_manage_project(
    pool: &PgPool,
    user_id: i32,
    project_id: i32,
) -> anyhow::Result<bool> {
    let staff = is_staff(pool, user_id).await?;
    project_team_repo::can_manage(pool, project_id, user_id, staff).await
}

/// チームの管理者でなければ 403(DBエラーは 500)を返す。
/// 今の判定。ハンドラは `_v` 版に移した(テストが今の規則を確かめるために使う)。フェーズ H で削除する
#[allow(dead_code)]
pub async fn require_team_manager(
    pool: &PgPool,
    user_id: i32,
    team_id: i32,
) -> Result<(), Response> {
    match can_manage_team(pool, user_id, team_id).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(reply(StatusCode::FORBIDDEN, TEAM_ONLY)),
        Err(e) => {
            tracing::error!("can_manage_team failed: {:?}", e);
            Err(reply(StatusCode::INTERNAL_SERVER_ERROR, SERVER_ERROR))
        }
    }
}

/// チームの管理操作の判定(閲覧者を受け取る版。アクセス制御の再設計 D-2)
///
/// 今の判定(`can_manage_team`: システム管理者・チームの管理者・管理者不在の救済)を、試運転の間はそのまま使う。
/// `on` のときは新しい規則(`policy::manages_team`: Team Owner の方針。キー経由・Guest は不可)を使う。
pub async fn require_team_manager_v(
    pool: &PgPool,
    viewer: &crate::domain::access::Viewer,
    team_id: i32,
    action: crate::domain::access::Action,
    route: &'static str,
) -> Result<(), Response> {
    let Some(user_id) = viewer.user_id() else {
        return Err(reply(StatusCode::FORBIDDEN, TEAM_ONLY));
    };
    let legacy = match can_manage_team(pool, user_id, team_id).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(reply(StatusCode::FORBIDDEN, TEAM_ONLY)),
        Err(e) => {
            tracing::error!("can_manage_team failed: {:?}", e);
            return Err(reply(StatusCode::INTERNAL_SERVER_ERROR, SERVER_ERROR));
        }
    };
    crate::presentation::extractors::authorize::gate_team(
        pool, viewer, team_id, action, legacy, route,
    )
    .await
}

/// Owner の操作(Owner を付ける・外す、Guest の追加)の判定。試運転のスイッチを通さず、すぐに適用する
/// (詳細設計書 §5.4。Owner を決める権限は、試運転の結果を待たずに閉じる。DEMO-000169)。
/// 今の判定(`require_team_manager_v`)に加えて呼ぶ。見えなければ 404、見えるが不可なら 403。
pub async fn require_owner_operation(
    pool: &PgPool,
    viewer: &crate::domain::access::Viewer,
    team_id: i32,
) -> Result<(), Response> {
    use crate::domain::access::{can, Action, Decision, ResourceRef};
    let facts = match crate::infrastructure::access::facts_repo::facts_for_team(pool, team_id).await
    {
        Ok(Some(f)) => f,
        Ok(None) => return Err(reply(StatusCode::NOT_FOUND, "見つかりません")),
        Err(e) => {
            tracing::error!("facts_for_team failed: {:?}", e);
            return Err(reply(StatusCode::INTERNAL_SERVER_ERROR, SERVER_ERROR));
        }
    };
    match can(viewer, Action::ManageOwners, &ResourceRef::Team(facts)) {
        Decision::Allow => Ok(()),
        Decision::NotFound => Err(reply(StatusCode::NOT_FOUND, "見つかりません")),
        Decision::Forbidden => Err(reply(
            StatusCode::FORBIDDEN,
            "Owner の指名・解除と Guest の追加は、チームの Owner とシステム管理者だけが行えます",
        )),
    }
}

/// 連携の対象(project / team)の管理者でなければ 403。どちらも指定が無ければ 400。
/// 両方指定された場合は両方の管理者であること(取りこぼしで他チームを操作されないため)。
/// 今の判定。ハンドラは `_v` 版に移した(テストが今の規則を確かめるために使う)。フェーズ H で削除する
#[allow(dead_code)]
pub async fn require_scope_manager(
    pool: &PgPool,
    user_id: i32,
    project: Option<i32>,
    team: Option<i32>,
) -> Result<(), Response> {
    if project.is_none() && team.is_none() {
        return Err(reply(StatusCode::BAD_REQUEST, SCOPE_REQUIRED));
    }
    if let Some(project_id) = project {
        match can_manage_project(pool, user_id, project_id).await {
            Ok(true) => {}
            Ok(false) => return Err(reply(StatusCode::FORBIDDEN, PROJECT_ONLY)),
            Err(e) => {
                tracing::error!("can_manage_project failed: {:?}", e);
                return Err(reply(StatusCode::INTERNAL_SERVER_ERROR, SERVER_ERROR));
            }
        }
    }
    if let Some(team_id) = team {
        require_team_manager(pool, user_id, team_id).await?;
    }
    Ok(())
}

/// 連携の設定の権限(アクセス制御の再設計 D-8)。今の判定は `require_scope_manager`。
/// 新しい判定: プロジェクトの設定を管理できること・チームの設定を管理できること(ManageSettings。
/// キー経由は不可)。切り替えはプロジェクト・チームのスイッチに従う。
pub async fn require_scope_manager_v(
    pool: &PgPool,
    viewer: &crate::domain::access::Viewer,
    project: Option<i32>,
    team: Option<i32>,
    route: &'static str,
) -> Result<(), Response> {
    let Some(user_id) = viewer.user_id() else {
        return Err(reply(StatusCode::FORBIDDEN, TEAM_ONLY));
    };
    if project.is_none() && team.is_none() {
        return Err(reply(StatusCode::BAD_REQUEST, SCOPE_REQUIRED));
    }
    if let Some(project_id) = project {
        let legacy = match can_manage_project(pool, user_id, project_id).await {
            Ok(true) => Ok(()),
            Ok(false) => Err(reply(StatusCode::FORBIDDEN, PROJECT_ONLY)),
            Err(e) => {
                tracing::error!("can_manage_project failed: {:?}", e);
                return Err(reply(StatusCode::INTERNAL_SERVER_ERROR, SERVER_ERROR));
            }
        };
        crate::presentation::extractors::authorize::gate_project(
            pool,
            viewer,
            project_id,
            crate::domain::access::Action::ManageSettings,
            legacy,
            route,
        )
        .await?;
    }
    if let Some(team_id) = team {
        require_team_manager_v(
            pool,
            viewer,
            team_id,
            crate::domain::access::Action::ManageSettings,
            route,
        )
        .await?;
    }
    Ok(())
}

/// Slack の Webhook URL は、管理者にだけ見せる(URLを知っていれば誰でも投稿できてしまうため)。
pub fn hide_webhook_unless_manager(team: &mut TeamOut, viewer_can_manage: bool) {
    if !viewer_can_manage {
        team.slack_webhook_url = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{create_test_project, create_test_team, create_test_user, test_pool};
    use chrono::Utc;

    async fn add_membership(pool: &PgPool, team: i32, user: i32, role: &str) {
        sqlx::query(
            "INSERT INTO t_team_membership (team_id, user_id, role, joined_at) VALUES ($1, $2, $3, NOW())",
        )
        .bind(team as i64)
        .bind(user as i64)
        .bind(role)
        .execute(pool)
        .await
        .expect("所属の追加に失敗");
    }

    fn status_of(r: Result<(), Response>) -> Option<StatusCode> {
        r.err().map(|resp| resp.status())
    }

    #[tokio::test]
    async fn team_manager_is_staff_or_team_admin_only() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let team = create_test_team(&pool, "guard").await;
        let admin = create_test_user(&pool, "ga").await;
        let member = create_test_user(&pool, "gm").await;
        let outsider = create_test_user(&pool, "go").await;
        let staff = create_test_user(&pool, "gs").await;
        add_membership(&pool, team, admin, "admin").await;
        add_membership(&pool, team, member, "member").await;
        sqlx::query("UPDATE accounts_user SET is_staff = true WHERE id = $1")
            .bind(staff as i64)
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(
            status_of(require_team_manager(&pool, admin, team).await),
            None
        );
        assert_eq!(
            status_of(require_team_manager(&pool, staff, team).await),
            None
        );
        for denied in [member, outsider] {
            assert_eq!(
                status_of(require_team_manager(&pool, denied, team).await),
                Some(StatusCode::FORBIDDEN)
            );
        }
        // 別チームの管理者では通らない
        // (管理者のいるチームである必要がある。管理者不在のチームは救済規則で一般ユーザーにも開く)
        let other = create_test_team(&pool, "guard-other").await;
        let other_admin = create_test_user(&pool, "goa").await;
        add_membership(&pool, other, other_admin, "admin").await;
        assert_eq!(
            status_of(require_team_manager(&pool, admin, other).await),
            Some(StatusCode::FORBIDDEN)
        );
    }

    #[tokio::test]
    async fn scope_manager_checks_project_and_team_and_rejects_empty() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let team = create_test_team(&pool, "scope").await;
        let admin = create_test_user(&pool, "sa").await;
        let outsider = create_test_user(&pool, "so").await;
        add_membership(&pool, team, admin, "admin").await;
        let project = create_test_project(&pool, "SC", admin).await;
        sqlx::query("INSERT INTO tickets_project_teams (project_id, team_id) VALUES ($1, $2)")
            .bind(project as i64)
            .bind(team as i64)
            .execute(&pool)
            .await
            .unwrap();

        // 指定なしは 400(全件を返さない)
        assert_eq!(
            status_of(require_scope_manager(&pool, admin, None, None).await),
            Some(StatusCode::BAD_REQUEST)
        );
        // チーム管理者は team / project どちらでも通る
        assert_eq!(
            status_of(require_scope_manager(&pool, admin, None, Some(team)).await),
            None
        );
        assert_eq!(
            status_of(require_scope_manager(&pool, admin, Some(project), None).await),
            None
        );
        // 無関係のユーザーは 403
        assert_eq!(
            status_of(require_scope_manager(&pool, outsider, None, Some(team)).await),
            Some(StatusCode::FORBIDDEN)
        );
        assert_eq!(
            status_of(require_scope_manager(&pool, outsider, Some(project), None).await),
            Some(StatusCode::FORBIDDEN)
        );
        // 片方だけ管理者でも、両方指定なら通らない
        let other_team = create_test_team(&pool, "scope-other").await;
        let other_admin = create_test_user(&pool, "soa").await;
        add_membership(&pool, other_team, other_admin, "admin").await;
        assert_eq!(
            status_of(require_scope_manager(&pool, admin, None, Some(other_team)).await),
            Some(StatusCode::FORBIDDEN)
        );
    }

    #[tokio::test]
    async fn team_without_any_admin_is_open_to_members_but_not_outsiders_or_project_guests() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let team = create_test_team(&pool, "orphan").await;
        let user = create_test_user(&pool, "ou").await;
        let outsider = create_test_user(&pool, "oo").await;
        let guest = create_test_user(&pool, "og").await;
        let project = create_test_project(&pool, "OG", user).await;
        // 管理者のいないチームでは、そのチームのメンバーが管理操作できる(救済)
        add_membership(&pool, team, user, "member").await;
        assert_eq!(
            status_of(require_team_manager(&pool, user, team).await),
            None
        );
        // メンバーでない人には開かない(以前は開いていて、自分を管理者として追加できた。DEMO-000169)
        assert_eq!(
            status_of(require_team_manager(&pool, outsider, team).await),
            Some(StatusCode::FORBIDDEN)
        );
        // Projectゲスト(scoped 所属)には開かない
        sqlx::query(
            "INSERT INTO t_team_membership (team_id, user_id, role, scoped_project_id, joined_at) VALUES ($1, $2, 'member', $3, NOW())",
        )
        .bind(team as i64)
        .bind(guest as i64)
        .bind(project as i64)
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            status_of(require_team_manager(&pool, guest, team).await),
            Some(StatusCode::FORBIDDEN)
        );
        // 管理者が1人でもいれば、救済は適用されず、一般のユーザーは 403 に戻る
        let admin = create_test_user(&pool, "oa").await;
        add_membership(&pool, team, admin, "admin").await;
        assert_eq!(
            status_of(require_team_manager(&pool, user, team).await),
            Some(StatusCode::FORBIDDEN)
        );
    }

    #[test]
    fn webhook_is_hidden_from_non_managers_only() {
        let base = TeamOut {
            id: 1,
            name: "t".into(),
            slug: "t".into(),
            description: String::new(),
            icon: String::new(),
            color: String::new(),
            slack_webhook_url: Some("https://hooks.slack.com/services/T/B/X".into()),
            is_active: true,
            member_count: 0,
            project_count: 0,
            archived_at: None,
            viewer_can_manage: false,
            viewer_can_manage_owners: false,
            visibility: "public".into(),
            settings_policy: "members".into(),
            viewer_is_member: false,
            created_at: Utc::now(),
            prefix: None,
        };
        let mut hidden = base.clone();
        hide_webhook_unless_manager(&mut hidden, false);
        assert!(hidden.slack_webhook_url.is_none());
        let mut shown = base;
        hide_webhook_unless_manager(&mut shown, true);
        assert!(shown.slack_webhook_url.is_some());
    }
}
