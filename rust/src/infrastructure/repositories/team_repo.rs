/// infrastructure/repositories/team_repo.rs — チーム永続化
///
/// Teams (m_team) and Team Memberships (t_team_membership) の CRUD 操作

use sqlx::{PgPool, Row};

use crate::domain::models::team_api::*;
use crate::domain::models::ticket_api::UserSummaryOut;

// =============================================================================
// Teams - 一覧/詳細/CRUD
// =============================================================================

pub async fn find_all_teams(pool: &PgPool, page: i64) -> anyhow::Result<Vec<TeamOut>> {
    const PAGE_SIZE: i64 = 50;
    let page = page.max(1);
    let offset = (page - 1) * PAGE_SIZE;

    let rows = sqlx::query(
        "SELECT
            t.id::int4, t.name, t.slug, t.description, t.icon, t.color, t.slack_webhook_url, t.is_active, t.created_at,
            (SELECT COUNT(*)::int8 FROM t_team_membership WHERE team_id = t.id AND scoped_project_id IS NULL) as member_count,
            (SELECT COUNT(*)::int8 FROM tickets_project WHERE owner_team_id = t.id) as project_count,
            t.prefix
         FROM m_team t
         ORDER BY t.name ASC
         LIMIT $1 OFFSET $2"
    )
    .bind(PAGE_SIZE)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    let teams = rows
        .into_iter()
        .map(|row| TeamOut {
            id: row.get(0),
            name: row.get(1),
            slug: row.get(2),
            description: row.get(3),
            icon: row.get(4),
            color: row.get(5),
            slack_webhook_url: row.get(6),
            is_active: row.get(7),
            created_at: row.get(8),
            member_count: row.get(9),
            project_count: row.get(10),
            prefix: row.get(11),
        })
        .collect();

    Ok(teams)
}

pub async fn count_teams(pool: &PgPool) -> anyhow::Result<i64> {
    let row = sqlx::query("SELECT COUNT(*) as count FROM m_team")
        .fetch_one(pool)
        .await?;
    let count: i64 = row.get(0);
    Ok(count)
}

pub async fn find_team_by_id(pool: &PgPool, id: i32) -> anyhow::Result<Option<TeamOut>> {
    let row_opt = sqlx::query(
        "SELECT
            t.id::int4, t.name, t.slug, t.description, t.icon, t.color, t.slack_webhook_url, t.is_active, t.created_at,
            (SELECT COUNT(*)::int8 FROM t_team_membership WHERE team_id = t.id AND scoped_project_id IS NULL) as member_count,
            (SELECT COUNT(*)::int8 FROM tickets_project WHERE owner_team_id = t.id) as project_count,
            t.prefix
         FROM m_team t
         WHERE t.id = $1"
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    let team = row_opt.map(|row| TeamOut {
        id: row.get(0),
        name: row.get(1),
        slug: row.get(2),
        description: row.get(3),
        icon: row.get(4),
        color: row.get(5),
        slack_webhook_url: row.get(6),
        is_active: row.get(7),
        created_at: row.get(8),
        member_count: row.get(9),
        project_count: row.get(10),
        prefix: row.get(11),
    });

    Ok(team)
}

/// Prefix の正規化・検証
/// 入力 → trim → 大文字化 → 英数字チェック・長さチェック
/// 許可: A-Z / 0-9 のみ、長さ 1〜20
pub fn normalize_and_validate_prefix(input: Option<&str>) -> anyhow::Result<Option<String>> {
    match input {
        None => Ok(None),
        Some(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                let upper = trimmed.to_uppercase();
                if !upper.chars().all(|c| c.is_ascii_alphanumeric()) || upper.len() > 20 {
                    Err(anyhow::anyhow!("Prefix は英数字1〜20文字です"))
                } else {
                    Ok(Some(upper))
                }
            }
        }
    }
}

/// Prefix の一意性チェック（更新時、自チーム除外）
pub async fn check_prefix_uniqueness(
    pool: &sqlx::PgPool,
    prefix: &str,
    exclude_team_id: Option<i32>,
) -> anyhow::Result<bool> {
    let query = if let Some(id) = exclude_team_id {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM m_team WHERE UPPER(prefix) = UPPER($1) AND id <> $2")
            .bind(prefix)
            .bind(id)
            .fetch_one(pool)
            .await?
    } else {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM m_team WHERE UPPER(prefix) = UPPER($1)")
            .bind(prefix)
            .fetch_one(pool)
            .await?
    };
    Ok(query == 0)
}

