/// domain/services/export_service.rs — Excel エクスポート
///
/// チケット一覧とガントチャートの Excel 出力。
/// rust_xlsxwriter で現行 openpyxl スタイルを再現。

use chrono::{Datelike, NaiveDate};
use rust_xlsxwriter::*;

use crate::domain::models::ticket::Ticket;
use crate::domain::services::gantt_service::GanttRow;

/// チケット一覧 Excel 生成
pub fn export_ticket_list(tickets: &[Ticket]) -> anyhow::Result<Vec<u8>> {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();
    sheet.set_name("チケット一覧")?;

    // スタイル
    let hdr = Format::new()
        .set_font_name("Yu Gothic").set_font_size(11.0).set_bold()
        .set_font_color(Color::White).set_background_color(Color::RGB(0x1565C0))
        .set_align(FormatAlign::Center).set_align(FormatAlign::VerticalCenter)
        .set_text_wrap().set_border(FormatBorder::Thin).set_border_color(Color::RGB(0xBDBDBD));

    let cell = Format::new()
        .set_font_name("Yu Gothic").set_font_size(10.0)
        .set_align(FormatAlign::VerticalCenter).set_text_wrap()
        .set_border(FormatBorder::Thin).set_border_color(Color::RGB(0xBDBDBD));

    let center = Format::new()
        .set_font_name("Yu Gothic").set_font_size(10.0)
        .set_align(FormatAlign::Center).set_align(FormatAlign::VerticalCenter)
        .set_border(FormatBorder::Thin).set_border_color(Color::RGB(0xBDBDBD));

    let st_open = center.clone().set_background_color(Color::RGB(0xECEFF1));
    let st_prog = center.clone().set_background_color(Color::RGB(0xE3F2FD));
    let st_resv = center.clone().set_background_color(Color::RGB(0xFFF8E1));
    let st_done = center.clone().set_background_color(Color::RGB(0xE8F5E9));

    let pr_high = center.clone().set_background_color(Color::RGB(0xFFEBEE));
    let pr_med = center.clone().set_background_color(Color::RGB(0xFFF8E1));
    let pr_low = center.clone().set_background_color(Color::RGB(0xECEFF1));

    let overdue = Format::new()
        .set_font_name("Yu Gothic").set_font_size(10.0).set_bold()
        .set_font_color(Color::RGB(0xC62828))
        .set_align(FormatAlign::Center).set_align(FormatAlign::VerticalCenter)
        .set_border(FormatBorder::Thin).set_border_color(Color::RGB(0xBDBDBD))
        .set_background_color(Color::RGB(0xFFEBEE));

    // ヘッダー
    let headers: Vec<(&str, f64)> = vec![
        ("No.", 6.0), ("チケットキー", 14.0), ("タイトル", 40.0),
        ("ステータス", 10.0), ("優先度", 8.0), ("フェーズ", 12.0),
        ("カテゴリー", 14.0), ("担当者", 12.0), ("起票者", 12.0),
        ("マイルストーン", 16.0), ("開始日", 12.0), ("期限日", 12.0),
        ("作成日", 14.0), ("更新日", 14.0), ("説明", 50.0),
    ];

    for (col, (label, width)) in headers.iter().enumerate() {
        sheet.write_string_with_format(0, col as u16, *label, &hdr)?;
        sheet.set_column_width(col as u16, *width)?;
    }
    sheet.set_freeze_panes(1, 0)?;

    let today = chrono::Local::now().date_naive();

    for (i, t) in tickets.iter().enumerate() {
        let row = (i + 1) as u32;
        sheet.write_number_with_format(row, 0, (i + 1) as f64, &center)?;
        sheet.write_string_with_format(row, 1, &t.ticket_key, &cell)?;
        sheet.write_string_with_format(row, 2, &t.title, &cell)?;

        let sf = match t.status.as_str() {
            "未対応" => &st_open, "処理中" => &st_prog,
            "処理済み" => &st_resv, "完了" => &st_done, _ => &center,
        };
        sheet.write_string_with_format(row, 3, &t.status, sf)?;

        let pf = match t.priority.as_str() {
            "高" => &pr_high, "中" => &pr_med, "低" => &pr_low, _ => &center,
        };
        sheet.write_string_with_format(row, 4, &t.priority, pf)?;

        sheet.write_string_with_format(row, 5, t.phase_name.as_deref().unwrap_or(""), &center)?;
        sheet.write_string_with_format(row, 6, t.category_name.as_deref().unwrap_or(""), &center)?;
        sheet.write_string_with_format(row, 7, t.assignee_name.as_deref().unwrap_or(""), &center)?;
        sheet.write_string_with_format(row, 8, t.author_name.as_deref().unwrap_or(""), &center)?;
        sheet.write_string_with_format(row, 9, t.milestone_name.as_deref().unwrap_or(""), &center)?;

        // 日付
        let start_str = t.start_date.map(|d| d.format("%Y/%m/%d").to_string()).unwrap_or_default();
        sheet.write_string_with_format(row, 10, &start_str, &center)?;

        let due_str = t.due_date.map(|d| d.format("%Y/%m/%d").to_string()).unwrap_or_default();
        let is_overdue = t.due_date.map(|d| d < today && t.status != "完了").unwrap_or(false);
        sheet.write_string_with_format(row, 11, &due_str, if is_overdue { &overdue } else { &center })?;

        let created = t.created_at.format("%Y/%m/%d %H:%M").to_string();
        let updated = t.updated_at.format("%Y/%m/%d %H:%M").to_string();
        sheet.write_string_with_format(row, 12, &created, &cell)?;
        sheet.write_string_with_format(row, 13, &updated, &cell)?;

        let desc = if t.description.len() > 500 { &t.description[..500] } else { &t.description };
        sheet.write_string_with_format(row, 14, desc, &cell)?;

        sheet.set_row_height(row, 24.0)?;
    }

    sheet.autofilter(0, 0, tickets.len() as u32, 14)?;

    let buf = workbook.save_to_buffer()?;
    Ok(buf)
}

