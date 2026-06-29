/**
 * TicketTable.tsx — チケット一覧テーブル
 *
 * フィルタ + 検索 + ソート対応のテーブルビュー。
 * Linearスタイルの行ホバー + インラインステータス変更。
 */

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import { apiClient } from '@/shared/api/client';
import './TicketTable.css';

interface Ticket {
  id: number;
  ticketKey: string;
  title: string;
  status: string;
  priority: string;
  ticketType: string;
  assignee: { id: number; username: string; displayName: string } | null;
  dueDate: string | null;
  updatedAt: string;
  commentCount: number;
  childCount: number;
}

const STATUS_OPTIONS = ['open', 'in_progress', 'resolved', 'closed'] as const;
const PRIORITY_OPTIONS = ['urgent', 'high', 'medium', 'low'] as const;

const statusLabels: Record<string, string> = {
  open: 'Open',
  in_progress: 'In Progress',
  resolved: 'Resolved',
  closed: 'Closed',
};

const statusColors: Record<string, string> = {
  open: 'var(--color-status-open)',
  in_progress: 'var(--color-status-in-progress)',
  resolved: 'var(--color-status-resolved)',
  closed: 'var(--color-status-closed)',
};

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

export function TicketTable() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();

  // フィルタ状態
  const [search, setSearch] = useState('');
  const [statusFilter, setStatusFilter] = useState<string>('');
  const [priorityFilter, setPriorityFilter] = useState<string>('');

  // チケット取得
  const { data, isLoading } = useQuery<{ results: Ticket[] }>({
    queryKey: ['tickets', search, statusFilter, priorityFilter],
    queryFn: async () => {
      const params: Record<string, string> = {};
      if (search) params.search = search;
      if (statusFilter) params.status = statusFilter;
      if (priorityFilter) params.priority = priorityFilter;
      const res = await apiClient.get<{ results: Ticket[] }>('/tickets/', { params });
      return res.data;
    },
  });

  // ステータス変更
  const statusMutation = useMutation({
    mutationFn: async ({ id, status }: { id: number; status: string }) => {
      await apiClient.patch(`/tickets/${id}/`, { status });
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['tickets'] });
    },
  });

  const tickets = data?.results ?? [];

  return (
    <div className="ticket-table" data-testid="ticket-table-page">
      {/* ヘッダー */}
      <div className="ticket-table__header">
        <h1 className="ticket-table__title">{t('nav.tickets')}</h1>
        <Link to="/tickets/new" className="ticket-table__create-btn" data-testid="create-ticket-btn">
          + {t('ticket.create')}
        </Link>
      </div>

      {/* フィルタバー */}
      <div className="ticket-table__filters" data-testid="ticket-filters">
        <input
          type="text"
          className="ticket-table__search"
          placeholder={t('common.search')}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          data-testid="ticket-search"
        />
        <select
          className="ticket-table__filter-select"
          value={statusFilter}
          onChange={(e) => setStatusFilter(e.target.value)}
          data-testid="status-filter"
        >
          <option value="">All Status</option>
          {STATUS_OPTIONS.map((s) => (
            <option key={s} value={s}>{statusLabels[s]}</option>
          ))}
        </select>
        <select
          className="ticket-table__filter-select"
          value={priorityFilter}
          onChange={(e) => setPriorityFilter(e.target.value)}
          data-testid="priority-filter"
        >
          <option value="">All Priority</option>
          {PRIORITY_OPTIONS.map((p) => (
            <option key={p} value={p}>{p}</option>
          ))}
        </select>
      </div>

      {/* テーブル */}
      <div className="ticket-table__container">
        {isLoading ? (
          <div className="ticket-table__loading">
            {Array.from({ length: 5 }).map((_, i) => (
              <div key={i} className="ticket-table__skeleton-row" />
            ))}
          </div>
        ) : !tickets.length ? (
          <div className="ticket-table__empty" data-testid="no-tickets">
            {t('common.noResults')}
          </div>
        ) : (
          <table className="ticket-table__table" data-testid="ticket-list">
            <thead>
              <tr>
                <th className="ticket-table__th ticket-table__th--priority"></th>
                <th className="ticket-table__th ticket-table__th--key">Key</th>
                <th className="ticket-table__th ticket-table__th--title">Title</th>
                <th className="ticket-table__th ticket-table__th--status">Status</th>
                <th className="ticket-table__th ticket-table__th--assignee">Assignee</th>
                <th className="ticket-table__th ticket-table__th--due">Due</th>
                <th className="ticket-table__th ticket-table__th--meta"></th>
              </tr>
            </thead>
            <tbody>
              {tickets.map((ticket) => (
                <tr
                  key={ticket.id}
                  className="ticket-table__row"
                  data-testid={`ticket-row-${ticket.ticketKey}`}
                >
                  {/* 優先度 */}
                  <td className="ticket-table__td ticket-table__td--priority">
                    <span style={{ color: priorityColors[ticket.priority] ?? 'inherit' }}>
                      {priorityIcons[ticket.priority] ?? ''}
                    </span>
                  </td>

                  {/* キー */}
                  <td className="ticket-table__td ticket-table__td--key">
                    <Link to={`/tickets/${ticket.id}`} className="ticket-table__key-link">
                      {ticket.ticketKey}
                    </Link>
                  </td>

                  {/* タイトル */}
                  <td className="ticket-table__td ticket-table__td--title">
                    <Link to={`/tickets/${ticket.id}`} className="ticket-table__title-link">
                      {ticket.title}
                    </Link>
                  </td>

                  {/* ステータス（インライン変更） */}
                  <td className="ticket-table__td ticket-table__td--status">
                    <select
                      className="ticket-table__status-select"
                      value={ticket.status}
                      onChange={(e) => {
                        statusMutation.mutate({ id: ticket.id, status: e.target.value });
                      }}
                      style={{
                        color: statusColors[ticket.status] ?? 'inherit',
                        borderColor: statusColors[ticket.status] ?? 'transparent',
                      }}
                      data-testid={`status-select-${ticket.ticketKey}`}
                    >
                      {STATUS_OPTIONS.map((s) => (
                        <option key={s} value={s}>{statusLabels[s]}</option>
                      ))}
                    </select>
                  </td>

                  {/* 担当者 */}
                  <td className="ticket-table__td ticket-table__td--assignee">
                    {ticket.assignee ? (
                      <span className="ticket-table__assignee">
                        <span className="ticket-table__avatar">
                          {(ticket.assignee.displayName || ticket.assignee.username)[0]?.toUpperCase()}
                        </span>
                        {ticket.assignee.displayName || ticket.assignee.username}
                      </span>
                    ) : (
                      <span className="ticket-table__unassigned">—</span>
                    )}
                  </td>

                  {/* 期限 */}
                  <td className="ticket-table__td ticket-table__td--due">
                    {ticket.dueDate ? (
                      <span
                        className={
                          new Date(ticket.dueDate) < new Date()
                            ? 'ticket-table__due--overdue'
                            : ''
                        }
                      >
                        {new Date(ticket.dueDate).toLocaleDateString()}
                      </span>
                    ) : (
                      '—'
                    )}
                  </td>

                  {/* メタ情報 */}
                  <td className="ticket-table__td ticket-table__td--meta">
                    {ticket.commentCount > 0 && (
                      <span className="ticket-table__meta-badge" title="Comments">
                        💬 {ticket.commentCount}
                      </span>
                    )}
                    {ticket.childCount > 0 && (
                      <span className="ticket-table__meta-badge" title="Subtasks">
                        📋 {ticket.childCount}
                      </span>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
