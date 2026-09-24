/**
 * TicketsPage.tsx — 全チケット一覧ページ(グローバル)
 *
 * サイドバーの「チケット」から遷移する、project/teamスコープ無しの
 * チケット一覧。Filter + Saved View(★)機能を持つ。
 */

import { useState, useEffect, useRef } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { useProject } from '@/shared/hooks/useProject';
import { useTeam } from '@/shared/hooks/useTeam';
import { useAuthStore } from '@/shared/stores/authStore';
import { useToastStore } from '@/shared/stores/toastStore';
import { buildTicketDetailPath } from '../utils/ticketNavigation';
import './TicketsPage.css';

interface TicketListItem {
  id: number;
  ticketKey: string;
  title: string;
  status: string;
  priority: string;
  assignees: { id: number; username: string; displayName: string }[];
  project: number | null;
  projectPrefix: string | null;
  team: { id: number; name: string; slug: string; icon: string; color: string } | null;
  dueDate: string | null;
}

interface TicketsListResponse {
  count: number;
  results: TicketListItem[];
}

const STATUS_LABELS: Record<string, string> = {
  backlog: 'Backlog',
  open: '未対応',
  in_progress: '対応中',
  resolved: '解決済み',
  closed: '完了',
  canceled: 'キャンセル',
};

const PRIORITY_LABELS: Record<string, string> = {
  urgent: '緊急',
  high: '高',
  medium: '中',
  low: '低',
};

interface TicketViewFilters {
  status: string;
  priority: string;
  mine: boolean;
  projectPrefix: string;
  teamSlug: string;
}

interface TicketSavedView {
  id: number;
  name: string;
  filters: TicketViewFilters;
  isShared: boolean;
  ownerId: number;
  createdAt: string;
  updatedAt: string;
  viewType: string;
}

