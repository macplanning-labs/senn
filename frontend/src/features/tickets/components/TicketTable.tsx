/**
 * TicketTable.tsx — チケット一覧テーブル
 *
 * フィルタ + 検索 + ソート対応のテーブルビュー。
 * Linearスタイルの行ホバー + インラインステータス変更。
 */

import { useState, useRef, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Link, useNavigate, useSearchParams } from 'react-router-dom';
import { apiClient } from '@/shared/api/client';
import { useProject } from '@/shared/hooks/useProject';
import { useTeam } from '@/shared/hooks/useTeam';
import { useUIStore } from '@/shared/stores/uiStore';
import { useAuthStore } from '@/shared/stores/authStore';
import { useToastStore } from '@/shared/stores/toastStore';
import { useOptimisticMutation } from '@/shared/hooks/useOptimisticMutation';
import { localDateStr } from '@/shared/utils/localDateStr';
import {
  TICKET_DASHBOARD_INVALIDATE_KEYS,
} from '@/shared/utils/ticketQueryInvalidation';
import { useKeyboardNav } from '@/shared/hooks/useKeyboardNav';
import { InlineEdit } from '@/shared/components/ui/InlineEdit';
import { LabelList } from '@/shared/components/ui/LabelBadge';
import type { Label } from '@/shared/components/ui/LabelBadge';
import { FilterBar } from '@/shared/components/ui/FilterBar';
import { MarkdownImportModal } from './MarkdownImportModal';
import './TicketTable.css';
import { useColumnResize } from '@/shared/hooks/useColumnResize';
import type { ColumnDef } from '@/shared/hooks/useColumnResize';
import type { SavedView, SavedViewFilters } from '@/shared/api/types';
import { useWorkflowStatuses } from '@/features/settings/hooks/useWorkflowStatuses';
import {
  buildTicketDetailPath,
  buildTicketListPath,
} from '@/features/tickets/utils/ticketNavigation';
import { TeamTabPageHeader } from '@/features/teams/components/TeamTabPageHeader';
import { IconTicket } from '@/shared/components/layout/Sidebar';

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
  createdAt: string;
  commentCount: number;
  childCount: number;
  project?: number | null;
  projectPrefix?: string | null;
  team?: { id: number; slug: string } | null;
}

function ticketDetailPath(
  ticket: Ticket,
  pageProjectKey: string | null | undefined,
  cycleId: number | undefined,
  pageTeamSlug: string | null,
): string {
  // 表示中のページの文脈(チーム画面 or プロジェクト画面)を優先する。
  // プロジェクト付きチケットは必ずteam_idも持つため、チケット自身のprefixを
  // 優先すると、チーム画面で見ているだけなのにプロジェクト側へ遷移してしまう。
  if (pageTeamSlug) {
    return buildTicketDetailPath(null, ticket.ticketKey, undefined, pageTeamSlug);
  }
  if (pageProjectKey) {
    return buildTicketDetailPath(pageProjectKey, ticket.ticketKey, cycleId, null);
  }
  // ページ文脈が無い場合(埋め込み利用等)はチケット自身の所属から判断する。
  if (ticket.team?.slug) {
    return buildTicketDetailPath(null, ticket.ticketKey, undefined, ticket.team.slug);
  }
  return buildTicketDetailPath(ticket.projectPrefix ?? null, ticket.ticketKey, cycleId, null);
}

const STATUS_OPTIONS = ['backlog', 'open', 'in_progress', 'resolved', 'closed', 'canceled'] as const;
const PRIORITY_OPTIONS = ['urgent', 'high', 'medium', 'low'] as const;

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
  { key: 'created', defaultWidth: 100, minWidth: 70 },
  { key: 'meta', defaultWidth: 80, minWidth: 50 },
];

// バックエンドの overdue/due_soon 判定 (dashboard_api_repo.rs:41,43) と同じ日付境界。
interface TicketTableProps {
  /** 指定時、このサイクルのチケットのみに絞り込む（Cycle詳細画面からの埋め込み用） */
  cycleId?: number;
}

