/// infrastructure/repositories/roadmap_repo.rs — ロードマップの管理
///
/// 設計（WIPAPPDEV-000066 Phase 3）:
/// - ロードマップは全社共通（チーム単位ではない）。
/// - 名前は 1〜100 文字、大文字小文字を区別せず一意。
/// - CRUD、所属の追加・削除（冪等）、「プロジェクトが属するロードマップ一覧」。

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

/// ロードマップ（一覧・詳細）
#[derive(Debug, Clone, Serialize)]
pub struct RoadmapOut {
    pub id: i32,
    pub name: String,
    pub description: String,
    #[serde(rename = "ownerId")]
    pub owner_id: Option<i32>,
    #[serde(rename = "projectCount")]
    pub project_count: i64,
}

/// ロードマップの詳細（所属プロジェクトの一覧つき）
#[derive(Debug, Clone, Serialize)]
pub struct RoadmapDetailOut {
    pub id: i32,
    pub name: String,
    pub description: String,
    #[serde(rename = "ownerId")]
    pub owner_id: Option<i32>,
    pub projects: Vec<RoadmapProjectOut>,
}

/// ロードマップに所属するプロジェクト
#[derive(Debug, Clone, Serialize)]
pub struct RoadmapProjectOut {
    pub id: i32,
    pub prefix: String,
    pub name: String,
    pub status: String,
    #[serde(rename = "ticketCount")]
    pub ticket_count: i64,
    #[serde(rename = "completedCount")]
    pub completed_count: i64,
    pub progress: Option<f64>,
}

/// ロードマップの入力
#[derive(Debug, Clone, Deserialize)]
pub struct RoadmapCreateIn {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RoadmapUpdateIn {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

// =============================================================================
// CRUD
// =============================================================================

/// ロードマップの一覧（プロジェクト件数つき）
pub async fn list_roadmaps(pool: &PgPool) -> anyhow::Result<Vec<RoadmapOut>> {
    let rows = sqlx::query_as::<_, (i32, String, String, Option<i32>, i64)>(
        "SELECT
            r.id::int4,
            r.name,
            r.description,
            r.owner_id::int4,
            COUNT(rp.project_id)::int8 as project_count
         FROM roadmaps r
         LEFT JOIN roadmap_projects rp ON r.id = rp.roadmap_id
         GROUP BY r.id, r.name, r.description, r.owner_id
         ORDER BY r.name ASC",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, name, description, owner_id, project_count)| RoadmapOut {
            id,
            name,
            description,
            owner_id,
            project_count,
        })
        .collect())
}

/// ロードマップを作成する。名前が重複（大文字小文字を区別しない）なら Err。
/// ロードマップ名として妥当か(前後の空白を除いて 1〜100 文字。文字数で数える。日本語も1文字)。
pub fn is_valid_roadmap_name(name: &str) -> bool {
    let n = name.trim();
    !n.is_empty() && n.chars().count() <= 100
}

pub async fn create_roadmap(pool: &PgPool, input: &RoadmapCreateIn, owner_id: Option<i32>) -> anyhow::Result<i32> {
    // 名前の妥当性チェック(前後の空白は取り除いて保存する)
    if !is_valid_roadmap_name(&input.name) {
        anyhow::bail!("roadmap name must be 1-100 characters");
    }
    let name = input.name.trim();

    // 重複チェック（大文字小文字を区別しない）
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM roadmaps WHERE LOWER(name) = LOWER($1))",
    )
    .bind(name)
    .fetch_one(pool)
    .await?;

    if exists {
        anyhow::bail!("roadmap name already exists");
    }

    let id: i32 = sqlx::query_scalar(
        "INSERT INTO roadmaps (name, description, owner_id)
         VALUES ($1, $2, $3)
         RETURNING id::int4",
    )
    .bind(name)
    .bind(&input.description)
    .bind(owner_id.map(|id| id as i64))
    .fetch_one(pool)
    .await
    .map_err(|e| match &e {
        // 同時に同じ名前が作られた場合(確認と挿入の間)も、重複として扱う
        sqlx::Error::Database(db) if db.code().as_deref() == Some("23505") => {
            anyhow::anyhow!("roadmap name already exists")
        }
        _ => anyhow::Error::from(e),
    })?;

    Ok(id)
}

