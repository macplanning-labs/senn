/**
 * Dashboard.tsx — ウィジェットベース・カスタムダッシュボード
 *
 * Phase 1: デフォルトダッシュボード + 6種プリセットウィジェット。
 * GET /api/v1/dashboard/default/ から全ウィジェットデータを取得し、
 * widgetType → コンポーネントのレジストリで描画する。
 */

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import { apiClient } from '@/shared/api/client';
import { useAuthStore } from '@/shared/stores/authStore';
import { SprintHealthWidget } from './SprintHealthWidget';
import { TicketDrilldownPopover } from './TicketDrilldownPopover';
import type { DrilldownFilter } from './TicketDrilldownPopover';
import './Dashboard.css';

// ── 型定義 ──────────────────────────────────

interface WidgetData {
  id: number;
  widgetType: string;
  position: number;
  span: number;
  config: Record<string, unknown>;
  data: unknown;
}

interface DashboardResponse {
  id: number;
  name: string;
  layout: string;
  isDefault: boolean;
  widgets: WidgetData[];
}

interface DashboardSummary {
  id: number;
  name: string;
  layout: string;
  isDefault: boolean;
  widgetCount: number;
}

interface WidgetProps {
  data: unknown;
}

// ── ステータス / 優先度カラー ──────────────

const statusColors: Record<string, string> = {
  backlog: 'var(--color-status-backlog, hsl(220, 10%, 42%))',
  open: 'var(--color-status-open, hsl(210, 70%, 55%))',
  in_progress: 'var(--color-status-in-progress, hsl(45, 80%, 55%))',
  resolved: 'var(--color-status-resolved, hsl(150, 60%, 50%))',
  closed: 'var(--color-status-closed, hsl(220, 10%, 50%))',
  canceled: 'var(--color-status-canceled, hsl(0, 0%, 60%))',
};

const priorityIcons: Record<string, string> = {
  urgent: '⚠',
  high: '▮▮▮',
  medium: '▮▮',
  low: '▮',
};

const priorityColors: Record<string, string> = {
  urgent: 'var(--color-priority-urgent, hsl(0, 80%, 55%))',
  high: 'var(--color-priority-high, hsl(25, 80%, 55%))',
  medium: 'var(--color-priority-medium, hsl(45, 60%, 55%))',
  low: 'var(--color-priority-low, hsl(210, 40%, 55%))',
};

// ── ヘルパー ─────────────────────────────

function formatRelativeDate(dateStr: string, t: (key: string, opts?: Record<string, unknown>) => string): string {
  const date = new Date(dateStr);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffMins = Math.floor(diffMs / 60000);
  if (diffMins < 1) return t('dashboard.justNow');
  if (diffMins < 60) return t('dashboard.minutesAgo', { count: diffMins });
  const diffHours = Math.floor(diffMins / 60);
  if (diffHours < 24) return t('dashboard.hoursAgo', { count: diffHours });
  const diffDays = Math.floor(diffHours / 24);
  if (diffDays < 7) return t('dashboard.daysAgo', { count: diffDays });
  return date.toLocaleDateString();
}

// ── ウィジェットコンポーネント ─────────────

