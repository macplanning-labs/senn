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
            (SELECT COUNT(*)::int8 FROM tickets_project_teams WHERE team_id = t.id) as project_count,
            t.prefix,
            t.archived_at
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
            archived_at: row.get(12),
            viewer_can_manage: false,
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
            (SELECT COUNT(*)::int8 FROM tickets_project_teams WHERE team_id = t.id) as project_count,
            t.prefix,
            t.archived_at
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
            archived_at: row.get(12),
            viewer_can_manage: false,
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

    let mut tx = pool.begin().await?;

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
    .fetch_one(&mut *tx)
    .await?;

    // デフォルトワークフローステータスを投入する。
    // project_id を指定せずチームレベルでチケットを作成した場合に参照するステータスが
    // 一件も無いと t_workflow_status のルックアップが必ず失敗するため(project 作成時の
    // resource_repo::create_project と同じ既定セット、team_id 版)。
    let workflow_statuses = [
        ("backlog", "Backlog", "backlog", "#666666", 0, false),
        ("open", "Todo", "unstarted", "#a0a0a0", 1, true),
        ("in_progress", "In Progress", "started", "#f5a623", 2, false),
        ("resolved", "Resolved", "completed", "#50e3c2", 3, false),
        ("closed", "Closed", "completed", "#5c6cff", 4, false),
        ("canceled", "Cancelled", "cancelled", "#ff4d4f", 5, false),
    ];

    for (status_slug, status_name, category, color, position, is_default) in workflow_statuses {
        sqlx::query(
            "INSERT INTO t_workflow_status (slug, name, category, color, position, is_default, team_id, project_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, NULL)
             ON CONFLICT (team_id, slug) WHERE team_id IS NOT NULL AND project_id IS NULL DO NOTHING"
        )
        .bind(status_slug)
        .bind(status_name)
        .bind(category)
        .bind(color)
        .bind(position as i32)
        .bind(is_default)
        .bind(team_id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

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

    // description 維持（空なら既存値を保つ）
    let description = if input.description.is_empty() {
        // 既存の description を使用するため、NULL を使う（SQL側で COALESCE）
        None
    } else {
        Some(input.description.as_str())
    };

    // icon 維持（空なら既存値を保つ）
    let icon = if input.icon.is_empty() {
        // 既存の icon を使用するため、NULL を使う（SQL側で COALESCE）
        None
    } else {
        Some(input.icon.as_str())
    };

    // color 維持（空なら既存値を保つ）
    let color = if input.color.is_empty() {
        // 既存の color を使用するため、NULL を使う（SQL側で COALESCE）
        None
    } else {
        Some(input.color.as_str())
    };

    let rows_affected = sqlx::query(
        "UPDATE m_team
         SET name = $1, slug = COALESCE($2::text, slug), description = COALESCE($3::text, description), icon = COALESCE($4::text, icon), color = COALESCE($5::text, color), slack_webhook_url = $6, is_active = $7, prefix = COALESCE($8, prefix)
         WHERE id = $9"
    )
    .bind(&input.name)
    .bind(slug)
    .bind(description)
    .bind(icon)
    .bind(color)
    .bind(input.slack_webhook_url.as_deref().unwrap_or(""))
    .bind(input.is_active)
    .bind(prefix.as_deref())
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected();

    Ok(rows_affected > 0)
}

/// チームに依存(サイクル/チケット/プロジェクト参加)が残っているため削除できない。
/// API では 409 とし、何が何件残っているかを利用者に伝える。
#[derive(Debug)]
pub struct TeamHasDependents {
    pub cycles: i64,
    pub tickets: i64,
    pub projects: i64,
}

impl std::fmt::Display for TeamHasDependents {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "team has dependents (cycles={}, tickets={}, projects={}); cannot delete",
            self.cycles, self.tickets, self.projects
        )
    }
}

impl std::error::Error for TeamHasDependents {}

impl TeamHasDependents {
    /// 利用者向けの理由(残っているものだけを件数つきで並べる)。
    pub fn user_message(&self) -> String {
        let mut parts = Vec::new();
        if self.projects > 0 {
            parts.push(format!("プロジェクト{}件", self.projects));
        }
        if self.tickets > 0 {
            parts.push(format!("チケット{}件", self.tickets));
        }
        if self.cycles > 0 {
            parts.push(format!("サイクル{}件", self.cycles));
        }
        let mut msg = format!("{}が残っているためチームを削除できません", parts.join("・"));
        if self.projects > 0 {
            msg.push_str("(プロジェクトを削除するか、別のチームに付け替えてください)");
        }
        msg
    }
}

