/// infrastructure/repositories/project_hierarchy_repo.rs — プロジェクトの親子関係・関連・ロードマップ
///
/// 設計（WIPAPPDEV-000066 Phase 3）:
/// - 親子: 自己参照カラム親_project_id。循環は DB トリガーが防止（最終防衛線）。
///        API 層は先に検査して、分かりやすい理由を 400/404/409 で返す。
/// - 関連: 対称。project_id < related_project_id に正規化して 1 行だけ持つ。
/// - ロードマップ: 全社共通。名前は大文字小文字を区別せず一意。

use serde::Serialize;
use sqlx::PgPool;
use std::error::Error;
use std::fmt;

// =============================================================================
// Error types
// =============================================================================

/// 親を設定するときの検査で引っかかったエラー。
#[derive(Debug)]
pub enum ProjectHierarchyError {
    /// 親 = 自分
    SelfAsParent,
    /// 新しい親が自分の子孫。引数はたどった経路（自分 → 親 → ... → 目標）
    Cycle(Vec<ProjectPath>),
    /// 階層が 5 を超える（新しい親の深さ + 1 + 自分の部分木の高さ > 5）。
    /// 引数は「現在の深さ」
    DepthExceeded(i64),
    /// 親が存在しない
    ParentNotFound,
}

impl fmt::Display for ProjectHierarchyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SelfAsParent => write!(f, "project cannot be its own parent"),
            Self::Cycle(path) => write!(f, "cycle: {}", path.iter().map(|p| p.id.to_string()).collect::<Vec<_>>().join(" <- ")),
            Self::DepthExceeded(depth) => write!(f, "depth exceeded: current {}", depth),
            Self::ParentNotFound => write!(f, "parent project not found"),
        }
    }
}

impl Error for ProjectHierarchyError {}

/// 祖先の経路の要素（id, prefix, name）。
#[derive(Debug, Clone, Serialize)]
pub struct ProjectPath {
    pub id: i32,
    pub prefix: String,
    pub name: String,
}

/// 親の設定・変更の候補。
#[derive(Debug, Clone, Serialize)]
pub struct ParentCandidate {
    pub id: i32,
    pub prefix: String,
    pub name: String,
}

/// 子プロジェクトの詳細（親の子一覧に表示）。
#[derive(Debug, Clone, Serialize)]
pub struct ChildProjectOut {
    pub id: i32,
    pub prefix: String,
    pub name: String,
    pub status: String,
    pub priority: String,
    #[serde(rename = "ownerId")]
    pub owner_id: Option<i32>,
    #[serde(rename = "ticketCount")]
    pub ticket_count: i64,
    #[serde(rename = "completedCount")]
    pub completed_count: i64,
    pub progress: Option<f64>,
    #[serde(rename = "childCount")]
    pub child_count: i64,
    pub teams: Vec<crate::domain::models::resource_api::ProjectTeamOut>,
}

/// プロジェクトの部分木の集計（自分を含む全子孫）。
#[derive(Debug, Clone, Serialize)]
pub struct RollupMetrics {
    #[serde(rename = "projectCount")]
    pub project_count: i64,
    #[serde(rename = "ticketCount")]
    pub ticket_count: i64,
    #[serde(rename = "completedCount")]
    pub completed_count: i64,
    pub progress: Option<f64>,
}

/// 関連プロジェクト（双方向で表示）。
#[derive(Debug, Clone, Serialize)]
pub struct RelatedProjectOut {
    pub id: i32,
    pub prefix: String,
    pub name: String,
    pub status: String,
    #[serde(rename = "ticketCount")]
    pub ticket_count: i64,
    pub progress: Option<f64>,
}

// =============================================================================
// 祖先・深さの計算
// =============================================================================

/// ルート（親が null）から直近の親までの祖先の一覧。ルート→直近の親の順。
pub async fn ancestors(pool: &PgPool, project_id: i32) -> anyhow::Result<Vec<ProjectPath>> {
    let mut conn = pool.acquire().await?;
    ancestors_conn(&mut conn, project_id).await
}

/// プロジェクトの深さ（ルート = 1）。
pub async fn depth(pool: &PgPool, project_id: i32) -> anyhow::Result<i64> {
    let mut conn = pool.acquire().await?;
    depth_conn(&mut conn, project_id).await
}

/// プロジェクトの部分木の高さ（自分が葉なら 0）。
pub async fn subtree_height(pool: &PgPool, project_id: i32) -> anyhow::Result<i64> {
    let mut conn = pool.acquire().await?;
    subtree_height_conn(&mut conn, project_id).await
}

// =============================================================================
// 親の設定
// =============================================================================

pub const MAX_PROJECT_DEPTH: i64 = 5;