function StatsCardsWidget({ data }: WidgetProps) {
  const { t } = useTranslation();
  const d = data as Record<string, number>;
  const [drilldown, setDrilldown] = useState<{ anchor: HTMLElement; filter: DrilldownFilter; label: string } | null>(null);

  const items: Array<{
    key: string; label: string; value: number; color: string; filter?: DrilldownFilter;
  }> = [
    { key: 'openTickets', label: t('dashboard.openTickets'), value: d.open_tickets ?? 0, color: 'var(--color-accent-primary, hsl(220, 80%, 60%))' },
    { key: 'overdue', label: t('dashboard.overdue'), value: d.overdue_tickets ?? 0, color: 'var(--color-error, hsl(0, 70%, 60%))', filter: { kind: 'due', value: 'overdue' } },
    { key: 'done7d', label: t('dashboard.done7d'), value: d.completed_this_week ?? 0, color: 'var(--color-success, hsl(140, 60%, 55%))' },
    { key: 'dueSoon', label: t('dashboard.dueSoon'), value: d.due_soon_tickets ?? 0, color: 'var(--color-warning, hsl(45, 80%, 55%))', filter: { kind: 'due', value: 'due_soon' } },
    { key: 'projects', label: t('dashboard.totalProjects'), value: d.total_projects ?? 0, color: 'var(--color-text-secondary)' },
  ];

  const openDrilldown = (e: React.MouseEvent<HTMLElement> | React.KeyboardEvent<HTMLElement>, filter: DrilldownFilter, label: string) => {
    setDrilldown({ anchor: e.currentTarget as HTMLElement, filter, label });
  };

  return (
    <div className="stats-cards">
      {items.map((item) => {
        const clickable = !!item.filter;
        const isZero = clickable && item.value === 0;
        return (
          <div
            key={item.key}
            className={`stats-cards__item ${clickable && !isZero ? 'stats-cards__item--clickable' : ''} ${isZero ? 'stats-cards__item--zero' : ''}`}
            role={clickable && !isZero ? 'button' : undefined}
            tabIndex={clickable && !isZero ? 0 : undefined}
            onClick={clickable && !isZero ? (e) => openDrilldown(e, item.filter!, item.label) : undefined}
            onKeyDown={clickable && !isZero ? (e) => {
              if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); openDrilldown(e, item.filter!, item.label); }
            } : undefined}
          >
            <div className="stats-cards__value" style={{ color: item.color }}>{item.value}</div>
            <div className="stats-cards__label">{item.label}</div>
          </div>
        );
      })}
      {drilldown && (
        <TicketDrilldownPopover
          anchorEl={drilldown.anchor}
          filter={drilldown.filter}
          label={drilldown.label}
          onClose={() => setDrilldown(null)}
        />
      )}
    </div>
  );
}

function TicketOverviewWidget({ data }: WidgetProps) {
  const { t } = useTranslation();
  const d = data as {
    open: number; in_progress: number; resolved: number; closed: number; total: number;
    by_project: Array<{ project_key: string; project_name: string; open: number; in_progress: number; resolved: number; closed: number }>;
  };
  const total = d.total || 1;
  const byProject = d.by_project ?? [];
  const segments = [
    { key: 'open', label: t('ticket.status.open'), count: d.open, color: 'hsl(210, 70%, 55%)' },
    { key: 'in_progress', label: t('ticket.status.in_progress'), count: d.in_progress, color: 'hsl(45, 80%, 55%)' },
    { key: 'resolved', label: t('ticket.status.resolved'), count: d.resolved, color: 'hsl(150, 60%, 50%)' },
    { key: 'closed', label: t('ticket.status.closed'), count: d.closed, color: 'hsl(220, 10%, 50%)' },
  ] as const;

  // conic-gradient を生成
  let accum = 0;
  const gradientParts = segments.map((seg) => {
    const pct = (seg.count / total) * 100;
    const start = accum;
    accum += pct;
    return `${seg.color} ${start}% ${accum}%`;
  });
  const gradient = `conic-gradient(${gradientParts.join(', ')})`;

  const [drilldown, setDrilldown] = useState<{ anchor: HTMLElement; filter: DrilldownFilter; label: string } | null>(null);

  return (
    <div className="ticket-overview">
      <div className="ticket-overview__donut" style={{ background: gradient }}>
        <span className="ticket-overview__donut-center">{d.total}</span>
      </div>
      <div className="ticket-overview__legend">
        {segments.map((seg) => {
          const breakdown = byProject
            .map((p) => ({ name: p.project_name, key: p.project_key, count: p[seg.key] }))
            .filter((p) => p.count > 0);
          return (
            <div
              key={seg.key}
              className="ticket-overview__legend-item ticket-overview__legend-item--clickable"
              role="button"
              tabIndex={0}
              onClick={(e) => setDrilldown({ anchor: e.currentTarget, filter: { kind: 'status', value: seg.key }, label: seg.label })}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault();
                  setDrilldown({ anchor: e.currentTarget, filter: { kind: 'status', value: seg.key }, label: seg.label });
                }
              }}
            >
              <span className="ticket-overview__legend-dot" style={{ background: seg.color }} />
              <span>{seg.label}</span>
              <span className="ticket-overview__legend-count">{seg.count}</span>
              {breakdown.length > 0 && (
                <div className="ticket-overview__tooltip" role="tooltip">
                  {breakdown.map((p) => (
                    <div
                      key={p.key}
                      className="ticket-overview__tooltip-row ticket-overview__tooltip-row--clickable"
                      role="button"
                      tabIndex={0}
                      onClick={(e) => {
                        e.stopPropagation();
                        setDrilldown({
                          anchor: e.currentTarget,
                          filter: { kind: 'status', value: seg.key, projectPrefix: p.key },
                          label: `${p.name} · ${seg.label}`,
                        });
                      }}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter' || e.key === ' ') {
                          e.preventDefault();
                          e.stopPropagation();
                          setDrilldown({
                            anchor: e.currentTarget,
                            filter: { kind: 'status', value: seg.key, projectPrefix: p.key },
                            label: `${p.name} · ${seg.label}`,
                          });
                        }
                      }}
                    >
                      <span>{p.name}</span>
                      <span>{p.count}</span>
                    </div>
                  ))}
                </div>
              )}
            </div>
          );
        })}
      </div>
      {drilldown && (
        <TicketDrilldownPopover
          anchorEl={drilldown.anchor}
          filter={drilldown.filter}
          label={drilldown.label}
          onClose={() => setDrilldown(null)}
        />
      )}
    </div>
  );
}

