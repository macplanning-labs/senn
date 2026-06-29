/// domain/services/gantt_service.rs — ガントチャートデータ集約
///
/// チケットの親子構造を保ちつつフラット化し、ガントチャート描画用のデータを構築。

use chrono::NaiveDate;
use sqlx::PgPool;

use crate::domain::models::ticket::{Ticket, TicketStatus};
use crate::infrastructure::repositories::ticket_repo::{self, TicketFilter};

/// ガントチャート用の1行データ
pub struct GanttRow {
    pub ticket: Ticket,
    pub is_child: bool,
    pub progress: i32,
}

/// ガントチャート用データ取得
pub async fn build_gantt_data(
    pool: &PgPool,
    project_id: Option<i32>,
) -> anyhow::Result<Vec<GanttRow>> {
    let filter = TicketFilter {
        project_id,
        ..Default::default()
    };
    let all_tickets = ticket_repo::find_all(pool, &filter).await?;

    let ordered = build_ordered_list(&all_tickets);

    let rows = ordered
        .into_iter()
        .map(|(ticket, is_child)| {
            let progress = calc_progress(&ticket, &all_tickets);
            GanttRow { ticket, is_child, progress }
        })
        .collect();

    Ok(rows)
}

/// 親子構造を維持しつつフラット化
fn build_ordered_list(tickets: &[Ticket]) -> Vec<(Ticket, bool)> {
    use std::collections::HashMap;

    let parent_tickets: Vec<&Ticket> = tickets.iter()
        .filter(|t| t.parent_id.is_none())
        .collect();

    let mut children_map: HashMap<i32, Vec<&Ticket>> = HashMap::new();
    for t in tickets {
        if let Some(pid) = t.parent_id {
            children_map.entry(pid).or_default().push(t);
        }
    }

    let mut result = Vec::new();
    for parent in parent_tickets {
        result.push((parent.clone(), false));
        if let Some(children) = children_map.get(&parent.id) {
            for child in children {
                result.push(((*child).clone(), true));
            }
        }
    }

    // 孤立した子チケット
    let seen_ids: std::collections::HashSet<i32> = result.iter().map(|(t, _)| t.id).collect();
    for t in tickets {
        if !seen_ids.contains(&t.id) {
            result.push((t.clone(), false));
        }
    }

    result
}

/// 進捗率計算（親: 子チケットの平均 / 子: ステータスから算出）
fn calc_progress(ticket: &Ticket, all_tickets: &[Ticket]) -> i32 {
    let children: Vec<&Ticket> = all_tickets.iter()
        .filter(|t| t.parent_id == Some(ticket.id))
        .collect();

    if !children.is_empty() {
        let total: i32 = children.iter()
            .map(|c| TicketStatus::from_db(&c.status).progress_pct())
            .sum();
        total / children.len() as i32
    } else {
        TicketStatus::from_db(&ticket.status).progress_pct()
    }
}

/// ガント表示用の日付範囲を計算
pub fn calc_date_range(
    start_override: Option<NaiveDate>,
    end_override: Option<NaiveDate>,
) -> (NaiveDate, NaiveDate) {
    let today = chrono::Local::now().date_naive();

    let range_start = start_override.unwrap_or_else(|| {
        today.with_day(1).unwrap_or(today)
    });

    let range_end = end_override.unwrap_or_else(|| {
        let two_months_later = today + chrono::Duration::days(60);
        month_end(two_months_later)
    });

    // 最大180日に制限
    let max_end = range_start + chrono::Duration::days(179);
    let capped_end = range_end.min(max_end);

    (range_start, capped_end)
}

use chrono::Datelike;

/// 月末日を取得
fn month_end(d: NaiveDate) -> NaiveDate {
    if d.month() == 12 {
        NaiveDate::from_ymd_opt(d.year(), 12, 31).unwrap_or(d)
    } else {
        NaiveDate::from_ymd_opt(d.year(), d.month() + 1, 1)
            .unwrap_or(d)
            .pred_opt()
            .unwrap_or(d)
    }
}
