/// infrastructure/repositories/resource_repo.rs — リソース API 永続化
///
/// Projects, Categories, Milestones, Labels の CRUD 操作。
/// Phase 3: Django API との互換性を重視した実装。

use sqlx::{PgPool, Row};

use crate::domain::models::resource_api::*;

// =============================================================================
// Projects
// =============================================================================

pub async fn find_all_projects(pool: &PgPool, page: i64) -> anyhow::Result<Vec<ProjectOut>> {
    const PAGE_SIZE: i64 = 50;
    let page = page.max(1);
    let offset = (page - 1) * PAGE_SIZE;

    let rows = sqlx::query(
        "SELECT
            p.id::int4, p.name, p.prefix, p.description, p.created_at,
            p.owner_team_id::int4,
            (SELECT COUNT(*)::int8 FROM tickets_ticket WHERE project_id = p.id) as ticket_count,
            (SELECT COUNT(*)::int8 FROM tickets_project_membership WHERE project_id = p.id) as member_count,
            t.id::int4 as team_id, t.name as team_name, t.slug as team_slug, t.icon as team_icon, t.color as team_color
         FROM tickets_project p
         LEFT JOIN m_team t ON p.owner_team_id = t.id
         ORDER BY p.name ASC
         LIMIT $1 OFFSET $2"
    )
    .bind(PAGE_SIZE)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    let projects = rows
        .into_iter()
        .map(|row| {
            let owner_team_id: Option<i32> = row.get(5);
            let owner_team = owner_team_id.and_then(|_| {
                Some(TeamSummaryOut {
                    id: row.get(8),
                    name: row.get(9),
                    slug: row.get(10),
                    icon: row.get(11),
                    color: row.get(12),
                })
            });

            ProjectOut {
                id: row.get(0),
                name: row.get(1),
                prefix: row.get(2),
                description: row.get(3),
                ticket_count: row.get(6),
                member_count: row.get(7),
                owner_team,
                created_at: row.get(4),
            }
        })
        .collect();

    Ok(projects)
}

pub async fn count_projects(pool: &PgPool) -> anyhow::Result<i64> {
    let row = sqlx::query("SELECT COUNT(*) as count FROM tickets_project")
        .fetch_one(pool)
        .await?;
    let count: i64 = row.get(0);
    Ok(count)
}

