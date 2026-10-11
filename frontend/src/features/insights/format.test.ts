/**
 * format.test.ts - Insights の純粋な整形関数のテスト
 */

import { describe, it, expect } from 'vitest';
import { formatDuration, formatTokens, formatUsd, percent, pivotSeries, shiftIsoDate, toIsoDate } from './format';

describe('formatDuration', () => {
  it('returns null when there is no value', () => {
    expect(formatDuration(null)).toBeNull();
    expect(formatDuration(undefined)).toBeNull();
    expect(formatDuration(-1)).toBeNull();
  });

  it('shows 0 minutes for zero seconds', () => {
    expect(formatDuration(0)).toEqual({ key: 'insights.duration.minutes', values: { minutes: 0 } });
  });

  it('shows less than a minute for 59 seconds', () => {
    expect(formatDuration(59)).toEqual({ key: 'insights.duration.lessThanMinute' });
  });

  it('shows minutes at exactly one minute', () => {
    expect(formatDuration(60)).toEqual({ key: 'insights.duration.minutes', values: { minutes: 1 } });
  });

  it('shows hours and minutes for 90 minutes', () => {
    expect(formatDuration(90 * 60)).toEqual({ key: 'insights.duration.hoursMinutes', values: { hours: 1, minutes: 30 } });
  });

  it('shows hours only when the minutes are zero', () => {
    expect(formatDuration(2 * 3600)).toEqual({ key: 'insights.duration.hours', values: { hours: 2 } });
  });

  it('shows days and hours from one day', () => {
    expect(formatDuration(86400 + 3 * 3600)).toEqual({ key: 'insights.duration.daysHours', values: { days: 1, hours: 3 } });
  });

  it('shows days only when the hours are zero', () => {
    expect(formatDuration(2 * 86400)).toEqual({ key: 'insights.duration.days', values: { days: 2 } });
  });
});

describe('formatTokens', () => {
  it('returns null when there is no value', () => {
    expect(formatTokens(null)).toBeNull();
  });

  it('keeps numbers under 1000 as they are', () => {
    expect(formatTokens(0)).toBe('0');
    expect(formatTokens(999)).toBe('999');
  });

  it('uses k from 1000', () => {
    expect(formatTokens(1000)).toBe('1k');
    expect(formatTokens(1200)).toBe('1.2k');
    expect(formatTokens(12345)).toBe('12k');
  });

  it('rolls over to M at the boundary', () => {
    expect(formatTokens(999_400)).toBe('999k');
    expect(formatTokens(999_950)).toBe('1M');
    expect(formatTokens(1_200_000)).toBe('1.2M');
  });
});

describe('percent and formatUsd', () => {
  it('returns the share to one decimal place', () => {
    expect(percent(1, 3)).toBe(33.3);
    expect(percent(50, 100)).toBe(50);
  });

  it('returns null when the whole is zero or missing', () => {
    expect(percent(1, 0)).toBeNull();
    expect(percent(null, 10)).toBeNull();
  });

  it('formats dollars with two decimals', () => {
    expect(formatUsd(1.5)).toBe('$1.50');
    expect(formatUsd(null)).toBeNull();
  });
});

describe('shiftIsoDate and toIsoDate', () => {
  it('moves dates across month boundaries', () => {
    expect(shiftIsoDate('2026-03-01', -1)).toBe('2026-02-28');
    expect(shiftIsoDate('2026-10-08', -90)).toBe('2026-07-10');
  });

  it('formats a local date as YYYY-MM-DD', () => {
    expect(toIsoDate(new Date(2026, 0, 5))).toBe('2026-01-05');
  });
});

describe('pivotSeries', () => {
  const empty = (periodStart: string, segment: 'human' | 'ai' | 'unassigned') => ({ periodStart, segment, count: 0 });

  it('fills missing segments with the empty value', () => {
    const rows = pivotSeries(
      [{ periodStart: '2026-10-05', segment: 'ai' as const, count: 3 }],
      empty,
    );
    expect(rows).toHaveLength(1);
    expect(rows[0].cells.ai.count).toBe(3);
    expect(rows[0].cells.human.count).toBe(0);
    expect(rows[0].cells.unassigned.count).toBe(0);
  });

  it('adds the given periods even when they have no data, in ascending order', () => {
    const rows = pivotSeries(
      [{ periodStart: '2026-10-12', segment: 'human' as const, count: 2 }],
      empty,
      ['2026-10-05', '2026-10-12', '2026-09-28'],
    );
    expect(rows.map((r) => r.periodStart)).toEqual(['2026-09-28', '2026-10-05', '2026-10-12']);
    expect(rows[0].cells.human.count).toBe(0);
    expect(rows[2].cells.human.count).toBe(2);
  });

  it('returns no rows for empty input', () => {
    expect(pivotSeries([], empty)).toEqual([]);
  });
});
