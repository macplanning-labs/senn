/// infrastructure/repositories/membership_repo.rs — チケットアクセス制御
///
/// L2②(2026-09-11)で旧 tickets_project_membership の CRUD を廃止。
/// メンバーシップは m_team / t_team_membership(team_repo.rs)に一本化された。
/// ここにはチケットへのアクセス可否判定(check_ticket_access)のみが残る。

use chrono::NaiveDate;
use sqlx::PgPool;

pub async fn check_ticket_access(
    pool: &PgPool,
    ticket_id: i32,
    user_id: i32,
) -> anyhow::Result<bool> {
    use crate::infrastructure::repositories::team_repo;

    let is_staff: bool = sqlx::query_scalar("SELECT is_staff FROM accounts_user WHERE id = $1")
        .bind(user_id)
        .fetch_one(pool)
        .await?;

    if is_staff {
        return Ok(true);
    }

    // project_id/team_idはDB上bigintなので、i32でdecodeするには::int4キャストが必須
    let row_opt: Option<(Option<i32>, Option<i32>)> = sqlx::query_as(
        "SELECT project_id::int4, team_id::int4 FROM tickets_ticket WHERE id = $1"
    )
    .bind(ticket_id)
    .fetch_optional(pool)
    .await?;

    if let Some((project_id, team_id)) = row_opt {
        if let Some(t_id) = team_id {
            if team_repo::check_team_scoped_access(pool, t_id, project_id, user_id).await? {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

// =============================================================================
// ヘルパー関数
// =============================================================================

/// メンバーシップのステータスを計算
/// (is_active, is_in_grace_period, days_until_expiry)
pub(crate) fn calculate_membership_status(
    end_date: Option<NaiveDate>,
    grace_period_days: i32,
) -> (bool, bool, Option<i64>) {
    use chrono::Local;

    let today = Local::now().naive_utc().date();

    match end_date {
        None => {
            // end_date がない場合は active
            (true, false, None)
        }
        Some(ed) => {
            let grace_end = ed + chrono::Duration::days(grace_period_days as i64);

            let is_active = today <= grace_end;
            let is_in_grace_period = ed < today && today <= grace_end;

            let days_until_expiry = (grace_end - today).num_days();

            (is_active, is_in_grace_period, Some(days_until_expiry))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;

    async fn add_team_membership(pool: &sqlx::PgPool, team_id: i32, user_id: i32, role: &str) {
        sqlx::query(
            "INSERT INTO t_team_membership (team_id, user_id, role, joined_at)
             VALUES ($1, $2, $3, NOW())"
        )
        .bind(team_id)
        .bind(user_id)
        .bind(role)
        .execute(pool)
        .await
        .expect("team membership 作成失敗");
    }

    async fn owner_team_id(pool: &sqlx::PgPool, project_id: i32) -> i32 {
        sqlx::query_scalar::<_, i32>(
            "SELECT owner_team_id::int4 FROM tickets_project WHERE id = $1"
        )
        .bind(project_id)
        .fetch_one(pool)
        .await
        .expect("owner_team_id 取得失敗")
    }

    async fn set_ticket_team(pool: &sqlx::PgPool, ticket_id: i32, team_id: i32) {
        sqlx::query("UPDATE tickets_ticket SET team_id = $1 WHERE id = $2")
            .bind(team_id)
            .bind(ticket_id)
            .execute(pool)
            .await
            .expect("ticket team_id 更新失敗");
    }

    #[tokio::test]
    async fn test_check_ticket_access_team_member_without_project() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let author = test_support::create_test_user(&pool, "acc_author2").await;
        let team_user = test_support::create_test_user(&pool, "acc_tm").await;
        let outsider = test_support::create_test_user(&pool, "acc_out").await;
        let project_id = test_support::create_test_project(&pool, "ACCT", author).await;
        let team_id = owner_team_id(&pool, project_id).await;
        let ticket_id = test_support::create_test_ticket(&pool, project_id, "ACCT", author).await;
        set_ticket_team(&pool, ticket_id, team_id).await;
        add_team_membership(&pool, team_id, team_user, "member").await;

        let ok = check_ticket_access(&pool, ticket_id, team_user)
            .await
            .expect("access check");
        assert!(ok, "Team メンバーはアクセスできる");

        let denied = check_ticket_access(&pool, ticket_id, outsider)
            .await
            .expect("access check outsider");
        assert!(!denied, "どちらにも属さないユーザーは拒否");
    }

    #[tokio::test]
    async fn test_check_ticket_access_staff_bypass() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let author = test_support::create_test_user(&pool, "acc_author4").await;
        let staff = test_support::create_test_user(&pool, "acc_staff").await;
        sqlx::query("UPDATE accounts_user SET is_staff = true WHERE id = $1")
            .bind(staff)
            .execute(&pool)
            .await
            .expect("staff 化失敗");
        let project_id = test_support::create_test_project(&pool, "ACCS", author).await;
        let ticket_id = test_support::create_test_ticket(&pool, project_id, "ACCS", author).await;

        let ok = check_ticket_access(&pool, ticket_id, staff)
            .await
            .expect("access check");
        assert!(ok, "staff は常にアクセスできる");
    }

    async fn add_scoped_team_membership(
        pool: &sqlx::PgPool,
        team_id: i32,
        user_id: i32,
        scoped_project_id: i32,
        end_date: Option<NaiveDate>,
    ) {
        sqlx::query(
            "INSERT INTO t_team_membership (team_id, user_id, role, scoped_project_id, end_date, joined_at)
             VALUES ($1, $2, 'member', $3, $4, NOW())"
        )
        .bind(team_id)
        .bind(user_id)
        .bind(scoped_project_id)
        .bind(end_date)
        .execute(pool)
        .await
        .expect("scoped team membership 作成失敗");
    }

    async fn create_project_under_team(pool: &sqlx::PgPool, team_id: i32, prefix_base: &str) -> i32 {
        let prefix = format!("{prefix_base}{}", test_support::unique_suffix());
        let prefix = prefix[..prefix.len().min(20)].to_string();
        sqlx::query_scalar::<_, i32>(
            "INSERT INTO tickets_project (name, prefix, description, status, created_at, grace_period_days, owner_team_id)
             VALUES ($1, $2, '', 'active', NOW(), 3, $3)
             RETURNING id::int4"
        )
        .bind(format!("テストプロジェクト-{prefix}"))
        .bind(&prefix)
        .bind(team_id)
        .fetch_one(pool)
        .await
        .expect("テストプロジェクト作成に失敗")
    }

    #[tokio::test]
    async fn test_check_ticket_access_scoped_guest_within_project() {
        // L2: Projectゲスト(scoped_project_id)は、期限内なら該当Projectのチケットにアクセスできる
        let Some(pool) = test_support::test_pool().await else { return; };
        let author = test_support::create_test_user(&pool, "acc_author_l2a").await;
        let guest = test_support::create_test_user(&pool, "acc_guest_l2a").await;
        let project_id = test_support::create_test_project(&pool, "ACCL2A", author).await;
        let team_id = owner_team_id(&pool, project_id).await;
        let ticket_id = test_support::create_test_ticket(&pool, project_id, "ACCL2A", author).await;
        set_ticket_team(&pool, ticket_id, team_id).await;

        let tomorrow = chrono::Local::now().naive_utc().date() + chrono::Duration::days(1);
        add_scoped_team_membership(&pool, team_id, guest, project_id, Some(tomorrow)).await;

        let ok = check_ticket_access(&pool, ticket_id, guest)
            .await
            .expect("access check");
        assert!(ok, "期限内のProjectゲストは該当Projectのチケットにアクセスできる");
    }

    #[tokio::test]
    async fn test_check_ticket_access_scoped_guest_expired() {
        // L2: end_date + grace_period_days を過ぎたProjectゲストはアクセス不可
        let Some(pool) = test_support::test_pool().await else { return; };
        let author = test_support::create_test_user(&pool, "acc_author_l2b").await;
        let guest = test_support::create_test_user(&pool, "acc_guest_l2b").await;
        let project_id = test_support::create_test_project(&pool, "ACCL2B", author).await;
        let team_id = owner_team_id(&pool, project_id).await;
        let ticket_id = test_support::create_test_ticket(&pool, project_id, "ACCL2B", author).await;
        set_ticket_team(&pool, ticket_id, team_id).await;

        // create_test_projectのgrace_period_daysは0なので、昨日の日付は確実に期限切れ
        let yesterday = chrono::Local::now().naive_utc().date() - chrono::Duration::days(1);
        add_scoped_team_membership(&pool, team_id, guest, project_id, Some(yesterday)).await;

        let ok = check_ticket_access(&pool, ticket_id, guest)
            .await
            .expect("access check");
        assert!(!ok, "期限切れのProjectゲストはアクセスできない");
    }

    #[tokio::test]
    async fn test_check_ticket_access_scoped_guest_wrong_project() {
        // L2: 同じチーム傘下でも、scoped_project_idと異なるProjectのチケットにはアクセスできない
        let Some(pool) = test_support::test_pool().await else { return; };
        let author = test_support::create_test_user(&pool, "acc_author_l2c").await;
        let guest = test_support::create_test_user(&pool, "acc_guest_l2c").await;
        let project_a = test_support::create_test_project(&pool, "ACCL2C", author).await;
        let team_id = owner_team_id(&pool, project_a).await;
        let project_b = create_project_under_team(&pool, team_id, "ACCL2D").await;
        let ticket_in_b = test_support::create_test_ticket(&pool, project_b, "ACCL2D", author).await;
        set_ticket_team(&pool, ticket_in_b, team_id).await;

        // guestはproject_aにのみscoped
        add_scoped_team_membership(&pool, team_id, guest, project_a, None).await;

        let ok = check_ticket_access(&pool, ticket_in_b, guest)
            .await
            .expect("access check");
        assert!(!ok, "別Projectに限定されたゲストは、同じチーム傘下の別Projectのチケットにアクセスできない");
    }
}