pub async fn delete_team(pool: &PgPool, id: i32) -> anyhow::Result<bool> {
    let mut tx = pool.begin().await?;

    // 依存があるチームは削除しない（CASCADE で Cycle/Ticket を消さない: DEMO-000166）
    let cycle_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM t_cycle WHERE team_id = $1"
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    let ticket_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tickets_ticket WHERE team_id = $1"
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    // プロジェクトとチームは N:M（tickets_project_teams）。owner_team_id 列は
    // 20260914100001_project_team_nm で削除済みのため、参加関係のみを見る。
    let participate_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tickets_project_teams WHERE team_id = $1"
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if cycle_count > 0 || ticket_count > 0 || participate_count > 0 {
        return Err(TeamHasDependents {
            cycles: cycle_count,
            tickets: ticket_count,
            projects: participate_count,
        }
        .into());
    }

    // m_team_rule は on_delete=CASCADE、そのチームルールを参照するチケットの
    // linked_rules(M2M)も先に削除する必要がある
    sqlx::query(
        "DELETE FROM tickets_ticket_linked_rules WHERE teamrulemodel_id IN
         (SELECT id FROM m_team_rule WHERE team_id = $1)"
    ).bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM m_team_rule WHERE team_id = $1")
        .bind(id).execute(&mut *tx).await?;

    // t_workflow_status / m_label の team_id FK は ON DELETE SET NULL のため、放置すると
    // 「チーム専用」の行が「ワークスペース共通」の行に変わってしまう。その結果、
    //  - 共通のワークフロー/ラベルとして残り続ける
    //  - 2つ目のチームを削除する時、同じ slug/name が unique_wf_slug_workspace /
    //    unique_label_name_workspace に衝突して 500 になる
    // ため、チーム専用の行(project_id IS NULL)を先に削除する。
    // (project_id を持つ行は、プロジェクト側の設定なので触らない)
    sqlx::query("DELETE FROM t_workflow_status WHERE team_id = $1 AND project_id IS NULL")
        .bind(id).execute(&mut *tx).await?;
    sqlx::query(
        "DELETE FROM tickets_ticket_labels WHERE labelmodel_id IN
         (SELECT id FROM m_label WHERE team_id = $1 AND project_id IS NULL)"
    ).bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM m_label WHERE team_id = $1 AND project_id IS NULL")
        .bind(id).execute(&mut *tx).await?;

    // t_team_membership は on_delete=CASCADE
    sqlx::query("DELETE FROM t_team_membership WHERE team_id = $1")
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
/// ユーザーが project の参加チームのいずれかのメンバーか、
/// または project-scoped ゲストであるかをチェック。
pub async fn check_project_access(pool: &PgPool, project_id: i32, user_id: i32) -> anyhow::Result<bool> {
    let has_access: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1
            FROM tickets_project_teams pt
            JOIN t_team_membership tm ON tm.team_id = pt.team_id
            WHERE pt.project_id = $1
              AND tm.user_id = $2
              AND (
                tm.scoped_project_id IS NULL
                OR tm.scoped_project_id = $1
              )
        )"
    )
    .bind(project_id)
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    Ok(has_access)
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