function RecentActivityWidget({ data }: WidgetProps) {
  const { t } = useTranslation();
  const activities = data as Array<{
    id: number;
    ticket_id: number | null;
    ticket_key: string | null;
    ticket_title: string | null;
    project_key: string | null;
    old_status: string;
    new_status: string;
    changed_by: string;
    changed_at: string;
  }>;

  if (!activities?.length) {
    return <div className="widget-list__empty">📋 {t('dashboard.noRecentActivity')}</div>;
  }

  return (
    <div className="widget-list">
      {activities.map((a) => (
        <div key={a.id} className="widget-list__item">
          <span className="widget-list__icon">🔄</span>
          <span className="widget-list__text">
            <strong>{a.changed_by}</strong>{' '}
            {a.ticket_key && a.project_key ? (
              <Link to={`/project/${a.project_key}/tickets/${a.ticket_key}`} className="widget-list__link">
                {a.ticket_key}
              </Link>
            ) : (
              a.ticket_key
            )}{' '}
            <span
              className="widget-badge"
              style={{ '--badge-color': statusColors[a.old_status] } as React.CSSProperties}
            >
              <span className="widget-badge__dot" />
              {a.old_status.replace('_', ' ')}
            </span>
            {' → '}
            <span
              className="widget-badge"
              style={{ '--badge-color': statusColors[a.new_status] } as React.CSSProperties}
            >
              <span className="widget-badge__dot" />
              {a.new_status.replace('_', ' ')}
            </span>
          </span>
          <span className="widget-list__meta">{formatRelativeDate(a.changed_at, t)}</span>
        </div>
      ))}
    </div>
  );
}