/// Djangoの `django.utils.text.slugify(value)`(allow_unicode=Falseの既定動作)相当。
/// categories等とは異なりteamsはASCII限定slugify: 非ASCII文字(日本語など)は削除される。
fn generate_slug(input: &str) -> String {
    let mut result = String::new();
    for c in input.chars() {
        if c.is_ascii_alphanumeric() {
            result.push(c.to_ascii_lowercase());
        } else if (c.is_ascii_whitespace() || c == '-' || c == '_')
            && !result.is_empty()
            && !result.ends_with('-')
        {
            result.push('-');
        }
        // 非ASCII文字(日本語など)はDjangoのASCII限定slugifyと同様に削除する
    }
    let slug = result.trim_matches('-').to_string();

    if slug.is_empty() {
        "team".to_string()
    } else {
        slug
    }
}

pub async fn create_team(pool: &PgPool, input: &TeamWriteIn) -> anyhow::Result<i32> {
    // Slug 生成（入力が空ならname から自動生成）
    let mut slug = if input.slug.is_empty() {
        generate_slug(&input.name)
    } else {
        input.slug.clone()
    };

    // 重複チェック＆連番付与
    let mut counter = 1;
    loop {
        let existing_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM m_team WHERE slug = $1")
            .bind(&slug)
            .fetch_one(pool)
            .await?;

        if existing_count == 0 {
            break;
        }

        let base = if input.slug.is_empty() {
            generate_slug(&input.name)
        } else {
            input.slug.clone()
        };
        slug = format!("{}-{}", base, counter);
        counter += 1;
    }

    // Prefix の正規化・検証
    let prefix = if let Some(ref p) = input.prefix {
        normalize_and_validate_prefix(Some(p))?
    } else {
        None
    };

    // Prefix の一意性チェック（明示的に指定されている場合）
    if let Some(ref p) = prefix {
        if !check_prefix_uniqueness(pool, p, None).await? {
            return Err(anyhow::anyhow!("この Prefix は別のチームで使われています"));
        }
    }

    // prefix が指定されていなければ slug から生成
    let final_prefix = prefix.or_else(|| {
        let derived: String = slug.chars().filter(|c| c.is_ascii_alphanumeric()).take(20).collect();
        if derived.is_empty() {
            None
        } else {
            Some(derived.to_uppercase())
        }
    });

    let team_id: i32 = sqlx::query_scalar(
        "INSERT INTO m_team (name, slug, description, icon, color, slack_webhook_url, is_active, prefix, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW())
         RETURNING id::int4"
    )
    .bind(&input.name)
    .bind(&slug)
    .bind(&input.description)
    .bind(&input.icon)
    .bind(&input.color)
    .bind(input.slack_webhook_url.as_deref().unwrap_or(""))
    .bind(input.is_active)
    .bind(final_prefix.as_deref())
    .fetch_one(pool)
    .await?;

    Ok(team_id)
}

pub async fn update_team(pool: &PgPool, id: i32, input: &TeamWriteIn) -> anyhow::Result<bool> {
    // slug 維持（空なら既存値を保つ）
    let slug = if input.slug.is_empty() {
        // 既存の slug を使用するため、NULL を使う（SQL側で COALESCE）
        None
    } else {
        Some(input.slug.as_str())
    };

    // prefix 維持（空なら既存値を保つ）
    let prefix = if let Some(ref p) = input.prefix {
        normalize_and_validate_prefix(Some(p))?
    } else {
        // 既存の prefix を使用するため、NULL を使う（SQL側で COALESCE）
        None
    };

    // prefix の一意性チェック（明示的に指定されている場合）
    if let Some(ref p) = prefix {
        if !check_prefix_uniqueness(pool, p, Some(id)).await? {
            return Err(anyhow::anyhow!("この Prefix は別のチームで使われています"));
        }
    }

    let rows_affected = sqlx::query(
        "UPDATE m_team
         SET name = $1, slug = COALESCE($2::text, slug), description = $3, icon = $4, color = $5, slack_webhook_url = $6, is_active = $7, prefix = COALESCE($8, prefix)
         WHERE id = $9"
    )
    .bind(&input.name)
    .bind(slug)
    .bind(&input.description)
    .bind(&input.icon)
    .bind(&input.color)
    .bind(input.slack_webhook_url.as_deref().unwrap_or(""))
    .bind(input.is_active)
    .bind(prefix.as_deref())
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected();

    Ok(rows_affected > 0)
}

