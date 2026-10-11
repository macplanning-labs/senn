/**
 * insights/format.ts - Insights 画面の純粋な整形関数
 *
 * 画面に出る文言は作らず、i18n のキーと値(I18nDescriptor)を返す。
 * 表示側で t(key, values) を呼ぶ。
 */

import type { InsightsSegment } from './api';

/** i18n のキーと値。純粋関数はこれを返し、文言は表示側で t() に通す */
export interface I18nDescriptor {
  key: string;
  values?: Record<string, number>;
}

/**
 * 秒を、読みやすい所要時間にする。
 * 例: 0 → 0分 / 59秒 → 1分未満 / 90分 → 1時間 30分 / 1日 3時間
 */
export function formatDuration(seconds: number | null | undefined): I18nDescriptor | null {
  if (seconds == null || !Number.isFinite(seconds) || seconds < 0) return null;
  if (seconds === 0) return { key: 'insights.duration.minutes', values: { minutes: 0 } };
  if (seconds < 60) return { key: 'insights.duration.lessThanMinute' };

  const totalMinutes = Math.floor(seconds / 60);
  const days = Math.floor(totalMinutes / (60 * 24));
  const hours = Math.floor((totalMinutes % (60 * 24)) / 60);
  const minutes = totalMinutes % 60;

  if (days > 0) {
    return hours > 0
      ? { key: 'insights.duration.daysHours', values: { days, hours } }
      : { key: 'insights.duration.days', values: { days } };
  }
  if (hours > 0) {
    return minutes > 0
      ? { key: 'insights.duration.hoursMinutes', values: { hours, minutes } }
      : { key: 'insights.duration.hours', values: { hours } };
  }
  return { key: 'insights.duration.minutes', values: { minutes } };
}

/**
 * トークン数を、短い表記にする。例: 950 → 950 / 12345 → 12.3k → 12k / 1200000 → 1.2M
 */
export function formatTokens(n: number | null | undefined): string | null {
  if (n == null || !Number.isFinite(n)) return null;
  const value = Math.max(0, Math.round(n));
  if (value < 1000) return String(value);

  const k = value / 1000;
  const kText = k >= 10 ? Math.round(k) : Math.round(k * 10) / 10;
  if (kText < 1000) return `${kText}k`;

  const m = value / 1_000_000;
  const mText = m >= 10 ? Math.round(m) : Math.round(m * 10) / 10;
  return `${mText}M`;
}

/** 割合(%)。分母が 0 や null のときは null。小数第 1 位まで */
export function percent(part: number | null | undefined, whole: number | null | undefined): number | null {
  if (part == null || whole == null || whole <= 0) return null;
  return Math.round((part / whole) * 1000) / 10;
}

/** API 料金換算の金額。例: 1.5 → $1.50 */
export function formatUsd(amount: number | null | undefined): string | null {
  if (amount == null || !Number.isFinite(amount)) return null;
  return `$${amount.toFixed(2)}`;
}

/** 日付(YYYY-MM-DD)を、グラフの横軸用の短い表記にする。例: 2026-10-05 → 10/5 */
export function formatPeriodLabel(periodStart: string): string {
  const parts = periodStart.split('-');
  return `${Number(parts[1])}/${Number(parts[2])}`;
}

/** ローカル時刻の Date を YYYY-MM-DD にする */
export function toIsoDate(date: Date): string {
  const y = date.getFullYear();
  const m = String(date.getMonth() + 1).padStart(2, '0');
  const d = String(date.getDate()).padStart(2, '0');
  return `${y}-${m}-${d}`;
}

/** YYYY-MM-DD に日数を足す(引くときは負の数) */
export function shiftIsoDate(iso: string, days: number): string {
  const [y = 1970, m = 1, d = 1] = iso.split('-').map(Number);
  return toIsoDate(new Date(y, m - 1, d + days));
}

/** 表示する区分の順番 */
export const SEGMENT_ORDER: InsightsSegment[] = ['human', 'ai', 'unassigned'];

export interface PivotRow<T> {
  periodStart: string;
  cells: Record<InsightsSegment, T>;
}

/**
 * 系列を「期間 × 区分」の表に整える。
 * 無い区分は empty() で埋め、periods に渡した期間も(データが無くても)行に入れる。
 * 期間は昇順。
 */
export function pivotSeries<T extends { periodStart: string; segment: InsightsSegment }>(
  series: readonly T[],
  empty: (periodStart: string, segment: InsightsSegment) => T,
  periods: readonly string[] = [],
): PivotRow<T>[] {
  const periodSet = new Set<string>(periods);
  const bySegment = new Map<string, Partial<Record<InsightsSegment, T>>>();
  for (const row of series) {
    periodSet.add(row.periodStart);
    const cells = bySegment.get(row.periodStart) ?? {};
    cells[row.segment] = row;
    bySegment.set(row.periodStart, cells);
  }
  return [...periodSet].sort().map((periodStart) => {
    const cells = bySegment.get(periodStart) ?? {};
    return {
      periodStart,
      cells: {
        human: cells.human ?? empty(periodStart, 'human'),
        ai: cells.ai ?? empty(periodStart, 'ai'),
        unassigned: cells.unassigned ?? empty(periodStart, 'unassigned'),
      },
    };
  });
}