function MyTicketsWidget({ data }: WidgetProps) {
  const { t } = useTranslation();
  const tickets = data as Array<{
    id: number;
    ticket_key: string;
    title: string;
    status: string;
    priority: string;
    due_date: string | null;
    updated_at: string;
    project_key?: string;
  }>;

  if (!tickets?.length) {
    return <div className="widget-list__empty">🎉 {t('dashboard.noOpenTickets')}</div>;
  }

  return (
    <div className="widget-list">
      {tickets.map((t) => (
        <Link key={t.id} to={`/project/${t.project_key || '_'}/tickets/${t.ticket_key}`} className="widget-list__item">
          <span
            className="widget-priority"
            style={{ color: priorityColors[t.priority] }}
          >
            {priorityIcons[t.priority] || '—'}
          </span>
          <span className="widget-list__text">
            <strong>{t.ticket_key}</strong> {t.title}
          </span>
          {t.due_date && (
            <span
              className={`widget-due ${
                new Date(t.due_date) < new Date() ? 'widget-due--overdue' : ''
              }`}
            >
              {new Date(t.due_date).toLocaleDateString()}
            </span>
          )}
          <span
            className="widget-badge"
            style={{ '--badge-color': statusColors[t.status] } as React.CSSProperties}
          >
            <span className="widget-badge__dot" />
            {t.status.replace('_', ' ')}
          </span>
        </Link>
      ))}
    </div>
  );
}

function RecentWikiWidget({ data }: WidgetProps) {
  const { t } = useTranslation();
  const pages = data as Array<{
    id: number;
    title: string;
    slug: string;
    projectKey: string | null;
    updatedAt: string;
    lastEditor: string | null;
  }>;

  if (!pages?.length) {
    return <div className="widget-list__empty">📖 {t('dashboard.noWikiPages')}</div>;
  }

  return (
    <div className="widget-list">
      {pages.map((p) => (
        <Link key={p.id} to={`/project/${p.projectKey || '_'}/wiki`} className="widget-list__item">
          <span className="widget-list__icon">📄</span>
          <span className="widget-list__text">{p.title}</span>
          <span className="widget-list__meta">
            {p.lastEditor && `${p.lastEditor} · `}
            {formatRelativeDate(p.updatedAt, t)}
          </span>
        </Link>
      ))}
    </div>
  );
}

function UnreadNotificationsWidget({ data }: WidgetProps) {
  const { t } = useTranslation();
  const d = data as {
    count: number;
    items: Array<{
      id: number;
      title: string;
      message: string;
      category: string;
      ticketId: number | null;
      createdAt: string;
    }>;
  };

  if (!d?.items?.length) {
    return <div className="widget-list__empty">✅ {t('dashboard.allCaughtUp')}</div>;
  }

  const categoryIcons: Record<string, string> = {
    assignment: '👤',
    comment: '💬',
    status_change: '🔄',
    mention: '📢',
  };

  return (
    <div className="widget-list">
      <div className="widget-list__item" style={{ justifyContent: 'space-between', borderBottom: 'none' }}>
        <span>{t('dashboard.unread')}</span>
        <span className="widget-notif-badge">{d.count}</span>
      </div>
      {d.items.map((n) => (
        <Link
          key={n.id}
          to={n.ticketId ? `/tickets/${n.ticketId}` : '/notifications'}
          className="widget-list__item"
        >
          <span className="widget-list__icon">{categoryIcons[n.category] || '🔔'}</span>
          <span className="widget-list__text">{n.title}</span>
          <span className="widget-list__meta">{formatRelativeDate(n.createdAt, t)}</span>
        </Link>
      ))}
    </div>
  );
}

// ── ウィジェットレジストリ ──────────────────

const WIDGET_REGISTRY: Record<string, React.FC<WidgetProps>> = {
  stats_cards: StatsCardsWidget,
  ticket_overview: TicketOverviewWidget,
  recent_activity: RecentActivityWidget,
  my_tickets: MyTicketsWidget,
  recent_wiki: RecentWikiWidget,
  unread_notifications: UnreadNotificationsWidget,
  sprint_health: SprintHealthWidget,
};

