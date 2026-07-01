/**
 * CycleDetail.tsx — サイクル詳細ページ
 *
 * 上部: 進捗サマリー（ドーナツチャート + 統計カード4枚）
 * 下部: サイクル内チケット一覧
 */
import { useParams, useNavigate } from 'react-router-dom';
import { useCycle, useCycleProgress } from '../hooks/useCycles';
import { BurndownChart } from './BurndownChart';
import './CycleDetail.css';
import { useTranslation } from 'react-i18next';

const statusLabels: Record<string, string> = {
  planned: '計画中',
  active: '進行中',
  completed: '完了',
};

export function CycleDetail() {
  const { t } = useTranslation();
  const { cycleId } = useParams<{ cycleId: string }>();
  const navigate = useNavigate();
  const { data: cycle, isLoading: cycleLoading } = useCycle(
    cycleId ? parseInt(cycleId) : undefined
  );
  const { data: progress } = useCycleProgress(
    cycleId ? parseInt(cycleId) : undefined
  );

  if (cycleLoading || !cycle) {
    return <div className="cycle-detail__loading">{t('common.loading')}</div>;
  }

  const completionPct = progress?.completionRate ?? 0;
  // SVG conic-gradient equivalent using stroke-dasharray
  const circumference = 2 * Math.PI * 40;
  const filled = (completionPct / 100) * circumference;

  return (
    <div className="cycle-detail">
      {/* ヘッダー */}
      <div className="cycle-detail__header">
        <button
          className="cycle-detail__back"
          onClick={() => navigate(-1)}
        >
          ← 戻る
        </button>
        <h1 className="cycle-detail__title">{cycle.name}</h1>
        <span className="cycle-detail__badge">
          {statusLabels[cycle.status]}
        </span>
        <span className="cycle-detail__dates">
          {cycle.startDate} — {cycle.endDate}
        </span>
      </div>

      {/* 進捗サマリー */}
      <div className="cycle-detail__summary">
        {/* ドーナツチャート */}
        <div className="cycle-detail__donut">
          <svg viewBox="0 0 100 100" className="cycle-detail__donut-svg">
            <circle
              cx="50" cy="50" r="40"
              fill="none"
              stroke="var(--color-bg-tertiary)"
              strokeWidth="8"
            />
            <circle
              cx="50" cy="50" r="40"
              fill="none"
              stroke="var(--color-accent)"
              strokeWidth="8"
              strokeDasharray={`${filled} ${circumference - filled}`}
              strokeDashoffset={circumference * 0.25}
              strokeLinecap="round"
              style={{ transition: 'stroke-dasharray 0.5s ease' }}
            />
            <text
              x="50" y="50"
              textAnchor="middle"
              dominantBaseline="central"
              fill="var(--color-text-primary)"
              fontSize="16"
              fontWeight="700"
            >
              {Math.round(completionPct)}%
            </text>
          </svg>
        </div>

        {/* 統計カード */}
        <div className="cycle-detail__stats">
          <div className="cycle-detail__stat-card">
            <span className="cycle-detail__stat-value">
              {progress?.ticketCount ?? 0}
            </span>
            <span className="cycle-detail__stat-label">チケット</span>
          </div>
          <div className="cycle-detail__stat-card">
            <span className="cycle-detail__stat-value cycle-detail__stat-value--accent">
              {progress?.completedCount ?? 0}
            </span>
            <span className="cycle-detail__stat-label">完了</span>
          </div>
          <div className="cycle-detail__stat-card">
            <span className="cycle-detail__stat-value">
              {progress?.totalPoints ?? 0}
            </span>
            <span className="cycle-detail__stat-label">総ポイント</span>
          </div>
          <div className="cycle-detail__stat-card">
            <span className="cycle-detail__stat-value cycle-detail__stat-value--accent">
              {progress?.completedPoints ?? 0}
            </span>
            <span className="cycle-detail__stat-label">完了ポイント</span>
          </div>
        </div>
      </div>

      {/* バーンダウンチャート */}
      {cycleId && <BurndownChart cycleId={parseInt(cycleId)} />}

      {/* チケット一覧への誘導 */}
      <div className="cycle-detail__tickets-hint">
        <p>{t('cycle.ticketHint')}</p>
      </div>
    </div>
  );
}
