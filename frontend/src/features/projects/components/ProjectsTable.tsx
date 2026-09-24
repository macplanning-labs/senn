/**
 * ProjectsTable.tsx — プロジェクト一覧テーブル(共通)
 *
 * Name(+チームバッジ)/Priority/Target date/Issues/Status の列を持つ。
 * グローバルなプロジェクト一覧ページとチーム配下のプロジェクト一覧
 * ページの両方で共有する。
 * 階層表示対応: hierarchyEnabled=true の場合、ツリー構造で表示する。
 */

import { useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import type { Project } from '@/shared/hooks/useProject';
import { buildHierarchyRows, type HierarchyRow } from '../utils/projectHierarchy';
import './ProjectsTable.css';

interface ProjectsTableProps {
  projects: Project[];
  hierarchyEnabled?: boolean;
  allProjects?: Project[];
}

const STATUS_KEYS: Record<string, string> = {
  in_progress: 'sidebar.projectStatus.inProgress',
  planned: 'sidebar.projectStatus.planned',
  paused: 'sidebar.projectStatus.paused',
  completed: 'sidebar.projectStatus.completed',
};

const PRIORITY_KEYS: Record<string, string> = {
  urgent: 'ticket.priority_label.urgent',
  high: 'ticket.priority_label.high',
  medium: 'ticket.priority_label.medium',
  low: 'ticket.priority_label.low',
};

export function ProjectsTable({ projects, hierarchyEnabled, allProjects }: ProjectsTableProps) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const [collapsedNodes, setCollapsedNodes] = useState<Set<number>>(() => new Set());

  if (projects.length === 0) {
    return null;
  }

  // 階層表示: このグループに表示するプロジェクトを、親の下に並べる。
  // 親が別のグループ/絞り込みで外れている場合は、文脈として親(薄く表示)を加える。
  const hierarchyRows: HierarchyRow[] | null =
    hierarchyEnabled && allProjects
      ? buildHierarchyRows(allProjects, projects, collapsedNodes)
      : null;

  const toggleExpand = (projectId: number) => {
    setCollapsedNodes((prev) => {
      const next = new Set(prev);
      if (next.has(projectId)) {
        next.delete(projectId);
      } else {
        next.add(projectId);
      }
      return next;
    });
  };

  const renderProjectRow = (project: Project, depth: number = 0, isContext: boolean = false, canCollapse: boolean = false) => {
    const statusI18nKey = (project.status ? STATUS_KEYS[project.status as keyof typeof STATUS_KEYS] : undefined) ?? STATUS_KEYS['in_progress'];
    const priorityI18nKey = (project.priority ? PRIORITY_KEYS[project.priority as keyof typeof PRIORITY_KEYS] : undefined) ?? PRIORITY_KEYS['medium'];
    const isExpanded = !collapsedNodes.has(project.id);

    return (
      <tr
        key={project.id}
        className={`projects-table__row ${isContext ? 'projects-table__row--context' : ''}`}
        data-testid={`projects-table-row-${project.id}`}
        onClick={() => navigate(`/project/${project.prefix}`)}
        style={{ cursor: 'pointer' }}
      >
        <td className="projects-table__td projects-table__td--name">
          {hierarchyEnabled && (
            <span
              className="projects-table__indent"
              style={{ width: `${depth * 16}px` }}
              data-depth={depth}
            />
          )}
          {hierarchyEnabled && canCollapse && (
            <button
              className={`projects-table__expand-btn ${isExpanded ? 'projects-table__expand-btn--open' : ''}`}
              onClick={(e) => {
                e.stopPropagation();
                toggleExpand(project.id);
              }}
              aria-expanded={isExpanded}
              title={isExpanded ? t('sidebar.projectView.collapse') : t('sidebar.projectView.expand')}
              aria-label={isExpanded ? t('sidebar.projectView.collapse') : t('sidebar.projectView.expand')}
              data-testid={`projects-table-expand-${project.id}`}
            >
              ▸
            </button>
          )}
          {hierarchyEnabled && !canCollapse && (
            <span className="projects-table__expand-placeholder" />
          )}
          <Link
            to={`/project/${project.prefix}`}
            className="projects-table__link"
            onClick={(e) => e.stopPropagation()}
          >
            <span className="projects-table__icon">{project.prefix[0]?.toUpperCase() ?? 'P'}</span>
            <span className="projects-table__name">{project.name}</span>
            <span className="projects-table__prefix">{project.prefix}</span>
          </Link>
          {hierarchyEnabled && (project.childCount ?? 0) > 0 && (
            <span className="projects-table__child-count" data-testid={`projects-table-child-count-${project.id}`}>
              {t('sidebar.projectView.childCount', { count: project.childCount })}
            </span>
          )}
          {project.teams && project.teams.length > 0 && (
            <span
              className="projects-table__teams"
              title={project.teams.map((team) => team.name).join(', ')}
            >
              {project.teams.map((team) => (
                <span
                  key={team.id}
                  className="projects-table__team-dot"
                  style={{ backgroundColor: team.color }}
                />
              ))}
            </span>
          )}
        </td>
        <td className="projects-table__td">{t(priorityI18nKey as string)}</td>
        <td className="projects-table__td">{project.targetEndDate ?? '—'}</td>
        <td className="projects-table__td">{project.ticketCount ?? 0}</td>
        <td className="projects-table__td">{t(statusI18nKey as string)}</td>
      </tr>
    );
  };

  return (
    <table className="projects-table">
      <thead>
        <tr>
          <th className="projects-table__th projects-table__th--name">{t('sidebar.projectName')}</th>
          <th className="projects-table__th">{t('ticket.priority')}</th>
          <th className="projects-table__th">Target date</th>
          <th className="projects-table__th">Issues</th>
          <th className="projects-table__th">Status</th>
        </tr>
      </thead>
      <tbody>
        {hierarchyRows
          ? hierarchyRows.map((row) => renderProjectRow(row.project, row.depth, row.isContext, row.hasChildren))
          : projects.map((project) => renderProjectRow(project))}
      </tbody>
    </table>
  );
}