export function TicketsPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { user } = useAuthStore();
  const { projectList } = useProject();
  const { teamList } = useTeam();
  const { addToast } = useToastStore();
  const queryClient = useQueryClient();

  const [statusFilter, setStatusFilter] = useState('');
  const [priorityFilter, setPriorityFilter] = useState('');
  const [mineOnly, setMineOnly] = useState(false);
  const [projectFilter, setProjectFilter] = useState('');
  const [teamFilter, setTeamFilter] = useState('');
  const [showPresetMenu, setShowPresetMenu] = useState(false);
  const presetRef = useRef<HTMLDivElement>(null);

  const ticketsQueryKey = ['tickets', 'global', statusFilter, priorityFilter, mineOnly, projectFilter, teamFilter];
  const { data, isLoading } = useQuery<TicketsListResponse>({
    queryKey: ticketsQueryKey,
    queryFn: async () => {
      const params: Record<string, string | number> = {};
      if (statusFilter) params.status = statusFilter;
      if (priorityFilter) params.priority = priorityFilter;
      if (mineOnly && user?.id) params.assignees = user.id;
      if (projectFilter) params.project__prefix = projectFilter;
      if (teamFilter) params.team_slug = teamFilter;
      const res = await apiClient.get<TicketsListResponse>('/tickets/', { params });
      return res.data;
    },
  });

  const tickets = data?.results ?? [];

  const savedViewsQueryKey = ['saved-views', 'tickets'];
  const { data: savedViewsData = [] } = useQuery<TicketSavedView[]>({
    queryKey: savedViewsQueryKey,
    queryFn: async () => {
      const res = await apiClient.get<TicketSavedView[]>('/saved-views/', {
        params: { view_type: 'tickets' },
      });
      return res.data;
    },
  });

  const createSavedViewMutation = useMutation({
    mutationFn: async (viewName: string) => {
      const viewFilters: TicketViewFilters = {
        status: statusFilter,
        priority: priorityFilter,
        mine: mineOnly,
        projectPrefix: projectFilter,
        teamSlug: teamFilter,
      };
      const res = await apiClient.post<TicketSavedView>('/saved-views/', {
        name: viewName,
        filters: viewFilters,
        viewType: 'tickets',
      });
      return res.data;
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: savedViewsQueryKey });
      setShowPresetMenu(false);
      addToast({ message: 'ビューを保存しました', type: 'success' });
    },
    onError: (error: unknown) => {
      const axiosErr = error as { response?: { status?: number; data?: { detail?: string } } };
      const detail = axiosErr.response?.data?.detail ?? '';
      if (axiosErr.response?.status === 400 && detail.includes('同じ名前')) {
        addToast({ message: '同じ名前のビューが既にあります', type: 'error' });
      } else {
        addToast({ message: 'ビューの保存に失敗しました', type: 'error' });
      }
    },
  });

  const deleteSavedViewMutation = useMutation({
    mutationFn: async (viewId: number) => {
      await apiClient.delete(`/saved-views/${viewId}/`);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: savedViewsQueryKey });
      addToast({ message: 'ビューを削除しました', type: 'success' });
    },
    onError: () => {
      addToast({ message: 'ビューの削除に失敗しました', type: 'error' });
    },
  });

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
    const name = prompt('ビュー名を入力してください');
    if (!name?.trim()) return;
    createSavedViewMutation.mutate(name.trim());
  };

  const applyPreset = (view: TicketSavedView) => {
    setStatusFilter(view.filters.status ?? '');
    setPriorityFilter(view.filters.priority ?? '');
    setMineOnly(!!view.filters.mine);
    setProjectFilter(view.filters.projectPrefix ?? '');
    setTeamFilter(view.filters.teamSlug ?? '');
    setShowPresetMenu(false);
  };

  const deletePreset = (viewId: number) => {
    if (!window.confirm('このビューを削除しますか？')) return;
    deleteSavedViewMutation.mutate(viewId);
  };

  return (
    <div className="tickets-page" data-testid="tickets-page">
      <header className="tickets-page__header">
        <h1 className="tickets-page__title">{t('nav.tickets')}</h1>
      </header>

      <div className="tickets-page__filter-row">
        <select className="tickets-page__filter" value={statusFilter} onChange={(e) => setStatusFilter(e.target.value)} data-testid="tickets-page-status-filter">
          <option value="">Status</option>
          {Object.entries(STATUS_LABELS).map(([value, label]) => (
            <option key={value} value={value}>{label}</option>
          ))}
        </select>

        <select className="tickets-page__filter" value={priorityFilter} onChange={(e) => setPriorityFilter(e.target.value)} data-testid="tickets-page-priority-filter">
          <option value="">{t('ticket.priority')}</option>
          {Object.entries(PRIORITY_LABELS).map(([value, label]) => (
            <option key={value} value={value}>{label}</option>
          ))}
        </select>

        <select className="tickets-page__filter" value={projectFilter} onChange={(e) => setProjectFilter(e.target.value)} data-testid="tickets-page-project-filter">
          <option value="">{t('nav.projects')}</option>
          {projectList.map((project) => (
            <option key={project.id} value={project.prefix}>{project.name}</option>
          ))}
        </select>

        <select className="tickets-page__filter" value={teamFilter} onChange={(e) => setTeamFilter(e.target.value)} data-testid="tickets-page-team-filter">
          <option value="">{t('sidebar.yourTeams')}</option>
          {teamList.map((team) => (
            <option key={team.id} value={team.slug}>{team.name}</option>
          ))}
        </select>

        <label className="tickets-page__mine-toggle">
          <input type="checkbox" checked={mineOnly} onChange={(e) => setMineOnly(e.target.checked)} data-testid="tickets-page-mine-checkbox" />
          自分の担当のみ
        </label>

        <div className="tickets-page__preset-wrapper" ref={presetRef} style={{ position: 'relative' }}>
          <button
            type="button"
            className="tickets-page__preset-btn"
            onClick={() => setShowPresetMenu(!showPresetMenu)}
            title="保存済みビュー"
            data-testid="tickets-page-preset-btn"
          >
            ⭐ {savedViewsData.length > 0 && <span className="tickets-page__preset-count">{savedViewsData.length}</span>}
          </button>
          {showPresetMenu && (
            <div className="tickets-page__preset-menu">
              {savedViewsData.length === 0 ? (
                <div className="tickets-page__preset-empty">保存済みのビューはありません</div>
              ) : (
                savedViewsData.map((view) => (
                  <div key={view.id} className="tickets-page__preset-item">
                    <button className="tickets-page__preset-apply" onClick={() => applyPreset(view)}>
                      {view.name}
                    </button>
                    <button
                      className="tickets-page__preset-delete"
                      onClick={() => deletePreset(view.id)}
                      title={t('common.delete')}
                      disabled={deleteSavedViewMutation.isPending}
                    >
                      ×
                    </button>
                  </div>
                ))
              )}
              <button
                className="tickets-page__preset-save"
                onClick={savePreset}
                data-testid="tickets-page-save-preset-btn"
                disabled={createSavedViewMutation.isPending}
              >
                + 現在の条件をビューとして保存
              </button>
            </div>
          )}
        </div>
      </div>

      {isLoading ? (
        <div className="tickets-page__empty">{t('common.loading')}</div>
      ) : tickets.length === 0 ? (
        <div className="tickets-page__empty" data-testid="tickets-page-empty">
          <p>該当するチケットがありません</p>
        </div>
      ) : (
        <table className="tickets-page__table">
          <thead>
            <tr>
              <th className="tickets-page__th">Key</th>
              <th className="tickets-page__th tickets-page__th--title">Title</th>
              <th className="tickets-page__th">Status</th>
              <th className="tickets-page__th">{t('ticket.priority')}</th>
              <th className="tickets-page__th">{t('nav.projects')}</th>
              <th className="tickets-page__th">{t('ticket.assignee')}</th>
            </tr>
          </thead>
          <tbody>
            {tickets.map((ticket) => {
              const path = buildTicketDetailPath(
                ticket.team?.slug ? null : ticket.projectPrefix,
                ticket.ticketKey,
                undefined,
                ticket.team?.slug ?? null,
              );
              return (
                <tr
                  key={ticket.id}
                  className="tickets-page__row"
                  onClick={() => navigate(path)}
                  style={{ cursor: 'pointer' }}
                >
                  <td className="tickets-page__td">
                    <Link to={path} className="tickets-page__key-link" onClick={(e) => e.stopPropagation()}>
                      {ticket.ticketKey}
                    </Link>
                  </td>
                  <td className="tickets-page__td tickets-page__td--title">
                    <Link to={path} className="tickets-page__title-link" onClick={(e) => e.stopPropagation()}>
                      {ticket.title}
                    </Link>
                  </td>
                  <td className="tickets-page__td">{STATUS_LABELS[ticket.status] ?? ticket.status}</td>
                  <td className="tickets-page__td">{PRIORITY_LABELS[ticket.priority] ?? ticket.priority}</td>
                  <td className="tickets-page__td">{ticket.projectPrefix ?? ticket.team?.name ?? '—'}</td>
                  <td className="tickets-page__td">
                    {ticket.assignees.length > 0
                      ? ticket.assignees.map((a) => a.displayName || a.username).join(', ')
                      : '—'}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
    </div>
  );
}
