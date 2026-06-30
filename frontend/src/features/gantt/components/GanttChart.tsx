/**
 * GanttChart.tsx — ガントチャート
 *
 * 簡易ガントチャート（Canvas/SVGなし、CSS Grid で実装）。
 * チケットの start_date 〜 due_date を視覚的に表示。
 */

import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { useQuery } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import { apiClient } from '@/shared/api/client';
import { useProject } from '@/shared/hooks/useProject';
import './GanttChart.css';

interface GanttTicket {
  id: number;
  ticketKey: string;
  title: string;
  status: string;
  priority: string;
  startDate: string | null;
  dueDate: string | null;
  assignees: { id: number; displayName: string; username: string }[];
}

const statusColors: Record<string, string> = {
  backlog: 'var(--color-status-backlog, #6b7280)',
  open: 'var(--color-status-open)',
  in_progress: 'var(--color-status-in-progress)',
  resolved: 'var(--color-status-resolved)',
  closed: 'var(--color-status-closed)',
  canceled: 'var(--color-status-canceled, #9ca3af)',
};

function addDays(date: Date, days: number): Date {
  const d = new Date(date);
  d.setDate(d.getDate() + days);
  return d;
}

function daysBetween(a: Date, b: Date): number {
  return Math.round((b.getTime() - a.getTime()) / (1000 * 60 * 60 * 24));
}

function formatShortDate(date: Date): string {
  return date.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
}

