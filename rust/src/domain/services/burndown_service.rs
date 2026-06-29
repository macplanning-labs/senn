/// domain/services/burndown_service.rs — バーンダウンチャート計算ロジック
///
/// マイルストーン内のチケットステータス変更履歴からバーンダウンデータを構築。

use chrono::{NaiveDate, Datelike};
use sqlx::PgPool;
use std::collections::BTreeMap;

use crate::domain::models::ticket::TicketStatus;
use crate::infrastructure::repositories::{history_repo, ticket_repo, milestone_repo};

/// バーンダウンチャートのデータポイント
#[derive(Debug, Clone, serde::Serialize)]
pub struct BurndownPoint {
    pub date: String,
    pub total: i32,
    pub remaining: i32,
    pub ideal: f64,
}

/// バーンダウンデータ生成
pub async fn build_burndown_data(
    pool: &PgPool,
    milestone_id: i32,
) -> anyhow::Result<Vec<BurndownPoint>> {
    // マイルストーン情報
    let milestone = milestone_repo::find_by_id(pool, milestone_id).await?
        .ok_or_else(|| anyhow::anyhow!("マイルストーンが見つかりません"))?;

    let due_date = milestone.due_date
        .ok_or_else(|| anyhow::anyhow!("期限日が設定されていません"))?;

    // マイルストーン内の全チケット
    let filter = crate::infrastructure::repositories::ticket_repo::TicketFilter {
        milestone_id: Some(milestone_id),
        ..Default::default()
    };
    let tickets = ticket_repo::find_all(pool, &filter).await?;
    let total = tickets.len() as i32;

    if total == 0 {
        return Ok(vec![]);
    }

    // ステータス変更履歴
    let histories = history_repo::find_by_milestone(pool, milestone_id).await?;

    // 日付ごとの完了チケット数を集計
    let mut closed_by_date: BTreeMap<NaiveDate, i32> = BTreeMap::new();

    // 現在完了しているチケットのIDセット
    let mut closed_ids: std::collections::HashSet<i32> = std::collections::HashSet::new();

    for h in &histories {
        let date = h.changed_at.date_naive();
        let new_status = TicketStatus::from_db(&h.new_status);
        let old_status = TicketStatus::from_db(&h.old_status);

        if new_status.is_terminal() && !old_status.is_terminal() {
            closed_ids.insert(h.ticket_id);
        } else if !new_status.is_terminal() && old_status.is_terminal() {
            closed_ids.remove(&h.ticket_id);
        }

        closed_by_date.insert(date, closed_ids.len() as i32);
    }

    // 開始日（最初の履歴 or マイルストーン作成日）
    let start_date = histories.first()
        .map(|h| h.changed_at.date_naive())
        .unwrap_or_else(|| milestone.created_at.date_naive());

    // 日付範囲
    let today = chrono::Local::now().date_naive();
    let end_date = due_date.max(today);
    let total_days = (end_date - start_date).num_days().max(1) as f64;

    let mut points = Vec::new();
    let mut current_closed = 0;
    let mut current_date = start_date;

    while current_date <= end_date {
        // その日までの完了数を更新
        if let Some(&count) = closed_by_date.get(&current_date) {
            current_closed = count;
        }

        let day_offset = (current_date - start_date).num_days() as f64;
        let ideal = total as f64 * (1.0 - day_offset / total_days);

        points.push(BurndownPoint {
            date: current_date.format("%Y-%m-%d").to_string(),
            total,
            remaining: total - current_closed,
            ideal: (ideal * 10.0).round() / 10.0,
        });

        current_date = current_date.succ_opt().unwrap_or(current_date);
    }

    Ok(points)
}