/// ロードマップを取得する
pub async fn find_roadmap_by_id(pool: &PgPool, id: i32) -> anyhow::Result<Option<RoadmapDetailOut>> {
    let roadmap: Option<(i32, String, String, Option<i32>)> = sqlx::query_as(
        "SELECT id::int4, name, description, owner_id::int4 FROM roadmaps WHERE id = $1",
    )
    .bind(id as i64)
    .fetch_optional(pool)
    .await?;

    let Some((roadmap_id, name, description, owner_id)) = roadmap else {
        return Ok(None);
    };

    let projects = sqlx::query_as::<_, (i32, String, String, String, i64, i64, i64)>(
        "SELECT
            p.id::int4,
            p.prefix,
            p.name,
            p.status,
            (SELECT COUNT(*)::int8 FROM tickets_ticket WHERE project_id = p.id) as ticket_count,
            (SELECT COUNT(*)::int8 FROM tickets_ticket WHERE project_id = p.id AND status = 'closed') as completed_count,
            (SELECT COUNT(*)::int8 FROM tickets_ticket WHERE project_id = p.id AND status <> 'canceled') as active_count
         FROM roadmap_projects rp
         JOIN tickets_project p ON p.id = rp.project_id
         WHERE rp.roadmap_id = $1
         ORDER BY p.name ASC",
    )
    .bind(roadmap_id as i64)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(id, prefix, name, status, ticket_count, completed_count, active_count)| {
        let progress = if active_count > 0 {
            Some(completed_count as f64 / active_count as f64)
        } else {
            None
        };
        RoadmapProjectOut {
            id,
            prefix,
            name,
            status,
            ticket_count,
            completed_count,
            progress,
        }
    })
    .collect();

    Ok(Some(RoadmapDetailOut {
        id: roadmap_id,
        name,
        description,
        owner_id,
        projects,
    }))
}

/// ロードマップを更新する。存在しなければ Ok(false)。
pub async fn update_roadmap(pool: &PgPool, id: i32, input: &RoadmapUpdateIn) -> anyhow::Result<bool> {
    // 名前の妥当性チェック（指定されている場合）
    if let Some(ref name) = input.name {
        if !is_valid_roadmap_name(name) {
            anyhow::bail!("roadmap name must be 1-100 characters");
        }

        // 重複チェック（自分以外）
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM roadmaps WHERE LOWER(name) = LOWER($1) AND id <> $2)",
        )
        .bind(name)
        .bind(id as i64)
        .fetch_one(pool)
        .await?;

        if exists {
            anyhow::bail!("roadmap name already exists");
        }
    }

    let mut query = "UPDATE roadmaps SET ".to_string();
    let mut parts = Vec::new();

    if input.name.is_some() {
        parts.push("name = $2");
    }
    if input.description.is_some() {
        if parts.is_empty() {
            parts.push("description = $2");
        } else {
            parts.push("description = $3");
        }
    }

    if parts.is_empty() {
        // 何も更新しない
        let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM roadmaps WHERE id = $1)")
            .bind(id as i64)
            .fetch_one(pool)
            .await?;
        return Ok(exists);
    }

    query.push_str(&parts.join(", "));
    query.push_str(" WHERE id = $1");

    let result = match (input.name.as_ref(), input.description.as_ref()) {
        (Some(name), Some(desc)) => {
            sqlx::query(&query)
                .bind(id as i64)
                .bind(name)
                .bind(desc)
                .execute(pool)
                .await?
        }
        (Some(name), None) => {
            sqlx::query(&query)
                .bind(id as i64)
                .bind(name)
                .execute(pool)
                .await?
        }
        (None, Some(desc)) => {
            sqlx::query(&query)
                .bind(id as i64)
                .bind(desc)
                .execute(pool)
                .await?
        }
        (None, None) => {
            let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM roadmaps WHERE id = $1)")
                .bind(id as i64)
                .fetch_one(pool)
                .await?;
            return Ok(exists);
        }
    };

    Ok(result.rows_affected() > 0)
}

