/// infrastructure/repositories/category_repo.rs — カテゴリー永続化

use sqlx::PgPool;
use crate::domain::models::category::Category;

/// 全カテゴリー取得（ソート済み）
pub async fn find_all(pool: &PgPool) -> anyhow::Result<Vec<Category>> {
    let rows = sqlx::query_as::<_, Category>(
        "SELECT c.id, c.name, c.slug, c.level, c.parent_id, c.sort_order, c.color,
                p.name as parent_name
         FROM m_categories c
         LEFT JOIN m_categories p ON c.parent_id = p.id
         ORDER BY c.level, c.sort_order, c.name"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// フェーズ（level=1）一覧取得
#[allow(dead_code)]
pub async fn find_phases(pool: &PgPool) -> anyhow::Result<Vec<Category>> {
    let rows = sqlx::query_as::<_, Category>(
        "SELECT id, name, slug, level, parent_id, sort_order, color, NULL as parent_name
         FROM m_categories WHERE level = 1
         ORDER BY sort_order, name"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// フェーズ配下のカテゴリー取得
#[allow(dead_code)]
pub async fn find_by_phase(pool: &PgPool, phase_id: i32) -> anyhow::Result<Vec<Category>> {
    let rows = sqlx::query_as::<_, Category>(
        "SELECT c.id, c.name, c.slug, c.level, c.parent_id, c.sort_order, c.color,
                p.name as parent_name
         FROM m_categories c
         LEFT JOIN m_categories p ON c.parent_id = p.id
         WHERE c.parent_id = $1
         ORDER BY c.sort_order, c.name"
    )
    .bind(phase_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// カテゴリー作成
pub async fn create(
    pool: &PgPool, name: &str, slug: &str, level: i16,
    parent_id: Option<i32>, sort_order: i32, color: &str,
) -> anyhow::Result<i32> {
    let id = sqlx::query_scalar::<_, i32>(
        "INSERT INTO m_categories (name, slug, level, parent_id, sort_order, color)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id"
    )
    .bind(name).bind(slug).bind(level)
    .bind(parent_id).bind(sort_order).bind(color)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// カテゴリー更新
pub async fn update(pool: &PgPool, id: i32, name: &str, color: &str, sort_order: i32) -> anyhow::Result<()> {
    sqlx::query("UPDATE m_categories SET name=$2, color=$3, sort_order=$4 WHERE id=$1")
        .bind(id).bind(name).bind(color).bind(sort_order)
        .execute(pool)
        .await?;
    Ok(())
}

/// カテゴリー削除
pub async fn delete(pool: &PgPool, id: i32) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM m_categories WHERE id=$1")
        .bind(id).execute(pool).await?;
    Ok(())
}
