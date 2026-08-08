/// infrastructure/repositories/membership_repo.rs — プロジェクトメンバーシップ永続化
///
/// tickets_project_membership テーブル の CRUD 操作

use chrono::NaiveDate;
use sqlx::{PgPool, Row};

use crate::domain::models::membership_api::*;
use crate::domain::models::ticket_api::UserSummaryOut;

// =============================================================================
// Memberships - 一覧/詳細/CRUD
// =============================================================================

pub async fn find_all_memberships(
    pool: &PgPool,
    project: Option<i32>,
    page: i64,
) -> anyhow::Result<Vec<MembershipOut>> {
    const PAGE_SIZE: i64 = 50;
    let page = page.max(1);
    let offset = (page - 1) * PAGE_SIZE;

    let rows = match project {
        Some(project_id) => {
            sqlx::query(
                "SELECT
                    m.id::int4, m.user_id::int4, m.project_id::int4, m.start_date, m.end_date,
                    m.note, m.added_by_id::int4, m.created_at,
                    u.id::int4 as user_id, u.username, u.email, u.display_name,
                    ab.id::int4 as added_by_id, ab.username as added_by_username, ab.email as added_by_email, ab.display_name as added_by_display_name,
                    p.grace_period_days
                 FROM tickets_project_membership m
                 JOIN accounts_user u ON m.user_id = u.id
                 JOIN accounts_user ab ON m.added_by_id = ab.id
                 JOIN tickets_project p ON m.project_id = p.id
                 WHERE m.project_id = $1
                 ORDER BY u.username ASC
                 LIMIT $2 OFFSET $3"
            )
            .bind(project_id)
            .bind(PAGE_SIZE)
            .bind(offset)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                "SELECT
                    m.id::int4, m.user_id::int4, m.project_id::int4, m.start_date, m.end_date,
                    m.note, m.added_by_id::int4, m.created_at,
                    u.id::int4 as user_id, u.username, u.email, u.display_name,
                    ab.id::int4 as added_by_id, ab.username as added_by_username, ab.email as added_by_email, ab.display_name as added_by_display_name,
                    p.grace_period_days
                 FROM tickets_project_membership m
                 JOIN accounts_user u ON m.user_id = u.id
                 JOIN accounts_user ab ON m.added_by_id = ab.id
                 JOIN tickets_project p ON m.project_id = p.id
                 ORDER BY u.username ASC
                 LIMIT $1 OFFSET $2"
            )
            .bind(PAGE_SIZE)
            .bind(offset)
            .fetch_all(pool)
            .await?
        }
    };

    let memberships = rows
        .into_iter()
        .map(|row| {
            let user = UserSummaryOut {
                id: row.get(8),
                username: row.get(9),
                email: row.get(10),
                display_name: row.get(11),
            };
            let added_by = UserSummaryOut {
                id: row.get(12),
                username: row.get(13),
                email: row.get(14),
                display_name: row.get(15),
            };
            let end_date: Option<NaiveDate> = row.get(4);
            let grace_period_days: i32 = row.get(16);

            let (is_active, is_in_grace_period, days_until_expiry) =
                calculate_membership_status(end_date, grace_period_days);

            MembershipOut {
                id: row.get(0),
                user,
                project: row.get(2),
                start_date: row.get(3),
                end_date,
                note: row.get(5),
                added_by,
                created_at: row.get(7),
                is_active,
                is_in_grace_period,
                days_until_expiry,
            }
        })
        .collect();

    Ok(memberships)
}

pub async fn count_memberships(pool: &PgPool, project: Option<i32>) -> anyhow::Result<i64> {
    let count = match project {
        Some(project_id) => {
            let row = sqlx::query("SELECT COUNT(*) as count FROM tickets_project_membership WHERE project_id = $1")
                .bind(project_id)
                .fetch_one(pool)
                .await?;
            row.get::<i64, usize>(0)
        }
        None => {
            let row = sqlx::query("SELECT COUNT(*) as count FROM tickets_project_membership")
                .fetch_one(pool)
                .await?;
            row.get::<i64, usize>(0)
        }
    };
    Ok(count)
}