/// 指定Projectがそのチームの参加Projectであることを確認する
pub async fn project_belongs_to_team(pool: &PgPool, project_id: i32, team_id: i32) -> anyhow::Result<bool> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM tickets_project_teams WHERE project_id = $1 AND team_id = $2)"
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;

    fn write_in(name: &str) -> TeamWriteIn {
        TeamWriteIn {
            name: name.to_string(),
            slug: String::new(),
            description: String::new(),
            icon: String::new(),
            color: String::new(),
            slack_webhook_url: None,
            is_active: true,
            prefix: None,
        }
    }

    /// create_team が project_id 無し(team_id スコープ)のデフォルトワークフローステータスを
    /// 6件投入することを検証する。project_id を指定せずチーム画面から直接チケットを作成した際
    /// に「このチームに存在しないステータスです」で必ず失敗していた不具合の再発防止。
    #[tokio::test]
    async fn create_team_seeds_default_workflow_statuses() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let name = format!("テストチーム-{}", test_support::unique_suffix());
        let team_id = create_team(&pool, &write_in(&name)).await.unwrap();

        let rows = sqlx::query(
            "SELECT slug, is_default FROM t_workflow_status
             WHERE team_id = $1 AND project_id IS NULL
             ORDER BY position"
        )
        .bind(team_id)
        .fetch_all(&pool)
        .await
        .unwrap();

        assert_eq!(rows.len(), 6, "デフォルトの6ステータスが投入されていること");

        let slugs: Vec<String> = rows.iter().map(|r| r.get::<String, _>(0)).collect();
        assert_eq!(
            slugs,
            vec!["backlog", "open", "in_progress", "resolved", "closed", "canceled"]
        );

        let default_count = rows.iter().filter(|r| r.get::<bool, _>(1)).count();
        assert_eq!(default_count, 1, "is_default な行は1件(open)のみであること");
    }

    #[tokio::test]
    async fn delete_team_succeeds_without_dependents() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let name = format!("削除可-{}", test_support::unique_suffix());
        let team_id = create_team(&pool, &write_in(&name)).await.unwrap();
        let ok = delete_team(&pool, team_id).await.unwrap();
        assert!(ok);
        assert!(find_team_by_id(&pool, team_id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn delete_team_rejects_when_cycle_exists() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let user_id = test_support::create_test_user(&pool, "tdel").await;
        let name = format!("削除不可-{}", test_support::unique_suffix());
        let team_id = create_team(&pool, &write_in(&name)).await.unwrap();
        let today = chrono::Utc::now().date_naive();
        use crate::domain::models::cycle_api::CycleWriteIn;
        use crate::infrastructure::repositories::cycle_repo::create_cycle;
        create_cycle(
            &pool,
            &CycleWriteIn {
                project: None,
                name: "blocking cycle".to_string(),
                description: String::new(),
                start_date: today,
                end_date: today + chrono::Duration::days(7),
                status: "planned".to_string(),
                team_id: Some(team_id),
            },
            user_id,
        )
        .await
        .expect("create cycle");

        let err = delete_team(&pool, team_id).await.expect_err("should block");
        assert!(err.to_string().contains("team has dependents"));
        assert!(find_team_by_id(&pool, team_id).await.unwrap().is_some());
    }

    /// 回帰: 依存の無いチームでも、削除済みの owner_team_id 列を参照して常に失敗していた
    /// (20260914100001_project_team_nm 以降)。プロジェクトに参加しているチームは
    /// 「依存あり」として拒否(=API では 409)され、500 にならないこと。
    #[tokio::test]
    async fn delete_team_rejects_when_participating_in_project() {
        use crate::domain::models::resource_api::ProjectWriteIn;
        use crate::infrastructure::repositories::resource_repo::create_project;
        let Some(pool) = test_support::test_pool().await else { return; };
        let name = format!("参加中-{}", test_support::unique_suffix());
        let team_id = create_team(&pool, &write_in(&name)).await.unwrap();
        let prefix = format!("TD{}", &test_support::unique_suffix()[..6]).to_uppercase();
        create_project(
            &pool,
            &ProjectWriteIn {
                name: format!("proj-{prefix}"),
                prefix,
                description: String::new(),
                priority: "medium".to_string(),
                team_ids: vec![team_id],
            },
            None,
            None,
        )
        .await
        .expect("create project");

        let err = delete_team(&pool, team_id).await.expect_err("should block");
        assert!(err.to_string().contains("team has dependents"));
        let dep = err.downcast_ref::<TeamHasDependents>().expect("TeamHasDependents");
        assert_eq!((dep.projects, dep.tickets, dep.cycles), (1, 0, 0));
        assert!(find_team_by_id(&pool, team_id).await.unwrap().is_some());
    }

    #[test]
    fn team_has_dependents_message_lists_only_remaining_items_with_counts() {
        let only_projects = TeamHasDependents { cycles: 0, tickets: 0, projects: 2 };
        let m = only_projects.user_message();
        assert!(m.contains("プロジェクト2件"));
        assert!(!m.contains("チケット") && !m.contains("サイクル"));
        assert!(m.contains("付け替えてください"));

        let mixed = TeamHasDependents { cycles: 1, tickets: 3, projects: 0 };
        let m = mixed.user_message();
        assert!(m.contains("チケット3件・サイクル1件"));
        assert!(!m.contains("プロジェクト"));
    }

    /// 回帰: チームを削除すると、そのチームのワークフロー/ラベルが「ワークスペース共通」に
    /// 変わって残り、2つ目のチームの削除が一意制約違反(500)で失敗していた。
    /// 何度チームを削除しても成功し、共通行が増えないこと。
    #[tokio::test]
    async fn delete_team_twice_does_not_leave_workspace_level_rows() {
        let Some(pool) = test_support::test_pool().await else { return; };
        let workspace_rows = |table: &'static str| {
            let pool = pool.clone();
            async move {
                sqlx::query_scalar::<_, i64>(&format!(
                    "SELECT COUNT(*) FROM {table} WHERE team_id IS NULL AND project_id IS NULL"
                ))
                .fetch_one(&pool)
                .await
                .unwrap()
            }
        };
        let before_wf = workspace_rows("t_workflow_status").await;
        let before_label = workspace_rows("m_label").await;

        for i in 0..2 {
            let name = format!("連続削除{i}-{}", test_support::unique_suffix());
            let team_id = create_team(&pool, &write_in(&name)).await.unwrap();
            // 同じ名前のチームラベルを、どのチームにも持たせる(共通化されると衝突する)
            sqlx::query("INSERT INTO m_label (name, color, team_id, project_id, created_at) VALUES ('共通名ラベル', '#000000', $1, NULL, NOW())")
                .bind(team_id as i64)
                .execute(&pool)
                .await
                .unwrap();
            assert!(delete_team(&pool, team_id).await.unwrap(), "{i}回目の削除に失敗");
        }

        assert_eq!(workspace_rows("t_workflow_status").await, before_wf);
        assert_eq!(workspace_rows("m_label").await, before_label);
    }
}