pub async fn find_project_by_id(pool: &PgPool, id: i32) -> anyhow::Result<Option<ProjectOut>> {
    let row_opt = sqlx::query(
        "SELECT
            p.id::int4, p.name, p.prefix, p.description, p.created_at,
            p.owner_team_id::int4,
            (SELECT COUNT(*)::int8 FROM tickets_ticket WHERE project_id = p.id) as ticket_count,
            (SELECT COUNT(*)::int8 FROM tickets_project_membership WHERE project_id = p.id) as member_count,
            t.id::int4 as team_id, t.name as team_name, t.slug as team_slug, t.icon as team_icon, t.color as team_color
         FROM tickets_project p
         LEFT JOIN m_team t ON p.owner_team_id = t.id
         WHERE p.id = $1"
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    let project = row_opt.map(|row| {
        let owner_team_id: Option<i32> = row.get(5);
        let owner_team = owner_team_id.and_then(|_| {
            Some(TeamSummaryOut {
                id: row.get(8),
                name: row.get(9),
                slug: row.get(10),
                icon: row.get(11),
                color: row.get(12),
            })
        });

        ProjectOut {
            id: row.get(0),
            name: row.get(1),
            prefix: row.get(2),
            description: row.get(3),
            ticket_count: row.get(6),
            member_count: row.get(7),
            owner_team,
            created_at: row.get(4),
        }
    });

    Ok(project)
}

pub async fn create_project(pool: &PgPool, input: &ProjectWriteIn) -> anyhow::Result<i32> {
    // トランザクション開始
    let mut tx = pool.begin().await?;

    // プロジェクトを作成
    // grace_period_days はDjangoの ProjectCreateSerializer に含まれず、モデルのdefault=7が
    // 常に使われる(APIから変更不可)。Rust側も同じ既定値7を使う(0だと猶予なしになりDjangoと乖離する)。
    let project_id: i32 = sqlx::query_scalar(
        "INSERT INTO tickets_project (name, prefix, description, created_at, grace_period_days, status, owner_team_id)
         VALUES ($1, $2, $3, NOW(), 7, 'active', $4)
         RETURNING id::int4"
    )
    .bind(&input.name)
    .bind(&input.prefix)
    .bind(&input.description)
    .bind(input.owner_team)
    .fetch_one(&mut *tx)
    .await?;

    // デフォルトワークフロー状態を作成
    let workflow_statuses = vec![
        ("backlog", "Backlog", "backlog", "#666666", 0, false),
        ("open", "Todo", "unstarted", "#a0a0a0", 1, true),
        ("in_progress", "In Progress", "started", "#f5a623", 2, false),
        ("resolved", "Done", "completed", "#50e3c2", 3, false),
        ("closed", "Closed", "completed", "#5c6cff", 4, false),
        ("canceled", "Cancelled", "cancelled", "#ff4d4f", 5, false),
    ];

    for (slug, name, category, color, position, is_default) in workflow_statuses {
        let _ = sqlx::query(
            "INSERT INTO t_workflow_status (slug, name, category, color, position, is_default, project_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (project_id, slug) DO NOTHING"
        )
        .bind(slug)
        .bind(name)
        .bind(category)
        .bind(color)
        .bind(position as i32)
        .bind(is_default)
        .bind(project_id)
        .execute(&mut *tx)
        .await;
    }

    tx.commit().await?;
    Ok(project_id)
}

pub async fn update_project(pool: &PgPool, id: i32, input: &ProjectWriteIn) -> anyhow::Result<bool> {
    let rows_affected = sqlx::query(
        "UPDATE tickets_project
         SET name = $1, prefix = $2, description = $3, owner_team_id = $4
         WHERE id = $5"
    )
    .bind(&input.name)
    .bind(&input.prefix)
    .bind(&input.description)
    .bind(input.owner_team)
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected();

    Ok(rows_affected > 0)
}

pub enum DeleteProjectResult {
    Deleted,
    NotFound,
    HasTickets,
}

/// プロジェクト削除。
///
/// DjangoのFK制約はDB上NO ACTIONで、実際のcascade/SET_NULLはDjango ORMの
/// アプリケーション層collectorが担っている(DB自体には自動cascadeが無い)。
/// そのためRust側も同じ削除順序を手動で再現する必要がある。
/// tickets_ticket.project は on_delete=PROTECT のため、チケットが1件でも
/// 存在する場合は削除不可(Djangoと同じ挙動)。
pub async fn delete_project(pool: &PgPool, id: i32) -> anyhow::Result<DeleteProjectResult> {
    let mut tx = pool.begin().await?;

    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM tickets_project WHERE id = $1)")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    if !exists {
        return Ok(DeleteProjectResult::NotFound);
    }

    let ticket_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tickets_ticket WHERE project_id = $1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    if ticket_count > 0 {
        return Ok(DeleteProjectResult::HasTickets);
    }

    // wiki_page とその子孫(on_delete=CASCADE相当)
    sqlx::query("DELETE FROM wiki_page_linked_tickets WHERE wikipage_id IN (SELECT id FROM wiki_page WHERE project_id = $1)")
        .bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM notifications_notification WHERE wiki_page_id IN (SELECT id FROM wiki_page WHERE project_id = $1)")
        .bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM notifications_user_read_state WHERE wiki_page_id IN (SELECT id FROM wiki_page WHERE project_id = $1)")
        .bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM wiki_revision WHERE page_id IN (SELECT id FROM wiki_page WHERE project_id = $1)")
        .bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM wiki_page WHERE project_id = $1")
        .bind(id).execute(&mut *tx).await?;

    // m_label とその子孫(M2M中間テーブル)
    sqlx::query("DELETE FROM tickets_ticket_labels WHERE labelmodel_id IN (SELECT id FROM m_label WHERE project_id = $1)")
        .bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM m_label WHERE project_id = $1")
        .bind(id).execute(&mut *tx).await?;

    // t_dashboard とその子孫
    sqlx::query("DELETE FROM t_dashboard_widget WHERE dashboard_id IN (SELECT id FROM t_dashboard WHERE project_id = $1)")
        .bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM t_dashboard WHERE project_id = $1")
        .bind(id).execute(&mut *tx).await?;

    // t_git_integration とその子孫
    sqlx::query("DELETE FROM t_git_event WHERE integration_id IN (SELECT id FROM t_git_integration WHERE project_id = $1)")
        .bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM t_git_integration WHERE project_id = $1")
        .bind(id).execute(&mut *tx).await?;

    // その他直接の子(on_delete=CASCADE)
    sqlx::query("DELETE FROM tickets_project_membership WHERE project_id = $1")
        .bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM t_cycle WHERE project_id = $1")
        .bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM t_workflow_status WHERE project_id = $1")
        .bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM milestones_milestone WHERE project_id = $1")
        .bind(id).execute(&mut *tx).await?;

    // on_delete=SET_NULL
    sqlx::query("UPDATE t_triage_request SET project_id = NULL WHERE project_id = $1")
        .bind(id).execute(&mut *tx).await?;

    sqlx::query("DELETE FROM tickets_project WHERE id = $1")
        .bind(id).execute(&mut *tx).await?;

    tx.commit().await?;
    Ok(DeleteProjectResult::Deleted)
}

