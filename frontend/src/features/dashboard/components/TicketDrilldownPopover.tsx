/**
 * TicketDrilldownPopover.tsx — ダッシュボード統計からのドリルダウン
 *
 * 統計カード/チケット概要の凡例クリックで開き、条件に合致するチケットを
 * プロジェクト別にグルーピングして一覧表示する。行クリックで対象チケットの
 * 詳細パネル(/p/:projectKey/tickets/:ticketId)へ遷移する。
 */
import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { useQuery } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import { apiClient } from '@/shared/api/client';
import { localDateStr } from '@/shared/utils/localDateStr';
import type { Project } from '@/shared/hooks/useProject';
import './TicketDrilldownPopover.css';

export type DrilldownFilter =
  | { kind: 'due'; value: 'overdue' | 'due_soon' }
  | { kind: 'status'; value: string; projectPrefix?: string };

interface DrilldownTicket {
  id: number;
  ticketKey: string;
  title: string;
  priority: string;
  dueDate: string | null;
  project: number;
}

interface TicketDrilldownPopoverProps {
  anchorEl: HTMLElement;
  filter: DrilldownFilter;
  label: string;
  onClose: () => void;
}

const priorityIcons: Record<string, string> = {
  urgent: '⚠', high: '▮▮▮', medium: '▮▮', low: '▮',
};

// バックエンドの overdue/due_soon 判定 (dashboard_api_repo.rs:41,43) と同じ日付境界。
function buildParams(filter: DrilldownFilter): Record<string, string> {
  if (filter.kind === 'due') {
    const base = { status__in: 'open,in_progress', ordering: 'due_date' };
    return filter.value === 'overdue'
      ? { ...base, due_date__lte: localDateStr(-1) }
      : { ...base, due_date__gte: localDateStr(0), due_date__lte: localDateStr(3) };
  }
  const params: Record<string, string> = { status: filter.value, ordering: 'due_date' };
  if (filter.projectPrefix) params.project__prefix = filter.projectPrefix;
  return params;
}

export function TicketDrilldownPopover({ anchorEl, filter, label, onClose }: TicketDrilldownPopoverProps) {
  const { t } = useTranslation();
  const popoverRef = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState<{ top: number; left: number } | null>(null);

  // アンカー要素の下端に揃える。右端/下端をはみ出す場合はそれぞれ左寄せ/上開きに切り替える。
  useEffect(() => {
    const rect = anchorEl.getBoundingClientRect();
    const width = 340;
    const maxHeight = 420; // .drilldown-popover の max-height と合わせる
    const left = rect.left + width > window.innerWidth - 16
      ? Math.max(16, rect.right - width)
      : rect.left;
    const top = rect.bottom + 8 + maxHeight > window.innerHeight
      ? Math.max(16, rect.top - 8 - maxHeight)
      : rect.bottom + 8;
    setPosition({ top, left });
  }, [anchorEl]);

  useEffect(() => {
    const handleClick = (e: MouseEvent) => {
      if (popoverRef.current && !popoverRef.current.contains(e.target as Node) && e.target !== anchorEl) {
        onClose();
      }
    };
    const handleKey = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose(); };
    document.addEventListener('mousedown', handleClick);
    document.addEventListener('keydown', handleKey);
    return () => {
      document.removeEventListener('mousedown', handleClick);
      document.removeEventListener('keydown', handleKey);
    };
  }, [anchorEl, onClose]);

  // プロジェクトID→prefix/name解決。useProject.ts と同じ queryKey で既存キャッシュを再利用する。
  const { data: projectsData } = useQuery<{ results: Project[] }>({
    queryKey: ['projects'],
    queryFn: async () => (await apiClient.get<{ results: Project[] }>('/projects/')).data,
    staleTime: 1000 * 60 * 10,
  });
  const projectById = new Map((projectsData?.results ?? []).map((p) => [p.id, p]));

  const { data, isLoading } = useQuery<{ count: number; results: DrilldownTicket[] }>({
    queryKey: ['tickets-drilldown', filter],
    queryFn: async () => (await apiClient.get('/tickets/', { params: buildParams(filter) })).data,
  });

  const tickets = data?.results ?? [];
  const groups = new Map<number, DrilldownTicket[]>();
  for (const ticket of tickets) {
    const list = groups.get(ticket.project) ?? [];
    list.push(ticket);
    groups.set(ticket.project, list);
  }
  const sortedProjectIds = [...groups.keys()].sort((a, b) =>
    (projectById.get(a)?.prefix ?? '').localeCompare(projectById.get(b)?.prefix ?? ''),
  );

  if (!position) return null;

  // .widget-card のフェードインアニメーションが transform を computed value として残すため
  // (animation ... both で translateY(0) が確定値になる)、fixed 配置の containing block が
  // ビューポートではなく .widget-card になってしまう。document.body へポータルして回避する。
  return createPortal(
    <div
      ref={popoverRef}
      className="drilldown-popover"
      style={{ top: position.top, left: position.left }}
      role="dialog"
      aria-label={label}
    >
      <div className="drilldown-popover__head">
        <span className="drilldown-popover__title">{label}</span>
        {data && (
          <span className="drilldown-popover__count">
            {t('dashboard.showingOf', { shown: tickets.length, total: data.count })}
          </span>
        )}
      </div>
      {isLoading && <div className="widget-list__empty">…</div>}
      {!isLoading && tickets.length === 0 && (
        <div className="widget-list__empty">🎉 {t('dashboard.noMatchingTickets')}</div>
      )}
      <div className="drilldown-popover__body">
        {sortedProjectIds.map((projectId) => {
          const project = projectById.get(projectId);
          const projectTickets = groups.get(projectId) ?? [];
          return (
            <div key={projectId}>
              {sortedProjectIds.length > 1 && (
                <div className="drilldown-popover__group-label">
                  {project ? `${project.name}（${project.prefix}）` : `#${projectId}`}
                </div>
              )}
              {projectTickets.map((ticket) => (
                <Link
                  key={ticket.id}
                  to={project ? `/p/${project.prefix}/tickets/${ticket.ticketKey}` : '#'}
                  className="widget-list__item drilldown-popover__row"
                  onClick={onClose}
                >
                  <span className="widget-priority">{priorityIcons[ticket.priority] ?? '—'}</span>
                  <span className="widget-list__text">
                    <strong>{ticket.ticketKey}</strong> {ticket.title}
                  </span>
                  {ticket.dueDate && (
                    <span className={`widget-due ${new Date(ticket.dueDate) < new Date() ? 'widget-due--overdue' : ''}`}>
                      {new Date(ticket.dueDate).toLocaleDateString()}
                    </span>
                  )}
                </Link>
              ))}
            </div>
          );
        })}
      </div>
    </div>,
    document.body,
  );
}
