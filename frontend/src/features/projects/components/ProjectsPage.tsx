/**
 * ProjectsPage.tsx — 全プロジェクト一覧ページ
 *
 * サイドバーには個別プロジェクトを列挙せず(件数が多いと破綻するため)、
 * ここでステータス別アコーディオン + クイックフィルターを使って一覧する。
 */

import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Link } from 'react-router-dom';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { useProject } from '@/shared/hooks/useProject';
import { useTeam } from '@/shared/hooks/useTeam';
import { useToastStore } from '@/shared/stores/toastStore';
import { useRoadmaps } from '@/features/projects/hooks/useProjectStructure';
import { ProjectCreateModal } from './ProjectCreateModal';
import { ProjectsTable } from './ProjectsTable';
import { filterProjects, groupProjects, type ProjectFilters } from '../utils/projectHierarchy';
import './ProjectsPage.css';

type StatusGroupKey = 'in_progress' | 'planned' | 'paused' | 'completed';
type FilterKey = 'all' | 'mine';

interface ProjectViewFilters {
  mine: boolean;
  priority: string;
  teamId: string;
  parentProjectId?: string; // '' | 'none' | id
  roadmapId?: string;
  relatedTo?: string;
  hierarchy?: boolean;
  groupBy?: 'status' | 'roadmap';
}

interface ProjectSavedView {
  id: number;
  name: string;
  filters: ProjectViewFilters;
  isShared: boolean;
  ownerId: number;
  createdAt: string;
  updatedAt: string;
  viewType: string;
}

const STATUS_GROUPS: { key: StatusGroupKey; labelKey: string; defaultOpen: boolean }[] = [
  { key: 'in_progress', labelKey: 'sidebar.projectStatus.inProgress', defaultOpen: true },
  { key: 'planned', labelKey: 'sidebar.projectStatus.planned', defaultOpen: true },
  { key: 'paused', labelKey: 'sidebar.projectStatus.paused', defaultOpen: true },
  { key: 'completed', labelKey: 'sidebar.projectStatus.completed', defaultOpen: false },
];

function normalizeStatus(status: string | undefined): StatusGroupKey {
  if (status === 'planned' || status === 'paused' || status === 'completed') {
    return status;
  }
  return 'in_progress';
}

const GROUP_STATE_STORAGE_KEY = 'wip-projects-page-status-groups';

function loadGroupOpenState(): Record<string, boolean> {
  try {
    const stored = localStorage.getItem(GROUP_STATE_STORAGE_KEY);
    return stored ? (JSON.parse(stored) as Record<string, boolean>) : {};
  } catch {
    return {};
  }
}