/// 親を設定・変更する。親なしにすることも可（new_parent = None）。
/// 1トランザクション内で advisory lock を取り、自己/循環/深さ/存在を検査。
/// 通れば UPDATE。エラーは型付きエラー（API が downcast_ref で判別できる）。
pub async fn set_parent(pool: &PgPool, project_id: i32, new_parent: Option<i32>) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;

    // advisory lock を取る（DB のトリガーと同じ鍵。トランザクション内で直列化）
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('tickets_project_hierarchy'))")
        .execute(&mut *tx)
        .await?;

    // 親が None の場合は親を外すだけ
    if new_parent.is_none() {
        sqlx::query("UPDATE tickets_project SET parent_project_id = NULL WHERE id = $1")
            .bind(project_id as i64)
            .execute(&mut *tx)
            .await
            .map_err(|e| map_hierarchy_trigger_error(e))?;
        tx.commit().await?;
        return Ok(());
    }

    let new_parent = new_parent.unwrap();

    // 自己チェック
    if project_id == new_parent {
        return Err(ProjectHierarchyError::SelfAsParent.into());
    }

    // 親の存在チェック
    let parent_exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM tickets_project WHERE id = $1)")
            .bind(new_parent as i64)
            .fetch_one(&mut *tx)
            .await?;
    if !parent_exists {
        return Err(ProjectHierarchyError::ParentNotFound.into());
    }

    // 循環チェック: 新しい親の祖先に自分が含まれるかを Rust 側でチェック
    let parent_ancestors = ancestors_conn(&mut tx, new_parent).await?;
    if let Some(pos) = parent_ancestors.iter().position(|p| p.id == project_id) {
        // 循環の経路: 自分 → … → 新しい親(自分から見て、新しい親は子孫にあたる)
        let mut path: Vec<ProjectPath> = parent_ancestors.into_iter().skip(pos).collect();
        let parent_row: Option<(String, String)> = sqlx::query_as(
            "SELECT prefix, name FROM tickets_project WHERE id = $1",
        )
        .bind(new_parent as i64)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some((prefix, name)) = parent_row {
            path.push(ProjectPath { id: new_parent, prefix, name });
        }
        return Err(ProjectHierarchyError::Cycle(path).into());
    }

    // 深さチェック（新しい親の深さ + 1 + 自分の部分木の高さ > 5）
    let parent_depth = depth_conn(&mut tx, new_parent).await?;
    let my_height = subtree_height_conn(&mut tx, project_id).await?;
    if parent_depth + 1 + my_height > MAX_PROJECT_DEPTH {
        return Err(ProjectHierarchyError::DepthExceeded(parent_depth).into());
    }

    // UPDATE（トリガーで循環検出の可能性がある）
    sqlx::query("UPDATE tickets_project SET parent_project_id = $2 WHERE id = $1")
        .bind(project_id as i64)
        .bind(new_parent as i64)
        .execute(&mut *tx)
        .await
        .map_err(|e| map_hierarchy_trigger_error(e))?;

    tx.commit().await?;
    Ok(())
}

/// ancestors の実体(接続上で実行。set_parent はトランザクション上で呼ぶ)
async fn ancestors_conn(conn: &mut sqlx::PgConnection, project_id: i32) -> anyhow::Result<Vec<ProjectPath>> {
    let rows = sqlx::query_as::<_, (i32, String, String)>(
        "WITH RECURSIVE up(id, path) AS (
            SELECT parent_project_id::bigint, ARRAY[id::bigint, parent_project_id::bigint]
            FROM tickets_project
            WHERE id = $1 AND parent_project_id IS NOT NULL
            UNION ALL
            SELECT p.parent_project_id::bigint, up.path || p.parent_project_id::bigint
            FROM tickets_project p
            JOIN up ON p.id = up.id
            WHERE p.parent_project_id IS NOT NULL
              AND p.parent_project_id <> ALL (up.path[2:])
              AND array_length(up.path, 1) < 50
        )
        SELECT p.id::int4, p.prefix, p.name
        FROM up
        JOIN tickets_project p ON p.id = up.id
        ORDER BY array_length(up.path, 1) DESC",
    )
    .bind(project_id as i64)
    .fetch_all(&mut *conn)
    .await?;

    Ok(rows.into_iter().map(|(id, prefix, name)| ProjectPath { id, prefix, name }).collect())
}

/// depth の実体
async fn depth_conn(conn: &mut sqlx::PgConnection, project_id: i32) -> anyhow::Result<i64> {
    let ancs = ancestors_conn(conn, project_id).await?;
    Ok((ancs.len() + 1) as i64)
}

/// subtree_height の実体
async fn subtree_height_conn(conn: &mut sqlx::PgConnection, project_id: i32) -> anyhow::Result<i64> {
    let height: Option<i64> = sqlx::query_scalar(
        "WITH RECURSIVE down(id, level) AS (
            SELECT $1::bigint, 0
            UNION ALL
            SELECT p.id, down.level + 1
            FROM tickets_project p
            JOIN down ON p.parent_project_id = down.id
            WHERE down.level < 10
        )
        SELECT COALESCE(MAX(level), 0)::int8 FROM down",
    )
    .bind(project_id as i64)
    .fetch_one(&mut *conn)
    .await?;
    Ok(height.unwrap_or(0))
}

/// DB トリガーが投げた project_hierarchy_cycle エラーを ProjectHierarchyError::Cycle に変換
fn map_hierarchy_trigger_error(err: sqlx::Error) -> anyhow::Error {
    match err {
        sqlx::Error::Database(db_err) if db_err.message().contains("project_hierarchy_cycle") => {
            ProjectHierarchyError::Cycle(vec![]).into()
        }
        _ => err.into(),
    }
}

// =============================================================================
// 子プロジェクト
// =============================================================================

/// 直接の子プロジェクトの一覧。進捗（closed のみ完了、canceled は分母から除外、分母 0 は None）つき。
pub async fn children(pool: &PgPool, parent_id: i32) -> anyhow::Result<Vec<ChildProjectOut>> {
    let rows = sqlx::query_as::<_, (i32, String, String, String, String, Option<i32>, i64, i64, i64, i64, Option<serde_json::Value>)>(
        "SELECT
            p.id::int4,
            p.prefix,
            p.name,
            p.status,
            p.priority,
            p.owner_id::int4,
            (SELECT COUNT(*)::int8 FROM tickets_ticket WHERE project_id = p.id) as ticket_count,
            (SELECT COUNT(*)::int8 FROM tickets_ticket WHERE project_id = p.id AND status = 'closed') as completed_count,
            (SELECT COUNT(*)::int8 FROM tickets_project WHERE parent_project_id = p.id) as child_count,
            (SELECT COUNT(*)::int8 FROM tickets_ticket WHERE project_id = p.id AND status <> 'canceled') as active_count,
            COALESCE(
              (
                SELECT json_agg(json_build_object(
                  'id', t.id,
                  'name', t.name,
                  'slug', t.slug,
                  'icon', t.icon,
                  'color', t.color,
                  'archived', (t.archived_at IS NOT NULL)
                ) ORDER BY t.name)
                FROM tickets_project_teams pt
                JOIN m_team t ON t.id = pt.team_id
                WHERE pt.project_id = p.id
              ),
              '[]'::json
            ) as teams
         FROM tickets_project p
         WHERE p.parent_project_id = $1
         ORDER BY p.name ASC",
    )
    .bind(parent_id as i64)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, prefix, name, status, priority, owner_id, ticket_count, completed_count, child_count, active_count, teams_json)| {
            let teams: Vec<crate::domain::models::resource_api::ProjectTeamOut> = teams_json
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default();
            let progress = if active_count > 0 {
                Some(completed_count as f64 / active_count as f64)
            } else {
                None
            };

            ChildProjectOut {
                id,
                prefix,
                name,
                status,
                priority,
                owner_id,
                ticket_count,
                completed_count,
                progress,
                child_count,
                teams,
            }
        })
        .collect())
}

