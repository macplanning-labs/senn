import { describe, it, expect } from 'vitest';
import { prBadges, changeStats, reviewersSummary } from './gitActivity';

const base = {
  eventType: 'pull_request',
  prState: 'open',
  isDraft: false,
  reviewState: 'none',
  ciState: 'none',
} as const;

describe('prBadges', () => {
  it('コミットには出さない', () => {
    expect(prBadges({ ...base, eventType: 'commit' })).toEqual([]);
  });

  it('値が無い(古い)PR にはバッジを出さない', () => {
    expect(prBadges(base)).toEqual([]);
  });

  it('Draft・承認・CI 成功', () => {
    const b = prBadges({ ...base, isDraft: true, reviewState: 'approved', ciState: 'success' });
    expect(b.map((x) => x.i18nKey)).toEqual([
      'gitActivity.draft',
      'gitActivity.approved',
      'gitActivity.ciSuccess',
    ]);
    expect(b.map((x) => x.tone)).toEqual(['neutral', 'good', 'good']);
  });

  it('変更依頼・CI 失敗は bad、実行中は warn', () => {
    expect(prBadges({ ...base, reviewState: 'changes_requested' })[0]).toMatchObject({
      tone: 'bad',
      i18nKey: 'gitActivity.changesRequested',
    });
    expect(prBadges({ ...base, ciState: 'failure' })[0]).toMatchObject({
      tone: 'bad',
      i18nKey: 'gitActivity.ciFailure',
    });
    expect(prBadges({ ...base, ciState: 'pending' })[0]).toMatchObject({
      tone: 'warn',
      i18nKey: 'gitActivity.ciPending',
    });
  });

  it('マージ済み・閉じた PR には Draft を出さない', () => {
    expect(prBadges({ ...base, isDraft: true, prState: 'merged' })).toEqual([]);
    expect(prBadges({ ...base, isDraft: true, prState: 'closed' })).toEqual([]);
  });
});

describe('changeStats', () => {
  it('追加・削除が無ければ null', () => {
    expect(changeStats({ additions: null, deletions: null, changedFiles: null })).toBeNull();
  });
  it('片方だけでも出す(無い方は 0)', () => {
    expect(changeStats({ additions: 5, deletions: null, changedFiles: 2 })).toEqual({
      additions: 5,
      deletions: 0,
      changedFiles: 2,
    });
  });
});

describe('reviewersSummary', () => {
  it('いなければ null', () => {
    expect(reviewersSummary([])).toBeNull();
    expect(reviewersSummary(undefined)).toBeNull();
  });
  it('3 人まで出し、残りは人数', () => {
    expect(reviewersSummary(['a', 'b'])).toEqual({ shown: ['a', 'b'], others: 0 });
    expect(reviewersSummary(['a', 'b', 'c', 'd', 'e'])).toEqual({
      shown: ['a', 'b', 'c'],
      others: 2,
    });
  });
});
