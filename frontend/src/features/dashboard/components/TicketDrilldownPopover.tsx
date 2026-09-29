/**
 * TicketDrilldownPopover.tsx — ダッシュボード統計からのドリルダウン
 *
 * 統計カード/チケット概要の凡例クリックで開き、条件に合致するチケットを
 * プロジェクト別にグルーピングして一覧表示する。行クリックで対象チケットの
 * 詳細パネル(/project/:projectKey/tickets/:ticketId)へ遷移する。
 */
import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { Link } from 'react-router-dom';
import { useProjects } from '@/shared/sync/repos/projectRepo';
import { useTicketList } from '@/shared/sync/repos/ticketRepo';
import { localDateStr } from '@/shared/utils/localDateStr';
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
  project: number | null;
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

  // プロジェクト一覧を取得
  const { projects } = useProjects();
  const projectById = new Map(projects.map((p) => [p.id, p]));

  // チケット一覧を取得
  const { tickets: rows, isLoading } = useTicketList(buildParams(filter));

  // DrilldownTicket 形式に変換
  const tickets: DrilldownTicket[] = rows.map((r) => ({
    id: r.id,
    ticketKey: r.ticketKey,
    title: r.title,
    priority: r.priority,
    dueDate: r.dueDate,
    project: r.projectId,
  }));

  const groups = new Map<number | null, DrilldownTicket[]>();
  for (const ticket of tickets) {
    const list = groups.get(ticket.project) ?? [];
    list.push(ticket);
    groups.set(ticket.project, list);
  }
  const sortedProjectIds = [...groups.keys()].sort((a, b) => {
    const aPrefix = a !== null ? projectById.get(a)?.prefix ?? '' : '';
    const bPrefix = b !== null ? projectById.get(b)?.prefix ?? '' : '';
    if (aPrefix === '' && bPrefix === '') return 0;
    if (aPrefix === '') return 1; // null は最後
    if (bPrefix === '') return -1;
    return aPrefix.localeCompare(bPrefix);
  });

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
        {!isLoading && tickets.length > 0 && (
          <span className="drilldown-popover__count">
            {t('dashboard.showingOf', { shown: tickets.length, total: tickets.length })}
          </span>
        )}
      </div>
      {isLoading && <div className="widget-list__empty">…</div>}
      {!isLoading && tickets.length === 0 && (
        <div className="widget-list__empty">🎉 {t('dashboard.noMatchingTickets')}</div>
      )}
      <div className="drilldown-popover__body">
        {sortedProjectIds.map((projectId) => {
          const project = projectId !== null ? projectById.get(projectId) : undefined;
          const projectTickets = groups.get(projectId) ?? [];
          return (
            <div key={projectId ?? 'null'}>
              {sortedProjectIds.length > 1 && (
                <div className="drilldown-popover__group-label">
                  {project ? `${project.name}（${project.prefix}）` : t('dashboard.noProjectGroup')}
                </div>
              )}
              {projectTickets.map((ticket) => (
                <Link
                  key={ticket.id}
                  to={project ? `/project/${project.prefix}/tickets/${ticket.ticketKey}` : '#'}
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
