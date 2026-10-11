/**
 * gitActivity.ts — チケットの「GIT ACTIVITY」に出す PR の詳細(Draft・レビュー結果・CI・レビュアー・変更量)の整形。
 * 画面から切り離した純粋関数(テストしやすいように)。設計: docs/design/詳細設計_GitHub連携Phase2.md §5.3
 */
import type { GitEvent } from '@/shared/api/types';

export type BadgeTone = 'neutral' | 'good' | 'bad' | 'warn';

export interface PrBadge {
  kind: 'draft' | 'review' | 'ci';
  tone: BadgeTone;
  /** i18n のキー(gitActivity.*) */
  i18nKey: string;
}

/** PR の詳細のバッジ。値が無い(古い PR など)ものは出さない。マージ済み・閉じた PR には、Draft を出さない。 */
export function prBadges(
  event: Pick<GitEvent, 'eventType' | 'prState' | 'isDraft' | 'reviewState' | 'ciState'>,
): PrBadge[] {
  if (event.eventType !== 'pull_request') return [];
  const badges: PrBadge[] = [];
  if (event.isDraft && event.prState === 'open') {
    badges.push({ kind: 'draft', tone: 'neutral', i18nKey: 'gitActivity.draft' });
  }
  if (event.reviewState === 'approved') {
    badges.push({ kind: 'review', tone: 'good', i18nKey: 'gitActivity.approved' });
  } else if (event.reviewState === 'changes_requested') {
    badges.push({ kind: 'review', tone: 'bad', i18nKey: 'gitActivity.changesRequested' });
  }
  if (event.ciState === 'success') {
    badges.push({ kind: 'ci', tone: 'good', i18nKey: 'gitActivity.ciSuccess' });
  } else if (event.ciState === 'failure') {
    badges.push({ kind: 'ci', tone: 'bad', i18nKey: 'gitActivity.ciFailure' });
  } else if (event.ciState === 'pending') {
    badges.push({ kind: 'ci', tone: 'warn', i18nKey: 'gitActivity.ciPending' });
  }
  return badges;
}

export interface ChangeStats {
  additions: number;
  deletions: number;
  changedFiles: number | null;
}

/** 変更量。追加・削除のどちらも無ければ null(古い PR は出さない)。 */
export function changeStats(
  event: Pick<GitEvent, 'additions' | 'deletions' | 'changedFiles'>,
): ChangeStats | null {
  if (event.additions == null && event.deletions == null) return null;
  return {
    additions: event.additions ?? 0,
    deletions: event.deletions ?? 0,
    changedFiles: event.changedFiles ?? null,
  };
}

/** レビュアーの表示: 先頭の `max` 人と、残りの人数。いなければ null。 */
export function reviewersSummary(
  reviewers: string[] | undefined,
  max = 3,
): { shown: string[]; others: number } | null {
  if (!reviewers || reviewers.length === 0) return null;
  return { shown: reviewers.slice(0, max), others: Math.max(0, reviewers.length - max) };
}
