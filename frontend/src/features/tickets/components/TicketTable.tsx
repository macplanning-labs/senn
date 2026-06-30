/**
 * TicketTable.tsx — チケット一覧テーブル
 *
 * フィルタ + 検索 + ソート対応のテーブルビュー。
 * Linearスタイルの行ホバー + インラインステータス変更。
 */

import { useState, useRef, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { useQuery } from '@tanstack/react-query';
import { Link, useNavigate } from 'react-router-dom';
import { apiClient } from '@/shared/api/client';
import { useProject } from '@/shared/hooks/useProject';
import { useOptimisticMutation } from '@/shared/hooks/useOptimisticMutation';
import { useKeyboardNav } from '@/shared/hooks/useKeyboardNav';
import { InlineEdit } from '@/shared/components/ui/InlineEdit';
import { LabelList } from '@/shared/components/ui/LabelBadge';
import type { Label } from '@/shared/components/ui/LabelBadge';
import './TicketTable.css';
import { useColumnResize } from '@/shared/hooks/useColumnResize';
import type { ColumnDef } from '@/shared/hooks/useColumnResize';

interface Ticket {
  id: number;
  ticketKey: string;
  title: string;
  status: string;
  priority: string;
  ticketType: string;
  assignees: { id: number; username: string; displayName: string }[];
  labels: Label[];
  dueDate: string | null;
  updatedAt: string;
  commentCount: number;
  childCount: number;
}

const STATUS_OPTIONS = ['backlog', 'open', 'in_progress', 'resolved', 'closed', 'canceled'] as const;
const PRIORITY_OPTIONS = ['urgent', 'high', 'medium', 'low'] as const;

const statusLabels: Record<string, string> = {
  backlog: 'Backlog',
  open: 'Open',
  in_progress: 'In Progress',
  resolved: 'Resolved',
  closed: 'Closed',
  canceled: 'Canceled',
};

const statusColors: Record<string, string> = {
  backlog: 'var(--color-status-backlog, #6b7280)',
  open: 'var(--color-status-open)',
  in_progress: 'var(--color-status-in-progress)',
  resolved: 'var(--color-status-resolved)',
  closed: 'var(--color-status-closed)',
  canceled: 'var(--color-status-canceled, #9ca3af)',
};

const priorityIcons: Record<string, string> = {
  urgent: '⚠',
  high: '▮▮▮',
  medium: '▮▮',
  low: '▮',
};

const priorityColors: Record<string, string> = {
  urgent: 'var(--color-priority-urgent)',
  high: 'var(--color-priority-high)',
  medium: 'var(--color-priority-medium)',
  low: 'var(--color-priority-low)',
};

// --- カラム定義（リサイズ対応）---
const TABLE_COLUMNS: ColumnDef[] = [
  { key: 'priority', defaultWidth: 50, minWidth: 40 },
  { key: 'key', defaultWidth: 100, minWidth: 70 },
  { key: 'title', defaultWidth: 320, minWidth: 120 },
  { key: 'status', defaultWidth: 120, minWidth: 80 },
  { key: 'assignee', defaultWidth: 140, minWidth: 80 },
  { key: 'labels', defaultWidth: 140, minWidth: 60 },
  { key: 'due', defaultWidth: 100, minWidth: 70 },
  { key: 'meta', defaultWidth: 80, minWidth: 50 },
];

export function TicketTable() {
  const { t } = useTranslation();
  const { projectKey, currentProject } = useProject();

  // フィルタ状態
  const [search, setSearch] = useState('');
  const [statusFilter, setStatusFilter] = useState<string>('');
  const [priorityFilter, setPriorityFilter] = useState<string>('');

  // QueryKey（フィルタ状態を含む）
  const ticketsQueryKey = ['tickets', currentProject?.id, search, statusFilter, priorityFilter];

  // --- フィルタプリセット ---
  interface FilterPreset {
    name: string;
    search: string;
    status: string;
    priority: string;
  }
  const PRESET_KEY = `filter-presets-${projectKey ?? 'global'}`;
  const [presets, setPresets] = useState<FilterPreset[]>(() => {
    try {
      return JSON.parse(localStorage.getItem(PRESET_KEY) || '[]');
    } catch { return []; }
  });
  const [showPresetMenu, setShowPresetMenu] = useState(false);
  const presetRef = useRef<HTMLDivElement>(null);

  // 外部クリックでプリセットメニューを閉じる
  useEffect(() => {
    if (!showPresetMenu) return;
    const handleClickOutside = (e: MouseEvent) => {
      if (presetRef.current && !presetRef.current.contains(e.target as Node)) {
        setShowPresetMenu(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [showPresetMenu]);

  const savePreset = () => {
    const name = prompt('フィルタプリセット名を入力:');
    if (!name?.trim()) return;
    const newPresets = [...presets, { name: name.trim(), search, status: statusFilter, priority: priorityFilter }];
    setPresets(newPresets);
    localStorage.setItem(PRESET_KEY, JSON.stringify(newPresets));
    setShowPresetMenu(false);
  };

  const applyPreset = (preset: FilterPreset) => {
    setSearch(preset.search);
    setStatusFilter(preset.status);
    setPriorityFilter(preset.priority);
    setShowPresetMenu(false);
  };

  const deletePreset = (index: number) => {
    const newPresets = presets.filter((_, i) => i !== index);
    setPresets(newPresets);
    localStorage.setItem(PRESET_KEY, JSON.stringify(newPresets));
  };

  // --- バルク操作 ---
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());

  const toggleSelect = (id: number) => {
    setSelectedIds(prev => {
      const next = new Set(prev);
      next.has(id) ? next.delete(id) : next.add(id);
      return next;
    });
  };

  const toggleSelectAll = () => {
    if (selectedIds.size === tickets.length) {
      setSelectedIds(new Set());
    } else {
      setSelectedIds(new Set(tickets.map(t => t.id)));
    }
  };

  const bulkStatusMutation = useOptimisticMutation<void, { ids: number[]; status: string }>({
    mutationFn: async ({ ids, status }) => {
      await Promise.all(ids.map(id => apiClient.patch(`/tickets/${id}/`, { status })));
    },
    queryKey: ticketsQueryKey,
    updater: (currentData, { ids, status }) => {
      const data = currentData as { results: Ticket[] } | undefined;
      if (!data?.results) return currentData;
      const idSet = new Set(ids);
      return {
        ...data,
        results: data.results.map((t) =>
          idSet.has(t.id) ? { ...t, status } : t,
        ),
      };
    },
    onSuccessCallback: () => {
      setSelectedIds(new Set());
    },
    invalidateKeys: [['tickets']],
    errorMessage: '一括ステータス変更に失敗しました。元に戻しました。',
  });

  // カラムリサイズ
  const { widths, onResizeStart, isResizing } = useColumnResize('ticket-table', TABLE_COLUMNS);

  // チケット取得
  const { data, isLoading } = useQuery<{ results: Ticket[] }>({
    queryKey: ticketsQueryKey,
    queryFn: async () => {
      const params: Record<string, string> = {};
      if (currentProject?.id) params.project = String(currentProject.id);
      if (search) params.search = search;
      if (statusFilter) params.status = statusFilter;
      if (priorityFilter) params.priority = priorityFilter;
      const res = await apiClient.get<{ results: Ticket[] }>('/tickets/', { params });
      return res.data;
    },
  });

  // 楽観的ステータス変更 — ドロップダウン変更の瞬間にテーブル行が即更新（0ms）
  const statusMutation = useOptimisticMutation<void, { id: number; status: string }>({
    mutationFn: async ({ id, status }) => {
      await apiClient.patch(`/tickets/${id}/`, { status });
    },
    queryKey: ticketsQueryKey,
    updater: (currentData, { id, status }) => {
      const data = currentData as { results: Ticket[] } | undefined;
      if (!data?.results) return currentData;
      return {
        ...data,
        results: data.results.map((t) =>
          t.id === id ? { ...t, status } : t,
        ),
      };
    },
    invalidateKeys: [['tickets']],
    errorMessage: 'ステータス変更に失敗しました。元に戻しました。',
  });

  const tickets = data?.results ?? [];
  const navigate = useNavigate();
  const tableRef = useRef<HTMLDivElement>(null);

  // キーボードナビゲーション
  const { selectedIndex } = useKeyboardNav({
    itemCount: tickets.length,
    onOpen: (index) => {
      const ticket = tickets[index];
      if (ticket && projectKey) {
        navigate(`/p/${projectKey}/tickets/${ticket.id}`);
      }
    },
    onClose: () => {
      if (projectKey) navigate(`/p/${projectKey}/tickets`);
    },
    onCreate: () => {
      if (projectKey) navigate(`/p/${projectKey}/tickets/new`);
    },
  });

  // 選択行のスクロール追従
  useEffect(() => {
    if (selectedIndex < 0) return;
    const rows = tableRef.current?.querySelectorAll('.ticket-table__row');
    rows?.[selectedIndex]?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  }, [selectedIndex]);

  return (
    <div className="ticket-table" data-testid="ticket-table-page">
      {/* ヘッダー */}
      <div className="ticket-table__header">
        <h1 className="ticket-table__title">{t('nav.tickets')}</h1>
        <Link to={projectKey ? `/p/${projectKey}/tickets/new` : '/tickets/new'} className="ticket-table__create-btn" data-testid="create-ticket-btn">
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

        {/* プリセット */}
        <div className="ticket-table__preset-wrapper" ref={presetRef} style={{ position: 'relative' }}>
          <button
            className="ticket-table__preset-btn"
            onClick={() => setShowPresetMenu(!showPresetMenu)}
            data-testid="preset-toggle"
            title="フィルタプリセット"
          >
            ⭐ {presets.length > 0 && <span className="ticket-table__preset-count">{presets.length}</span>}
          </button>
          {showPresetMenu && (
            <div className="ticket-table__preset-menu">
              {presets.length === 0 ? (
                <div className="ticket-table__preset-empty">保存済みプリセットなし</div>
              ) : (
                presets.map((p, i) => (
                  <div key={i} className="ticket-table__preset-item">
                    <button className="ticket-table__preset-apply" onClick={() => applyPreset(p)}>
                      {p.name}
                      <span className="ticket-table__preset-detail">
                        {[p.status && statusLabels[p.status], p.priority, p.search && `"${p.search}"`].filter(Boolean).join(' · ') || 'All'}
                      </span>
                    </button>
                    <button className="ticket-table__preset-delete" onClick={() => deletePreset(i)} title="削除">×</button>
                  </div>
                ))
              )}
              <button className="ticket-table__preset-save" onClick={savePreset} data-testid="save-preset-btn">
                + 現在のフィルタを保存
              </button>
            </div>
          )}
        </div>
      </div>

      {/* バルクアクションバー */}
      {selectedIds.size > 0 && (
        <div className="ticket-table__bulk-bar" data-testid="bulk-actions">
          <span className="ticket-table__bulk-count">{selectedIds.size}件選択中</span>
          <select
            className="ticket-table__bulk-select"
            defaultValue=""
            onChange={(e) => {
              if (e.target.value) {
                bulkStatusMutation.mutate({ ids: Array.from(selectedIds), status: e.target.value });
                e.target.value = '';
              }
            }}
            data-testid="bulk-status-select"
          >
            <option value="">ステータス変更...</option>
            {STATUS_OPTIONS.map((s) => (
              <option key={s} value={s}>{statusLabels[s]}</option>
            ))}
          </select>
          <button
            className="ticket-table__bulk-clear"
            onClick={() => setSelectedIds(new Set())}
          >
            選択解除
          </button>
        </div>
      )}

      {/* テーブル */}
      <div className="ticket-table__container" ref={tableRef}>
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
          <table className={`ticket-table__table ${isResizing ? 'ticket-table__table--resizing' : ''}`} data-testid="ticket-list" style={{ tableLayout: 'fixed' }}>
            <colgroup>
              {TABLE_COLUMNS.map((col) => (
                <col key={col.key} style={{ width: widths[col.key] ?? col.defaultWidth }} />
              ))}
            </colgroup>
            <thead>
              <tr>
                <th className="ticket-table__th ticket-table__th--priority">
                  <input
                    type="checkbox"
                    className="ticket-table__checkbox"
                    checked={tickets.length > 0 && selectedIds.size === tickets.length}
                    onChange={toggleSelectAll}
                    data-testid="select-all-checkbox"
                  />
                </th>
                <th className="ticket-table__th ticket-table__th--key">
                  Key
                  <span className="ticket-table__resize-handle" onMouseDown={(e) => onResizeStart('key', e)} />
                </th>
                <th className="ticket-table__th ticket-table__th--title">
                  Title
                  <span className="ticket-table__resize-handle" onMouseDown={(e) => onResizeStart('title', e)} />
                </th>
                <th className="ticket-table__th ticket-table__th--status">
                  Status
                  <span className="ticket-table__resize-handle" onMouseDown={(e) => onResizeStart('status', e)} />
                </th>
                <th className="ticket-table__th ticket-table__th--assignee">
                  Assignee
                  <span className="ticket-table__resize-handle" onMouseDown={(e) => onResizeStart('assignee', e)} />
                </th>
                <th className="ticket-table__th ticket-table__th--labels">
                  Labels
                  <span className="ticket-table__resize-handle" onMouseDown={(e) => onResizeStart('labels', e)} />
                </th>
                <th className="ticket-table__th ticket-table__th--due">
                  Due
                  <span className="ticket-table__resize-handle" onMouseDown={(e) => onResizeStart('due', e)} />
                </th>
                <th className="ticket-table__th ticket-table__th--meta"></th>
              </tr>
            </thead>
            <tbody>
              {tickets.map((ticket, index) => (
                <tr
                  key={ticket.id}
                  className={`ticket-table__row ${index === selectedIndex ? 'ticket-table__row--selected' : ''}`}
                  data-testid={`ticket-row-${ticket.ticketKey}`}
                  onClick={() => {
                    if (projectKey) navigate(`/p/${projectKey}/tickets/${ticket.id}`);
                  }}
                  style={{ cursor: 'pointer' }}
                >
                  {/* チェックボックス + 優先度 */}
                  <td className="ticket-table__td ticket-table__td--priority">
                    <span style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                      <input
                        type="checkbox"
                        className="ticket-table__checkbox"
                        checked={selectedIds.has(ticket.id)}
                        onChange={(e) => { e.stopPropagation(); toggleSelect(ticket.id); }}
                        onClick={(e) => e.stopPropagation()}
                      />
                      <span
                        style={{ color: priorityColors[ticket.priority] ?? 'inherit', cursor: 'help' }}
                        title={`Priority: ${ticket.priority}`}
                      >
                        {priorityIcons[ticket.priority] ?? ''}
                      </span>
                    </span>
                  </td>

                  {/* キー */}
                  <td className="ticket-table__td ticket-table__td--key">
                    <Link to={projectKey ? `/p/${projectKey}/tickets/${ticket.id}` : `/tickets/${ticket.id}`} className="ticket-table__key-link">
                      {ticket.ticketKey}
                    </Link>
                  </td>

                  {/* タイトル（ダブルクリックでインライン編集） */}
                  <td className="ticket-table__td ticket-table__td--title">
                    <InlineEdit
                      type="text"
                      value={ticket.title}
                      onSave={async (newTitle) => {
                        await apiClient.patch(`/tickets/${ticket.id}/`, { title: newTitle });
                      }}
                      placeholder="Untitled"
                    />
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
                    {ticket.assignees?.length > 0 ? (
                      <span className="ticket-table__assignee">
                        {ticket.assignees.map((a) => (
                          <span key={a.id} className="ticket-table__avatar" title={a.displayName || a.username}>
                            {(a.displayName || a.username)[0]?.toUpperCase()}
                          </span>
                        ))}
                        {ticket.assignees.length === 1 && ticket.assignees[0] && (
                          <span>{ticket.assignees[0].displayName || ticket.assignees[0].username}</span>
                        )}
                        {ticket.assignees.length > 1 && (
                          <span>{ticket.assignees.length}人</span>
                        )}
                      </span>
                    ) : (
                      <span className="ticket-table__unassigned">—</span>
                    )}
                  </td>

                  {/* Labels */}
                  <td className="ticket-table__td ticket-table__td--labels">
                    <LabelList labels={ticket.labels ?? []} max={2} />
                  </td>

                  {/* Due date */}
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
