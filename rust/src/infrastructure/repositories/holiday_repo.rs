/// infrastructure/repositories/holiday_repo.rs — 休日永続化

use sqlx::PgPool;
use crate::domain::models::holiday::Holiday;

pub async fn find_all(pool: &PgPool) -> anyhow::Result<Vec<Holiday>> {
    let rows = sqlx::query_as::<_, Holiday>(
        "SELECT id, date, name, recurring FROM m_holidays ORDER BY date"
    ).fetch_all(pool).await?;
    Ok(rows)
}

pub async fn find_dates(pool: &PgPool) -> anyhow::Result<Vec<chrono::NaiveDate>> {
    let rows: Vec<(chrono::NaiveDate,)> = sqlx::query_as(
        "SELECT date FROM m_holidays ORDER BY date"
    ).fetch_all(pool).await?;
    Ok(rows.into_iter().map(|(d,)| d).collect())
}

pub async fn bulk_add(pool: &PgPool, holidays: &[(chrono::NaiveDate, String)]) -> anyhow::Result<i32> {
    let mut count = 0;
    for (date, name) in holidays {
        let result = sqlx::query(
            "INSERT INTO m_holidays (date, name) VALUES ($1, $2) ON CONFLICT (date) DO NOTHING"
        ).bind(date).bind(name).execute(pool).await?;
        count += result.rows_affected() as i32;
    }
    Ok(count)
}

pub async fn delete(pool: &PgPool, id: i32) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM m_holidays WHERE id=$1").bind(id).execute(pool).await?;
    Ok(())
}