pub async fn delete_team(pool: &PgPool, id: i32) -> anyhow::Result<bool> {
    let mut tx = pool.begin().await?;

    // m_team_rule は on_delete=CASCADE、そのチームルールを参照するチケットの
    // linked_rules(M2M)も先に削除する必要がある
    sqlx::query(
        "DELETE FROM tickets_ticket_linked_rules WHERE teamrulemodel_id IN
         (SELECT id FROM m_team_rule WHERE team_id = $1)"
    ).bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM m_team_rule WHERE team_id = $1")
        .bind(id).execute(&mut *tx).await?;

    // t_team_membership は on_delete=CASCADE
    sqlx::query("DELETE FROM t_team_membership WHERE team_id = $1")
        .bind(id).execute(&mut *tx).await?;

    // tickets_project.owner_team は on_delete=SET_NULL
    sqlx::query("UPDATE tickets_project SET owner_team_id = NULL WHERE owner_team_id = $1")
        .bind(id).execute(&mut *tx).await?;

    let rows_affected = sqlx::query("DELETE FROM m_team WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();

    tx.commit().await?;
    Ok(rows_affected > 0)
}

// =============================================================================
// Team Memberships
// =============================================================================

/// チームの「本来のメンバー」一覧(L2: Projectゲスト限定の scoped_project_id 付き行は含めない。
/// 現行のTeam設定画面はProjectゲストを表示・管理するUIを持たないため、当面は従来通りの
/// 「チーム全体メンバー」のみを返す。ゲスト管理UIは別途 MemberSettings.tsx 側の対応が必要)
pub async fn find_team_members(pool: &PgPool, team_id: i32) -> anyhow::Result<Vec<TeamMembershipOut>> {
    let rows = sqlx::query(
        "SELECT
            tm.id::int4, tm.team_id::int4, tm.user_id::int4, tm.role, tm.joined_at,
            u.id::int4 as user_id, u.username, u.email, u.display_name
         FROM t_team_membership tm
         JOIN accounts_user u ON tm.user_id = u.id
         WHERE tm.team_id = $1 AND tm.scoped_project_id IS NULL
         ORDER BY u.username ASC"
    )
    .bind(team_id)
    .fetch_all(pool)
    .await?;

    let members = rows
        .into_iter()
        .map(|row| {
            let user = UserSummaryOut {
                id: row.get(5),
                username: row.get(6),
                email: row.get(7),
                display_name: row.get(8),
            };
            TeamMembershipOut {
                id: row.get(0),
                team: row.get(1),
                user,
                role: row.get(3),
                joined_at: row.get(4),
            }
        })
        .collect();

    Ok(members)
}

pub async fn check_user_exists(pool: &PgPool, user_id: i32) -> anyhow::Result<bool> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM accounts_user WHERE id = $1")
        .bind(user_id)
        .fetch_one(pool)
        .await?;
    Ok(count > 0)
}

/// チームの「本来のメンバー」かどうか(L2: Projectゲスト限定の scoped_project_id 付き行は含めない。
/// チーム設定・チーム招待の重複チェック等、Projectスコープを問わないチーム全体の権限判定に使う)
pub async fn check_team_membership_exists(
    pool: &PgPool,
    team_id: i32,
    user_id: i32,
) -> anyhow::Result<bool> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM t_team_membership WHERE team_id = $1 AND user_id = $2 AND scoped_project_id IS NULL"
    )
    .bind(team_id)
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}