export function TicketTable({ cycleId }: TicketTableProps = {}) {
  const { t } = useTranslation();
  const statusLabel = (s: string) => t(`ticket.status.${s}`, { defaultValue: s });
  const priorityLabel = (p: string) => t(`ticket.priority_label.${p}`, { defaultValue: p });
  const { projectKey, currentProject } = useProject();
  const { teamSlug, currentTeam } = useTeam();
  const { data: workflowStatuses = [] } = useWorkflowStatuses(
    currentProject?.id,
    currentProject?.id ? undefined : currentTeam?.id,
  );
  const statusOptions = workflowStatuses.length > 0
    ? workflowStatuses.map((s) => s.slug)
    : [...STATUS_OPTIONS];
  const { user } = useAuthStore();
  const { addToast } = useToastStore();
  const queryClient = useQueryClient();
  const { openTicketFormModal } = useUIStore();

  const isProjectOwner =
    !!user && !!currentProject && currentProject.ownerId === user.id;

  // フィルタ状態
  const [searchParams, setSearchParams] = useSearchParams();
  const [search, setSearch] = useState(() => searchParams.get('search') ?? '');
  const [statusFilter, setStatusFilter] = useState<string>(() => searchParams.get('status') ?? '');
  const [priorityFilter, setPriorityFilter] = useState<string>(() => searchParams.get('priority') ?? '');
  const [dueFilter, setDueFilter] = useState<string>(() => searchParams.get('due') ?? '');
  const [statusInFilter, setStatusInFilter] = useState<string>(() => searchParams.get('status_in') ?? '');

  // QueryKey（フィルタ状態を含む）
  const ticketsQueryKey = ['tickets', currentProject?.id, teamSlug, cycleId, search, statusFilter, priorityFilter, dueFilter, statusInFilter];

  // --- フィルタプリセット（Saved Views） ---
  const savedViewsQueryKey = ['saved-views', currentProject?.id ?? null, currentTeam?.id ?? null];
  const { data: savedViewsData = [] } = useQuery<SavedView[]>({
    queryKey: savedViewsQueryKey,
    queryFn: async () => {
      if (currentProject?.id) {
        const res = await apiClient.get<SavedView[]>(
          `/projects/${currentProject.id}/saved-views/`
        );
        return res.data;
      }
      if (currentTeam?.id) {
        const res = await apiClient.get<SavedView[]>(
          `/teams/${currentTeam.id}/saved-views/`
        );
        return res.data;
      }
      return [];
    },
    enabled: !!currentProject?.id || !!currentTeam?.id,
  });

  const [showPresetMenu, setShowPresetMenu] = useState(false);
  const presetRef = useRef<HTMLDivElement>(null);
  const migrationDialogRef = useRef<HTMLDivElement>(null);
  const [showMigrationDialog, setShowMigrationDialog] = useState(false);
  const [pendingPresets, setPendingPresets] = useState<Array<{ name: string; filters: SavedViewFilters }>>([]);

  const createSavedViewMutation = useMutation({
    mutationFn: async (viewName: string) => {
      const filters: SavedViewFilters = {
        search,
        status: statusFilter,
        priority: priorityFilter,
        due: dueFilter,
        status_in: statusInFilter,
      };
      if (currentProject?.id) {
        const res = await apiClient.post<SavedView>(
          `/projects/${currentProject.id}/saved-views/`,
          { name: viewName, filters }
        );
        return res.data;
      }
      if (currentTeam?.id) {
        const res = await apiClient.post<SavedView>(
          `/teams/${currentTeam.id}/saved-views/`,
          { name: viewName, filters }
        );
        return res.data;
      }
      throw new Error('No project or team');
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: savedViewsQueryKey });
      setShowPresetMenu(false);
      addToast({ message: t('savedView.created'), type: 'success' });
    },
    onError: (error: unknown) => {
      const axiosErr = error as { response?: { status?: number; data?: { detail?: string } } };
      const detail = axiosErr.response?.data?.detail ?? '';
      if (
        axiosErr.response?.status === 400 &&
        (detail.includes('同じ名前') || detail.toLowerCase().includes('unique') || detail.includes('already'))
      ) {
        addToast({ message: t('savedView.nameExists'), type: 'error' });
      } else {
        addToast({ message: t('savedView.createFailed'), type: 'error' });
      }
    },
  });

  const deleteSavedViewMutation = useMutation({
    mutationFn: async (viewId: number) => {
      await apiClient.delete(`/saved-views/${viewId}/`);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: savedViewsQueryKey });
      addToast({ message: t('savedView.deleted'), type: 'success' });
    },
    onError: () => {
      addToast({ message: t('savedView.deleteFailed'), type: 'error' });
    },
  });

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

  // フィルタ変更をURLへ反映(ブックマーク・ダッシュボードからの遷移で共有可能にする)。
  // 履歴を汚さないよう replace で更新する。他コンポーネント(Cycle詳細のpanel等)が
  // 管理する無関係なクエリパラメータを消さないよう、既存のURLをベースにマージする。
  useEffect(() => {
    setSearchParams((prev) => {
      const next = new URLSearchParams(prev);
      if (search) next.set('search', search); else next.delete('search');
      if (statusFilter) next.set('status', statusFilter); else next.delete('status');
      if (priorityFilter) next.set('priority', priorityFilter); else next.delete('priority');
      if (dueFilter) next.set('due', dueFilter); else next.delete('due');
      if (statusInFilter) next.set('status_in', statusInFilter); else next.delete('status_in');
      return next;
    }, { replace: true });
  }, [search, statusFilter, priorityFilter, dueFilter, statusInFilter, setSearchParams]);

  // Sidebar 等から URL query が変わった場合に state を同期
  useEffect(() => {
    const newSearch = searchParams.get('search') ?? '';
    const newStatus = searchParams.get('status') ?? '';
    const newPriority = searchParams.get('priority') ?? '';
    const newDue = searchParams.get('due') ?? '';
    const newStatusIn = searchParams.get('status_in') ?? '';
    if (newSearch !== search) setSearch(newSearch);
    if (newStatus !== statusFilter) setStatusFilter(newStatus);
    if (newPriority !== priorityFilter) setPriorityFilter(newPriority);
    if (newDue !== dueFilter) setDueFilter(newDue);
    if (newStatusIn !== statusInFilter) setStatusInFilter(newStatusIn);
  }, [searchParams]);

  const savePreset = () => {
    const name = prompt(t('savedView.promptName'));
    if (!name?.trim()) return;
    createSavedViewMutation.mutate(name.trim());
  };

  const applyPreset = (view: SavedView) => {
    const f = view.filters ?? { search: '', status: '', priority: '', due: '', status_in: '' };
    setSearch(f.search ?? '');
    setStatusFilter(f.status ?? '');
    setPriorityFilter(f.priority ?? '');
    setDueFilter(f.due ?? '');
    setStatusInFilter(f.status_in ?? '');
    setShowPresetMenu(false);
  };

  const deletePreset = (viewId: number) => {
    if (!window.confirm(t('savedView.deleteConfirm'))) return;
    deleteSavedViewMutation.mutate(viewId);
  };

  // localStorage 移行処理
  const migrateLocalStoragePresets = async (presets: Array<{ name: string; filters: SavedViewFilters }>) => {
    let successCount = 0;
    let skippedCount = 0;
    let hardFail = false;

    for (const preset of presets) {
      try {
        if (!currentProject?.id) continue;
        await apiClient.post<SavedView>(
          `/projects/${currentProject.id}/saved-views/`,
          { name: preset.name, filters: preset.filters }
        );
        successCount++;
      } catch (error: unknown) {
        const axiosErr = error as { response?: { status?: number; data?: { detail?: string } } };
        const detail = axiosErr.response?.data?.detail ?? '';
        if (
          axiosErr.response?.status === 400 &&
          (detail.includes('同じ名前') || detail.toLowerCase().includes('unique') || detail.includes('already'))
        ) {
          skippedCount++;
        } else {
          hardFail = true;
          addToast({ message: t('savedView.migrationFailed'), type: 'error' });
        }
      }
    }

    // 通信エラー等で途中失敗した場合はフラグを立てず、localStorage も残して再試行可能にする
    if (hardFail) {
      setShowMigrationDialog(false);
      setPendingPresets([]);
      return;
    }

    if (projectKey) {
      localStorage.setItem(`filter-presets-migrated-${projectKey}`, '1');
      localStorage.removeItem(`filter-presets-${projectKey}`);
    }

    if (successCount > 0 && skippedCount === 0) {
      addToast({ message: t('savedView.migrationSuccess', { count: successCount }), type: 'success' });
    } else if (successCount > 0 || skippedCount > 0) {
      addToast({
        message: t('savedView.migrationPartial', { success: successCount, total: presets.length, skipped: skippedCount }),
        type: 'success',
      });
    }

    void queryClient.invalidateQueries({ queryKey: savedViewsQueryKey });
    setShowMigrationDialog(false);
    setPendingPresets([]);
  };

  // プロジェクト切替時に localStorage 移行をチェック
  useEffect(() => {
    if (!projectKey || !currentProject?.id) return;

    const migrationFlag = localStorage.getItem(`filter-presets-migrated-${projectKey}`);
    if (migrationFlag) return; // 既に移行済み

    const presetsJson = localStorage.getItem(`filter-presets-${projectKey}`);
    if (!presetsJson) return; // localStorage にプリセットがない

    let presets: Array<{ name?: string; search?: string; status?: string; priority?: string; due?: string; status_in?: string }> = [];
    try {
      presets = JSON.parse(presetsJson);
    } catch {
      return; // JSON パース失敗
    }

    if (!Array.isArray(presets) || presets.length === 0) return; // 配列でない、または空

    // 移行対象のプリセットを変換
    const presetsToMigrate = presets
      .filter((p) => p.name)
      .map((p) => ({
        name: p.name || '',
        filters: {
          search: p.search ?? '',
          status: p.status ?? '',
          priority: p.priority ?? '',
          due: p.due ?? '',
          status_in: p.status_in ?? '',
        },
      }));

    if (presetsToMigrate.length === 0) return;

    setPendingPresets(presetsToMigrate);
    setShowMigrationDialog(true);
  }, [projectKey, currentProject?.id]);

  // 外部クリックで移行ダイアログを閉じる（キャンセル処理付き）
  useEffect(() => {
    if (!showMigrationDialog) return;
    const handleClickOutside = (e: MouseEvent) => {
      if (migrationDialogRef.current && !migrationDialogRef.current.contains(e.target as Node)) {
        // キャンセル処理
        if (projectKey) {
          localStorage.setItem(`filter-presets-migrated-${projectKey}`, '1');
        }
        setShowMigrationDialog(false);
        setPendingPresets([]);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [showMigrationDialog, projectKey]);

  // --- バルク操作 ---
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [selectAllInProject, setSelectAllInProject] = useState(false);
  const [markdownImportOpen, setMarkdownImportOpen] = useState(false);

  const toggleSelect = (ticketKey: string) => {
    setSelectedIds(prev => {
      const next = new Set(prev);
      if (next.has(ticketKey)) {
        next.delete(ticketKey);
      } else {
        next.add(ticketKey);
      }
      return next;
    });
  };

  const toggleSelectAll = () => {
    if (selectedIds.size === tickets.length && !selectAllInProject) {
      setSelectedIds(new Set());
      setSelectAllInProject(false);
    } else {
      setSelectedIds(new Set(tickets.map(t => t.ticketKey)));
      setSelectAllInProject(false);
    }
  };

  const selectAllTicketsInProject = async () => {
    if (hasActiveFilters) {
      const keys = await fetchAllTicketKeys();
      setSelectedIds(new Set(keys));
      setSelectAllInProject(false);
    } else {
      setSelectAllInProject(true);
      setSelectedIds(new Set(tickets.map((t) => t.ticketKey)));
    }
  };

  const buildListParams = (page?: number): Record<string, string> => {
    const params: Record<string, string> = {};
    if (currentProject?.id) params.project = String(currentProject.id);
    // Team-onlyの場合は team_id で絞り込み（APIで実装されていると想定）
    // NOTE: G6-1 API で team_id パラメータがサポートされていることを前提
    if (teamSlug && !currentProject?.id) {
      // チーム情報の取得は useTeam で取得したものが使われるが、
      // ここではteamSlug を送り、API側で解決するように設計
      params.team_slug = teamSlug;
    }
    if (cycleId) params.cycle = String(cycleId);
    if (search) params.search = search;
    if (statusInFilter) {
      params.status__in = statusInFilter;
    } else if (statusFilter) {
      params.status = statusFilter;
    }
    if (priorityFilter) params.priority = priorityFilter;
    if (dueFilter === 'overdue') {
      params.due_date__lte = localDateStr(-1);
    } else if (dueFilter === 'due_soon') {
      params.due_date__gte = localDateStr(0);
      params.due_date__lte = localDateStr(3);
    }
    if (page) params.page = String(page);
    return params;
  };

  const hasActiveFilters = !!(search || statusFilter || priorityFilter || dueFilter || statusInFilter);

  const fetchAllTicketKeys = async (): Promise<string[]> => {
    const keys: string[] = [];
    let page = 1;
    while (true) {
      const res = await apiClient.get<{ results: Ticket[]; next: string | null }>(
        '/tickets/',
        { params: buildListParams(page) },
      );
      keys.push(...res.data.results.map((t) => t.ticketKey));
      if (!res.data.next) break;
      page += 1;
    }
    return keys;
  };

  const bulkStatusMutation = useOptimisticMutation<void, { ticketKeys: string[]; status: string }>({
    mutationFn: async ({ ticketKeys, status }) => {
      await Promise.all(ticketKeys.map(ticketKey => apiClient.patch(`/tickets/${ticketKey}/`, { status })));
    },
    queryKey: ticketsQueryKey,
    updater: (currentData, { ticketKeys, status }) => {
      const data = currentData as { results: Ticket[] } | undefined;
      if (!data?.results) return currentData;
      const ticketKeySet = new Set(ticketKeys);
      return {
        ...data,
        results: data.results.map((t) =>
          ticketKeySet.has(t.ticketKey) ? { ...t, status } : t,
        ),
      };
    },
    onSuccessCallback: () => {
      setSelectedIds(new Set());
    },
    // ['ticket'] も無効化: 開いている詳細パネルが古いステータスのまま残るのを防ぐ
    // cycleId がある場合は Cycle 詳細の進捗サマリー(未完了/完了件数)も無効化する
    invalidateKeys: [
      ...TICKET_DASHBOARD_INVALIDATE_KEYS,
      ['ticket'],
      ...(cycleId ? [['cycle-progress', cycleId]] : []),
    ],
    errorMessage: '一括ステータス変更に失敗しました。元に戻しました。',
  });

  const bulkDeleteMutation = useMutation({
    mutationFn: async () => {
      if (selectAllInProject && currentProject?.id && !hasActiveFilters) {
        await apiClient.post('/tickets/bulk-delete/', {
          project_id: currentProject.id,
          delete_all: true,
        });
      } else {
        await apiClient.post('/tickets/bulk-delete/', {
          ticket_keys: Array.from(selectedIds),
        });
      }
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['tickets'] });
      void queryClient.invalidateQueries({ queryKey: ['projects'] });
      setSelectedIds(new Set());
      setSelectAllInProject(false);
      addToast({ message: '選択したチケットを削除しました', type: 'success' });
    },
    onError: (error: unknown) => {
      const axiosErr = error as { response?: { data?: { detail?: string } } };
      addToast({
        message: axiosErr.response?.data?.detail ?? '一括削除に失敗しました',
        type: 'error',
      });
    },
  });

  const handleBulkDelete = async () => {
    const count = selectAllInProject ? totalCount : selectedIds.size;
    if (count === 0) return;
    const message = selectAllInProject
      ? `プロジェクト内の全${count}件のチケットを削除します。この操作は取り消せません。`
      : `選択した${count}件のチケットを削除します。この操作は取り消せません。`;
    if (!window.confirm(message)) return;
    bulkDeleteMutation.mutate();
  };

  // カラムリサイズ
  const { widths, onResizeStart, isResizing } = useColumnResize('ticket-table', TABLE_COLUMNS);

  // チケット取得
  const { data, isLoading } = useQuery<{ count: number; results: Ticket[]; next: string | null }>({
    queryKey: ticketsQueryKey,
    queryFn: async () => {
      const res = await apiClient.get<{ count: number; results: Ticket[]; next: string | null }>(
        '/tickets/',
        { params: buildListParams() },
      );
      return res.data;
    },
  });

  // 楽観的ステータス変更 — ドロップダウン変更の瞬間にテーブル行が即更新（0ms）
  const statusMutation = useOptimisticMutation<void, { ticketKey: string; status: string }>({
    mutationFn: async ({ ticketKey, status }) => {
      await apiClient.patch(`/tickets/${ticketKey}/`, { status });
    },
    queryKey: ticketsQueryKey,
    updater: (currentData, { ticketKey, status }) => {
      const data = currentData as { results: Ticket[] } | undefined;
      if (!data?.results) return currentData;
      return {
        ...data,
        results: data.results.map((t) =>
          t.ticketKey === ticketKey ? { ...t, status } : t,
        ),
      };
    },
    // ['ticket'] も無効化: 開いている詳細パネルが古いステータスのまま残るのを防ぐ
    // cycleId がある場合は Cycle 詳細の進捗サマリー(未完了/完了件数)も無効化する
    invalidateKeys: [
      ...TICKET_DASHBOARD_INVALIDATE_KEYS,
      ['ticket'],
      ...(cycleId ? [['cycle-progress', cycleId]] : []),
    ],
    errorMessage: 'ステータス変更に失敗しました。元に戻しました。',
  });

  const tickets = data?.results ?? [];
  const totalCount = data?.count ?? tickets.length;
  const navigate = useNavigate();
  const tableRef = useRef<HTMLDivElement>(null);

  // キーボードナビゲーション
  const { selectedIndex } = useKeyboardNav({
    itemCount: tickets.length,
    onOpen: (index) => {
      const ticket = tickets[index];
      if (ticket) {
        navigate(ticketDetailPath(ticket, projectKey, cycleId, teamSlug));
      }
    },
    onClose: () => {
      navigate(buildTicketListPath(projectKey, cycleId, teamSlug));
    },
    onCreate: () => {
      if (projectKey) {
        openTicketFormModal(projectKey);
      } else if (teamSlug) {
        openTicketFormModal(null, teamSlug);
      }
    },
  });

  // 選択行のスクロール追従
  useEffect(() => {
    if (selectedIndex < 0) return;
    const rows = tableRef.current?.querySelectorAll('.ticket-table__row');
    rows?.[selectedIndex]?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  }, [selectedIndex]);

  const ticketHeaderActions = (
    <>
      <button
        className="ticket-table__export-btn"
        onClick={async () => {
          try {
            const params = new URLSearchParams();
            if (currentProject?.id) params.set('project', String(currentProject.id));
            if (statusFilter) params.set('status', statusFilter);
            if (priorityFilter) params.set('priority', priorityFilter);
            const res = await apiClient.get(`/tickets/export/csv/?${params}`, {
              responseType: 'blob',
            });
            const url = window.URL.createObjectURL(new Blob([res.data as BlobPart]));
            const a = document.createElement('a');
            a.href = url;
            a.download = `tickets_${projectKey || 'all'}_${new Date().toISOString().slice(0, 10)}.csv`;
            a.click();
            window.URL.revokeObjectURL(url);
          } catch {
            // silent fail
          }
        }}
        data-testid="csv-export-btn"
      >
        📥 CSV
      </button>
      <button
        className="ticket-table__import-btn"
        onClick={() => setMarkdownImportOpen(true)}
        data-testid="markdown-import-btn"
        title="Markdownからインポート"
      >
        📄 MD
      </button>
      <button
        type="button"
        className="ticket-table__create-btn"
        onClick={() => {
          if (projectKey) {
            openTicketFormModal(projectKey);
          } else if (teamSlug) {
            openTicketFormModal(null, teamSlug);
          } else {
            navigate('/tickets/new');
          }
        }}
        data-testid="create-ticket-btn"
      >
        + {t('ticket.create')}
      </button>
    </>
  );

  return (
    <div className="ticket-table" data-testid="ticket-table-page">
      {/* 固定ヘッダー（スクロールしても検索欄・フィルタが隠れないように） */}
      <div className="ticket-table__sticky-top">
        {/* ヘッダー */}
        {cycleId ? (
          <div className="ticket-table__header">
            <div style={{ display: 'flex', gap: '8px', alignItems: 'center', marginLeft: 'auto' }}>
              {ticketHeaderActions}
            </div>
          </div>
        ) : (
          <TeamTabPageHeader
            icon={IconTicket}
            title={t('nav.tickets')}
            actions={ticketHeaderActions}
          />
        )}

        {/* フィルタバー（共有コンポーネント） */}
        <FilterBar
          searchValue={search}
          onSearchChange={setSearch}
          searchPlaceholder={t('common.search')}
          testId="ticket-filters"
        >
          <select
            className="ticket-table__filter-select"
            value={statusInFilter ? '' : statusFilter}
            onChange={(e) => { setStatusFilter(e.target.value); setStatusInFilter(''); }}
            data-testid="status-filter"
          >
            <option value="">{t('ticketTable.allStatus')}</option>
            {statusOptions.map((s) => (
              <option key={s} value={s}>{statusLabel(s)}</option>
            ))}
          </select>
          <select
            className="ticket-table__filter-select"
            value={priorityFilter}
            onChange={(e) => setPriorityFilter(e.target.value)}
            data-testid="priority-filter"
          >
            <option value="">{t('ticketTable.allPriority')}</option>
            {PRIORITY_OPTIONS.map((p) => (
              <option key={p} value={p}>{priorityLabel(p)}</option>
            ))}
          </select>
          <select
            className="ticket-table__filter-select"
            value={dueFilter}
            onChange={(e) => setDueFilter(e.target.value)}
            data-testid="due-filter"
          >
            <option value="">{t('ticketTable.allDueDates')}</option>
            <option value="overdue">{t('ticketTable.overdue')}</option>
            <option value="due_soon">{t('ticketTable.dueSoon')}</option>
          </select>

          {/* プリセット（Saved Views） */}
          <div className="ticket-table__preset-wrapper" ref={presetRef} style={{ position: 'relative' }}>
            <button
              className="ticket-table__preset-btn"
              onClick={() => setShowPresetMenu(!showPresetMenu)}
              data-testid="preset-toggle"
              title={t('savedView.title')}
            >
              ⭐ {savedViewsData.length > 0 && <span className="ticket-table__preset-count">{savedViewsData.length}</span>}
            </button>
            {showPresetMenu && (
              <div className="ticket-table__preset-menu">
                {savedViewsData.length === 0 ? (
                  <div className="ticket-table__preset-empty">{t('savedView.noViews')}</div>
                ) : (
                  savedViewsData.map((view) => (
                    <div key={view.id} className="ticket-table__preset-item">
                      <button className="ticket-table__preset-apply" onClick={() => applyPreset(view)}>
                        {view.name}
                        <span className="ticket-table__preset-detail">
                          {[view.filters.status && statusLabel(view.filters.status), view.filters.priority, view.filters.search && `"${view.filters.search}"`].filter(Boolean).join(' · ') || 'All'}
                        </span>
                      </button>
                      <button className="ticket-table__preset-delete" onClick={() => deletePreset(view.id)} title={t('common.delete')} disabled={deleteSavedViewMutation.isPending}>×</button>
                    </div>
                  ))
                )}
                <button className="ticket-table__preset-save" onClick={savePreset} data-testid="save-preset-btn" disabled={createSavedViewMutation.isPending}>
                  + {t('savedView.save')}
                </button>
              </div>
            )}
          </div>
        </FilterBar>
      </div>

      {/* バルクアクションバー */}
      {selectedIds.size > 0 && (
        <div className="ticket-table__bulk-bar" data-testid="bulk-actions">
          <span className="ticket-table__bulk-count">
            {selectAllInProject ? `全${totalCount}件` : `${selectedIds.size}件`}選択中
          </span>
          {isProjectOwner && totalCount > tickets.length && !selectAllInProject && selectedIds.size === tickets.length && (
            <button
              type="button"
              className="ticket-table__bulk-select-all"
              onClick={() => void selectAllTicketsInProject()}
              data-testid="select-all-in-project-btn"
            >
              プロジェクト内の全{totalCount}件を選択
            </button>
          )}
          <select
            className="ticket-table__bulk-select"
            defaultValue=""
            onChange={(e) => {
              if (e.target.value) {
                bulkStatusMutation.mutate({ ticketKeys: Array.from(selectedIds), status: e.target.value });
                e.target.value = '';
              }
            }}
            data-testid="bulk-status-select"
          >
            <option value="">ステータス変更...</option>
            {statusOptions.map((s) => (
              <option key={s} value={s}>{statusLabel(s)}</option>
            ))}
          </select>
          {isProjectOwner && (
            <button
              type="button"
              className="ticket-table__bulk-delete"
              onClick={() => void handleBulkDelete()}
              disabled={bulkDeleteMutation.isPending}
              data-testid="bulk-delete-btn"
            >
              {bulkDeleteMutation.isPending ? '削除中...' : '削除'}
            </button>
          )}
          <button
            className="ticket-table__bulk-clear"
            onClick={() => { setSelectedIds(new Set()); setSelectAllInProject(false); }}
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
                    checked={
                      tickets.length > 0 &&
                      (selectAllInProject
                        ? totalCount > 0
                        : selectedIds.size === tickets.length)
                    }
                    onChange={toggleSelectAll}
                    data-testid="select-all-checkbox"
                  />
                </th>
                <th className="ticket-table__th ticket-table__th--key">
                  {t('ticketTable.key')}
                  <span className="ticket-table__resize-handle" onMouseDown={(e) => onResizeStart('key', e)} />
                </th>
                <th className="ticket-table__th ticket-table__th--title">
                  {t('ticketTable.title')}
                  <span className="ticket-table__resize-handle" onMouseDown={(e) => onResizeStart('title', e)} />
                </th>
                <th className="ticket-table__th ticket-table__th--status">
                  {t('ticketTable.status')}
                  <span className="ticket-table__resize-handle" onMouseDown={(e) => onResizeStart('status', e)} />
                </th>
                <th className="ticket-table__th ticket-table__th--assignee">
                  {t('ticketTable.assignee')}
                  <span className="ticket-table__resize-handle" onMouseDown={(e) => onResizeStart('assignee', e)} />
                </th>
                <th className="ticket-table__th ticket-table__th--labels">
                  {t('ticketTable.labels')}
                  <span className="ticket-table__resize-handle" onMouseDown={(e) => onResizeStart('labels', e)} />
                </th>
                <th className="ticket-table__th ticket-table__th--due">
                  {t('ticketTable.due')}
                  <span className="ticket-table__resize-handle" onMouseDown={(e) => onResizeStart('due', e)} />
                </th>
                <th className="ticket-table__th ticket-table__th--created">
                  {t('ticketTable.created')}
                  <span className="ticket-table__resize-handle" onMouseDown={(e) => onResizeStart('created', e)} />
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
                    navigate(ticketDetailPath(ticket, projectKey, cycleId, teamSlug));
                  }}
                  style={{ cursor: 'pointer' }}
                >
                  {/* チェックボックス + 優先度 */}
                  <td className="ticket-table__td ticket-table__td--priority">
                    <span style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                      <input
                        type="checkbox"
                        className="ticket-table__checkbox"
                        checked={selectedIds.has(ticket.ticketKey)}
                        onChange={(e) => {
                          e.stopPropagation();
                          setSelectAllInProject(false);
                          toggleSelect(ticket.ticketKey);
                        }}
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
                    <Link to={ticketDetailPath(ticket, projectKey, cycleId, teamSlug)} className="ticket-table__key-link">
                      {ticket.ticketKey}
                    </Link>
                  </td>

                  {/* タイトル（ダブルクリックでインライン編集） */}
                  <td className="ticket-table__td ticket-table__td--title">
                    <InlineEdit
                      type="text"
                      value={ticket.title}
                      onSave={async (newTitle) => {
                        await apiClient.patch(`/tickets/${ticket.ticketKey}/`, { title: newTitle });
                      }}
                      placeholder="Untitled"
                    />
                  </td>

                  {/* ステータス（インライン変更） */}
                  <td className="ticket-table__td ticket-table__td--status">
                    <select
                      className="ticket-table__status-select"
                      value={ticket.status}
                      onClick={(e) => e.stopPropagation()}
                      onChange={(e) => {
                        statusMutation.mutate({ ticketKey: ticket.ticketKey, status: e.target.value });
                      }}
                      style={{
                        color: statusColors[ticket.status] ?? 'inherit',
                        borderColor: statusColors[ticket.status] ?? 'transparent',
                      }}
                      data-testid={`status-select-${ticket.ticketKey}`}
                    >
                      {statusOptions.map((s) => (
                        <option key={s} value={s}>{statusLabel(s)}</option>
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

                  {/* Created date */}
                  <td className="ticket-table__td ticket-table__td--created">
                    {ticket.createdAt
                      ? new Date(ticket.createdAt).toLocaleDateString()
                      : '—'}
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

      {/* Markdownインポートモーダル */}
      <MarkdownImportModal
        isOpen={markdownImportOpen}
        onClose={() => setMarkdownImportOpen(false)}
        onSuccess={() => {
          setMarkdownImportOpen(false);
          // TicketTableは自動的に再レンダリングされ、クエリが再実行される
        }}
      />

      {/* localStorage 移行ダイアログ */}
      {showMigrationDialog && (
        <div
          style={{
            position: 'fixed',
            inset: 0,
            backgroundColor: 'rgba(0,0,0,0.5)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            zIndex: 1000,
          }}
          onClick={() => {
            if (projectKey) {
              localStorage.setItem(`filter-presets-migrated-${projectKey}`, '1');
            }
            setShowMigrationDialog(false);
            setPendingPresets([]);
          }}
        >
          <div
            ref={migrationDialogRef}
            style={{
              backgroundColor: 'var(--color-bg-primary, white)',
              borderRadius: '8px',
              padding: '24px',
              maxWidth: '500px',
              boxShadow: '0 4px 12px rgba(0,0,0,0.15)',
            }}
            onClick={(e) => e.stopPropagation()}
          >
            <h2 style={{ marginTop: 0 }}>{t('savedView.migrateTitle')}</h2>
            <p>{t('savedView.migrateMessage', { count: pendingPresets.length })}</p>
            <div style={{ display: 'flex', gap: '12px', justifyContent: 'flex-end' }}>
              <button
                type="button"
                onClick={() => {
                  if (projectKey) {
                    localStorage.setItem(`filter-presets-migrated-${projectKey}`, '1');
                  }
                  setShowMigrationDialog(false);
                  setPendingPresets([]);
                }}
                style={{
                  padding: '8px 16px',
                  border: '1px solid var(--color-border, #ccc)',
                  borderRadius: '4px',
                  cursor: 'pointer',
                  backgroundColor: 'var(--color-bg-primary, white)',
                }}
              >
                {t('savedView.migrateCancel')}
              </button>
              <button
                type="button"
                onClick={() => {
                  void migrateLocalStoragePresets(pendingPresets);
                }}
                style={{
                  padding: '8px 16px',
                  border: 'none',
                  borderRadius: '4px',
                  cursor: 'pointer',
                  backgroundColor: 'var(--color-primary, #3b82f6)',
                  color: 'white',
                }}
              >
                {t('savedView.migrateOk')}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