/// ロードマップを削除する。存在しなければ Ok(false)。所属プロジェクトは ON DELETE CASCADE で自動削除。
pub async fn delete_roadmap(pool: &PgPool, id: i32) -> anyhow::Result<bool> {
    let result = sqlx::query("DELETE FROM roadmaps WHERE id = $1")
        .bind(id as i64)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

// =============================================================================
// 所属
// =============================================================================

/// ロードマップにプロジェクトを追加する。既に所属なら冪等に成功を返す。
/// 戻り値: 新規追加なら true、既に所属していたら false。
pub async fn add_project(pool: &PgPool, roadmap_id: i32, project_id: i32, added_by: Option<i32>) -> anyhow::Result<bool> {
    // 両者が存在するか(ロードマップとプロジェクトの id は別の表なので、同じ値になり得る。
    // UNION で id を混ぜると1件に潰れるため、それぞれ存在確認する)
    let both_exist: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM roadmaps WHERE id = $1)
            AND EXISTS (SELECT 1 FROM tickets_project WHERE id = $2)",
    )
    .bind(roadmap_id as i64)
    .bind(project_id as i64)
    .fetch_one(pool)
    .await?;

    if !both_exist {
        anyhow::bail!("roadmap or project not found");
    }

    // UPSERT（既に所属なら何もしない）
    let result = sqlx::query(
        "INSERT INTO roadmap_projects (roadmap_id, project_id, added_by)
         VALUES ($1, $2, $3)
         ON CONFLICT (roadmap_id, project_id) DO NOTHING",
    )
    .bind(roadmap_id as i64)
    .bind(project_id as i64)
    .bind(added_by.map(|id| id as i64))
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// ロードマップからプロジェクトを削除する。所属していなければ Ok(false)。
pub async fn remove_project(pool: &PgPool, roadmap_id: i32, project_id: i32) -> anyhow::Result<bool> {
    let result = sqlx::query("DELETE FROM roadmap_projects WHERE roadmap_id = $1 AND project_id = $2")
        .bind(roadmap_id as i64)
        .bind(project_id as i64)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// プロジェクトが属するロードマップの一覧
pub async fn roadmaps_for_project(pool: &PgPool, project_id: i32) -> anyhow::Result<Vec<RoadmapOut>> {
    let rows = sqlx::query_as::<_, (i32, String, String, Option<i32>, i64)>(
        "SELECT
            r.id::int4,
            r.name,
            r.description,
            r.owner_id::int4,
            COUNT(rp.project_id)::int8 as project_count
         FROM roadmap_projects rp
         JOIN roadmaps r ON r.id = rp.roadmap_id
         WHERE rp.project_id = $1
         GROUP BY r.id, r.name, r.description, r.owner_id
         ORDER BY r.name ASC",
    )
    .bind(project_id as i64)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, name, description, owner_id, project_count)| RoadmapOut {
            id,
            name,
            description,
            owner_id,
            project_count,
        })
        .collect())
}

/// プロジェクトが属するロードマップの一覧（for_project は roadmaps_for_project の別名）
pub async fn for_project(pool: &PgPool, project_id: i32) -> anyhow::Result<Vec<RoadmapOut>> {
    roadmaps_for_project(pool, project_id).await
}