// =============================================================================
// Rollup（自分を含む全子孫）
// =============================================================================

/// プロジェクトとその全子孫の集計。進捗は closed のみ完了、canceled は分母から除外。
pub async fn rollup(pool: &PgPool, project_id: i32) -> anyhow::Result<RollupMetrics> {
    let (project_count, ticket_count, completed_count, active_count): (i64, i64, i64, i64) =
        sqlx::query_as(
            "WITH RECURSIVE tree(id) AS (
                SELECT $1::bigint
                UNION
                SELECT p.id
                FROM tickets_project p
                JOIN tree ON p.parent_project_id = tree.id
            )
            SELECT
                COUNT(DISTINCT tree.id)::int8 as project_count,
                COUNT(t.id)::int8 as ticket_count,
                COUNT(CASE WHEN t.status = 'closed' THEN 1 END)::int8 as completed_count,
                COUNT(CASE WHEN t.status <> 'canceled' THEN 1 END)::int8 as active_count
            FROM tree
            LEFT JOIN tickets_ticket t ON t.project_id = tree.id",
        )
        .bind(project_id as i64)
        .fetch_one(pool)
        .await?;

    let progress = if active_count > 0 {
        Some(completed_count as f64 / active_count as f64)
    } else {
        None
    };

    Ok(RollupMetrics {
        project_count,
        ticket_count,
        completed_count,
        progress,
    })
}

// =============================================================================
// 親の候補（自分・子孫・深さ超過・can_manage なし除外）
// =============================================================================

/// 変更権限の条件(SQL)。`project_team_repo::can_manage` と同じ意味
/// (システム管理者 / プロジェクトのオーナー / 参加チームの管理者)。
/// 一括で絞り込むために SQL で持つ。両者が食い違わないことはテストで固定している。
/// `$staff`(bool)と `$user`(bigint)のバインドが必要。`p` は tickets_project の別名。
const CAN_MANAGE_SQL: &str = "(
    $STAFF
    OR p.owner_id = $USER
    OR EXISTS (
        SELECT 1 FROM tickets_project_teams pt
        JOIN t_team_membership tm ON tm.team_id = pt.team_id
        WHERE pt.project_id = p.id
          AND tm.user_id = $USER
          AND tm.role = 'admin'
          AND tm.scoped_project_id IS NULL
    )
)";

fn can_manage_sql(staff_param: &str, user_param: &str) -> String {
    CAN_MANAGE_SQL.replace("$STAFF", staff_param).replace("$USER", user_param)
}

/// 親の候補: 自分・自分の子孫・深さ超過(候補の深さ > 5 - 1 - 自分の部分木の高さ)を除き、
/// 変更権限(can_manage)があるもの。1クエリで絞り込む。
pub async fn parent_candidates(pool: &PgPool, project_id: i32, user_id: i32, is_staff: bool) -> anyhow::Result<Vec<ParentCandidate>> {
    let my_height = subtree_height(pool, project_id).await?;
    let max_parent_depth = MAX_PROJECT_DEPTH - 1 - my_height;

    let sql = format!(
        "WITH RECURSIVE tree(id) AS (
            SELECT $1::bigint
            UNION
            SELECT c.id FROM tickets_project c JOIN tree ON c.parent_project_id = tree.id
        ),
        depths(id, depth) AS (
            SELECT id, 1 FROM tickets_project WHERE parent_project_id IS NULL
            UNION ALL
            SELECT c.id, d.depth + 1
            FROM tickets_project c JOIN depths d ON c.parent_project_id = d.id
            WHERE d.depth < 20
        )
        SELECT p.id::int4, p.prefix, p.name
        FROM tickets_project p
        JOIN depths d ON d.id = p.id
        WHERE p.id NOT IN (SELECT id FROM tree)
          AND d.depth <= $2
          AND {}
        ORDER BY p.name",
        can_manage_sql("$3::bool", "$4::bigint")
    );
    let rows = sqlx::query_as::<_, (i32, String, String)>(&sql)
        .bind(project_id as i64)
        .bind(max_parent_depth)
        .bind(is_staff)
        .bind(user_id as i64)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|(id, prefix, name)| ParentCandidate { id, prefix, name }).collect())
}

// =============================================================================
// 関連プロジェクト
// =============================================================================

/// 関連を追加する。既に関連なら冪等に成功を返す。自己は拒否。
pub async fn add_relation(pool: &PgPool, project_id: i32, related_id: i32) -> anyhow::Result<bool> {
    if project_id == related_id {
        anyhow::bail!("cannot relate a project to itself");
    }

    // project_id < related_id に正規化
    let (p1, p2) = if project_id < related_id {
        (project_id as i64, related_id as i64)
    } else {
        (related_id as i64, project_id as i64)
    };

    // 両プロジェクトが存在するか
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::int8 FROM tickets_project WHERE id IN ($1, $2)",
    )
    .bind(p1)
    .bind(p2)
    .fetch_one(pool)
    .await?;
    if count != 2 {
        anyhow::bail!("one or both projects not found");
    }

    // UPSERT（既に関連なら何もしない）
    let inserted = sqlx::query(
        "INSERT INTO project_relations (project_id, related_project_id, relation_type, created_by)
         VALUES ($1, $2, 'related', NULL)
         ON CONFLICT (project_id, related_project_id, relation_type) DO NOTHING",
    )
    .bind(p1)
    .bind(p2)
    .execute(pool)
    .await?
    .rows_affected();

    // 新しく関連が作られたら true。既に関連だった(冪等な再追加)なら false
    Ok(inserted > 0)
}