/// L2: チームメンバーシップによるアクセス可否(Projectスコープ・期限を考慮する版)。
/// - scoped_project_id IS NULL の行(チーム全体メンバー): 無条件でアクセス可
/// - scoped_project_id が対象Projectと一致する行(Projectゲスト): end_date + Projectの
///   grace_period_days による期限判定を行う(membership_repo::calculate_membership_status)
///
/// `project_id` が None(Projectに属さないチケット等)の場合、Projectゲスト行は対象にならず、
/// チーム全体メンバーかどうかのみで判定する。
pub async fn check_team_scoped_access(
    pool: &PgPool,
    team_id: i32,
    project_id: Option<i32>,
    user_id: i32,
) -> anyhow::Result<bool> {
    use crate::infrastructure::repositories::membership_repo::calculate_membership_status;

    let rows = sqlx::query(
        "SELECT tm.scoped_project_id::int4, tm.end_date, p.grace_period_days
         FROM t_team_membership tm
         LEFT JOIN tickets_project p ON tm.scoped_project_id = p.id
         WHERE tm.team_id = $1 AND tm.user_id = $2
           AND (tm.scoped_project_id IS NULL OR ($3::int4 IS NOT NULL AND tm.scoped_project_id = $3))"
    )
    .bind(team_id)
    .bind(user_id)
    .bind(project_id)
    .fetch_all(pool)
    .await?;

    for row in &rows {
        let scoped_project_id: Option<i32> = row.get(0);
        if scoped_project_id.is_none() {
            // チーム全体メンバー: 無条件でアクセス可
            return Ok(true);
        }

        let end_date: Option<chrono::NaiveDate> = row.get(1);
        let grace_period_days: i32 = row.get(2);
        let (is_active, _, _) = calculate_membership_status(end_date, grace_period_days);
        if is_active {
            return Ok(true);
        }
    }

    Ok(false)
}

/// L2②: Project単体からのアクセス可否判定(旧 membership_repo::check_membership_exists の後継)。
/// Projectのowner_team_idを解決してからcheck_team_scoped_accessに委譲する。
/// owner_team_idが無いProject(データ不整合)はアクセス不可として扱う。
pub async fn check_project_access(pool: &PgPool, project_id: i32, user_id: i32) -> anyhow::Result<bool> {
    let owner_team_id: Option<i32> = sqlx::query_scalar(
        "SELECT owner_team_id::int4 FROM tickets_project WHERE id = $1"
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await?
    .flatten();

    match owner_team_id {
        Some(team_id) => check_team_scoped_access(pool, team_id, Some(project_id), user_id).await,
        None => Ok(false),
    }
}

pub async fn add_team_member(
    pool: &PgPool,
    team_id: i32,
    user_id: i32,
    role: &str,
) -> anyhow::Result<i32> {
    let membership_id: i32 = sqlx::query_scalar(
        "INSERT INTO t_team_membership (team_id, user_id, role, joined_at)
         VALUES ($1, $2, $3, NOW())
         RETURNING id::int4"
    )
    .bind(team_id)
    .bind(user_id)
    .bind(role)
    .fetch_one(pool)
    .await?;

    Ok(membership_id)
}

/// チーム全体メンバーの解除(L2: scoped_project_id付きのProjectゲスト行は対象外。
/// ゲストの解除は remove_team_guest を使う)
pub async fn remove_team_member(pool: &PgPool, team_id: i32, user_id: i32) -> anyhow::Result<bool> {
    let rows_affected = sqlx::query(
        "DELETE FROM t_team_membership WHERE team_id = $1 AND user_id = $2 AND scoped_project_id IS NULL"
    )
    .bind(team_id)
    .bind(user_id)
    .execute(pool)
    .await?
    .rows_affected();

    Ok(rows_affected > 0)
}

pub async fn get_team_member_by_id(
    pool: &PgPool,
    membership_id: i32,
) -> anyhow::Result<Option<TeamMembershipOut>> {
    let row_opt = sqlx::query(
        "SELECT
            tm.id::int4, tm.team_id::int4, tm.user_id::int4, tm.role, tm.joined_at,
            u.id::int4 as user_id, u.username, u.email, u.display_name
         FROM t_team_membership tm
         JOIN accounts_user u ON tm.user_id = u.id
         WHERE tm.id = $1"
    )
    .bind(membership_id)
    .fetch_optional(pool)
    .await?;

    let member = row_opt.map(|row| {
        let user = UserSummaryOut {
            id: row.get(5),
            username: row.get(6),
            email: row.get(7),
            display_name: row.get(8),
        };
        TeamMembershipOut {
            id: row.get(0),
            team: row.get(1),
            user,
            role: row.get(3),
            joined_at: row.get(4),
        }
    });

    Ok(member)
}

// =============================================================================
// L2: Project ゲスト(scoped_project_id 付き t_team_membership)
// =============================================================================

/// チームのProjectゲスト一覧(scoped_project_id IS NOT NULL の行のみ)
pub async fn find_team_guests(pool: &PgPool, team_id: i32) -> anyhow::Result<Vec<TeamGuestOut>> {
    use crate::infrastructure::repositories::membership_repo::calculate_membership_status;

    let rows = sqlx::query(
        "SELECT
            tm.id::int4, tm.team_id::int4, tm.end_date, tm.joined_at,
            u.id::int4 as user_id, u.username, u.email, u.display_name,
            p.id::int4 as project_id, p.name as project_name, p.prefix as project_prefix, p.grace_period_days
         FROM t_team_membership tm
         JOIN accounts_user u ON tm.user_id = u.id
         JOIN tickets_project p ON tm.scoped_project_id = p.id
         WHERE tm.team_id = $1 AND tm.scoped_project_id IS NOT NULL
         ORDER BY p.prefix ASC, u.username ASC"
    )
    .bind(team_id)
    .fetch_all(pool)
    .await?;

    let guests = rows
        .into_iter()
        .map(|row| {
            let user = UserSummaryOut {
                id: row.get(4),
                username: row.get(5),
                email: row.get(6),
                display_name: row.get(7),
            };
            let end_date: Option<chrono::NaiveDate> = row.get(2);
            let grace_period_days: i32 = row.get(11);
            let (is_active, is_in_grace_period, _) = calculate_membership_status(end_date, grace_period_days);
            TeamGuestOut {
                id: row.get(0),
                team: row.get(1),
                user,
                project: row.get(8),
                project_name: row.get(9),
                project_prefix: row.get(10),
                end_date,
                is_active,
                is_in_grace_period,
                joined_at: row.get(3),
            }
        })
        .collect();

    Ok(guests)
}

/// 指定Projectがそのチームの所有Projectであることを確認する(他チームのProjectを誤って指定できないようにする)
pub async fn project_belongs_to_team(pool: &PgPool, project_id: i32, team_id: i32) -> anyhow::Result<bool> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM tickets_project WHERE id = $1 AND owner_team_id = $2)"
    )
    .bind(project_id)
    .bind(team_id)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

