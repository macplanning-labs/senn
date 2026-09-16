/**
 * TeamDashboard.tsx — Team ダッシュボード（薄い集計）
 *
 * Team スコープのチケット集計を表示。
 * 所属 Team の勢い（TODO/進行中/完了数）が見える。
 */

import { useParams, useNavigate } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { apiClient } from '@/shared/api/client';
import './Dashboard.css';

interface TeamSummary {
  team_id: number;
  team_slug: string;
  tickets: {
    open: number;
    in_progress: number;
    resolved: number;
    closed: number;
    total: number;
  };
}

export function TeamDashboard() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { teamSlug } = useParams<{ teamSlug: string }>();

  const { data: summary, isLoading, error } = useQuery<TeamSummary>({
    queryKey: ['team-dashboard', teamSlug],
    queryFn: async () => {
      const res = await apiClient.get<TeamSummary>(`/dashboard/team/${teamSlug}/summary/`);
      return res.data;
    },
    enabled: !!teamSlug,
  });

  if (isLoading) {
    return (
      <div className="dashboard" data-testid="team-dashboard">
        <div className="dashboard__loading">
          <div className="dashboard__spinner" />
        </div>
      </div>
    );
  }

  if (error || !summary) {
    return (
      <div className="dashboard" data-testid="team-dashboard">
        <div className="dashboard__header">
          <h1 className="dashboard__title">{teamSlug} Dashboard</h1>
        </div>
        <div style={{ padding: '2rem', textAlign: 'center', color: 'var(--color-text-tertiary)' }}>
          {error ? 'Team not found or access denied' : 'No data'}
        </div>
      </div>
    );
  }

  const stats = summary.tickets;
  const total = stats.total || 1;

  // 進捗率（TODO+進行中 / 合計）
  const inProgressCount = stats.open + stats.in_progress;
  const progressPct = total > 0 ? Math.round(((stats.resolved + stats.closed) / total) * 100) : 0;

  return (
    <div className="dashboard" data-testid="team-dashboard">
      <div className="dashboard__header">
        <div className="dashboard__header-left">
          <h1 className="dashboard__title">{teamSlug}</h1>
          <p className="dashboard__greeting">{t('dashboard.welcomeBack', { name: 'Team' })}</p>
        </div>
        {/* クイック移動リンク */}
        <div className="dashboard__go-to">
          <button
            className="dashboard__go-to-link"
            onClick={() => navigate(`/team/${teamSlug}/tickets`)}
            title={t('shortcuts.goToTickets')}
          >
            {t('teamHome.goToTickets')}
          </button>
          <button
            className="dashboard__go-to-link"
            onClick={() => navigate(`/team/${teamSlug}/projects`)}
            title={t('shortcuts.goToProjects')}
          >
            {t('teamHome.goToProjects')}
          </button>
          <button
            className="dashboard__go-to-link"
            onClick={() => navigate(`/team/${teamSlug}/settings`)}
            title={t('shortcuts.goToSettings')}
          >
            {t('teamHome.goToSettings')}
          </button>
        </div>
      </div>

      {/* 簡潔なサマリー */}
      <div className="dashboard__grid" style={{ gap: '1.5rem' }}>
        {/* カード1: ステータス分布 */}
        <div className="widget-card">
          <div className="widget-card__header">
            <span className="widget-card__icon">🎯</span>
            <span className="widget-card__title">{t('dashboard.ticketOverview')}</span>
          </div>
          <div className="stats-cards">
            <div className="stats-cards__item">
              <div className="stats-cards__value" style={{ color: 'hsl(210, 70%, 55%)' }}>
                {stats.open}
              </div>
              <div className="stats-cards__label">Open</div>
            </div>
            <div className="stats-cards__item">
              <div className="stats-cards__value" style={{ color: 'hsl(45, 80%, 55%)' }}>
                {stats.in_progress}
              </div>
              <div className="stats-cards__label">In Progress</div>
            </div>
            <div className="stats-cards__item">
              <div className="stats-cards__value" style={{ color: 'hsl(150, 60%, 50%)' }}>
                {stats.resolved}
              </div>
              <div className="stats-cards__label">Resolved</div>
            </div>
            <div className="stats-cards__item">
              <div className="stats-cards__value" style={{ color: 'hsl(220, 10%, 50%)' }}>
                {stats.closed}
              </div>
              <div className="stats-cards__label">Closed</div>
            </div>
          </div>
        </div>

        {/* カード2: 進捗バー */}
        <div className="widget-card">
          <div className="widget-card__header">
            <span className="widget-card__icon">📊</span>
            <span className="widget-card__title">Progress</span>
          </div>
          <div style={{ padding: '1.5rem' }}>
            <div style={{ marginBottom: '1rem' }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: '0.5rem', fontSize: '0.875rem' }}>
                <span>Completion</span>
                <span style={{ fontWeight: 600 }}>{progressPct}%</span>
              </div>
              <div style={{
                width: '100%',
                height: '8px',
                backgroundColor: 'var(--color-bg-tertiary)',
                borderRadius: '4px',
                overflow: 'hidden',
              }}>
                <div style={{
                  width: `${progressPct}%`,
                  height: '100%',
                  backgroundColor: 'hsl(150, 60%, 50%)',
                  transition: 'width 0.3s ease',
                }} />
              </div>
            </div>
            <div style={{ fontSize: '0.8125rem', color: 'var(--color-text-tertiary)' }}>
              {stats.resolved + stats.closed} of {stats.total} completed
            </div>
          </div>
        </div>

        {/* カード3: 未処理 */}
        <div className="widget-card">
          <div className="widget-card__header">
            <span className="widget-card__icon">⚠️</span>
            <span className="widget-card__title">Work Pending</span>
          </div>
          <div style={{ padding: '1.5rem' }}>
            <div style={{ fontSize: '2.5rem', fontWeight: 700, color: 'var(--color-text-primary)', marginBottom: '0.5rem' }}>
              {inProgressCount}
            </div>
            <div style={{ fontSize: '0.8125rem', color: 'var(--color-text-tertiary)' }}>
              Tickets awaiting action
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
