/**
 * WorkloadReportPage.tsx — 稼働レポートページ
 *
 * タイムエントリを集計・可視化するレポートページ。
 * - 日別の作業時間棒グラフ（CSS棒グラフ）
 * - ユーザー別・プロジェクト別のランキング
 * - 期間切替（Week / Month / Year）
 */

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useQuery } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import './WorkloadReportPage.css';

interface WorkloadData {
  period: string;
  startDate: string;
  endDate: string;
  summary: {
    totalMinutes: number;
    totalHours: number;
    daysWithWork: number;
    avgMinutesPerDay: number;
  };
  daily: { date: string; total_minutes: number; entry_count: number }[];
  byUser: { user_id: number; username: string; display_name: string; total_minutes: number; entry_count: number }[];
  byProject: { project_id: number; project_name: string; project_prefix: string; total_minutes: number; entry_count: number }[];
}

type Period = 'week' | 'month' | 'year';

function formatMinutes(min: number): string {
  const h = Math.floor(min / 60);
  const m = min % 60;
  if (h === 0) return `${m}m`;
  return m > 0 ? `${h}h ${m}m` : `${h}h`;
}

export function WorkloadReportPage() {
  const { t } = useTranslation();
  const [period, setPeriod] = useState<Period>('week');

  const { data, isLoading } = useQuery<WorkloadData>({
    queryKey: ['workload-report', period],
    queryFn: async () => (await apiClient.get(`/reports/workload/?period=${period}`)).data,
  });

  const maxDaily = data ? Math.max(...data.daily.map(d => d.total_minutes), 1) : 1;

  return (
    <div className="workload-report" data-testid="workload-report-page">
      {/* ヘッダー */}
      <div className="workload-report__header">
        <h1 className="workload-report__title">📊 {t('report.workload')}</h1>
        <div className="workload-report__period-tabs">
          {(['week', 'month', 'year'] as Period[]).map(p => (
            <button
              key={p}
              className={`workload-report__tab ${period === p ? 'workload-report__tab--active' : ''}`}
              onClick={() => setPeriod(p)}
            >
              {t(`report.${p}`)}
            </button>
          ))}
        </div>
      </div>

      {isLoading && <div className="workload-report__loading">Loading...</div>}

      {data && (
        <>
          {/* サマリーカード */}
          <div className="workload-report__summary">
            <div className="workload-report__card">
              <span className="workload-report__card-value">{data.summary.totalHours}h</span>
              <span className="workload-report__card-label">{t('report.totalHours')}</span>
            </div>
            <div className="workload-report__card">
              <span className="workload-report__card-value">{data.summary.daysWithWork}</span>
              <span className="workload-report__card-label">{t('report.daysWorked')}</span>
            </div>
            <div className="workload-report__card">
              <span className="workload-report__card-value">{formatMinutes(data.summary.avgMinutesPerDay)}</span>
              <span className="workload-report__card-label">{t('report.avgPerDay')}</span>
            </div>
            <div className="workload-report__card">
              <span className="workload-report__card-value">{data.daily.reduce((s, d) => s + d.entry_count, 0)}</span>
              <span className="workload-report__card-label">{t('report.totalEntries')}</span>
            </div>
          </div>

          {/* 日別棒グラフ */}
          <div className="workload-report__section">
            <h2 className="workload-report__section-title">{t('report.dailyBreakdown')}</h2>
            <div className="workload-report__chart">
              {data.daily.map(d => (
                <div key={d.date} className="workload-report__bar-wrapper">
                  <div
                    className="workload-report__bar"
                    style={{ height: `${(d.total_minutes / maxDaily) * 100}%` }}
                    title={`${d.date}: ${formatMinutes(d.total_minutes)}`}
                  />
                  <span className="workload-report__bar-label">
                    {new Date(d.date + 'T00:00:00').toLocaleDateString(undefined, { weekday: 'short', day: 'numeric' })}
                  </span>
                </div>
              ))}
            </div>
          </div>

          {/* ユーザー別 & プロジェクト別 */}
          <div className="workload-report__grid">
            {/* ユーザー別 */}
            <div className="workload-report__section">
              <h2 className="workload-report__section-title">{t('report.byUser')}</h2>
              {data.byUser.length === 0 && (
                <p className="workload-report__empty">{t('report.noData')}</p>
              )}
              {data.byUser.map(u => (
                <div key={u.user_id} className="workload-report__rank-item">
                  <span className="workload-report__rank-name">{u.display_name || u.username}</span>
                  <div className="workload-report__rank-bar-bg">
                    <div
                      className="workload-report__rank-bar"
                      style={{ width: `${(u.total_minutes / (data.byUser[0]?.total_minutes || 1)) * 100}%` }}
                    />
                  </div>
                  <span className="workload-report__rank-value">{formatMinutes(u.total_minutes)}</span>
                </div>
              ))}
            </div>

            {/* プロジェクト別 */}
            <div className="workload-report__section">
              <h2 className="workload-report__section-title">{t('report.byProject')}</h2>
              {data.byProject.length === 0 && (
                <p className="workload-report__empty">{t('report.noData')}</p>
              )}
              {data.byProject.map(p => (
                <div key={p.project_id} className="workload-report__rank-item">
                  <span className="workload-report__rank-name">
                    <span className="workload-report__rank-prefix">{p.project_prefix}</span>
                    {p.project_name}
                  </span>
                  <div className="workload-report__rank-bar-bg">
                    <div
                      className="workload-report__rank-bar workload-report__rank-bar--project"
                      style={{ width: `${(p.total_minutes / (data.byProject[0]?.total_minutes || 1)) * 100}%` }}
                    />
                  </div>
                  <span className="workload-report__rank-value">{formatMinutes(p.total_minutes)}</span>
                </div>
              ))}
            </div>
          </div>
        </>
      )}
    </div>
  );
}