pub async fn find_membership_by_id(pool: &PgPool, id: i32) -> anyhow::Result<Option<MembershipOut>> {
    let row_opt = sqlx::query(
        "SELECT
            m.id::int4, m.user_id::int4, m.project_id::int4, m.start_date, m.end_date,
            m.note, m.added_by_id::int4, m.created_at,
            u.id::int4 as user_id, u.username, u.email, u.display_name,
            ab.id::int4 as added_by_id, ab.username as added_by_username, ab.email as added_by_email, ab.display_name as added_by_display_name,
            p.grace_period_days
         FROM tickets_project_membership m
         JOIN accounts_user u ON m.user_id = u.id
         JOIN accounts_user ab ON m.added_by_id = ab.id
         JOIN tickets_project p ON m.project_id = p.id
         WHERE m.id = $1"
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    let membership = row_opt.map(|row| {
        let user = UserSummaryOut {
            id: row.get(8),
            username: row.get(9),
            email: row.get(10),
            display_name: row.get(11),
        };
        let added_by = UserSummaryOut {
            id: row.get(12),
            username: row.get(13),
            email: row.get(14),
            display_name: row.get(15),
        };
        let end_date: Option<NaiveDate> = row.get(4);
        let grace_period_days: i32 = row.get(16);

        let (is_active, is_in_grace_period, days_until_expiry) =
            calculate_membership_status(end_date, grace_period_days);

        MembershipOut {
            id: row.get(0),
            user,
            project: row.get(2),
            start_date: row.get(3),
            end_date,
            note: row.get(5),
            added_by,
            created_at: row.get(7),
            is_active,
            is_in_grace_period,
            days_until_expiry,
        }
    });

    Ok(membership)
}

pub async fn check_membership_exists(
    pool: &PgPool,
    project_id: i32,
    user_id: i32,
) -> anyhow::Result<bool> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tickets_project_membership WHERE project_id = $1 AND user_id = $2"
    )
    .bind(project_id)
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}

pub async fn create_membership(
    pool: &PgPool,
    input: &MembershipCreateIn,
    added_by_id: i32,
) -> anyhow::Result<i32> {
    let membership_id: i32 = sqlx::query_scalar(
        "INSERT INTO tickets_project_membership (project_id, user_id, start_date, end_date, note, added_by_id, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, NOW())
         RETURNING id::int4"
    )
    .bind(input.project)
    .bind(input.user_id)
    .bind(input.start_date)
    .bind(input.end_date)
    .bind(&input.note)
    .bind(added_by_id)
    .fetch_one(pool)
    .await?;

    Ok(membership_id)
}

pub async fn update_membership(
    pool: &PgPool,
    id: i32,
    input: &MembershipUpdateIn,
) -> anyhow::Result<bool> {
    // 部分更新(Option値のうちSomeのものだけ更新)
    // COALESCE を使用して、Noneの場合は既存値を保持
    let rows_affected = sqlx::query(
        "UPDATE tickets_project_membership
         SET start_date = COALESCE($1, start_date),
             end_date = COALESCE($2, end_date),
             note = COALESCE($3, note)
         WHERE id = $4"
    )
    .bind(input.start_date)
    .bind(input.end_date)
    .bind(&input.note)
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected();

    Ok(rows_affected > 0)
}

pub async fn delete_membership(pool: &PgPool, id: i32) -> anyhow::Result<bool> {
    let rows_affected = sqlx::query("DELETE FROM tickets_project_membership WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();

    Ok(rows_affected > 0)
}

pub async fn get_membership_user_id(pool: &PgPool, id: i32) -> anyhow::Result<Option<i32>> {
    let row_opt = sqlx::query("SELECT user_id::int4 FROM tickets_project_membership WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?;

    Ok(row_opt.map(|row| row.get(0)))
}

// =============================================================================
// ヘルパー関数
// =============================================================================

/// メンバーシップのステータスを計算
/// (is_active, is_in_grace_period, days_until_expiry)
fn calculate_membership_status(
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
