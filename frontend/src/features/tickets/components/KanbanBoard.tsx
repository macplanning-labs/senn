/**
 * KanbanBoard.tsx — Kanban board view
 *
 * 4-column board: Open → In Progress → Resolved → Closed
 * HTML5 Drag & Drop for status changes.
 * Clicking a card opens the TicketDetailPanel.
 */

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router-dom';
import { useProject } from '@/shared/hooks/useProject';
import { useTeam } from '@/shared/hooks/useTeam';
import { useTicketList, type TicketListParams } from '@/shared/sync/repos/ticketRepo';
import type { LocalTicket } from '@/shared/sync/db';
import { syncStateOf } from '@/shared/sync/ticketMapping';
import { isTempTicketKey, localUpdateTicket } from '@/shared/sync/ticketWrites';
import { LabelList } from '@/shared/components/ui/LabelBadge';
import { useWorkflowStatuses } from '@/features/settings/hooks/useWorkflowStatuses';
import { TicketDetailPanel } from '@/features/tickets/components/TicketDetailPanel';
import '@/shared/sync/syncState.css';
import './KanbanBoard.css';
import { TeamTabPageHeader } from '@/features/teams/components/TeamTabPageHeader';
import { IconBoard } from '@/shared/components/layout/Sidebar';

// フォールバック用の固定カラム（ワークフローが未シードの場合）
const FALLBACK_COLUMNS = [
  { status: 'open', label: 'Open', color: 'var(--color-status-open)' },
  { status: 'in_progress', label: 'In Progress', color: 'var(--color-status-in-progress)' },
  { status: 'resolved', label: 'Resolved', color: 'var(--color-status-resolved)' },
  { status: 'closed', label: 'Closed', color: 'var(--color-status-closed)' },
] as const;

const priorityIcons: Record<string, string> = {
  urgent: '⬆⬆',
  high: '⬆',
  medium: '—',
  low: '⬇',
};

const priorityColors: Record<string, string> = {
  urgent: 'var(--color-priority-urgent)',
  high: 'var(--color-priority-high)',
  medium: 'var(--color-priority-medium)',
  low: 'var(--color-priority-low)',
};

