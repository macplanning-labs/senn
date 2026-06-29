/**
 * Dashboard.tsx — ダッシュボードページ
 *
 * 統計カード + 自分のチケット一覧。
 * TanStack Query でAPIデータを取得。
 */

import { useTranslation } from 'react-i18next';
import { useQuery } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import { apiClient } from '@/shared/api/client';
import { useAuthStore } from '@/shared/stores/authStore';
import './Dashboard.css';

// 型定義
interface TicketSummary {
  id: number;
  ticketKey: string;
  title: string;
  status: string;
  priority: string;
  dueDate: string | null;
  updatedAt: string;
}

interface DashboardStats {
  openTickets: number;
  overdueTickets: number;
  completedThisWeek: number;
  totalProjects: number;
}

// ステータスバッジの色マッピング
const statusColors: Record<string, string> = {
  open: 'var(--color-status-open)',
  in_progress: 'var(--color-status-in-progress)',
  resolved: 'var(--color-status-resolved)',
  closed: 'var(--color-status-closed)',
};

const priorityColors: Record<string, string> = {
  urgent: 'var(--color-priority-urgent)',
  high: 'var(--color-priority-high)',
  medium: 'var(--color-priority-medium)',
  low: 'var(--color-priority-low)',
};

function StatCard({
  label,
  value,
  accent,
}: {
  label: string;
  value: number;
  accent?: string;
}) {
  return (
    <div className="dashboard__stat-card" data-testid={`stat-${label}`}>
      <span
        className="dashboard__stat-value"
        style={accent ? { color: accent } : undefined}
      >
        {value}
      </span>
      <span className="dashboard__stat-label">{label}</span>
    </div>
  );
}

function StatusBadge({ status }: { status: string }) {
  const color = statusColors[status] ?? 'var(--color-text-tertiary)';
  const label = status.replace('_', ' ');
  return (
    <span
      className="dashboard__badge"
      style={{ '--badge-color': color } as React.CSSProperties}
      data-testid={`status-${status}`}
    >
      <span className="dashboard__badge-dot" />
      {label}
    </span>
  );
}

function PriorityIndicator({ priority }: { priority: string }) {
  const color = priorityColors[priority] ?? 'var(--color-text-tertiary)';
  return (
    <span
      className="dashboard__priority"
      style={{ color }}
      title={priority}
      data-testid={`priority-${priority}`}
    >
      {priority === 'urgent' ? '⬆⬆' : priority === 'high' ? '⬆' : priority === 'medium' ? '—' : '⬇'}
    </span>
  );
}

function formatRelativeDate(dateStr: string): string {
  const date = new Date(dateStr);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffMins = Math.floor(diffMs / 60000);
  if (diffMins < 1) return 'just now';
  if (diffMins < 60) return `${diffMins}m ago`;
  const diffHours = Math.floor(diffMins / 60);
  if (diffHours < 24) return `${diffHours}h ago`;
  const diffDays = Math.floor(diffHours / 24);
  if (diffDays < 7) return `${diffDays}d ago`;
  return date.toLocaleDateString();
}

export function Dashboard() {
  const { t } = useTranslation();
  const user = useAuthStore((s) => s.user);

  // 自分のチケットを取得
  const { data: myTickets, isLoading: ticketsLoading } = useQuery<TicketSummary[]>({
    queryKey: ['my-tickets', user?.id],
    queryFn: async () => {
      const res = await apiClient.get<{ results: TicketSummary[] }>('/tickets/', {
        params: {
          assignee: user?.id,
          status__in: 'open,in_progress',
          ordering: '-updated_at',
        },
      });
      return res.data.results ?? [];
    },
    enabled: !!user,
  });

  // 統計情報を取得
  const { data: stats } = useQuery<DashboardStats>({
    queryKey: ['dashboard-stats'],
    queryFn: async () => {
      const [ticketsRes, projectsRes] = await Promise.all([
        apiClient.get<{ results: TicketSummary[] }>('/tickets/', {
          params: { status__in: 'open,in_progress' },
        }),
        apiClient.get<{ results: unknown[] }>('/projects/'),
      ]);

      const tickets = ticketsRes.data.results ?? [];
      const now = new Date();


      return {
        openTickets: tickets.length,
        overdueTickets: tickets.filter(
          (t) => t.dueDate && new Date(t.dueDate) < now,
        ).length,
        completedThisWeek: 0, // 別クエリで取得可能
        totalProjects: (projectsRes.data.results ?? []).length,
      };
    },
  });

  return (
    <div className="dashboard" data-testid="dashboard-page">
      {/* ヘッダー */}
      <div className="dashboard__header">
        <h1 className="dashboard__title">{t('dashboard.title')}</h1>
        <p className="dashboard__greeting">
          {user ? `Welcome back, ${user.firstName || user.username}` : ''}
        </p>
      </div>

      {/* 統計カード */}
      <div className="dashboard__stats" data-testid="dashboard-stats">
        <StatCard
          label={t('dashboard.openTickets')}
          value={stats?.openTickets ?? 0}
          accent="var(--color-accent-primary)"
        />
        <StatCard
          label={t('dashboard.overdue')}
          value={stats?.overdueTickets ?? 0}
          accent="var(--color-error)"
        />
        <StatCard
          label={t('dashboard.completedThisWeek')}
          value={stats?.completedThisWeek ?? 0}
          accent="var(--color-success)"
        />
        <StatCard
          label={t('dashboard.totalProjects')}
          value={stats?.totalProjects ?? 0}
        />
      </div>

      {/* 自分のチケット */}
      <div className="dashboard__section">
        <div className="dashboard__section-header">
          <h2 className="dashboard__section-title">{t('dashboard.myTickets')}</h2>
          <Link to="/tickets" className="dashboard__view-all">
            View all →
          </Link>
        </div>

        {ticketsLoading ? (
          <div className="dashboard__loading">
            <div className="dashboard__skeleton" />
            <div className="dashboard__skeleton" />
            <div className="dashboard__skeleton" />
          </div>
        ) : !myTickets?.length ? (
          <div className="dashboard__empty" data-testid="no-tickets">
            <p>🎉 No open tickets assigned to you!</p>
          </div>
        ) : (
          <div className="dashboard__ticket-list" data-testid="my-ticket-list">
            {myTickets.map((ticket) => (
              <Link
                key={ticket.id}
                to={`/tickets/${ticket.id}`}
                className="dashboard__ticket-row"
                data-testid={`ticket-${ticket.ticketKey}`}
              >
                <div className="dashboard__ticket-left">
                  <PriorityIndicator priority={ticket.priority} />
                  <span className="dashboard__ticket-key">{ticket.ticketKey}</span>
                  <span className="dashboard__ticket-title">{ticket.title}</span>
                </div>
                <div className="dashboard__ticket-right">
                  <StatusBadge status={ticket.status} />
                  {ticket.dueDate && (
                    <span
                      className={`dashboard__ticket-due ${
                        new Date(ticket.dueDate) < new Date()
                          ? 'dashboard__ticket-due--overdue'
                          : ''
                      }`}
                    >
                      {new Date(ticket.dueDate).toLocaleDateString()}
                    </span>
                  )}
                  <span className="dashboard__ticket-updated">
                    {formatRelativeDate(ticket.updatedAt)}
                  </span>
                </div>
              </Link>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