// =============================================================================
// Categories
// =============================================================================

pub async fn find_all_categories(pool: &PgPool) -> anyhow::Result<Vec<CategoryOut>> {
    let rows = sqlx::query(
        "SELECT id::int4, name, slug, level, sort_order, color, parent_id::int4
         FROM tickets_category
         ORDER BY id ASC"
    )
    .fetch_all(pool)
    .await?;

    let categories = rows
        .into_iter()
        .map(|row| CategoryOut {
            id: row.get(0),
            name: row.get(1),
            slug: row.get(2),
            level: row.get(3),
            parent: row.get(6),
            sort_order: row.get(4),
            color: row.get(5),
        })
        .collect();

    Ok(categories)
}

pub async fn find_category_by_id(pool: &PgPool, id: i32) -> anyhow::Result<Option<CategoryOut>> {
    let row_opt = sqlx::query(
        "SELECT id::int4, name, slug, level, sort_order, color, parent_id::int4
         FROM tickets_category
         WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    let category = row_opt.map(|row| CategoryOut {
        id: row.get(0),
        name: row.get(1),
        slug: row.get(2),
        level: row.get(3),
        parent: row.get(6),
        sort_order: row.get(4),
        color: row.get(5),
    });

    Ok(category)
}

/// Simple slug generation: replace non-alphanumeric with '-', collapse consecutive '-', trim
/// Djangoの `django.utils.text.slugify(value, allow_unicode=True)`相当。
/// Unicode文字(日本語含む)の英数字はそのまま保持し、それ以外の区切り文字を
/// ハイフンに正規化する。ASCII限定でストリップすると日本語名がすべて
/// 空文字列(→フォールバックの"category"連番)になってしまうため注意。
fn generate_slug(input: &str) -> String {
    let lower = input.to_lowercase();
    let mut result = String::new();
    for c in lower.chars() {
        if c.is_alphanumeric() || c == '_' {
            result.push(c);
        } else if !result.ends_with('-') {
            result.push('-');
        }
    }
    let slug = result.trim_matches(|c| c == '-' || c == '_').to_string();

    if slug.is_empty() {
        "category".to_string()
    } else {
        slug
    }
}

pub async fn create_category(pool: &PgPool, input: &CategoryWriteIn) -> anyhow::Result<i32> {
    // Slug 生成（入力が空ならname から自動生成）
    let mut slug = if input.slug.is_empty() {
        generate_slug(&input.name)
    } else {
        input.slug.clone()
    };

    // 重複チェック＆連番付与
    let mut counter = 1;
    loop {
        let existing_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM tickets_category WHERE slug = $1"
        )
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

    let category_id: i32 = sqlx::query_scalar(
        "INSERT INTO tickets_category (name, slug, level, parent_id, sort_order, color)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id::int4"
    )
    .bind(&input.name)
    .bind(&slug)
    .bind(input.level)
    .bind(input.parent)
    .bind(input.sort_order)
    .bind(&input.color)
    .fetch_one(pool)
    .await?;

    Ok(category_id)
}

pub async fn update_category(pool: &PgPool, id: i32, input: &CategoryWriteIn) -> anyhow::Result<bool> {
    // Slug 生成
    let slug = if input.slug.is_empty() {
        generate_slug(&input.name)
    } else {
        input.slug.clone()
    };

    let rows_affected = sqlx::query(
        "UPDATE tickets_category
         SET name = $1, slug = $2, level = $3, parent_id = $4, sort_order = $5, color = $6
         WHERE id = $7"
    )
    .bind(&input.name)
    .bind(&slug)
    .bind(input.level)
    .bind(input.parent)
    .bind(input.sort_order)
    .bind(&input.color)
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected();

    Ok(rows_affected > 0)
}

pub async fn delete_category(pool: &PgPool, id: i32) -> anyhow::Result<bool> {
    let mut tx = pool.begin().await?;

    // tickets_ticket.category は on_delete=SET_NULL
    sqlx::query("UPDATE tickets_ticket SET category_id = NULL WHERE category_id = $1")
        .bind(id).execute(&mut *tx).await?;

    let rows_affected = sqlx::query("DELETE FROM tickets_category WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();

    tx.commit().await?;
    Ok(rows_affected > 0)
}

// =============================================================================
// Milestones
// =============================================================================

pub async fn find_all_milestones(pool: &PgPool, project_id: Option<i32>, page: i64) -> anyhow::Result<Vec<MilestoneOut>> {
    const PAGE_SIZE: i64 = 50;
    let page = page.max(1);
    let offset = (page - 1) * PAGE_SIZE;

    let rows = sqlx::query(
        "SELECT
            m.id::int4, m.name, m.due_date, m.description, m.created_at,
            m.project_id::int4,
            (SELECT COUNT(*)::int8 FROM tickets_ticket WHERE milestone_id = m.id AND status != 'closed') as open_count,
            (SELECT COUNT(*)::int8 FROM tickets_ticket WHERE milestone_id = m.id AND status = 'closed') as closed_count
         FROM milestones_milestone m
         WHERE ($1::int4 IS NULL OR m.project_id = $1::int4)
         ORDER BY m.due_date ASC NULLS LAST
         LIMIT $2 OFFSET $3"
    )
    .bind(project_id)
    .bind(PAGE_SIZE)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    let milestones = rows
        .into_iter()
        .map(|row| MilestoneOut {
            id: row.get(0),
            name: row.get(1),
            due_date: row.get(2),
            description: row.get(3),
            project: row.get(5),
            open_ticket_count: row.get(6),
            closed_ticket_count: row.get(7),
            created_at: row.get(4),
        })
        .collect();

    Ok(milestones)
}

pub async fn count_milestones(pool: &PgPool, project_id: Option<i32>) -> anyhow::Result<i64> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM milestones_milestone WHERE ($1::int4 IS NULL OR project_id = $1::int4)"
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?;

    Ok(count)
}

pub async fn find_milestone_by_id(pool: &PgPool, id: i32) -> anyhow::Result<Option<MilestoneOut>> {
    let row_opt = sqlx::query(
        "SELECT
            m.id::int4, m.name, m.due_date, m.description, m.created_at,
            m.project_id::int4,
            (SELECT COUNT(*)::int8 FROM tickets_ticket WHERE milestone_id = m.id AND status != 'closed') as open_count,
            (SELECT COUNT(*)::int8 FROM tickets_ticket WHERE milestone_id = m.id AND status = 'closed') as closed_count
         FROM milestones_milestone m
         WHERE m.id = $1"
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    let milestone = row_opt.map(|row| MilestoneOut {
        id: row.get(0),
        name: row.get(1),
        due_date: row.get(2),
        description: row.get(3),
        project: row.get(5),
        open_ticket_count: row.get(6),
        closed_ticket_count: row.get(7),
        created_at: row.get(4),
    });

    Ok(milestone)
}

pub async fn create_milestone(pool: &PgPool, input: &MilestoneWriteIn) -> anyhow::Result<i32> {
    let milestone_id: i32 = sqlx::query_scalar(
        "INSERT INTO milestones_milestone (name, due_date, description, created_at, project_id)
         VALUES ($1, $2, $3, NOW(), $4)
         RETURNING id::int4"
    )
    .bind(&input.name)
    .bind(input.due_date)
    .bind(&input.description)
    .bind(input.project)
    .fetch_one(pool)
    .await?;

    Ok(milestone_id)
}

pub async fn update_milestone(pool: &PgPool, id: i32, input: &MilestoneWriteIn) -> anyhow::Result<bool> {
    let rows_affected = sqlx::query(
        "UPDATE milestones_milestone
         SET name = $1, due_date = $2, description = $3, project_id = $4
         WHERE id = $5"
    )
    .bind(&input.name)
    .bind(input.due_date)
    .bind(&input.description)
    .bind(input.project)
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected();

    Ok(rows_affected > 0)
}

pub async fn delete_milestone(pool: &PgPool, id: i32) -> anyhow::Result<bool> {
    let mut tx = pool.begin().await?;

    // tickets_ticket.milestone は on_delete=SET_NULL
    sqlx::query("UPDATE tickets_ticket SET milestone_id = NULL WHERE milestone_id = $1")
        .bind(id).execute(&mut *tx).await?;

    let rows_affected = sqlx::query("DELETE FROM milestones_milestone WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();

    tx.commit().await?;
    Ok(rows_affected > 0)
}

// =============================================================================
// Labels
// =============================================================================

pub async fn find_all_labels(pool: &PgPool, project_id: Option<i32>) -> anyhow::Result<Vec<LabelOut>> {
    let rows = sqlx::query(
        "SELECT id::int4, name, color, created_at, project_id::int4, description, category, is_ai_enabled
         FROM m_label
         WHERE ($1::int4 IS NULL OR project_id = $1::int4)
         ORDER BY created_at DESC
         LIMIT 100"
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;

    let labels = rows
        .into_iter()
        .map(|row| LabelOut {
            id: row.get(0),
            name: row.get(1),
            color: row.get(2),
            created_at: row.get(3),
            project: row.get(4),
            description: row.get(5),
            category: row.get(6),
            is_ai_enabled: row.get(7),
        })
        .collect();

    Ok(labels)
}

pub async fn find_label_by_id(pool: &PgPool, id: i32) -> anyhow::Result<Option<LabelOut>> {
    let row_opt = sqlx::query(
        "SELECT id::int4, name, color, created_at, project_id::int4, description, category, is_ai_enabled
         FROM m_label
         WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    let label = row_opt.map(|row| LabelOut {
        id: row.get(0),
        name: row.get(1),
        color: row.get(2),
        created_at: row.get(3),
        project: row.get(4),
        description: row.get(5),
        category: row.get(6),
        is_ai_enabled: row.get(7),
    });

    Ok(label)
}

/// プロジェクト内で名前一致するラベルを探し、無ければ作成してIDを返す。
/// AI経由のチケット作成など、呼び出し側がラベルIDではなく名前しか持たない場合に使う。
pub async fn find_or_create_label(pool: &PgPool, project_id: i32, name: &str) -> anyhow::Result<i32> {
    if let Some(id) = sqlx::query_scalar::<_, i32>(
        "SELECT id::int4 FROM m_label WHERE project_id = $1 AND name = $2"
    )
    .bind(project_id)
    .bind(name)
    .fetch_optional(pool)
    .await?
    {
        return Ok(id);
    }

    // 既定色: AIが指定しなかった場合の汎用グレー
    const DEFAULT_LABEL_COLOR: &str = "#6B7280";
    let label_id: i32 = sqlx::query_scalar(
        "INSERT INTO m_label (name, color, created_at, project_id, description, category, is_ai_enabled)
         VALUES ($1, $2, NOW(), $3, NULL, NULL, false)
         RETURNING id::int4"
    )
    .bind(name)
    .bind(DEFAULT_LABEL_COLOR)
    .bind(project_id)
    .fetch_one(pool)
    .await?;

    Ok(label_id)
}

pub async fn create_label(pool: &PgPool, input: &LabelWriteIn) -> anyhow::Result<i32> {
    let label_id: i32 = sqlx::query_scalar(
        "INSERT INTO m_label (name, color, created_at, project_id, description, category, is_ai_enabled)
         VALUES ($1, $2, NOW(), $3, $4, $5, $6)
         RETURNING id::int4"
    )
    .bind(&input.name)
    .bind(&input.color)
    .bind(input.project)
    .bind(&input.description)
    .bind(&input.category)
    .bind(input.is_ai_enabled)
    .fetch_one(pool)
    .await?;

    Ok(label_id)
}

pub async fn update_label(pool: &PgPool, id: i32, input: &LabelWriteIn) -> anyhow::Result<bool> {
    let rows_affected = sqlx::query(
        "UPDATE m_label
         SET name = $1, color = $2, project_id = $3, description = $4, category = $5, is_ai_enabled = $6
         WHERE id = $7"
    )
    .bind(&input.name)
    .bind(&input.color)
    .bind(input.project)
    .bind(&input.description)
    .bind(&input.category)
    .bind(input.is_ai_enabled)
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected();

    Ok(rows_affected > 0)
}

pub async fn delete_label(pool: &PgPool, id: i32) -> anyhow::Result<bool> {
    let mut tx = pool.begin().await?;

    // tickets_ticket_labels はM2M中間テーブル(DjangoのManyToManyField削除はjoin行を自動除去)
    sqlx::query("DELETE FROM tickets_ticket_labels WHERE labelmodel_id = $1")
        .bind(id).execute(&mut *tx).await?;

    let rows_affected = sqlx::query("DELETE FROM m_label WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();

    tx.commit().await?;
    Ok(rows_affected > 0)
}
