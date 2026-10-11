/// git_status_rule_repo.rs — トリガーごとのステータス設定(t_git_status_rule)
///
/// 行が無いトリガーは「標準」(現在の動作)。設計: docs/design/詳細設計_GitHub連携Phase2.md §3
use sqlx::{PgPool, Row};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredRule {
    pub trigger: String,
    /// "set" | "none"
    pub action: String,
    pub target_slug: Option<String>,
    pub target_category: Option<String>,
}

fn row_to_rule(r: &sqlx::postgres::PgRow) -> StoredRule {
    StoredRule {
        trigger: r.get("trigger"),
        action: r.get("action"),
        target_slug: r.get("target_slug"),
        target_category: r.get("target_category"),
    }
}

/// チケットに効く規則を 1 つ返す。プロジェクトの規則があればそれを、無ければチームの規則を使う。
/// どちらも無ければ None(標準)。
pub async fn find_effective_rule(
    pool: &PgPool,
    project_id: Option<i32>,
    team_id: Option<i32>,
    trigger: &str,
) -> anyhow::Result<Option<StoredRule>> {
    let row = sqlx::query(
        "SELECT trigger, action, target_slug, target_category FROM t_git_status_rule
         WHERE trigger = $3
           AND ((project_id IS NOT NULL AND project_id = $1::bigint)
             OR (team_id IS NOT NULL AND team_id = $2::bigint))
         ORDER BY (project_id IS NOT NULL) DESC
         LIMIT 1",
    )
    .bind(project_id)
    .bind(team_id)
    .bind(trigger)
    .fetch_optional(pool)
    .await?;
    Ok(row.as_ref().map(row_to_rule))
}

/// 1 つのプロジェクト、または 1 つのチームに保存されている規則(画面の表示用)。
pub async fn list_for_scope(
    pool: &PgPool,
    project_id: Option<i32>,
    team_id: Option<i32>,
) -> anyhow::Result<Vec<StoredRule>> {
    let rows = sqlx::query(
        "SELECT trigger, action, target_slug, target_category FROM t_git_status_rule
         WHERE (project_id IS NOT NULL AND project_id = $1::bigint)
            OR (team_id IS NOT NULL AND team_id = $2::bigint)
         ORDER BY id",
    )
    .bind(project_id)
    .bind(team_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(row_to_rule).collect())
}

/// 1 つのスコープの規則を、渡したもので入れ替える(渡されなかったトリガーは「標準」に戻る)。
/// project_id と team_id は、どちらか一方だけを渡す。
pub async fn replace_for_scope(
    pool: &PgPool,
    project_id: Option<i32>,
    team_id: Option<i32>,
    rules: &[StoredRule],
    user_id: i32,
) -> anyhow::Result<()> {
    if project_id.is_some() == team_id.is_some() {
        anyhow::bail!("exactly one of project or team");
    }
    let mut tx = pool.begin().await?;
    sqlx::query(
        "DELETE FROM t_git_status_rule
         WHERE (project_id IS NOT NULL AND project_id = $1::bigint)
            OR (team_id IS NOT NULL AND team_id = $2::bigint)",
    )
    .bind(project_id)
    .bind(team_id)
    .execute(&mut *tx)
    .await?;
    for r in rules {
        sqlx::query(
            "INSERT INTO t_git_status_rule
                (project_id, team_id, trigger, action, target_slug, target_category, updated_by_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(project_id)
        .bind(team_id)
        .bind(&r.trigger)
        .bind(&r.action)
        .bind(&r.target_slug)
        .bind(&r.target_category)
        .bind(user_id as i64)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// 既存のプロジェクト/チームの prefix(大文字)。小文字のブランチ名からキーを拾うときの検証に使う。
pub async fn known_ticket_prefixes(
    pool: &PgPool,
) -> anyhow::Result<std::collections::HashSet<String>> {
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT upper(prefix) FROM tickets_project WHERE prefix <> ''
         UNION
         SELECT upper(prefix) FROM m_team WHERE prefix IS NOT NULL AND prefix <> ''",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().collect())
}