/// ガントチャート Excel 生成
pub fn export_gantt(
    rows: &[GanttRow],
    date_range: (NaiveDate, NaiveDate),
    holidays: &[NaiveDate],
    milestones: &[(NaiveDate, String)],
) -> anyhow::Result<Vec<u8>> {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();
    sheet.set_name("ガントチャート")?;

    let (range_start, range_end) = date_range;
    let total_days = (range_end - range_start).num_days() + 1;
    let date_list: Vec<NaiveDate> = (0..total_days)
        .map(|i| range_start + chrono::Duration::days(i))
        .collect();
    let today = chrono::Local::now().date_naive();

    let holiday_set: std::collections::HashSet<NaiveDate> = holidays.iter().copied().collect();
    let milestone_map: std::collections::HashMap<NaiveDate, &str> = milestones.iter()
        .map(|(d, n)| (*d, n.as_str()))
        .collect();

    // スタイル
    let g_hdr = Format::new()
        .set_font_name("Yu Gothic").set_font_size(10.0).set_bold()
        .set_font_color(Color::White).set_background_color(Color::RGB(0x1A237E))
        .set_align(FormatAlign::Center).set_align(FormatAlign::VerticalCenter)
        .set_border(FormatBorder::Thin).set_border_color(Color::RGB(0xCFD8DC));

    let g_month = Format::new()
        .set_font_name("Yu Gothic").set_font_size(9.0).set_bold()
        .set_font_color(Color::White).set_background_color(Color::RGB(0x283593))
        .set_align(FormatAlign::Center).set_align(FormatAlign::VerticalCenter)
        .set_border(FormatBorder::Thin).set_border_color(Color::RGB(0xCFD8DC));

    let g_cell = Format::new()
        .set_font_name("Yu Gothic").set_font_size(9.0)
        .set_align(FormatAlign::VerticalCenter).set_text_wrap()
        .set_border(FormatBorder::Hair).set_border_color(Color::RGB(0xE0E0E0));

    let g_center = Format::new()
        .set_font_name("Yu Gothic").set_font_size(9.0)
        .set_align(FormatAlign::Center).set_align(FormatAlign::VerticalCenter)
        .set_border(FormatBorder::Hair).set_border_color(Color::RGB(0xE0E0E0));

    let g_key = Format::new()
        .set_font_name("Yu Gothic").set_font_size(9.0).set_bold()
        .set_font_color(Color::RGB(0x1565C0))
        .set_align(FormatAlign::Center).set_align(FormatAlign::VerticalCenter)
        .set_border(FormatBorder::Hair).set_border_color(Color::RGB(0xE0E0E0));

    let parent_bg = Format::new()
        .set_font_name("Yu Gothic").set_font_size(9.0).set_bold()
        .set_align(FormatAlign::VerticalCenter).set_text_wrap()
        .set_border(FormatBorder::Thin).set_border_color(Color::RGB(0xCFD8DC))
        .set_background_color(Color::RGB(0xE8EAF6));

    // バー色
    let bar_fmts: std::collections::HashMap<&str, Format> = [
        ("未対応", Format::new().set_background_color(Color::RGB(0x90A4AE)).set_border(FormatBorder::Hair).set_border_color(Color::RGB(0xE0E0E0))),
        ("処理中", Format::new().set_background_color(Color::RGB(0x42A5F5)).set_border(FormatBorder::Hair).set_border_color(Color::RGB(0xE0E0E0))),
        ("処理済み", Format::new().set_background_color(Color::RGB(0xFFA726)).set_border(FormatBorder::Hair).set_border_color(Color::RGB(0xE0E0E0))),
        ("完了", Format::new().set_background_color(Color::RGB(0x66BB6A)).set_border(FormatBorder::Hair).set_border_color(Color::RGB(0xE0E0E0))),
    ].into();

    let wkend = Format::new().set_background_color(Color::RGB(0xEEEEEE))
        .set_border(FormatBorder::Hair).set_border_color(Color::RGB(0xE0E0E0));
    let empty_c = Format::new()
        .set_border(FormatBorder::Hair).set_border_color(Color::RGB(0xE0E0E0));

    // 左側列
    let info_cols: Vec<(&str, f64)> = vec![
        ("No.", 5.0), ("チケットキー", 14.0), ("タイトル", 36.0),
        ("ステータス", 10.0), ("優先度", 7.0), ("担当者", 10.0),
        ("開始日", 11.0), ("期限日", 11.0), ("進捗", 7.0),
    ];
    let info_count = info_cols.len() as u16;

    // Row 0: タイトル + 月ヘッダー
    sheet.merge_range(0, 0, 0, info_count - 1, "ガントチャート", &g_hdr)?;

    // 月グループ
    let month_groups = group_by_month(&date_list);
    let mut col = info_count;
    for (label, days) in &month_groups {
        let end_col = col + days.len() as u16 - 1;
        if col < end_col {
            sheet.merge_range(0, col, 0, end_col, label, &g_month)?;
        } else {
            sheet.write_string_with_format(0, col, label, &g_month)?;
        }
        col = end_col + 1;
    }

    // Row 1: カラムヘッダー + 日付
    for (c, (label, width)) in info_cols.iter().enumerate() {
        sheet.write_string_with_format(1, c as u16, *label, &g_hdr)?;
        sheet.set_column_width(c as u16, *width)?;
    }

    for (di, d) in date_list.iter().enumerate() {
        let c = info_count + di as u16;
        let is_today = *d == today;
        let is_weekend = d.weekday().num_days_from_monday() >= 5;
        let day_fmt = Format::new()
            .set_font_name("Yu Gothic").set_font_size(7.0).set_bold()
            .set_font_color(if is_today { Color::RGB(0xD32F2F) } else { Color::RGB(0x37474F) })
            .set_align(FormatAlign::Center).set_align(FormatAlign::VerticalCenter)
            .set_border(FormatBorder::Thin).set_border_color(Color::RGB(0xCFD8DC))
            .set_background_color(
                if is_today { Color::RGB(0xFFEBEE) }
                else if is_weekend || holiday_set.contains(d) { Color::RGB(0xEEEEEE) }
                else { Color::RGB(0xECEFF1) }
            );
        sheet.write_number_with_format(1, c, d.day() as f64, &day_fmt)?;
        sheet.set_column_width(c, 3.2)?;
    }

    // Row 2: マイルストーン行
    sheet.merge_range(2, 0, 2, info_count - 1, "マイルストーン", &g_cell)?;
    let ms_fmt = Format::new()
        .set_font_name("Yu Gothic").set_font_size(7.0).set_bold()
        .set_font_color(Color::RGB(0x7B1FA2))
        .set_align(FormatAlign::Center).set_align(FormatAlign::VerticalCenter)
        .set_background_color(Color::RGB(0xEDE7F6))
        .set_border(FormatBorder::Hair).set_border_color(Color::RGB(0xE0E0E0));
    let today_fmt = Format::new()
        .set_font_name("Yu Gothic").set_font_size(7.0).set_bold()
        .set_font_color(Color::RGB(0xD32F2F))
        .set_align(FormatAlign::Center).set_align(FormatAlign::VerticalCenter)
        .set_border(FormatBorder::Thin).set_border_color(Color::RGB(0xD32F2F));

    for (di, d) in date_list.iter().enumerate() {
        let c = info_count + di as u16;
        if milestone_map.contains_key(d) {
            sheet.write_string_with_format(2, c, "◆", &ms_fmt)?;
        } else if *d == today {
            sheet.write_string_with_format(2, c, "▼", &today_fmt)?;
        }
    }
    sheet.set_row_height(2, 16.0)?;

    // データ行
    let st_fmts: std::collections::HashMap<&str, Format> = [
        ("未対応", g_center.clone().set_background_color(Color::RGB(0xECEFF1))),
        ("処理中", g_center.clone().set_background_color(Color::RGB(0xE3F2FD))),
        ("処理済み", g_center.clone().set_background_color(Color::RGB(0xFFF8E1))),
        ("完了", g_center.clone().set_background_color(Color::RGB(0xE8F5E9))),
    ].into();

    for (idx, gantt_row) in rows.iter().enumerate() {
        let row = 3 + idx as u32;
        let t = &gantt_row.ticket;
        sheet.set_row_height(row, 22.0)?;

        sheet.write_number_with_format(row, 0, (idx + 1) as f64, &g_center)?;
        sheet.write_string_with_format(row, 1, &t.ticket_key, if !gantt_row.is_child { &g_key } else { &g_center })?;

        let title = if gantt_row.is_child { format!("  └ {}", t.title) } else { t.title.clone() };
        sheet.write_string_with_format(row, 2, &title, if !gantt_row.is_child { &parent_bg } else { &g_cell })?;

        let sf = st_fmts.get(t.status.as_str()).unwrap_or(&g_center);
        sheet.write_string_with_format(row, 3, &t.status, sf)?;
        sheet.write_string_with_format(row, 4, &t.priority, &g_center)?;
        sheet.write_string_with_format(row, 5, t.assignee_name.as_deref().unwrap_or(""), &g_center)?;

        let start_str = t.start_date.map(|d| d.format("%m/%d").to_string()).unwrap_or_default();
        let due_str = t.due_date.map(|d| d.format("%m/%d").to_string()).unwrap_or_default();
        sheet.write_string_with_format(row, 6, &start_str, &g_center)?;
        sheet.write_string_with_format(row, 7, &due_str, &g_center)?;
        sheet.write_string_with_format(row, 8, &format!("{}%", gantt_row.progress), &g_center)?;

        // バー描画
        let t_start = t.start_date.or(t.due_date);
        let t_end = t.due_date.or(t.start_date);
        let bar = bar_fmts.get(t.status.as_str());

        for (di, d) in date_list.iter().enumerate() {
            let c = info_count + di as u16;
            let in_range = t_start.is_some() && t_end.is_some()
                && t_start.unwrap() <= *d && *d <= t_end.unwrap();

            if in_range {
                if let Some(bf) = bar {
                    sheet.write_string_with_format(row, c, "", bf)?;
                }
            } else {
                let is_weekend = d.weekday().num_days_from_monday() >= 5;
                if is_weekend || holiday_set.contains(d) {
                    sheet.write_string_with_format(row, c, "", &wkend)?;
                } else {
                    sheet.write_string_with_format(row, c, "", &empty_c)?;
                }
            }
        }
    }

    sheet.set_freeze_panes(3, info_count)?;

    // 凡例シート
    let legend = workbook.add_worksheet();
    legend.set_name("凡例")?;
    legend.set_column_width(0, 20.0)?;
    legend.set_column_width(1, 10.0)?;

    let lg = Format::new().set_font_name("Yu Gothic").set_font_size(10.0);
    let items: Vec<(&str, Option<u32>)> = vec![
        ("ステータス色", None), ("未対応", Some(0x90A4AE)), ("処理中", Some(0x42A5F5)),
        ("処理済み", Some(0xFFA726)), ("完了", Some(0x66BB6A)),
        ("", None), ("記号", None),
        ("◆ マイルストーン", Some(0x7B1FA2)), ("▼ 本日", Some(0xD32F2F)),
        ("", None), ("背景色", None),
        ("週末", Some(0xEEEEEE)), ("休日", Some(0xF5F5F5)), ("期限超過", Some(0xFFCDD2)),
    ];
    for (i, (label, color)) in items.iter().enumerate() {
        legend.write_string_with_format(i as u32, 0, *label, &lg)?;
        if let Some(c) = color {
            let fill = Format::new().set_background_color(Color::RGB(*c));
            legend.write_string_with_format(i as u32, 1, "  ", &fill)?;
        }
    }

    let buf = workbook.save_to_buffer()?;
    Ok(buf)
}

/// 日付リストを月ごとにグループ化
fn group_by_month(dates: &[NaiveDate]) -> Vec<(String, Vec<NaiveDate>)> {
    let mut groups: Vec<(String, Vec<NaiveDate>)> = Vec::new();
    let mut current_key = String::new();

    for d in dates {
        let key = format!("{}年{:02}月", d.year(), d.month());
        if key != current_key {
            groups.push((key.clone(), vec![*d]));
            current_key = key;
        } else if let Some(last) = groups.last_mut() {
            last.1.push(*d);
        }
    }

    groups
}
