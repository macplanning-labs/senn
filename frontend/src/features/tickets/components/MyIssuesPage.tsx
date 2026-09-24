/**
 * MyIssuesPage.tsx — 自分のチケット一覧（ログイン直後の入口）
 *
 * 担当チケット一覧 + 右ペイン。
 * キーボード: j/k 選択、Enter 開く、Esc 閉じる、C 作成。
 * 画面上の呼称は「チケット」（Issue にしない）。
 */

import { useEffect, useRef, useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { apiClient } from '@/shared/api/client';
import { useKeyboardNav } from '@/shared/hooks/useKeyboardNav';
import { usePanelResize } from '@/shared/hooks/usePanelResize';
import { useUIStore } from '@/shared/stores/uiStore';
import { getLastTeamSlug, useTeam } from '@/shared/hooks/useTeam';
import { getLastProjectKey, useProject } from '@/shared/hooks/useProject';
import { useTeams } from '@/features/teams/hooks/useTeams';
import { GettingStartedChecklist } from '@/features/onboarding/components/GettingStartedChecklist';
import { TicketDetailPanel } from './TicketDetailPanel';
import './MyIssuesPage.css';

interface MyTicket {
  id: number;
  ticket_key: string;
  title: string;
  status: string;
  priority: string;
  due_date: string | null;
  updated_at: string;
  project_key: string | null;
}

const priorityIcons: Record<string, string> = {
  urgent: '⚠',
  high: '▮▮▮',
  medium: '▮▮',
  low: '▮',
};

export function MyIssuesPage() {
  const { t } = useTranslation();
  const { ticketId } = useParams<{ ticketId?: string }>();
  const navigate = useNavigate();
  const { openTicketFormModal } = useUIStore();
  const { width: panelWidth, onResizeStart, isResizing } = usePanelResize('ticket-detail', 380);
  const listRef = useRef<HTMLDivElement>(null);

  const {
    data: tickets = [],
    isLoading,
    isError,
    refetch,
  } = useQuery<MyTicket[]>({
    queryKey: ['my-issues'],
    queryFn: async () => {
      const res = await apiClient.get<MyTicket[]>('/dashboard/my-tickets/', {
        params: { limit: 50 },
      });
      return res.data;
    },
  });

  const { isLoading: teamsLoading } = useTeams();
  const { teamList, isLoading: teamsHookLoading } = useTeam();
  const { projectList, isLoading: projectsLoading } = useProject();

  const hasTeam = teamList.length > 0;
  const hasProject = projectList.length > 0;
  // サンプル生成後はプロジェクトが1件になるが、完了メッセージを残すためチェックリストを表示し続ける
  const [demoPrefix, setDemoPrefix] = useState<string | null>(null);

  const { selectedIndex } = useKeyboardNav({
    itemCount: tickets.length,
    enabled: true,
    onOpen: (index) => {
      const ticket = tickets[index];
      if (ticket) navigate(`/my-issues/${ticket.ticket_key}`);
    },
    onClose: () => navigate('/my-issues'),
    onCreate: () => {
      const teamSlug = getLastTeamSlug();
      const projectKey = getLastProjectKey();
      if (teamSlug) {
        openTicketFormModal(null, teamSlug);
      } else if (projectKey) {
        openTicketFormModal(projectKey);
      } else {
        openTicketFormModal(null);
      }
    },
  });

  useEffect(() => {
    if (selectedIndex < 0) return;
    const rows = listRef.current?.querySelectorAll('.my-issues__row');
    rows?.[selectedIndex]?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  }, [selectedIndex]);

  const handleClosePanel = () => navigate('/my-issues');

  return (
    <div
      className={`my-issues ${ticketId ? 'my-issues--with-panel' : ''} ${isResizing ? 'my-issues--resizing' : ''}`}
      data-testid="my-issues-page"
      style={ticketId ? { gridTemplateColumns: `1fr ${panelWidth}px` } : undefined}
    >
      <div className="my-issues__main" ref={listRef}>
        <header className="my-issues__header">
          <h1 className="my-issues__title">{t('nav.myIssues')}</h1>
          <div className="my-issues__tabs">
            <span className="my-issues__tab my-issues__tab--active">{t('myIssues.assigned')}</span>
          </div>
        </header>

        {isLoading || teamsLoading || teamsHookLoading || projectsLoading ? (
          <div className="my-issues__empty">{t('common.loading')}</div>
        ) : isError ? (
          <div className="my-issues__empty" data-testid="my-issues-error">
            <p>{t('common.error')}</p>
            <button type="button" className="my-issues__create-btn" onClick={() => refetch()}>
              {t('common.retry')}
            </button>
          </div>
        ) : tickets.length === 0 ? (
          <div className="my-issues__empty" data-testid="my-issues-empty">
            {!hasTeam || !hasProject || demoPrefix ? (
              <GettingStartedChecklist createdPrefix={demoPrefix} onDemoCreated={setDemoPrefix} />
            ) : (
              <>
                <p>{t('myIssues.empty')}</p>
                <button
                  type="button"
                  className="my-issues__create-btn"
                  onClick={() => {
                    const teamSlug = getLastTeamSlug();
                    const projectKey = getLastProjectKey();
                    if (teamSlug) openTicketFormModal(null, teamSlug);
                    else if (projectKey) openTicketFormModal(projectKey);
                    else openTicketFormModal(null);
                  }}
                >
                  {t('ticket.create')}
                </button>
                <p className="my-issues__hint">
                  <kbd>C</kbd> {t('myIssues.createHint')}
                </p>
              </>
            )}
          </div>
        ) : (
          <ul className="my-issues__list">
            {tickets.map((ticket, index) => {
              const selected = selectedIndex === index || ticketId === ticket.ticket_key;
              return (
                <li key={ticket.id}>
                  <button
                    type="button"
                    className={`my-issues__row ${selected ? 'my-issues__row--selected' : ''}`}
                    onClick={() => navigate(`/my-issues/${ticket.ticket_key}`)}
                    data-testid={`my-issue-${ticket.ticket_key}`}
                  >
                    <span className="my-issues__priority" title={ticket.priority}>
                      {priorityIcons[ticket.priority] ?? '—'}
                    </span>
                    <span className="my-issues__key">{ticket.ticket_key}</span>
                    <span className="my-issues__row-title">{ticket.title}</span>
                    <span className="my-issues__status">{ticket.status.replace('_', ' ')}</span>
                    {ticket.due_date && (
                      <span
                        className={`my-issues__due ${
                          new Date(ticket.due_date) < new Date() ? 'my-issues__due--overdue' : ''
                        }`}
                      >
                        {ticket.due_date}
                      </span>
                    )}
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </div>

      {ticketId && (
        <div className="my-issues__panel">
          <div
            className="my-issues__resize-handle"
            onMouseDown={onResizeStart}
            data-testid="panel-resize-handle"
          />
          <TicketDetailPanel ticketId={ticketId} onClose={handleClosePanel} />
        </div>
      )}
    </div>
  );
}