/// 既に同じ(team, user, project)のゲスト行があるか(重複チェック用)
pub async fn check_team_guest_exists(
    pool: &PgPool,
    team_id: i32,
    user_id: i32,
    project_id: i32,
) -> anyhow::Result<bool> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM t_team_membership WHERE team_id = $1 AND user_id = $2 AND scoped_project_id = $3)"
    )
    .bind(team_id)
    .bind(user_id)
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

/// Projectゲストとして追加(role は member 固定)
pub async fn add_team_guest(
    pool: &PgPool,
    team_id: i32,
    user_id: i32,
    project_id: i32,
    end_date: Option<chrono::NaiveDate>,
) -> anyhow::Result<i32> {
    let membership_id: i32 = sqlx::query_scalar(
        "INSERT INTO t_team_membership (team_id, user_id, role, scoped_project_id, end_date, joined_at)
         VALUES ($1, $2, 'member', $3, $4, NOW())
         RETURNING id::int4"
    )
    .bind(team_id)
    .bind(user_id)
    .bind(project_id)
    .bind(end_date)
    .fetch_one(pool)
    .await?;

    Ok(membership_id)
}

/// Projectゲストの解除(scoped_project_id IS NOT NULL の行のみ対象。通常メンバー行は誤って消さない)
pub async fn remove_team_guest(pool: &PgPool, team_id: i32, membership_id: i32) -> anyhow::Result<bool> {
    let rows_affected = sqlx::query(
        "DELETE FROM t_team_membership WHERE id = $1 AND team_id = $2 AND scoped_project_id IS NOT NULL"
    )
    .bind(membership_id)
    .bind(team_id)
    .execute(pool)
    .await?
    .rows_affected();

    Ok(rows_affected > 0)
}