export function ProjectsPage() {
  const { t } = useTranslation();
  const { projectList, isLoading } = useProject();
  const { teamList } = useTeam();
  const { data: roadmaps = [] } = useRoadmaps();
  const { addToast } = useToastStore();
  const queryClient = useQueryClient();

  const [filter, setFilter] = useState<FilterKey>('all');
  const [groupOpen, setGroupOpen] = useState<Record<string, boolean>>(() => loadGroupOpenState());
  const [createModalOpen, setCreateModalOpen] = useState(false);

  const [priorityFilter, setPriorityFilter] = useState('');
  const [teamFilter, setTeamFilter] = useState('');
  const [parentProjectIdFilter, setParentProjectIdFilter] = useState('');
  const [roadmapIdFilter, setRoadmapIdFilter] = useState('');
  const [relatedToFilter, setRelatedToFilter] = useState('');
  const [hierarchyEnabled, setHierarchyEnabled] = useState(false);
  const [groupByMode, setGroupByMode] = useState<'status' | 'roadmap'>('status');
  const [showPresetMenu, setShowPresetMenu] = useState(false);
  const presetRef = useRef<HTMLDivElement>(null);

  const savedViewsQueryKey = ['saved-views', 'projects'];
  const { data: savedViewsData = [] } = useQuery<ProjectSavedView[]>({
    queryKey: savedViewsQueryKey,
    queryFn: async () => {
      const res = await apiClient.get<ProjectSavedView[]>('/saved-views/', {
        params: { view_type: 'projects' },
      });
      return res.data;
    },
  });

  // relatedTo フィルタの関連プロジェクト ID 集合を取得
  const { data: relatedIds } = useQuery<number[]>({
    queryKey: ['projects', 'related', relatedToFilter],
    queryFn: async () => {
      const res = await apiClient.get<{ results: Array<{ id: number }> }>('/projects/', {
        params: { relatedTo: relatedToFilter },
      });
      return res.data.results.map((p) => p.id);
    },
    enabled: !!relatedToFilter,
  });

  const createSavedViewMutation = useMutation({
    mutationFn: async (viewName: string) => {
      const viewFilters: ProjectViewFilters = {
        mine: filter === 'mine',
        priority: priorityFilter,
        teamId: teamFilter,
        parentProjectId: parentProjectIdFilter || undefined,
        roadmapId: roadmapIdFilter || undefined,
        relatedTo: relatedToFilter || undefined,
        hierarchy: hierarchyEnabled || undefined,
        groupBy: groupByMode !== 'status' ? groupByMode : undefined,
      };
      const res = await apiClient.post<ProjectSavedView>('/saved-views/', {
        name: viewName,
        filters: viewFilters,
        viewType: 'projects',
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

  const applyPreset = (view: ProjectSavedView) => {
    setFilter(view.filters.mine ? 'mine' : 'all');
    setPriorityFilter(view.filters.priority ?? '');
    setTeamFilter(view.filters.teamId ?? '');
    setParentProjectIdFilter(view.filters.parentProjectId ?? '');
    setRoadmapIdFilter(view.filters.roadmapId ?? '');
    setRelatedToFilter(view.filters.relatedTo ?? '');
    setHierarchyEnabled(view.filters.hierarchy ?? false);
    setGroupByMode(view.filters.groupBy ?? 'status');
    setShowPresetMenu(false);
  };

  const deletePreset = (viewId: number) => {
    if (!window.confirm('このビューを削除しますか？')) return;
    deleteSavedViewMutation.mutate(viewId);
  };

  const toggleGroup = (key: StatusGroupKey) => {
    setGroupOpen((prev) => {
      const currentlyOpen = prev[key] ?? STATUS_GROUPS.find((g) => g.key === key)?.defaultOpen ?? false;
      const next = { ...prev, [key]: !currentlyOpen };
      try {
        localStorage.setItem(GROUP_STATE_STORAGE_KEY, JSON.stringify(next));
      } catch {
        /* ignore */
      }
      return next;
    });
  };

  // フィルタを実行
  const projectFilters: ProjectFilters = {
    mine: filter === 'mine',
    priority: priorityFilter || undefined,
    teamId: teamFilter || undefined,
    parentProjectId: parentProjectIdFilter || undefined,
    roadmapId: roadmapIdFilter || undefined,
    relatedIds: relatedIds ? new Set(relatedIds) : undefined,
  };

  const filteredProjects = filterProjects(projectList, projectFilters);

  // グルーピングを実行（status or roadmap）
  const groups =
    groupByMode === 'roadmap'
      ? groupProjects(filteredProjects, 'roadmap', roadmaps).map((g) => ({
          key: g.key,
          label: g.label,
          labelKey: g.key === 'unassigned' ? 'sidebar.projectView.unassigned' : undefined,
          projects: g.projects,
          isRoadmapGroup: true,
        }))
      : // status グルーピング
        STATUS_GROUPS.map((group) => ({
          ...group,
          projects: filteredProjects.filter((project) => normalizeStatus(project.status) === group.key),
          isRoadmapGroup: false,
        })).filter((g) => g.projects.length > 0);

  return (
    <div className="projects-page" data-testid="projects-page">
      <header className="projects-page__header">
        <h1 className="projects-page__title">{t('nav.projects')}</h1>
        <div className="projects-page__header-actions">
          <Link to="/roadmaps" className="projects-page__roadmaps-link" data-testid="projects-page-roadmaps-link">
            {t('sidebar.projectView.roadmapsLink')}
          </Link>
          <button
            type="button"
            className="projects-page__create-btn"
            onClick={() => setCreateModalOpen(true)}
            data-testid="projects-page-create-btn"
          >
            + {t('sidebar.newProject')}
          </button>
        </div>
      </header>

      <div className="projects-page__filter-row">
        <select
          className="projects-page__filter"
          value={filter}
          onChange={(e) => setFilter(e.target.value as FilterKey)}
          aria-label={t('sidebar.projectFilter.label')}
          data-testid="projects-page-filter-select"
        >
          <option value="all">{t('sidebar.projectFilter.all')}</option>
          <option value="mine">{t('sidebar.projectFilter.mine')}</option>
        </select>

        <select
          className="projects-page__filter"
          value={priorityFilter}
          onChange={(e) => setPriorityFilter(e.target.value)}
          aria-label={t('ticket.priority')}
          data-testid="projects-page-priority-filter"
        >
          <option value="">{t('ticket.priority')}</option>
          <option value="urgent">{t('ticket.priority_label.urgent')}</option>
          <option value="high">{t('ticket.priority_label.high')}</option>
          <option value="medium">{t('ticket.priority_label.medium')}</option>
          <option value="low">{t('ticket.priority_label.low')}</option>
        </select>

        <select
          className="projects-page__filter"
          value={teamFilter}
          onChange={(e) => setTeamFilter(e.target.value)}
          aria-label={t('sidebar.yourTeams')}
          data-testid="projects-page-team-filter"
        >
          <option value="">{t('sidebar.yourTeams')}</option>
          {teamList.map((team) => (
            <option key={team.id} value={String(team.id)}>{team.name}</option>
          ))}
        </select>

        <select
          className="projects-page__filter"
          value={parentProjectIdFilter}
          onChange={(e) => setParentProjectIdFilter(e.target.value)}
          aria-label={t('sidebar.projectFilter.parentProject')}
          data-testid="projects-page-parent-filter"
        >
          <option value="">{t('sidebar.projectFilter.parentProject')}</option>
          <option value="none">{t('sidebar.projectFilter.rootOnly')}</option>
          {projectList
            .filter((p) => !p.parentProjectId)
            .sort((a, b) => a.name.localeCompare(b.name, 'ja'))
            .map((project) => (
              <option key={project.id} value={String(project.id)}>
                {project.name}
              </option>
            ))}
        </select>

        <select
          className="projects-page__filter"
          value={roadmapIdFilter}
          onChange={(e) => setRoadmapIdFilter(e.target.value)}
          aria-label={t('sidebar.projectFilter.roadmap')}
          data-testid="projects-page-roadmap-filter"
        >
          <option value="">{t('sidebar.projectFilter.roadmap')}</option>
          {roadmaps.map((roadmap) => (
            <option key={roadmap.id} value={String(roadmap.id)}>
              {roadmap.name}
            </option>
          ))}
        </select>

        <select
          className="projects-page__filter"
          value={relatedToFilter}
          onChange={(e) => setRelatedToFilter(e.target.value)}
          aria-label={t('sidebar.projectFilter.related')}
          data-testid="projects-page-related-filter"
        >
          <option value="">{t('sidebar.projectFilter.related')}</option>
          {projectList.map((project) => (
            <option key={project.id} value={String(project.id)}>
              {project.name}
            </option>
          ))}
        </select>

        <label className="projects-page__hierarchy-toggle">
          <input
            type="checkbox"
            checked={hierarchyEnabled}
            onChange={(e) => setHierarchyEnabled(e.target.checked)}
            data-testid="projects-page-hierarchy-toggle"
          />
          {t('sidebar.projectView.hierarchy')}
        </label>

        <select
          className="projects-page__filter"
          value={groupByMode}
          onChange={(e) => setGroupByMode(e.target.value as 'status' | 'roadmap')}
          aria-label={t('sidebar.projectView.groupBy')}
          data-testid="projects-page-groupby-select"
        >
          <option value="status">{t('sidebar.projectView.groupByStatus')}</option>
          <option value="roadmap">{t('sidebar.projectView.groupByRoadmap')}</option>
        </select>

        <div className="projects-page__preset-wrapper" ref={presetRef} style={{ position: 'relative' }}>
          <button
            type="button"
            className="projects-page__preset-btn"
            onClick={() => setShowPresetMenu(!showPresetMenu)}
            title="保存済みビュー"
            data-testid="projects-page-preset-btn"
          >
            ⭐ {savedViewsData.length > 0 && <span className="projects-page__preset-count">{savedViewsData.length}</span>}
          </button>
          {showPresetMenu && (
            <div className="projects-page__preset-menu">
              {savedViewsData.length === 0 ? (
                <div className="projects-page__preset-empty">保存済みのビューはありません</div>
              ) : (
                savedViewsData.map((view) => (
                  <div key={view.id} className="projects-page__preset-item">
                    <button className="projects-page__preset-apply" onClick={() => applyPreset(view)}>
                      {view.name}
                    </button>
                    <button
                      className="projects-page__preset-delete"
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
                className="projects-page__preset-save"
                onClick={savePreset}
                data-testid="projects-page-save-preset-btn"
                disabled={createSavedViewMutation.isPending}
              >
                + 現在の条件をビューとして保存
              </button>
            </div>
          )}
        </div>
      </div>

      {isLoading ? (
        <div className="projects-page__empty">{t('common.loading')}</div>
      ) : projectList.length === 0 ? (
        <div className="projects-page__empty" data-testid="projects-page-empty">
          <p>{t('sidebar.noProjects')}</p>
        </div>
      ) : (
        groups.map((group: any) => {
          if (group.projects.length === 0) return null;
          const isOpen = groupOpen[group.key] ?? (group.defaultOpen ?? true);
          const groupLabel = group.labelKey ? t(group.labelKey) : group.label;
          return (
            <div key={group.key} className="projects-page__group">
              <button
                type="button"
                className="projects-page__group-header"
                onClick={() => toggleGroup(group.key)}
                aria-expanded={isOpen}
                data-testid={`projects-page-group-toggle-${group.key}`}
              >
                <span className={`projects-page__chevron ${isOpen ? 'projects-page__chevron--open' : ''}`}>▸</span>
                {groupLabel} ({group.projects.length})
              </button>
              {isOpen && (
                <ProjectsTable
                  projects={group.projects}
                  hierarchyEnabled={hierarchyEnabled}
                  allProjects={projectList}
                />
              )}
            </div>
          );
        })
      )}

      {createModalOpen && <ProjectCreateModal onClose={() => setCreateModalOpen(false)} />}
    </div>
  );
}
