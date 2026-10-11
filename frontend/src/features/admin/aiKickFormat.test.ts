/**
 * aiKickFormat.test.ts
 */

import { describe, it, expect } from 'vitest';
import {
  formatTokens,
  usageBarClass,
  validateRunnerName,
  validateRepo,
  validateLimitsDraft,
} from './aiKickFormat';

describe('formatTokens', () => {
  it('formats millions', () => {
    expect(formatTokens(1_200_000)).toBe('1.2M');
    expect(formatTokens(1_000_000)).toBe('1M');
  });

  it('formats thousands', () => {
    expect(formatTokens(500_000)).toBe('500K');
    expect(formatTokens(1_000)).toBe('1K');
  });

  it('handles small numbers', () => {
    expect(formatTokens(500)).toBe('500');
    expect(formatTokens(0)).toBe('0');
  });

  it('handles null/undefined', () => {
    expect(formatTokens(null)).toBe('—');
    expect(formatTokens(undefined)).toBe('—');
  });
});

describe('usageBarClass', () => {
  it('returns error for >= 100%', () => {
    expect(usageBarClass(100)).toBe('ai-kick__usage-bar--error');
    expect(usageBarClass(150)).toBe('ai-kick__usage-bar--error');
  });

  it('returns warning for >= 80%', () => {
    expect(usageBarClass(80)).toBe('ai-kick__usage-bar--warning');
    expect(usageBarClass(99)).toBe('ai-kick__usage-bar--warning');
  });

  it('returns caution for >= 50%', () => {
    expect(usageBarClass(50)).toBe('ai-kick__usage-bar--caution');
    expect(usageBarClass(79)).toBe('ai-kick__usage-bar--caution');
  });

  it('returns default for < 50%', () => {
    expect(usageBarClass(0)).toBe('ai-kick__usage-bar--default');
    expect(usageBarClass(49)).toBe('ai-kick__usage-bar--default');
  });

  it('handles null/undefined', () => {
    expect(usageBarClass(null)).toBe('ai-kick__usage-bar--default');
    expect(usageBarClass(undefined)).toBe('ai-kick__usage-bar--default');
  });
});

describe('validateRunnerName', () => {
  it('accepts valid names', () => {
    expect(validateRunnerName('my-runner')).toBeNull();
    expect(validateRunnerName('runner_123')).toBeNull();
    expect(validateRunnerName('runner.prod')).toBeNull();
    expect(validateRunnerName('a')).toBeNull();
  });

  it('rejects empty', () => {
    expect(validateRunnerName('')).toBeTruthy();
    expect(validateRunnerName('   ')).toBeTruthy();
  });

  it('rejects too long', () => {
    expect(validateRunnerName('a'.repeat(81))).toBeTruthy();
  });

  it('rejects invalid characters', () => {
    expect(validateRunnerName('runner@prod')).toBeTruthy();
    expect(validateRunnerName('runner (1)')).toBeTruthy();
  });
});

describe('validateRepo', () => {
  it('accepts valid repos', () => {
    expect(validateRepo('owner/name')).toBeNull();
    expect(validateRepo('user_name/repo-name')).toBeNull();
    expect(validateRepo('user.name/repo.name')).toBeNull();
  });

  it('rejects empty', () => {
    expect(validateRepo('')).toBeTruthy();
  });

  it('rejects invalid format', () => {
    expect(validateRepo('owner')).toBeTruthy();
    expect(validateRepo('owner/name/extra')).toBeTruthy();
    expect(validateRepo('owner@name/repo')).toBeTruthy();
  });
});

describe('validateLimitsDraft', () => {
  it('accepts valid limits', () => {
    const errors = validateLimitsDraft({
      maxTurns: 10,
      maxMinutes: 60,
      dailyBudgetSystem: 1_000_000,
      dailyBudgetProject: 500_000,
    });
    expect(errors).toEqual([]);
  });

  it('rejects non-integers', () => {
    const errors = validateLimitsDraft({ maxTurns: 10.5 });
    expect(errors.some((e) => e.includes('整数'))).toBe(true);
  });

  it('rejects negative values', () => {
    const errors = validateLimitsDraft({ maxTurns: -5 });
    expect(errors.some((e) => e.includes('0 以上'))).toBe(true);
  });

  it('validates project > system daily budget', () => {
    const errors = validateLimitsDraft({
      dailyBudgetSystem: 500_000,
      dailyBudgetProject: 1_000_000,
    });
    expect(errors.some((e) => e.includes('プロジェクト日次'))).toBe(true);
  });

  it('validates max tokens > daily budget', () => {
    const errors = validateLimitsDraft({
      maxWeightedTokens: 1_000_000,
      dailyBudgetSystem: 500_000,
    });
    expect(errors.some((e) => e.includes('1 実行の最大トークン'))).toBe(true);
  });

  it('validates per-user-day >= per-user-hour', () => {
    const errors = validateLimitsDraft({
      perUserHour: 100,
      perUserDay: 50,
    });
    expect(errors.some((e) => e.includes('1 人あたり 1 日'))).toBe(true);
  });

  it('validates hardCeilingPercent > 100', () => {
    const errors = validateLimitsDraft({ hardCeilingPercent: 100 });
    expect(errors.some((e) => e.includes('上限の線'))).toBe(true);
  });
});
