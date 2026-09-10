/**
 * CycleSummaryBar.tsx — Cycle詳細画面のミニ進捗サマリー（1行）
 *
 * 「未完了/完了」の件数をクリックすると、下のチケット一覧(TicketTable)を
 * status_in クエリパラメータで即座に絞り込む。完了判定はバックエンドの
 * 集計(cycle_repo.rs:451 の `status IN ('closed','resolved')`)に合わせる。
 */
import { useSearchParams } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import type { CycleProgress } from '@/shared/api/types';
import './CycleSummaryBar.css';

const INCOMPLETE_STATUSES = 'backlog,open,in_progress';
const COMPLETE_STATUSES = 'resolved,closed';

export function CycleSummaryBar({ progress }: { progress: CycleProgress | undefined }) {
  const { t } = useTranslation();
  const [searchParams, setSearchParams] = useSearchParams();
  const activeStatusIn = searchParams.get('status_in') ?? '';

  const toggleFilter = (value: string) => {
    setSearchParams((prev) => {
      const next = new URLSearchParams(prev);
      if (activeStatusIn === value) {
        next.delete('status_in');
      } else {
        next.set('status_in', value);
        next.delete('status');
      }
      return next;
    }, { replace: true });
  };

  const completionPct = progress?.completionRate ?? 0;
  const ticketCount = progress?.ticketCount ?? 0;
  const completedCount = progress?.completedCount ?? 0;
  const incompleteCount = ticketCount - completedCount;
  const circumference = 2 * Math.PI * 16;
  const filled = (completionPct / 100) * circumference;

  return (
    <div className="cycle-summary-bar">
      <svg viewBox="0 0 40 40" className="cycle-summary-bar__ring" aria-hidden="true">
        <circle cx="20" cy="20" r="16" fill="none" stroke="var(--color-bg-tertiary)" strokeWidth="4" />
        <circle
          cx="20" cy="20" r="16" fill="none"
          stroke="var(--color-accent)" strokeWidth="4"
          strokeDasharray={`${filled} ${circumference - filled}`}
          strokeDashoffset={circumference * 0.25}
          strokeLinecap="round"
        />
      </svg>
      <span className="cycle-summary-bar__pct">{Math.round(completionPct)}%</span>

      <button
        type="button"
        className={`cycle-summary-bar__stat ${activeStatusIn === INCOMPLETE_STATUSES ? 'cycle-summary-bar__stat--active' : ''}`}
        onClick={() => toggleFilter(INCOMPLETE_STATUSES)}
        data-testid="cycle-summary-incomplete"
      >
        <strong>{incompleteCount}</strong> {t('cycle.incompleteCount')}
      </button>

      <button
        type="button"
        className={`cycle-summary-bar__stat ${activeStatusIn === COMPLETE_STATUSES ? 'cycle-summary-bar__stat--active' : ''}`}
        onClick={() => toggleFilter(COMPLETE_STATUSES)}
        data-testid="cycle-summary-completed"
      >
        <strong>{completedCount}</strong> {t('cycle.completedCount')}
      </button>

      <span className="cycle-summary-bar__points">
        {progress?.completedPoints ?? 0} / {progress?.totalPoints ?? 0} pt
      </span>

      {progress && (progress.initialPoints ?? 0) > 0 && (
        <span className="cycle-summary-bar__scope" data-testid="cycle-scope-badge">
          {t('cycle.initialScope')}: {progress.initialPoints ?? 0}pt
          <span className="cycle-summary-bar__scope-arrow">→</span>
          {progress.totalPoints ?? 0}pt
          {(progress.scopeChange ?? 0) !== 0 && (
            <span className={`cycle-summary-bar__scope-change ${(progress.scopeChange ?? 0) > 0 ? 'cycle-summary-bar__scope-change--added' : 'cycle-summary-bar__scope-change--removed'}`}>
              {(progress.scopeChange ?? 0) > 0 ? '+' : ''}{progress.scopeChange}
            </span>
          )}
        </span>
      )}
    </div>
  );
}