export function KanbanBoard() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { projectKey, currentProject } = useProject();
  const { teamSlug, currentTeam } = useTeam();
  const [selectedTicketId, setSelectedTicketId] = useState<string | null>(null);
  const [dragOverColumn, setDragOverColumn] = useState<string | null>(null);
  const [draggingId, setDraggingId] = useState<string | null>(null);

  // プロジェクト/チーム固有ワークフローステータスを取得
  const { data: workflowStatuses = [] } = useWorkflowStatuses(
    currentProject?.id,
    currentTeam?.id
  );

  // 動的カラム生成: ワークフローがあればそれを使用、なければフォールバック
  const columns = workflowStatuses.length > 0
    ? workflowStatuses.map(ws => ({
        status: ws.slug,
        label: ws.name,
        color: ws.color,
      }))
    : FALLBACK_COLUMNS;

  // Fetch tickets
  const buildListParams = (): TicketListParams => {
    const params: TicketListParams = {};
    if (currentProject?.id) params.project = currentProject.id;
    if (teamSlug) params.team_slug = teamSlug;
    return params;
  };

  const { tickets, isLoading } = useTicketList(buildListParams());

  // ステータス更新 — 端末内 DB に即時反映、送信は裏側で行う
  const statusMutation = {
    mutate: async ({ ticketKey, status }: { ticketKey: string; status: string }) => {
      await localUpdateTicket(ticketKey, { status }, { status });
    },
    isPending: false,
  };

  // Group tickets by status
  const ticketsByStatus = columns.reduce(
    (acc, col) => {
      acc[col.status] = tickets.filter((t) => t.status === col.status);
      return acc;
    },
    {} as Record<string, LocalTicket[]>,
  );

  // Drag handlers
  function handleDragStart(e: React.DragEvent, ticketKey: string) {
    e.dataTransfer.setData('ticketKey', ticketKey);
    e.dataTransfer.effectAllowed = 'move';
    setDraggingId(ticketKey);
  }

  function handleDragEnd() {
    setDraggingId(null);
    setDragOverColumn(null);
  }

  function handleDragOver(e: React.DragEvent, status: string) {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
    setDragOverColumn(status);
  }

  function handleDragLeave() {
    setDragOverColumn(null);
  }

  function handleDrop(e: React.DragEvent, newStatus: string) {
    e.preventDefault();
    setDragOverColumn(null);
    const ticketKey = e.dataTransfer.getData('ticketKey');
    const ticket = tickets.find((t) => t.ticketKey === ticketKey);
    if (ticket && ticket.status !== newStatus) {
      statusMutation.mutate({ ticketKey, status: newStatus });
    }
  }

  const handleClosePanel = () => {
    setSelectedTicketId(null);
    if (projectKey) {
      navigate(`/project/${projectKey}/board`);
    } else if (teamSlug) {
      navigate(`/team/${teamSlug}/board`);
    }
  };

  return (
    <div className="kanban" data-testid="kanban-board">
      {/* Header */}
      <TeamTabPageHeader
        icon={IconBoard}
        title={t('nav.board')}
        actions={
          <div className="kanban__stats">
            {columns.map((col) => (
              <span key={col.status} className="kanban__stat">
                <span
                  className="kanban__stat-dot"
                  style={{ backgroundColor: col.color }}
                />
                {ticketsByStatus[col.status]?.length ?? 0}
              </span>
            ))}
          </div>
        }
      />

      {/* Board */}
      <div className="kanban__board">
        {isLoading ? (
          <div className="kanban__loading">Loading...</div>
        ) : (
          columns.map((col) => (
            <div
              key={col.status}
              className={`kanban__column ${dragOverColumn === col.status ? 'kanban__column--drag-over' : ''}`}
              onDragOver={(e) => handleDragOver(e, col.status)}
              onDragLeave={handleDragLeave}
              onDrop={(e) => handleDrop(e, col.status)}
              data-testid={`kanban-column-${col.status}`}
            >
              {/* Column header */}
              <div className="kanban__column-header">
                <span
                  className="kanban__column-indicator"
                  style={{ backgroundColor: col.color }}
                />
                <span className="kanban__column-title">{col.label}</span>
                <span className="kanban__column-count">
                  {ticketsByStatus[col.status]?.length ?? 0}
                </span>
              </div>

              {/* Cards */}
              <div className="kanban__cards">
                {ticketsByStatus[col.status]?.map((ticket) => {
                  const isTemp = isTempTicketKey(ticket.ticketKey);
                  return (
                  <div
                    key={ticket.id}
                    className={`kanban__card ${draggingId === ticket.ticketKey ? 'kanban__card--dragging' : ''}`}
                    draggable
                    onDragStart={(e) => handleDragStart(e, ticket.ticketKey)}
                    onDragEnd={handleDragEnd}
                    onClick={() => {
                      setSelectedTicketId(ticket.ticketKey);
                      if (projectKey) {
                        navigate(`/project/${projectKey}/board/${ticket.ticketKey}`);
                      } else if (teamSlug) {
                        navigate(`/team/${teamSlug}/board/${ticket.ticketKey}`);
                      }
                    }}
                    data-testid={`kanban-card-${ticket.ticketKey}`}
                    data-sync-state={syncStateOf(ticket)}
                  >
                    {/* Card header: key + priority */}
                    <div className="kanban__card-header">
                      {isTemp ? (
                        <span className="sync-badge sync-badge--creating">{t('sync.creating')}</span>
                      ) : (
                        <span className="kanban__card-key">{ticket.ticketKey}</span>
                      )}
                      <span
                        className="kanban__card-priority"
                        style={{ color: priorityColors[ticket.priority] }}
                        title={ticket.priority}
                      >
                        {priorityIcons[ticket.priority] ?? ''}
                      </span>
                    </div>

                    {/* Card title */}
                    <div className="kanban__card-title">{ticket.title}</div>

                    {/* Labels */}
                    {ticket.labels?.length > 0 && (
                      <div className="kanban__card-labels">
                        <LabelList labels={ticket.labels} max={2} />
                      </div>
                    )}

                    {/* Card footer: assignee + meta */}
                    <div className="kanban__card-footer">
                      {ticket.assignees?.length > 0 ? (
                        <span className="kanban__card-assignee">
                          {ticket.assignees.slice(0, 3).map((a) => (
                            <span key={a.id} className="kanban__card-avatar" title={a.displayName || a.username}>
                              {(a.displayName || a.username)[0]?.toUpperCase()}
                            </span>
                          ))}
                          {ticket.assignees.length > 3 && (
                            <span className="kanban__card-avatar kanban__card-avatar--more">+{ticket.assignees.length - 3}</span>
                          )}
                        </span>
                      ) : (
                        <span />
                      )}
                      <span className="kanban__card-meta">
                        {ticket.commentCount > 0 && (
                          <span title="Comments">💬 {ticket.commentCount}</span>
                        )}
                        {ticket.dueDate && (
                          <span
                            className={
                              new Date(ticket.dueDate) < new Date()
                                ? 'kanban__card-due--overdue'
                                : ''
                            }
                            title={`Due: ${ticket.dueDate}`}
                          >
                            📅
                          </span>
                        )}
                      </span>
                    </div>
                  </div>
                );
                })}
              </div>
            </div>
          ))
        )}
      </div>

      {/* Detail panel */}
      {selectedTicketId && (
        <TicketDetailPanel
          ticketId={selectedTicketId}
          onClose={handleClosePanel}
        />
      )}
    </div>
  );
}