/// ユーザーが owner のロードマップの一覧
pub async fn for_owner(pool: &PgPool, user_id: i32) -> anyhow::Result<Vec<RoadmapOut>> {
    let rows = sqlx::query_as::<_, (i32, String, String, Option<i32>, i64)>(
        "SELECT
            r.id::int4,
            r.name,
            r.description,
            r.owner_id::int4,
            COUNT(rp.project_id)::int8 as project_count
         FROM roadmaps r
         LEFT JOIN roadmap_projects rp ON r.id = rp.roadmap_id
         WHERE r.owner_id = $1
         GROUP BY r.id, r.name, r.description, r.owner_id
         ORDER BY r.name ASC",
    )
    .bind(user_id as i64)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, name, description, owner_id, project_count)| RoadmapOut {
            id,
            name,
            description,
            owner_id,
            project_count,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_roadmap() {
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let suffix = crate::test_support::unique_suffix();
        let input = RoadmapCreateIn {
            name: format!("Q4 Roadmap {}", suffix),
            description: "2026 Q4".to_string(),
        };
        let id = create_roadmap(&pool, &input, None).await.unwrap();
        assert!(id > 0);

        let roadmap = find_roadmap_by_id(&pool, id).await.unwrap().unwrap();
        assert_eq!(roadmap.name, format!("Q4 Roadmap {}", suffix));

        // Clean up
        let _ = delete_roadmap(&pool, id).await;
    }

    #[tokio::test]
    async fn test_create_roadmap_duplicate_name() {
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let suffix = crate::test_support::unique_suffix();
        let input = RoadmapCreateIn {
            name: format!("Duplicate {}", suffix),
            description: "".to_string(),
        };
        let id1 = create_roadmap(&pool, &input, None).await.unwrap();
        let result = create_roadmap(&pool, &input, None).await;
        assert!(result.is_err());

        // Clean up
        let _ = delete_roadmap(&pool, id1).await;
    }

    #[tokio::test]
    async fn test_create_roadmap_case_insensitive_duplicate() {
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let suffix = crate::test_support::unique_suffix();
        let input1 = RoadmapCreateIn {
            name: format!("Roadmap {}", suffix),
            description: "".to_string(),
        };
        let input2 = RoadmapCreateIn {
            name: format!("roadmap {}", suffix), // 小文字
            description: "".to_string(),
        };
        let id1 = create_roadmap(&pool, &input1, None).await.unwrap();
        let result = create_roadmap(&pool, &input2, None).await;
        assert!(result.is_err());

        // Clean up
        let _ = delete_roadmap(&pool, id1).await;
    }

    #[tokio::test]
    async fn test_create_roadmap_name_too_long() {
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let input = RoadmapCreateIn {
            name: "a".repeat(101),
            description: "".to_string(),
        };
        let result = create_roadmap(&pool, &input, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_roadmaps() {
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let suffix = crate::test_support::unique_suffix();
        let input1 = RoadmapCreateIn {
            name: format!("RM1 {}", suffix),
            description: "".to_string(),
        };
        let input2 = RoadmapCreateIn {
            name: format!("RM2 {}", suffix),
            description: "".to_string(),
        };
        let id1 = create_roadmap(&pool, &input1, None).await.unwrap();
        let id2 = create_roadmap(&pool, &input2, None).await.unwrap();

        let roadmaps = list_roadmaps(&pool).await.unwrap();
        assert!(roadmaps.len() >= 2);

        // Clean up
        let _ = delete_roadmap(&pool, id1).await;
        let _ = delete_roadmap(&pool, id2).await;
    }

    #[tokio::test]
    async fn test_add_project_to_roadmap() {
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let user = crate::test_support::create_test_user(&pool, "rm").await;
        let project = crate::test_support::create_test_project(&pool, "RM", user).await;

        let suffix = crate::test_support::unique_suffix();
        let input = RoadmapCreateIn {
            name: format!("Roadmap {}", suffix),
            description: "".to_string(),
        };
        let roadmap_id = create_roadmap(&pool, &input, None).await.unwrap();

        add_project(&pool, roadmap_id, project, None).await.unwrap();

        let roadmap = find_roadmap_by_id(&pool, roadmap_id).await.unwrap().unwrap();
        assert_eq!(roadmap.projects.len(), 1);
        assert_eq!(roadmap.projects[0].id, project);

        // Clean up
        let _ = remove_project(&pool, roadmap_id, project).await;
        let _ = delete_roadmap(&pool, roadmap_id).await;
    }

    #[tokio::test]
    async fn test_add_project_idempotent() {
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let user = crate::test_support::create_test_user(&pool, "rm").await;
        let project = crate::test_support::create_test_project(&pool, "RM", user).await;

        let suffix = crate::test_support::unique_suffix();
        let input = RoadmapCreateIn {
            name: format!("Roadmap {}", suffix),
            description: "".to_string(),
        };
        let roadmap_id = create_roadmap(&pool, &input, None).await.unwrap();

        add_project(&pool, roadmap_id, project, None).await.unwrap();
        add_project(&pool, roadmap_id, project, None).await.unwrap(); // 2度目

        let roadmap = find_roadmap_by_id(&pool, roadmap_id).await.unwrap().unwrap();
        assert_eq!(roadmap.projects.len(), 1); // 1 つだけ

        // Clean up
        let _ = remove_project(&pool, roadmap_id, project).await;
        let _ = delete_roadmap(&pool, roadmap_id).await;
    }

    #[tokio::test]
    async fn test_remove_project_from_roadmap() {
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let user = crate::test_support::create_test_user(&pool, "rm").await;
        let project = crate::test_support::create_test_project(&pool, "RM", user).await;

        let suffix = crate::test_support::unique_suffix();
        let input = RoadmapCreateIn {
            name: format!("Roadmap {}", suffix),
            description: "".to_string(),
        };
        let roadmap_id = create_roadmap(&pool, &input, None).await.unwrap();

        add_project(&pool, roadmap_id, project, None).await.unwrap();
        let removed = remove_project(&pool, roadmap_id, project).await.unwrap();
        assert!(removed);

        let roadmap = find_roadmap_by_id(&pool, roadmap_id).await.unwrap().unwrap();
        assert_eq!(roadmap.projects.len(), 0);

        // Clean up
        let _ = delete_roadmap(&pool, roadmap_id).await;
    }

    #[tokio::test]
    async fn test_roadmap_add_and_remove_multiple_projects() {
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let user = crate::test_support::create_test_user(&pool, "rm").await;
        let project1 = crate::test_support::create_test_project(&pool, "RM1", user).await;
        let project2 = crate::test_support::create_test_project(&pool, "RM2", user).await;

        let suffix = crate::test_support::unique_suffix();
        let input = RoadmapCreateIn {
            name: format!("MultiRM {}", suffix),
            description: "".to_string(),
        };
        let roadmap_id = create_roadmap(&pool, &input, None).await.unwrap();

        // 複数のプロジェクトを追加
        add_project(&pool, roadmap_id, project1, None).await.unwrap();
        add_project(&pool, roadmap_id, project2, None).await.unwrap();

        let roadmap = find_roadmap_by_id(&pool, roadmap_id).await.unwrap().unwrap();
        assert_eq!(roadmap.projects.len(), 2);

        // 1つを削除
        remove_project(&pool, roadmap_id, project1).await.unwrap();
        let roadmap = find_roadmap_by_id(&pool, roadmap_id).await.unwrap().unwrap();
        assert_eq!(roadmap.projects.len(), 1);
        assert_eq!(roadmap.projects[0].id, project2);

        // Clean up
        let _ = remove_project(&pool, roadmap_id, project2).await;
        let _ = delete_roadmap(&pool, roadmap_id).await;
    }

    #[tokio::test]
    async fn test_roadmaps_for_project() {
        // プロジェクトに複数のロードマップを設定
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let user = crate::test_support::create_test_user(&pool, "rm").await;
        let project = crate::test_support::create_test_project(&pool, "RMPROJ", user).await;

        let suffix1 = crate::test_support::unique_suffix();
        let suffix2 = crate::test_support::unique_suffix();
        let input1 = RoadmapCreateIn {
            name: format!("Roadmap1 {}", suffix1),
            description: "".to_string(),
        };
        let input2 = RoadmapCreateIn {
            name: format!("Roadmap2 {}", suffix2),
            description: "".to_string(),
        };
        let roadmap_id1 = create_roadmap(&pool, &input1, None).await.unwrap();
        let roadmap_id2 = create_roadmap(&pool, &input2, None).await.unwrap();

        add_project(&pool, roadmap_id1, project, None).await.unwrap();
        add_project(&pool, roadmap_id2, project, None).await.unwrap();

        // find_roadmaps_for_project を呼び出す（存在するか確認）
        let sql = "SELECT id::int4 FROM roadmaps WHERE EXISTS (
            SELECT 1 FROM roadmap_projects WHERE project_id = $1 AND roadmap_id = roadmaps.id
        ) ORDER BY roadmaps.id";
        let roadmap_ids: Vec<i32> = sqlx::query_scalar(sql)
            .bind(project)
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(roadmap_ids.len(), 2);

        // Clean up
        let _ = remove_project(&pool, roadmap_id1, project).await;
        let _ = remove_project(&pool, roadmap_id2, project).await;
        let _ = delete_roadmap(&pool, roadmap_id1).await;
        let _ = delete_roadmap(&pool, roadmap_id2).await;
    }

    #[tokio::test]
    async fn test_roadmap_name_length_limit() {
        let Some(pool) = crate::test_support::test_pool().await else { return };

        // 100文字まで（OK）
        let input_100 = RoadmapCreateIn {
            name: "a".repeat(100),
            description: "".to_string(),
        };
        let id_100 = create_roadmap(&pool, &input_100, None).await.unwrap();
        assert!(id_100 > 0);

        // 101文字（エラー）
        let input_101 = RoadmapCreateIn {
            name: "b".repeat(101),
            description: "".to_string(),
        };
        let result_101 = create_roadmap(&pool, &input_101, None).await;
        assert!(result_101.is_err());

        // Clean up
        let _ = delete_roadmap(&pool, id_100).await;
    }

    #[test]
    fn roadmap_name_is_trimmed_and_counted_in_characters() {
        assert!(!is_valid_roadmap_name(""));
        assert!(!is_valid_roadmap_name("   "), "空白だけは不可");
        assert!(is_valid_roadmap_name("  Q4  "));
        assert!(is_valid_roadmap_name(&"あ".repeat(100)), "日本語も100文字まで可(バイト数ではなく文字数)");
        assert!(!is_valid_roadmap_name(&"あ".repeat(101)));
        assert!(!is_valid_roadmap_name(&format!("  {}  ", "x".repeat(101))));
    }

    /// ロードマップの id とプロジェクトの id が同じ値でも、所属を追加できる(別々の表の id)。
    #[tokio::test]
    async fn add_project_works_when_roadmap_id_equals_project_id() {
        let Some(pool) = crate::test_support::test_pool().await else { return };
        let user = crate::test_support::create_test_user(&pool, "same").await;
        // 衝突しにくい大きな id を明示して、ロードマップとプロジェクトの id を一致させる
        let id: i64 = 900_000_000 + (uuid::Uuid::new_v4().as_u128() % 90_000_000) as i64;
        let suffix = crate::test_support::unique_suffix();
        sqlx::query("INSERT INTO roadmaps (id, name, description) VALUES ($1, $2, '')")
            .bind(id).bind(format!("same-{suffix}")).execute(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO tickets_project (id, name, prefix, description, status, created_at, grace_period_days)
             VALUES ($1, $2, $3, '', 'planned', NOW(), 0)",
        )
        .bind(id).bind(format!("same-{suffix}")).bind(format!("SM{suffix}")).execute(&pool).await.unwrap();

        assert!(add_project(&pool, id as i32, id as i32, Some(user)).await.unwrap(), "新規に追加できる");
        assert!(!add_project(&pool, id as i32, id as i32, Some(user)).await.unwrap(), "再追加は冪等");
        // 存在しない側は、引き続き「見つからない」になる
        assert!(add_project(&pool, id as i32, 2_000_000_000, Some(user)).await.is_err());
        assert!(add_project(&pool, 2_000_000_000, id as i32, Some(user)).await.is_err());

        delete_roadmap(&pool, id as i32).await.unwrap();
        sqlx::query("DELETE FROM tickets_project WHERE id = $1").bind(id).execute(&pool).await.unwrap();
    }
}