/// 関連を削除する。なければ false を返す。
pub async fn remove_relation(pool: &PgPool, project_id: i32, related_id: i32) -> anyhow::Result<bool> {
    let (p1, p2) = if project_id < related_id {
        (project_id as i64, related_id as i64)
    } else {
        (related_id as i64, project_id as i64)
    };

    let result = sqlx::query("DELETE FROM project_relations WHERE project_id = $1 AND related_project_id = $2 AND relation_type = 'related'")
        .bind(p1)
        .bind(p2)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

/// 関連プロジェクトの一覧（双方向）。
pub async fn relations(pool: &PgPool, project_id: i32) -> anyhow::Result<Vec<RelatedProjectOut>> {
    let rows = sqlx::query_as::<_, (i32, String, String, String, i64, i64, i64)>(
        "SELECT
            CASE
              WHEN project_id = $1 THEN related_project_id::int4
              ELSE project_id::int4
            END as id,
            p.prefix,
            p.name,
            p.status,
            (SELECT COUNT(*)::int8 FROM tickets_ticket WHERE project_id = p.id) as ticket_count,
            (SELECT COUNT(*)::int8 FROM tickets_ticket WHERE project_id = p.id AND status = 'closed') as completed_count,
            (SELECT COUNT(*)::int8 FROM tickets_ticket WHERE project_id = p.id AND status <> 'canceled') as active_count
         FROM project_relations pr
         JOIN tickets_project p ON (
           CASE
             WHEN pr.project_id = $1 THEN pr.related_project_id = p.id
             ELSE pr.project_id = p.id
           END
         )
         WHERE (pr.project_id = $1 OR pr.related_project_id = $1)
           AND pr.relation_type = 'related'
         ORDER BY p.name ASC",
    )
    .bind(project_id as i64)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, prefix, name, status, ticket_count, completed_count, active_count)| {
            let progress = if active_count > 0 {
                Some(completed_count as f64 / active_count as f64)
            } else {
                None
            };
            RelatedProjectOut {
                id,
                prefix,
                name,
                status,
                ticket_count,
                progress,
            }
        })
        .collect())
}