export function GanttChart() {
  const { t } = useTranslation();
  const { projectKey } = useProject();

  const { data, isLoading } = useQuery<{ results: GanttTicket[] }>({
    queryKey: ['gantt-tickets', projectKey],
    queryFn: async () => {
      const res = await apiClient.get<{ results: GanttTicket[] }>('/tickets/', {
        params: {
          due_date__isnull: false,
          ordering: 'gantt_order,due_date',
          ...(projectKey ? { project__prefix: projectKey } : {}),
        },
      });
      return res.data;
    },
  });

  const tickets = useMemo(() => data?.results ?? [], [data]);

  // タイムライン範囲を計算
  const { timelineStart, totalDays, weekMarkers } = useMemo(() => {
    if (!tickets.length) {
      const today = new Date();
      return {
        timelineStart: today,
        totalDays: 30,
        weekMarkers: Array.from({ length: 5 }, (_, i) => addDays(today, i * 7)),
      };
    }

    const dates: Date[] = [];
    for (const t of tickets) {
      if (t.startDate) dates.push(new Date(t.startDate));
      if (t.dueDate) dates.push(new Date(t.dueDate));
    }

    const minDate = new Date(Math.min(...dates.map((d) => d.getTime())));
    const maxDate = new Date(Math.max(...dates.map((d) => d.getTime())));

    // 前後に余裕を持たせる
    const start = addDays(minDate, -3);
    const total = Math.max(daysBetween(start, addDays(maxDate, 7)), 14);

    const markers: Date[] = [];
    for (let i = 0; i <= total; i += 7) {
      markers.push(addDays(start, i));
    }

    return { timelineStart: start, totalDays: total, weekMarkers: markers };
  }, [tickets]);

  // 今日の位置（%）
  const todayOffset = useMemo(() => {
    const days = daysBetween(timelineStart, new Date());
    return Math.max(0, Math.min(100, (days / totalDays) * 100));
  }, [timelineStart, totalDays]);

  return (
    <div className="gantt" data-testid="gantt-page">
      <div className="gantt__header">
        <h1 className="gantt__title">{t('nav.gantt')}</h1>
        <div className="gantt__legend">
          <span className="gantt__legend-item">
            <span className="gantt__legend-swatch" style={{ backgroundColor: 'var(--color-status-backlog, #6b7280)' }} /> Backlog
          </span>
          <span className="gantt__legend-item">
            <span className="gantt__legend-swatch" style={{ backgroundColor: 'var(--color-status-open)' }} /> Open
          </span>
          <span className="gantt__legend-item">
            <span className="gantt__legend-swatch" style={{ backgroundColor: 'var(--color-status-in-progress)' }} /> In Progress
          </span>
          <span className="gantt__legend-item">
            <span className="gantt__legend-swatch" style={{ backgroundColor: 'var(--color-status-resolved)' }} /> Resolved
          </span>
          <span className="gantt__legend-item">
            <span className="gantt__legend-swatch" style={{ backgroundColor: 'var(--color-status-closed)' }} /> Closed
          </span>
          <span className="gantt__legend-item">
            <span className="gantt__legend-swatch" style={{ backgroundColor: 'var(--color-status-canceled, #9ca3af)' }} /> Canceled
          </span>
          <span className="gantt__legend-item">
            <span className="gantt__legend-swatch gantt__legend-swatch--overdue" /> Overdue
          </span>
        </div>
      </div>

      {isLoading ? (
        <div className="gantt__loading">
          {Array.from({ length: 5 }).map((_, i) => (
            <div key={i} className="gantt__skeleton-row" />
          ))}
        </div>
      ) : !tickets.length ? (
        <div className="gantt__empty" data-testid="gantt-empty">
          No tickets with due dates. Create tickets with start/due dates to see the Gantt chart.
        </div>
      ) : (
        <div className="gantt__container" data-testid="gantt-chart">
          {/* タイムラインヘッダー */}
          <div className="gantt__timeline-header">
            <div className="gantt__label-col" />
            <div className="gantt__timeline-scale">
              {weekMarkers.map((date, i) => (
                <span
                  key={i}
                  className="gantt__week-marker"
                  style={{ left: `${(daysBetween(timelineStart, date) / totalDays) * 100}%` }}
                >
                  {formatShortDate(date)}
                </span>
              ))}
            </div>
          </div>

          {/* チケット行 */}
          {tickets.map((ticket) => {
            const start = ticket.startDate
              ? new Date(ticket.startDate)
              : ticket.dueDate
                ? addDays(new Date(ticket.dueDate), -3)
                : new Date();
            const end = ticket.dueDate ? new Date(ticket.dueDate) : addDays(start, 3);

            const leftPct = Math.max(0, (daysBetween(timelineStart, start) / totalDays) * 100);
            const widthPct = Math.max(2, (daysBetween(start, end) / totalDays) * 100);
            const barColor = statusColors[ticket.status] ?? 'var(--color-accent-primary)';
            const isOverdue = ticket.dueDate && new Date(ticket.dueDate) < new Date() && ticket.status !== 'closed';

            return (
              <div
                key={ticket.id}
                className="gantt__row"
                data-testid={`gantt-row-${ticket.ticketKey}`}
              >
                {/* 左: ラベル */}
                <div className="gantt__label-col">
                  <Link to={`/tickets/${ticket.id}`} className="gantt__ticket-link">
                    <span className="gantt__ticket-key">{ticket.ticketKey}</span>
                    <span className="gantt__ticket-title">{ticket.title}</span>
                  </Link>
                </div>

                {/* 右: バー */}
                <div className="gantt__bar-col">
                  <div
                    className={`gantt__bar ${isOverdue ? 'gantt__bar--overdue' : ''}`}
                    style={{
                      left: `${leftPct}%`,
                      width: `${widthPct}%`,
                      '--bar-color': barColor,
                    } as React.CSSProperties}
                    title={`${formatShortDate(start)} → ${formatShortDate(end)}`}
                  >
                    {widthPct > 8 && (
                      <span className="gantt__bar-label">
                        {ticket.assignees?.map(a => a.displayName ?? a.username).join(', ') || ''}
                      </span>
                    )}
                  </div>
                </div>
              </div>
            );
          })}

          {/* 今日の線 */}
          <div
            className="gantt__today-line"
            style={{ left: `calc(200px + ${todayOffset}% * (100% - 200px) / 100)` }}
            data-testid="gantt-today"
          />
        </div>
      )}
    </div>
  );
}