function getWidgetMeta(t: (key: string) => string): Record<string, { icon: string; label: string; desc: string }> {
  return {
    stats_cards: { icon: '📊', label: t('dashboard.statsCards'), desc: t('dashboard.statsCardsDesc') },
    ticket_overview: { icon: '🍩', label: t('dashboard.ticketOverview'), desc: t('dashboard.ticketOverviewDesc') },
    recent_activity: { icon: '📰', label: t('dashboard.recentActivity'), desc: t('dashboard.recentActivityDesc') },
    my_tickets: { icon: '📝', label: t('dashboard.myTickets'), desc: t('dashboard.myTickets') },
    recent_wiki: { icon: '📖', label: t('dashboard.recentWiki'), desc: t('dashboard.recentWikiDesc') },
    unread_notifications: { icon: '🔔', label: t('dashboard.notificationsWidget'), desc: t('dashboard.notificationsDesc') },
    sprint_health: { icon: '🤖', label: t('dashboard.sprintHealth'), desc: t('dashboard.sprintHealthDesc') },
  };
}

// ── AddWidgetModal ────────────────────────

function AddWidgetModal({
  visibleTypes,
  onAdd,
  onClose,
  widgetMeta,
  t,
}: {
  visibleTypes: string[];
  onAdd: (type: string) => void;
  onClose: () => void;
  widgetMeta: Record<string, { icon: string; label: string; desc: string }>;
  t: (key: string) => string;
}) {
  const allTypes = Object.keys(widgetMeta);

  return (
    <div className="add-widget-overlay" onClick={onClose}>
      <div className="add-widget-modal" onClick={(e) => e.stopPropagation()}>
        <h3 className="add-widget-modal__title">{t('dashboard.addWidget')}</h3>
        <div className="add-widget-modal__list">
          {allTypes.map((type) => {
            const meta = widgetMeta[type];
            if (!meta) return null;
            const alreadyVisible = visibleTypes.includes(type);
            return (
              <button
                key={type}
                className="add-widget-modal__item"
                disabled={alreadyVisible}
                onClick={() => onAdd(type)}
              >
                <span className="add-widget-modal__item-icon">{meta.icon}</span>
                <div className="add-widget-modal__item-text">
                  <div className="add-widget-modal__item-name">
                    {meta.label}
                    {alreadyVisible && ' ✓'}
                  </div>
                  <div className="add-widget-modal__item-desc">{meta.desc}</div>
                </div>
              </button>
            );
          })}
        </div>
        <button className="add-widget-modal__close" onClick={onClose}>
          {t('common.cancel')}
        </button>
      </div>
    </div>
  );
}

// ── メインコンポーネント ────────────────────

