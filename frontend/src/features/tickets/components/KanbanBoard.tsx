/**
 * KanbanBoard.tsx — Kanban board view
 *
 * 4-column board: Open → In Progress → Resolved → Closed
 * HTML5 Drag & Drop for status changes.
 * Clicking a card opens the TicketDetailPanel.
 */

import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { apiClient } from '@/shared/api/client';
import { useProject } from '@/shared/hooks/useProject';
import { useOptimisticMutation } from '@/shared/hooks/useOptimisticMutation';
import { TICKET_DASHBOARD_INVALIDATE_KEYS } from '@/shared/utils/ticketQueryInvalidation';
import { LabelList } from '@/shared/components/ui/LabelBadge';
import type { Label } from '@/shared/components/ui/LabelBadge';
import { useWorkflowStatuses } from '@/features/settings/hooks/useWorkflowStatuses';
import { TicketDetailPanel } from '@/features/tickets/components/TicketDetailPanel';
import './KanbanBoard.css';

interface KanbanTicket {
  id: number;
  ticketKey: string;
  title: string;
  status: string;
  priority: string;
  assignees: { id: number; username: string; displayName: string }[];
  labels: Label[];
  dueDate: string | null;
  commentCount: number;
}

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
  const navigate = useNavigate();
  const { projectKey, currentProject } = useProject();
  const [selectedTicketId, setSelectedTicketId] = useState<string | null>(null);
  const [dragOverColumn, setDragOverColumn] = useState<string | null>(null);
  const [draggingId, setDraggingId] = useState<string | null>(null);

  // プロジェクト固有ワークフローステータスを取得
  const { data: workflowStatuses = [] } = useWorkflowStatuses(currentProject?.id);

  // 動的カラム生成: ワークフローがあればそれを使用、なければフォールバック
  const columns = workflowStatuses.length > 0
    ? workflowStatuses.map(ws => ({
        status: ws.slug,
        label: ws.name,
        color: ws.color,
      }))
    : FALLBACK_COLUMNS;

  const queryKey = ['tickets', 'kanban', currentProject?.id] as const;

  // Fetch tickets
  const { data, isLoading } = useQuery<{ results: KanbanTicket[] }>({
    queryKey,
    queryFn: async () => {
      const params: Record<string, string> = {};
      if (currentProject?.id) params.project = String(currentProject.id);
      const res = await apiClient.get<{ results: KanbanTicket[] }>('/tickets/', { params });
      return res.data;
    },
  });

  const tickets = data?.results ?? [];

  // 楽観的ステータス更新 — D&D時にカードが即座に移動先カラムに表示（0ms）
  const statusMutation = useOptimisticMutation<void, { ticketKey: string; status: string }>({
    mutationFn: async ({ ticketKey, status }) => {
      await apiClient.patch(`/tickets/${ticketKey}/`, { status });
    },
    queryKey,
    updater: (currentData, { ticketKey, status }) => {
      const data = currentData as { results: KanbanTicket[] } | undefined;
      if (!data?.results) return currentData;
      return {
        ...data,
        results: data.results.map((t) =>
          t.ticketKey === ticketKey ? { ...t, status } : t,
        ),
      };
    },
    // ['ticket'] も無効化: 開いている詳細パネルが古いステータスのまま残るのを防ぐ
    invalidateKeys: [...TICKET_DASHBOARD_INVALIDATE_KEYS, ['ticket']],
    errorMessage: 'ステータス変更に失敗しました。元に戻しました。',
  });

  // Group tickets by status
  const ticketsByStatus = columns.reduce(
    (acc, col) => {
      acc[col.status] = tickets.filter((t) => t.status === col.status);
      return acc;
    },
    {} as Record<string, KanbanTicket[]>,
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
    if (projectKey) navigate(`/p/${projectKey}/board`);
  };

  return (
    <div className="kanban" data-testid="kanban-board">
      {/* Header */}
      <div className="kanban__header">
        <h1 className="kanban__title">Board</h1>
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
      </div>

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
                {ticketsByStatus[col.status]?.map((ticket) => (
                  <div
                    key={ticket.id}
                    className={`kanban__card ${draggingId === ticket.ticketKey ? 'kanban__card--dragging' : ''}`}
                    draggable
                    onDragStart={(e) => handleDragStart(e, ticket.ticketKey)}
                    onDragEnd={handleDragEnd}
                    onClick={() => {
                      setSelectedTicketId(ticket.ticketKey);
                      if (projectKey) {
                        navigate(`/p/${projectKey}/board/${ticket.ticketKey}`);
                      }
                    }}
                    data-testid={`kanban-card-${ticket.ticketKey}`}
                  >
                    {/* Card header: key + priority */}
                    <div className="kanban__card-header">
                      <span className="kanban__card-key">{ticket.ticketKey}</span>
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
                ))}
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