/// 関連の候補（自分・既に関連のものを除き、変更権限(can_manage)のあるもの）。
pub async fn relation_candidates(pool: &PgPool, project_id: i32, user_id: i32, is_staff: bool) -> anyhow::Result<Vec<ParentCandidate>> {
    let sql = format!(
        "SELECT p.id::int4, p.prefix, p.name
         FROM tickets_project p
         WHERE p.id <> $1
           AND NOT EXISTS (
                SELECT 1 FROM project_relations r
                WHERE r.relation_type = 'related'
                  AND ((r.project_id = $1 AND r.related_project_id = p.id)
                    OR (r.related_project_id = $1 AND r.project_id = p.id)))
           AND {}
         ORDER BY p.name",
        can_manage_sql("$2::bool", "$3::bigint")
    );
    let rows = sqlx::query_as::<_, (i32, String, String)>(&sql)
        .bind(project_id as i64)
        .bind(is_staff)
        .bind(user_id as i64)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|(id, prefix, name)| ParentCandidate { id, prefix, name }).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project_candidate(id: i32) -> ParentCandidate {
        ParentCandidate {
            id,
            prefix: format!("P{}", id),
            name: format!("Project {}", id),
        }
    }

    #[tokio::test]
    async fn test_set_parent_self_as_parent() {
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let user = crate::test_support::create_test_user(&pool, "hier").await;
        let project = crate::test_support::create_test_project(&pool, "HIER", user).await;

        let result = set_parent(&pool, project, Some(project)).await;
        assert!(result.is_err(), "Expected error, got: {:?}", result);
        let err = result.unwrap_err();
        let err_msg = format!("{:?}", err);
        eprintln!("Error message: {}", err_msg);
        if err.downcast_ref::<ProjectHierarchyError>().is_some() {
            // This should work
        } else {
            panic!("Expected ProjectHierarchyError but got: {}", err_msg);
        }
    }

    #[tokio::test]
    async fn test_set_parent_nonexistent() {
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let user = crate::test_support::create_test_user(&pool, "hier").await;
        let project = crate::test_support::create_test_project(&pool, "HIER", user).await;

        let result = set_parent(&pool, project, Some(999999)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_set_parent_none() {
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let user = crate::test_support::create_test_user(&pool, "hier").await;
        let parent = crate::test_support::create_test_project(&pool, "P", user).await;
        let child = crate::test_support::create_test_project(&pool, "C", user).await;

        // 子を親に設定
        set_parent(&pool, child, Some(parent)).await.unwrap();

        // 親を外す
        set_parent(&pool, child, None).await.unwrap();

        // 確認
        let row: Option<i32> = sqlx::query_scalar("SELECT parent_project_id::int4 FROM tickets_project WHERE id = $1")
            .bind(child as i64)
            .fetch_optional(&pool)
            .await
            .unwrap()
            .flatten();
        assert_eq!(row, None);
    }

    #[tokio::test]
    async fn test_set_parent_cycle_direct() {
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let user = crate::test_support::create_test_user(&pool, "hier").await;
        let a = crate::test_support::create_test_project(&pool, "A", user).await;
        let b = crate::test_support::create_test_project(&pool, "B", user).await;

        // A -> B
        set_parent(&pool, a, Some(b)).await.unwrap();

        // B -> A（循環）
        let result = set_parent(&pool, b, Some(a)).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err.downcast_ref::<ProjectHierarchyError>() {
            Some(ProjectHierarchyError::Cycle(path)) => {
                // 経路は 自分(B) → … → 新しい親(A)
                assert_eq!(path.first().map(|p| p.id), Some(b));
                assert_eq!(path.last().map(|p| p.id), Some(a));
            }
            other => panic!("Cycle を期待: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_set_parent_depth_exceeded() {
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let user = crate::test_support::create_test_user(&pool, "hier").await;

        // 深さ 5 の階層を作る
        let mut projects = vec![];
        for i in 0..5 {
            let p = crate::test_support::create_test_project(&pool, &format!("D{}", i), user).await;
            if i > 0 {
                set_parent(&pool, p, Some(projects[i - 1])).await.unwrap();
            }
            projects.push(p);
        }

        // 深さ 6 を試す
        let p6 = crate::test_support::create_test_project(&pool, "D6", user).await;
        let result = set_parent(&pool, p6, Some(projects[4])).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err.downcast_ref::<ProjectHierarchyError>(), Some(ProjectHierarchyError::DepthExceeded(_))),
            "DepthExceeded を期待: {err:?}"
        );
        // 失敗した親付けは反映されない
        let parent: Option<i64> = sqlx::query_scalar("SELECT parent_project_id FROM tickets_project WHERE id = $1")
            .bind(p6 as i64).fetch_one(&pool).await.unwrap();
        assert_eq!(parent, None);
    }

    #[tokio::test]
    async fn test_progress_calculation() {
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let user = crate::test_support::create_test_user(&pool, "hier").await;
        let project = crate::test_support::create_test_project(&pool, "PROG", user).await;

        // チケットなし
        let children_list = children(&pool, project).await.unwrap();
        assert!(children_list.is_empty());

        // 子プロジェクトを作成
        let child = crate::test_support::create_test_project(&pool, "CHILD", user).await;
        set_parent(&pool, child, Some(project)).await.unwrap();

        let children_list = children(&pool, project).await.unwrap();
        assert_eq!(children_list.len(), 1);
        assert_eq!(children_list[0].progress, None); // チケットなし
    }

    #[tokio::test]
    async fn test_rollup_metrics() {
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let user = crate::test_support::create_test_user(&pool, "hier").await;
        let project = crate::test_support::create_test_project(&pool, "ROLL", user).await;

        let rollup = rollup(&pool, project).await.unwrap();
        assert_eq!(rollup.project_count, 1);
    }

    #[tokio::test]
    async fn test_add_relation() {
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let user = crate::test_support::create_test_user(&pool, "hier").await;
        let p1 = crate::test_support::create_test_project(&pool, "R1", user).await;
        let p2 = crate::test_support::create_test_project(&pool, "R2", user).await;

        add_relation(&pool, p1, p2).await.unwrap();

        // 関連を確認
        let relations = relations(&pool, p1).await.unwrap();
        assert_eq!(relations.len(), 1);
        assert_eq!(relations[0].id, p2);
    }

    #[tokio::test]
    async fn test_add_relation_idempotent() {
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let user = crate::test_support::create_test_user(&pool, "hier").await;
        let p1 = crate::test_support::create_test_project(&pool, "R1", user).await;
        let p2 = crate::test_support::create_test_project(&pool, "R2", user).await;

        add_relation(&pool, p1, p2).await.unwrap();
        add_relation(&pool, p1, p2).await.unwrap(); // 2度目

        let relations = relations(&pool, p1).await.unwrap();
        assert_eq!(relations.len(), 1); // 1 つだけ
    }

    #[tokio::test]
    async fn test_add_relation_self() {
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let user = crate::test_support::create_test_user(&pool, "hier").await;
        let p = crate::test_support::create_test_project(&pool, "R", user).await;

        let result = add_relation(&pool, p, p).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_remove_relation() {
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let user = crate::test_support::create_test_user(&pool, "hier").await;
        let p1 = crate::test_support::create_test_project(&pool, "R1", user).await;
        let p2 = crate::test_support::create_test_project(&pool, "R2", user).await;

        add_relation(&pool, p1, p2).await.unwrap();
        let removed = remove_relation(&pool, p1, p2).await.unwrap();
        assert!(removed);

        let relations = relations(&pool, p1).await.unwrap();
        assert_eq!(relations.len(), 0);
    }

    #[tokio::test]
    async fn test_remove_relation_not_found() {
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let user = crate::test_support::create_test_user(&pool, "hier").await;
        let p1 = crate::test_support::create_test_project(&pool, "R1", user).await;
        let p2 = crate::test_support::create_test_project(&pool, "R2", user).await;

        let removed = remove_relation(&pool, p1, p2).await.unwrap();
        assert!(!removed);
    }

    #[tokio::test]
    async fn test_set_parent_max_depth_ok() {
        // 深さ 5 は成功する
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let user = crate::test_support::create_test_user(&pool, "hier").await;

        let mut projects = vec![];
        for i in 0..5 {
            let p = crate::test_support::create_test_project(&pool, &format!("D{}", i), user).await;
            if i > 0 {
                set_parent(&pool, p, Some(projects[i - 1])).await.unwrap();
            }
            projects.push(p);
        }

        let depth_5_id = projects[4];
        let ancestors = ancestors(&pool, depth_5_id).await.unwrap();
        assert_eq!(ancestors.len(), 4); // 4 の親たち
        assert_eq!(ancestors[0].id, projects[0]); // ルート
        assert_eq!(ancestors[3].id, projects[3]); // 直近の親
    }

    #[tokio::test]
    async fn test_ancestors_order() {
        // ancestors はルート→直近の親の順序
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let user = crate::test_support::create_test_user(&pool, "hier").await;

        let a = crate::test_support::create_test_project(&pool, "A", user).await;
        let b = crate::test_support::create_test_project(&pool, "B", user).await;
        let c = crate::test_support::create_test_project(&pool, "C", user).await;

        set_parent(&pool, b, Some(a)).await.unwrap();
        set_parent(&pool, c, Some(b)).await.unwrap();

        let anc = ancestors(&pool, c).await.unwrap();
        assert_eq!(anc.len(), 2);
        assert_eq!(anc[0].id, a, "最初の祖先はルート");
        assert_eq!(anc[1].id, b, "2番目の祖先は直近の親");
    }

    #[tokio::test]
    async fn test_indirect_cycle() {
        // 間接循環: A→B→C を作り、C←A（循環を検出）
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let user = crate::test_support::create_test_user(&pool, "hier").await;

        let a = crate::test_support::create_test_project(&pool, "A", user).await;
        let b = crate::test_support::create_test_project(&pool, "B", user).await;
        let c = crate::test_support::create_test_project(&pool, "C", user).await;

        set_parent(&pool, b, Some(a)).await.unwrap();
        set_parent(&pool, c, Some(b)).await.unwrap();

        // A の親を C にしようとする（循環）
        let result = set_parent(&pool, a, Some(c)).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err.downcast_ref::<ProjectHierarchyError>() {
            Some(ProjectHierarchyError::Cycle(path)) => {
                assert_eq!(path.first().map(|p| p.id), Some(a));
                assert_eq!(path.last().map(|p| p.id), Some(c));
            }
            other => panic!("Cycle を期待: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_set_parent_depth_exceeded_with_subtree() {
        // 部分木を持つプロジェクトを深い位置に付け替えると深さ超過の可能性
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let user = crate::test_support::create_test_user(&pool, "hier").await;

        // 深さ 3 の階層を作る: L0 -> L1 -> L2
        let l0 = crate::test_support::create_test_project(&pool, "L0", user).await;
        let l1 = crate::test_support::create_test_project(&pool, "L1", user).await;
        let l2 = crate::test_support::create_test_project(&pool, "L2", user).await;
        set_parent(&pool, l1, Some(l0)).await.unwrap();
        set_parent(&pool, l2, Some(l1)).await.unwrap();

        // L1 の下に子を作る（subtree_height = 2）
        let l1_child = crate::test_support::create_test_project(&pool, "L1C", user).await;
        set_parent(&pool, l1_child, Some(l1)).await.unwrap();

        // 深さ 2 の別の階層を作る: R0 -> R1
        let r0 = crate::test_support::create_test_project(&pool, "R0", user).await;
        let r1 = crate::test_support::create_test_project(&pool, "R1", user).await;
        set_parent(&pool, r1, Some(r0)).await.unwrap();

        // R1（深さ 1） に L1（subtree_height 2）を付けようとする
        // 新しい深さ = 1 + 1 + 2 = 4（OK）
        let result = set_parent(&pool, l1, Some(r1)).await;
        assert!(result.is_ok(), "深さ 4 は OK");
    }

    #[tokio::test]
    async fn test_children_with_progress() {
        // children が進捗を返す
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let user = crate::test_support::create_test_user(&pool, "hier").await;

        let parent = crate::test_support::create_test_project(&pool, "PARENT", user).await;
        let child1 = crate::test_support::create_test_project(&pool, "CHILD1", user).await;
        let child2 = crate::test_support::create_test_project(&pool, "CHILD2", user).await;

        set_parent(&pool, child1, Some(parent)).await.unwrap();
        set_parent(&pool, child2, Some(parent)).await.unwrap();

        let children_list = children(&pool, parent).await.unwrap();
        assert_eq!(children_list.len(), 2);
        // チケットなしなので progress は None
        assert_eq!(children_list[0].progress, None);
        assert_eq!(children_list[1].progress, None);
    }

    #[tokio::test]
    async fn test_rollup_metrics_empty() {
        // rollup は自分を含む全子孫の集計を返す
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let user = crate::test_support::create_test_user(&pool, "hier").await;

        let project = crate::test_support::create_test_project(&pool, "ROLLUP", user).await;

        let rollup = rollup(&pool, project).await.unwrap();
        assert_eq!(rollup.project_count, 1); // 自分だけ
        // progress は closed のみ完了と数える
    }

    // ---- 候補・権限・削除・一覧の絞り込み(実DB) ----

    async fn add_member(pool: &PgPool, team_id: i32, user_id: i32, role: &str) {
        sqlx::query("INSERT INTO t_team_membership (team_id, user_id, role, joined_at) VALUES ($1, $2, $3, NOW())")
            .bind(team_id as i64).bind(user_id as i64).bind(role)
            .execute(pool).await.unwrap();
    }

    async fn team_of(pool: &PgPool, project_id: i32) -> i32 {
        sqlx::query_scalar::<_, i32>("SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1")
            .bind(project_id as i64).fetch_one(pool).await.unwrap()
    }

    async fn set_owner(pool: &PgPool, project_id: i32, user_id: i32) {
        sqlx::query("UPDATE tickets_project SET owner_id = $2 WHERE id = $1")
            .bind(project_id as i64).bind(user_id as i64).execute(pool).await.unwrap();
    }

    fn has(v: &[ParentCandidate], id: i32) -> bool {
        v.iter().any(|c| c.id == id)
    }

    /// 候補は、`project_team_repo::can_manage` と同じ判定になる(SQL 側の複製が食い違わないことの固定)。
    #[tokio::test]
    async fn candidates_match_can_manage_for_every_kind_of_user() {
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let owner = crate::test_support::create_test_user(&pool, "cm-own").await;
        let admin = crate::test_support::create_test_user(&pool, "cm-adm").await;
        let member = crate::test_support::create_test_user(&pool, "cm-mem").await;
        let outsider = crate::test_support::create_test_user(&pool, "cm-out").await;

        let subject = crate::test_support::create_test_project(&pool, "CS", owner).await;
        let owned = crate::test_support::create_test_project(&pool, "CO", owner).await;
        set_owner(&pool, owned, owner).await;
        let teamed = crate::test_support::create_test_project(&pool, "CT", owner).await;
        let team = team_of(&pool, teamed).await;
        add_member(&pool, team, admin, "admin").await;
        add_member(&pool, team, member, "member").await;
        let plain = crate::test_support::create_test_project(&pool, "CP", owner).await;

        // (ユーザー, システム管理者か)
        let viewers = [(owner, false), (admin, false), (member, false), (outsider, false), (outsider, true)];
        for (user, staff) in viewers {
            let parents = parent_candidates(&pool, subject, user, staff).await.unwrap();
            let related = relation_candidates(&pool, subject, user, staff).await.unwrap();
            for target in [owned, teamed, plain] {
                let allowed = crate::infrastructure::repositories::project_team_repo::can_manage(&pool, target, user, staff)
                    .await.unwrap();
                assert_eq!(has(&parents, target), allowed, "親候補 user={user} staff={staff} target={target}");
                assert_eq!(has(&related, target), allowed, "関連候補 user={user} staff={staff} target={target}");
            }
            assert!(!has(&parents, subject) && !has(&related, subject), "自分は候補に出ない");
        }
        // 具体的な期待(判定が常に true/false になる取り違えを防ぐ)
        let p = parent_candidates(&pool, subject, owner, false).await.unwrap();
        assert!(has(&p, owned) && !has(&p, plain) && !has(&p, teamed));
        let p = parent_candidates(&pool, subject, admin, false).await.unwrap();
        assert!(has(&p, teamed) && !has(&p, owned));
        let p = parent_candidates(&pool, subject, member, false).await.unwrap();
        assert!(!has(&p, teamed), "一般メンバーは不可");
        let p = parent_candidates(&pool, subject, outsider, true).await.unwrap();
        assert!(has(&p, owned) && has(&p, teamed) && has(&p, plain), "システム管理者は全部");
    }

    /// 親候補は、自分の子孫と、深さの超過になるものを除く。
    #[tokio::test]
    async fn parent_candidates_exclude_descendants_and_too_deep() {
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let user = crate::test_support::create_test_user(&pool, "pc").await;
        // S の下に K(子)がある → S の部分木の高さは 1
        let s = crate::test_support::create_test_project(&pool, "PS", user).await;
        let k = crate::test_support::create_test_project(&pool, "PK", user).await;
        set_parent(&pool, k, Some(s)).await.unwrap();
        // 別の系統: a1 > a2 > a3 > a4(深さ 1〜4)
        let mut chain = vec![];
        for i in 0..4 {
            let n = crate::test_support::create_test_project(&pool, &format!("PA{i}"), user).await;
            if i > 0 { set_parent(&pool, n, Some(chain[i - 1])).await.unwrap(); }
            chain.push(n);
        }
        let c = parent_candidates(&pool, s, user, true).await.unwrap();
        assert!(!has(&c, s), "自分は除く");
        assert!(!has(&c, k), "子孫は除く(循環になる)");
        // S の高さ 1 → 親の深さは 3 まで(3 + 1 + 1 = 5)。a3(深さ3)は可、a4(深さ4)は不可
        assert!(has(&c, chain[2]), "深さ3の親までは可");
        assert!(!has(&c, chain[3]), "深さ4の親にすると6段になる");
        // 候補にあるものは、実際に set_parent も通る(候補と検査が一致する)
        set_parent(&pool, s, Some(chain[2])).await.unwrap();
    }

    /// 関連候補は、既に関連のもの(どちらの向きでも)を除く。
    #[tokio::test]
    async fn relation_candidates_exclude_already_related_in_both_directions() {
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let user = crate::test_support::create_test_user(&pool, "rc").await;
        let a = crate::test_support::create_test_project(&pool, "RA", user).await;
        let b = crate::test_support::create_test_project(&pool, "RB", user).await;
        let c = crate::test_support::create_test_project(&pool, "RC", user).await;
        add_relation(&pool, b, a).await.unwrap(); // 入力の順序は問わない
        let cands = relation_candidates(&pool, a, user, true).await.unwrap();
        assert!(!has(&cands, b), "既に関連");
        assert!(has(&cands, c));
        // 相手側から見ても同じ
        let cands = relation_candidates(&pool, b, user, true).await.unwrap();
        assert!(!has(&cands, a));
    }

    /// 子を持つプロジェクトは削除できない(子の名前を返す)。子を外せば削除できる。
    #[tokio::test]
    async fn delete_project_is_refused_while_it_has_children() {
        use crate::infrastructure::repositories::resource_repo::{delete_project, DeleteProjectResult};
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let user = crate::test_support::create_test_user(&pool, "dp").await;
        let parent = crate::test_support::create_test_project(&pool, "DP", user).await;
        let child = crate::test_support::create_test_project(&pool, "DC", user).await;
        set_parent(&pool, child, Some(parent)).await.unwrap();
        let child_name: String = sqlx::query_scalar("SELECT name FROM tickets_project WHERE id = $1")
            .bind(child as i64).fetch_one(&pool).await.unwrap();

        match delete_project(&pool, parent).await.unwrap() {
            DeleteProjectResult::HasChildren(names) => assert!(names.contains(&child_name)),
            _ => panic!("HasChildren を期待"),
        }
        let still: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM tickets_project WHERE id = $1)")
            .bind(parent as i64).fetch_one(&pool).await.unwrap();
        assert!(still, "拒否された削除は反映されない");

        set_parent(&pool, child, None).await.unwrap();
        assert!(matches!(delete_project(&pool, parent).await.unwrap(), DeleteProjectResult::Deleted));
        assert!(matches!(delete_project(&pool, child).await.unwrap(), DeleteProjectResult::Deleted));
    }

    /// `GET /projects/` の絞り込み(親 / ロードマップ / 関連)と、その AND、省略時。
    #[tokio::test]
    async fn project_list_filters_by_parent_roadmap_and_relation() {
        use crate::domain::models::resource_api::ProjectListFilter;
        use crate::infrastructure::repositories::{resource_repo::find_all_projects, roadmap_repo};
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let user = crate::test_support::create_test_user(&pool, "pl").await;
        let parent = crate::test_support::create_test_project(&pool, "LP", user).await;
        let c1 = crate::test_support::create_test_project(&pool, "L1", user).await;
        let c2 = crate::test_support::create_test_project(&pool, "L2", user).await;
        let other = crate::test_support::create_test_project(&pool, "LO", user).await;
        set_parent(&pool, c1, Some(parent)).await.unwrap();
        set_parent(&pool, c2, Some(parent)).await.unwrap();
        add_relation(&pool, c1, other).await.unwrap();
        let rm = roadmap_repo::create_roadmap(
            &pool,
            &roadmap_repo::RoadmapCreateIn { name: format!("rm-{}", crate::test_support::unique_suffix()), description: String::new() },
            Some(user),
        ).await.unwrap();
        roadmap_repo::add_project(&pool, rm, c1, Some(user)).await.unwrap();

        let ids = |v: Vec<crate::domain::models::resource_api::ProjectOut>| -> Vec<i32> { v.into_iter().map(|p| p.id).collect() };
        let f = |parent: Option<&str>, roadmap: Option<i32>, related: Option<i32>| ProjectListFilter {
            parent_project_id: parent.map(String::from), roadmap_id: roadmap, related_to: related,
        };

        let by_parent = ids(find_all_projects(&pool, 1, None, &f(Some(&parent.to_string()), None, None)).await.unwrap());
        assert!(by_parent.contains(&c1) && by_parent.contains(&c2) && !by_parent.contains(&other) && !by_parent.contains(&parent));
        let by_roadmap = ids(find_all_projects(&pool, 1, None, &f(None, Some(rm), None)).await.unwrap());
        assert_eq!(by_roadmap, vec![c1]);
        let by_related = ids(find_all_projects(&pool, 1, None, &f(None, None, Some(other))).await.unwrap());
        assert_eq!(by_related, vec![c1], "関連は向きを問わない");
        let by_related_rev = ids(find_all_projects(&pool, 1, None, &f(None, None, Some(c1))).await.unwrap());
        assert_eq!(by_related_rev, vec![other]);
        // AND: 親=parent かつ ロードマップ=rm → c1 のみ
        let both = ids(find_all_projects(&pool, 1, None, &f(Some(&parent.to_string()), Some(rm), None)).await.unwrap());
        assert_eq!(both, vec![c1]);
        // ルートのみ(none)は、親を持つ c1 を含まない
        let roots = ids(find_all_projects(&pool, 1, None, &f(Some("none"), Some(rm), None)).await.unwrap());
        assert!(roots.is_empty());
        // 省略時: 従来どおり。新しい項目が返る
        let all = find_all_projects(&pool, 1, None, &ProjectListFilter::default()).await.unwrap();
        assert!(!all.is_empty());
        let one = find_all_projects(&pool, 1, None, &f(Some(&parent.to_string()), Some(rm), None)).await.unwrap();
        let one = one.first().expect("c1");
        assert_eq!(one.parent_project_id, Some(parent));
        assert_eq!(one.roadmap_ids, vec![rm]);

        roadmap_repo::delete_roadmap(&pool, rm).await.unwrap();
    }

    /// 関連の追加は、新規なら true・既に関連(どちらの向きでも)なら false を返す。
    #[tokio::test]
    async fn add_relation_reports_whether_it_created_the_relation() {
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let user = crate::test_support::create_test_user(&pool, "ar").await;
        let a = crate::test_support::create_test_project(&pool, "AR", user).await;
        let b = crate::test_support::create_test_project(&pool, "AS", user).await;
        assert!(add_relation(&pool, a, b).await.unwrap(), "新規");
        assert!(!add_relation(&pool, a, b).await.unwrap(), "再追加");
        assert!(!add_relation(&pool, b, a).await.unwrap(), "逆向きでも既存");
        assert_eq!(relations(&pool, a).await.unwrap().len(), 1);
    }

    /// rollup は、チケットが1件も無いプロジェクト(LEFT JOIN で1行になる)を、チケット1件として数えない。
    #[tokio::test]
    async fn rollup_does_not_count_projects_without_tickets_as_tickets() {
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let user = crate::test_support::create_test_user(&pool, "ru").await;
        let root = crate::test_support::create_test_project(&pool, "RUR", user).await; // チケット 0 件
        let child = crate::test_support::create_test_project(&pool, "RUC", user).await;
        let empty_child = crate::test_support::create_test_project(&pool, "RUE", user).await; // チケット 0 件
        set_parent(&pool, child, Some(root)).await.unwrap();
        set_parent(&pool, empty_child, Some(root)).await.unwrap();
        for status in ["closed", "open", "open", "canceled"] {
            let t = crate::test_support::create_test_ticket(&pool, child, "RU", user).await;
            sqlx::query("UPDATE tickets_ticket SET status = $2 WHERE id = $1")
                .bind(t as i64).bind(status).execute(&pool).await.unwrap();
        }
        let r = rollup(&pool, root).await.unwrap();
        assert_eq!(r.project_count, 3);
        assert_eq!(r.ticket_count, 4, "実際のチケット数(チケットの無いプロジェクトは数えない)");
        assert_eq!(r.completed_count, 1);
        // 進捗 = 完了 1 / (キャンセルを除く 3)
        assert!((r.progress.unwrap() - 1.0 / 3.0).abs() < 1e-9);
        // チケットがまったく無い場合は 0 件・進捗なし
        let none = rollup(&pool, empty_child).await.unwrap();
        assert_eq!((none.ticket_count, none.completed_count, none.progress), (0, 0, None));
    }
}
