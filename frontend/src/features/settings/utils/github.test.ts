import { describe, it, expect, beforeEach } from 'vitest';
import {
  formatRelativeTime,
  calculateRelativeTime,
  filterRepositories,
  getDeliveryStatusColor,
  shouldWarnNotMember,
  type RelativeTime,
} from './github';

describe('GitHub Utils', () => {
  describe('calculateRelativeTime', () => {
    it('returns seconds unit for recent timestamps', () => {
      const now = new Date();
      const secondsAgo = new Date(now.getTime() - 30 * 1000);
      const result = calculateRelativeTime(secondsAgo.toISOString());
      expect(result.unit).toBe('seconds');
      expect(result.value).toBe(30);
    });

    it('returns minutes unit for timestamps < 1 hour ago', () => {
      const now = new Date();
      const minutesAgo = new Date(now.getTime() - 5 * 60 * 1000);
      const result = calculateRelativeTime(minutesAgo.toISOString());
      expect(result.unit).toBe('minutes');
      expect(result.value).toBe(5);
    });

    it('returns hours unit for timestamps < 1 day ago', () => {
      const now = new Date();
      const hoursAgo = new Date(now.getTime() - 3 * 60 * 60 * 1000);
      const result = calculateRelativeTime(hoursAgo.toISOString());
      expect(result.unit).toBe('hours');
      expect(result.value).toBe(3);
    });

    it('returns days unit for timestamps < 30 days ago', () => {
      const now = new Date();
      const daysAgo = new Date(now.getTime() - 7 * 24 * 60 * 60 * 1000);
      const result = calculateRelativeTime(daysAgo.toISOString());
      expect(result.unit).toBe('days');
      expect(result.value).toBe(7);
    });

    it('returns months unit for timestamps >= 30 days ago', () => {
      const now = new Date();
      const monthsAgo = new Date(now.getTime() - 60 * 24 * 60 * 60 * 1000);
      const result = calculateRelativeTime(monthsAgo.toISOString());
      expect(result.unit).toBe('months');
      expect(result.value).toBeGreaterThan(0);
    });
  });

  describe('formatRelativeTime', () => {
    it('returns language-agnostic format', () => {
      const now = new Date();
      const minutesAgo = new Date(now.getTime() - 5 * 60 * 1000);
      const result = formatRelativeTime(minutesAgo.toISOString());
      expect(result).toBe('5m');
    });

    it('uses unit abbreviations', () => {
      const now = new Date();
      const hoursAgo = new Date(now.getTime() - 3 * 60 * 60 * 1000);
      const result = formatRelativeTime(hoursAgo.toISOString());
      expect(result).toBe('3h');
    });
  });

  describe('filterRepositories', () => {
    const repos = [
      { fullName: 'org/repo1' },
      { fullName: 'org/repo2' },
      { fullName: 'another/project' },
    ];

    it('空のクエリで全リポジトリを返す', () => {
      const result = filterRepositories(repos, '');
      expect(result).toEqual(repos);
    });

    it('クエリで絞り込む', () => {
      const result = filterRepositories(repos, 'repo');
      expect(result).toHaveLength(2);
      expect(result).toEqual([
        { fullName: 'org/repo1' },
        { fullName: 'org/repo2' },
      ]);
    });

    it('大文字小文字を区別しない', () => {
      const result = filterRepositories(repos, 'REPO');
      expect(result).toHaveLength(2);
    });

    it('マッチなしで空配列を返す', () => {
      const result = filterRepositories(repos, 'nonexistent');
      expect(result).toHaveLength(0);
    });

    it('空白を無視する', () => {
      const result = filterRepositories(repos, '  ');
      expect(result).toEqual(repos);
    });
  });

  describe('getDeliveryStatusColor', () => {
    it('ok は success を返す', () => {
      expect(getDeliveryStatusColor('ok')).toBe('success');
    });

    it('no_ticket は warning を返す', () => {
      expect(getDeliveryStatusColor('no_ticket')).toBe('warning');
    });

    it('ignored は warning を返す', () => {
      expect(getDeliveryStatusColor('ignored')).toBe('warning');
    });

    it('error は error を返す', () => {
      expect(getDeliveryStatusColor('error')).toBe('error');
    });

    it('null は neutral を返す', () => {
      expect(getDeliveryStatusColor(null)).toBe('neutral');
    });

    it('undefined は neutral を返す', () => {
      expect(getDeliveryStatusColor(undefined)).toBe('neutral');
    });
  });
});

describe('shouldWarnNotMember', () => {
  it('メンバーではない(false)ときだけ案内を出す', () => {
    expect(shouldWarnNotMember({ viewerIsMember: false })).toBe(true);
    expect(shouldWarnNotMember({ viewerIsMember: true })).toBe(false);
  });
  it('範囲なし・プロジェクト単位(null)や未指定・状態なしでは出さない', () => {
    expect(shouldWarnNotMember({ viewerIsMember: null })).toBe(false);
    expect(shouldWarnNotMember({})).toBe(false);
    expect(shouldWarnNotMember(undefined)).toBe(false);
  });
});