export function Dashboard() {
  const { t } = useTranslation();
  const user = useAuthStore((s) => s.user);
  const queryClient = useQueryClient();
  const [showAddModal, setShowAddModal] = useState(false);
  const [activeDashboardId, setActiveDashboardId] = useState<number | null>(null);
  const [isCreating, setIsCreating] = useState(false);
  const [newName, setNewName] = useState('');
  const [editingName, setEditingName] = useState<number | null>(null);
  const [editNameValue, setEditNameValue] = useState('');

  // D&D 状態
  const [dragIdx, setDragIdx] = useState<number | null>(null);

  // ダッシュボード一覧取得
  const { data: dashboardList } = useQuery<DashboardSummary[]>({
    queryKey: ['dashboard-list'],
    queryFn: async () => {
      const res = await apiClient.get<DashboardSummary[]>('/dashboard/list/');
      return res.data;
    },
  });

  // アクティブなダッシュボードID（初期はデフォルト）
  const effectiveId = activeDashboardId ?? dashboardList?.[0]?.id ?? null;

  // ダッシュボード詳細取得
  const { data: dashboard, isLoading } = useQuery<DashboardResponse>({
    queryKey: ['dashboard-detail', effectiveId],
    queryFn: async () => {
      if (!effectiveId) {
        const res = await apiClient.get<DashboardResponse>('/dashboard/default/');
        return res.data;
      }
      const res = await apiClient.get<DashboardResponse>(`/dashboard/detail/${effectiveId}/`);
      return res.data;
    },
    enabled: effectiveId !== null || !dashboardList,
  });

  // ── Mutations ──

  const removeMutation = useMutation({
    mutationFn: async (widgetId: number) => {
      await apiClient.delete('/dashboard/remove-widget/', { data: { widgetId } });
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['dashboard-detail'] });
    },
  });

  const addMutation = useMutation({
    mutationFn: async (widgetType: string) => {
      await apiClient.post('/dashboard/add-widget/', {
        widgetType,
        dashboardId: dashboard?.id,
      });
    },
    onSuccess: () => {
      setShowAddModal(false);
      void queryClient.invalidateQueries({ queryKey: ['dashboard-detail'] });
    },
  });

  const reorderMutation = useMutation({
    mutationFn: async (widgetOrder: number[]) => {
      await apiClient.post('/dashboard/reorder-widgets/', { widgetOrder });
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['dashboard-detail'] });
    },
  });

  const createMutation = useMutation({
    mutationFn: async (name: string) => {
      const res = await apiClient.post<{ id: number }>('/dashboard/create/', { name });
      return res.data;
    },
    onSuccess: (data) => {
      setIsCreating(false);
      setNewName('');
      setActiveDashboardId(data.id);
      void queryClient.invalidateQueries({ queryKey: ['dashboard-list'] });
    },
  });

  const deleteMutation = useMutation({
    mutationFn: async (dashboardId: number) => {
      await apiClient.delete('/dashboard/delete/', { data: { dashboardId } });
    },
    onSuccess: () => {
      setActiveDashboardId(null);
      void queryClient.invalidateQueries({ queryKey: ['dashboard-list'] });
      void queryClient.invalidateQueries({ queryKey: ['dashboard-detail'] });
    },
  });

  const renameMutation = useMutation({
    mutationFn: async ({ dashboardId, name }: { dashboardId: number; name: string }) => {
      await apiClient.patch('/dashboard/update/', { dashboardId, name });
    },
    onSuccess: () => {
      setEditingName(null);
      void queryClient.invalidateQueries({ queryKey: ['dashboard-list'] });
      void queryClient.invalidateQueries({ queryKey: ['dashboard-detail'] });
    },
  });

  // ── D&D ハンドラ ──

  const handleDragStart = (idx: number) => {
    setDragIdx(idx);
  };

  const handleDragOver = (e: React.DragEvent, idx: number) => {
    e.preventDefault();
    if (dragIdx === null || dragIdx === idx) return;

    // Optimistic: ローカルで入れ替え（UIだけ）
    const items = [...(dashboard?.widgets ?? [])];
    const [moved] = items.splice(dragIdx, 1);
    if (!moved) return;
    items.splice(idx, 0, moved);
    setDragIdx(idx);

    // queryClient のキャッシュを直接書き換え（アニメーション滑らか化）
    queryClient.setQueryData(['dashboard-detail', effectiveId], (old: DashboardResponse | undefined) => {
      if (!old) return old;
      return { ...old, widgets: items };
    });
  };

  const handleDrop = () => {
    if (dragIdx === null) return;
    setDragIdx(null);
    const widgetOrder = (dashboard?.widgets ?? []).map((w) => w.id);
    reorderMutation.mutate(widgetOrder);
  };

  // ── Loading ──

  if (isLoading && !dashboard) {
    return (
      <div className="dashboard" data-testid="dashboard-page">
        <div className="dashboard__loading">
          <div className="dashboard__spinner" />
        </div>
      </div>
    );
  }

  const widgets = dashboard?.widgets ?? [];
  const visibleTypes = widgets.map((w) => w.widgetType);
  const widgetMeta = getWidgetMeta(t);

  return (
    <div className="dashboard" data-testid="dashboard-page">
      {/* ダッシュボードタブ */}
      {dashboardList && dashboardList.length > 0 && (
        <div className="dashboard__tabs">
          {dashboardList.map((d) => (
            <button
              key={d.id}
              className={`dashboard__tab ${effectiveId === d.id ? 'dashboard__tab--active' : ''}`}
              onClick={() => setActiveDashboardId(d.id)}
            >
              {editingName === d.id ? (
                <input
                  className="dashboard__tab-input"
                  value={editNameValue}
                  onChange={(e) => setEditNameValue(e.target.value)}
                  onBlur={() => renameMutation.mutate({ dashboardId: d.id, name: editNameValue })}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') renameMutation.mutate({ dashboardId: d.id, name: editNameValue });
                    if (e.key === 'Escape') setEditingName(null);
                  }}
                  autoFocus
                  onClick={(e) => e.stopPropagation()}
                />
              ) : (
                <span
                  onDoubleClick={(e) => {
                    e.stopPropagation();
                    setEditingName(d.id);
                    setEditNameValue(d.name);
                  }}
                >
                  {d.name}
                </span>
              )}
              {!d.isDefault && effectiveId === d.id && (
                <button
                  className="dashboard__tab-delete"
                  onClick={(e) => {
                    e.stopPropagation();
                    if (confirm(t('dashboard.deleteConfirm', { name: d.name }))) {
                      deleteMutation.mutate(d.id);
                    }
                  }}
                  title={t('dashboard.deleteDashboard')}
                >
                  ×
                </button>
              )}
            </button>
          ))}
          {isCreating ? (
            <div className="dashboard__tab dashboard__tab--new">
              <input
                className="dashboard__tab-input"
                placeholder={t('dashboard.dashboardName')}
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && newName.trim()) createMutation.mutate(newName.trim());
                  if (e.key === 'Escape') setIsCreating(false);
                }}
                autoFocus
              />
            </div>
          ) : (
            <button
              className="dashboard__tab dashboard__tab--add"
              onClick={() => setIsCreating(true)}
              title={t('dashboard.newDashboard')}
            >
              +
            </button>
          )}
        </div>
      )}

      {/* ヘッダー */}
      <div className="dashboard__header">
        <div className="dashboard__header-left">
          <h1 className="dashboard__title">
            {dashboard?.name || t('dashboard.title')}
          </h1>
          <p className="dashboard__greeting">
            {user ? t('dashboard.welcomeBack', { name: user.firstName || user.username }) : ''}
          </p>
        </div>
        <button
          className="dashboard__add-btn"
          onClick={() => setShowAddModal(true)}
          data-testid="add-widget-btn"
        >
          + {t('dashboard.addWidget')}
        </button>
      </div>

      {/* ウィジェットグリッド（D&D対応） */}
      <div className="dashboard__grid">
        {widgets.map((w, i) => {
          const Widget = WIDGET_REGISTRY[w.widgetType];
          const meta = widgetMeta[w.widgetType];
          if (!Widget) return null;
          return (
            <div
              key={w.id}
              className={`widget-card ${w.span >= 2 ? 'widget-card--span-2' : ''} ${
                dragIdx === i ? 'widget-card--dragging' : ''
              }`}
              style={{ animationDelay: `${i * 60}ms` }}
              data-testid={`widget-${w.widgetType}`}
              draggable
              onDragStart={() => handleDragStart(i)}
              onDragOver={(e) => handleDragOver(e, i)}
              onDrop={handleDrop}
              onDragEnd={() => setDragIdx(null)}
            >
              <button
                className="widget-card__remove"
                onClick={() => removeMutation.mutate(w.id)}
                title={t('dashboard.removeWidget')}
                data-testid={`remove-${w.widgetType}`}
              >
                ×
              </button>
              <div className="widget-card__header">
                <span className="widget-card__drag-handle" title={t('dashboard.dragToReorder')}>⠿</span>
                <span className="widget-card__icon">{meta?.icon}</span>
                <span className="widget-card__title">{meta?.label}</span>
              </div>
              <Widget data={w.data} />
            </div>
          );
        })}
      </div>

      {/* ウィジェット追加モーダル */}
      {showAddModal && (
        <AddWidgetModal
          visibleTypes={visibleTypes}
          onAdd={(type) => addMutation.mutate(type)}
          onClose={() => setShowAddModal(false)}
          widgetMeta={widgetMeta}
          t={t}
        />
      )}
    </div>
  );
}

