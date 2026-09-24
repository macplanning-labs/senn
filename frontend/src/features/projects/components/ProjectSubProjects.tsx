/**
 * ProjectSubProjects.tsx — Projects タブ（親子・関連・ロードマップ）
 *
 * プロジェクトの構造(親・子・関連・ロードマップ)を表示・管理する。
 * 画面は サーバーの canManage / candidates に従い、権限を判定しない。
 */

import { useState } from 'react';
import { Link } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { useProject } from '@/shared/hooks/useProject';
import {
  useProjectStructure,
  useSetParent,
  useAddRelation,
  useRemoveRelation,
  useAddToRoadmap,
  useRemoveFromRoadmap,
  useCreateRoadmap,
} from '../hooks/useProjectStructure';
import { formatProgress } from '../utils/projectProgress';
import { projectStatusLabelKey } from '../utils/projectStatus';
import './ProjectSubProjects.css';

export function ProjectSubProjects() {
  const { t } = useTranslation();
  const { currentProject } = useProject();

  const { data, isLoading, isError } = useProjectStructure(currentProject?.id);

  if (!currentProject) {
    return <div className="project-sub-projects__state">{t('common.loading')}</div>;
  }

  if (isLoading) {
    return <div className="project-sub-projects__state">{t('common.loading')}</div>;
  }

  if (isError || !data) {
    return (
      <div className="project-sub-projects__state project-sub-projects__state--error">
        {t('projectSubProjects.loadFailed')}
      </div>
    );
  }

  return (
    <div className="project-sub-projects" data-testid="project-sub-projects">
      <HierarchySection structure={data} projectId={currentProject.id} projectName={currentProject.name} />
      <ChildrenSection structure={data} />
      <RelatedSection structure={data} projectId={currentProject.id} />
      <RoadsSection structure={data} projectId={currentProject.id} />
    </div>
  );
}

// ============================================================
// 階層セクション
// ============================================================

interface SectionProps {
  structure: Awaited<ReturnType<typeof useProjectStructure>>['data'];
  projectId?: number;
}

function HierarchySection({ structure, projectId, projectName }: SectionProps & { projectName: string }) {
  const { t } = useTranslation();
  const [parentSelectOpen, setParentSelectOpen] = useState(false);
  const setParent = useSetParent(projectId ?? 0);

  if (!structure || !projectId) return null;

  return (
    <section className="project-sub-projects__section">
      <h2 className="project-sub-projects__title">{t('projectSubProjects.hierarchy')}</h2>

      <nav className="project-sub-projects__breadcrumb" aria-label={t('projectSubProjects.hierarchy')} data-testid="hierarchy-breadcrumb">
        <span className="project-sub-projects__breadcrumb-item">{t('projectSubProjects.root')}</span>
        {structure.ancestors.map((anc) => (
          <span key={anc.id} className="project-sub-projects__breadcrumb-item">
            <span aria-hidden="true"> › </span>
            <Link to={`/project/${anc.prefix}`} className="project-sub-projects__breadcrumb-link">
              {anc.name}
            </Link>
          </span>
        ))}
        <span className="project-sub-projects__breadcrumb-item project-sub-projects__breadcrumb-current" aria-current="page">
          <span aria-hidden="true"> › </span>
          {projectName}
        </span>
      </nav>

      {structure.canManage && (
        <div className="project-sub-projects__parent-control" data-testid="hierarchy-parent-control">
          {parentSelectOpen ? (
            <select
              value={structure.parent?.id ?? ''}
              onChange={(e) => {
                const pid = e.target.value ? parseInt(e.target.value, 10) : null;
                void setParent.mutate(pid);
                setParentSelectOpen(false);
              }}
              disabled={setParent.isPending}
              autoFocus
              onBlur={() => setParentSelectOpen(false)}
              className="project-sub-projects__parent-select"
            >
              <option value="">{t('projectSubProjects.parentNone')}</option>
              {structure.candidates.parent.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.prefix} {p.name}
                </option>
              ))}
            </select>
          ) : (
            <>
              <span className="project-sub-projects__parent-label" data-testid="hierarchy-parent-current">
                {t('projectSubProjects.parentLabel')}:{' '}
                {structure.parent ? `${structure.parent.prefix} ${structure.parent.name}` : t('projectSubProjects.parentNone')}
              </span>
              <button
                type="button"
                onClick={() => setParentSelectOpen(true)}
                className="project-sub-projects__parent-button"
                data-testid="hierarchy-parent-change"
              >
                {t('projectSubProjects.parentChange')}
              </button>
            </>
          )}
        </div>
      )}
    </section>
  );
}

// ============================================================
// 子プロジェクトセクション
// ============================================================

function ChildrenSection({ structure }: SectionProps) {
  const { t } = useTranslation();
  if (!structure) return null;

  return (
    <section className="project-sub-projects__section">
      <h2 className="project-sub-projects__title">{t('projectSubProjects.children')}</h2>

      {structure.rollup.projectCount > 1 && (
        <div className="project-sub-projects__rollup">
          <div className="project-sub-projects__rollup-stat">
            {t('projectSubProjects.rollupCount', { count: structure.rollup.projectCount })}
          </div>
          <div className="project-sub-projects__rollup-progress">
            {structure.rollup.progress !== null ? (
              <>
                <div className="project-sub-projects__progress-bar">
                  <div
                    className="project-sub-projects__progress-fill"
                    style={{ width: `${Math.round(structure.rollup.progress * 100)}%` }}
                  />
                </div>
                <span className="project-sub-projects__progress-label">
                  {formatProgress(structure.rollup.progress)} ({structure.rollup.completedCount} / {structure.rollup.ticketCount})
                </span>
              </>
            ) : (
              <span className="project-sub-projects__progress-label">{formatProgress(null)}</span>
            )}
          </div>
        </div>
      )}

      {structure.children.length === 0 ? (
        <div className="project-sub-projects__empty">{t('projectSubProjects.childrenEmpty')}</div>
      ) : (
        <table className="project-sub-projects__table">
          <thead>
            <tr>
              <th>{t('projectSubProjects.name')}</th>
              <th>{t('projectSubProjects.status')}</th>
              <th>{t('projectSubProjects.progress')}</th>
              <th>{t('projectSubProjects.teams')}</th>
            </tr>
          </thead>
          <tbody>
            {structure.children.map((child) => (
              <tr key={child.id}>
                <td className="project-sub-projects__name-cell">
                  <Link to={`/project/${child.prefix}`}>{child.name}</Link>
                </td>
                <td className="project-sub-projects__status-cell">
                  <span className={`project-sub-projects__status project-sub-projects__status--${child.status}`}>
                    {(() => {
                      const key = projectStatusLabelKey(child.status);
                      return key ? t(key) : child.status;
                    })()}
                  </span>
                </td>
                <td className="project-sub-projects__progress-cell">
                  <div className="project-sub-projects__cell-flex">
                  {child.progress !== null ? (
                    <>
                      <div className="project-sub-projects__progress-bar-small">
                        <div
                          className="project-sub-projects__progress-fill"
                          style={{ width: `${Math.round(child.progress * 100)}%` }}
                        />
                      </div>
                      <span className="project-sub-projects__progress-pct">{formatProgress(child.progress)}</span>
                    </>
                  ) : (
                    formatProgress(null)
                  )}
                  </div>
                </td>
                <td className="project-sub-projects__teams-cell">
                  <div className="project-sub-projects__cell-flex">
                  {child.teams.map((team) => (
                    <span
                      key={team.id}
                      className="project-sub-projects__team-badge"
                      title={team.name}
                      style={team.color ? { backgroundColor: team.color } : undefined}
                    >
                      {team.icon || team.name.slice(0, 1)}
                    </span>
                  ))}
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}

// ============================================================
// 関連プロジェクトセクション
// ============================================================

function RelatedSection({ structure, projectId }: SectionProps) {
  const { t } = useTranslation();
  const [selectedRelated, setSelectedRelated] = useState('');
  const add = useAddRelation(projectId ?? 0);
  const remove = useRemoveRelation(projectId ?? 0);

  if (!structure || !projectId) return null;

  return (
    <section className="project-sub-projects__section">
      <h2 className="project-sub-projects__title">{t('projectSubProjects.related')}</h2>

      {structure.related.length === 0 ? (
        <div className="project-sub-projects__empty">{t('projectSubProjects.relatedEmpty')}</div>
      ) : (
        <ul className="project-sub-projects__list">
          {structure.related.map((rel) => (
            <li key={rel.id} className="project-sub-projects__list-item">
              <Link to={`/project/${rel.prefix}`}>{rel.name}</Link>
              {structure.canManage && (
                <button
                  type="button"
                  onClick={() => void remove.mutate(rel.id)}
                  disabled={remove.isPending}
                  className="project-sub-projects__remove-btn"
                  data-testid="related-remove"
                >
                  {t('projectSubProjects.remove')}
                </button>
              )}
            </li>
          ))}
        </ul>
      )}

      {structure.canManage && structure.candidates.related.length > 0 && (
        <div className="project-sub-projects__add-control">
          <select
            value={selectedRelated}
            onChange={(e) => {
              const rid = parseInt(e.target.value, 10);
              if (!isNaN(rid)) {
                void add.mutate(rid, { onSuccess: () => setSelectedRelated('') });
              }
            }}
            disabled={add.isPending}
            className="project-sub-projects__select"
          >
            <option value="">{t('projectSubProjects.addRelated')}</option>
            {structure.candidates.related.map((p) => (
              <option key={p.id} value={p.id}>
                {p.prefix} {p.name}
              </option>
            ))}
          </select>
        </div>
      )}
    </section>
  );
}

// ============================================================
// ロードマップセクション
// ============================================================

function RoadsSection({ structure, projectId }: SectionProps) {
  const { t } = useTranslation();
  const [selectedRoadmap, setSelectedRoadmap] = useState('');
  const [creatingRoadmap, setCreatingRoadmap] = useState(false);
  const [newRoadmapName, setNewRoadmapName] = useState('');

  const add = useAddToRoadmap(projectId ?? 0);
  const remove = useRemoveFromRoadmap(projectId ?? 0);
  const create = useCreateRoadmap();

  if (!structure || !projectId) return null;

  const handleCreateAndAdd = async () => {
    if (!newRoadmapName.trim()) return;
    try {
      const roadmap = await create.mutateAsync({ name: newRoadmapName });
      await add.mutateAsync(roadmap.id);
      setNewRoadmapName('');
      setCreatingRoadmap(false);
    } catch {
      // Error is handled by mutation callback
    }
  };

  return (
    <section className="project-sub-projects__section">
      <h2 className="project-sub-projects__title">{t('projectSubProjects.roadmaps')}</h2>
      <p className="project-sub-projects__roadmaps-all">
        <Link to="/roadmaps" data-testid="roadmaps-all-link">
          {t('projectSubProjects.roadmapsAll')}
        </Link>
      </p>

      {structure.roadmaps.length === 0 ? (
        <div className="project-sub-projects__empty">{t('projectSubProjects.roadsEmpty')}</div>
      ) : (
        <div className="project-sub-projects__badges">
          {structure.roadmaps.map((road) => (
            <div key={road.id} className="project-sub-projects__badge">
              <Link to={`/roadmaps/${road.id}`} className="project-sub-projects__roadmap-link" data-testid={`roadmap-link-${road.id}`}>
                {road.name}
              </Link>
              {road.canRemove && (
                <button
                  type="button"
                  onClick={() => void remove.mutate(road.id)}
                  disabled={remove.isPending}
                  className="project-sub-projects__badge-remove"
                  data-testid={`roadmap-remove-${road.id}`}
                >
                  ×
                </button>
              )}
            </div>
          ))}
        </div>
      )}

      {structure.candidates.roadmaps.length > 0 || structure.canManage ? (
        <div className="project-sub-projects__roadmap-control">
          {creatingRoadmap ? (
            <div className="project-sub-projects__create-inline">
              <input
                type="text"
                value={newRoadmapName}
                onChange={(e) => setNewRoadmapName(e.target.value)}
                placeholder={t('projectSubProjects.roadmapNamePlaceholder')}
                className="project-sub-projects__input"
                onKeyDown={(e) => {
                  if (e.key === 'Enter') void handleCreateAndAdd();
                  if (e.key === 'Escape') setCreatingRoadmap(false);
                }}
                autoFocus
              />
              <button
                type="button"
                onClick={() => void handleCreateAndAdd()}
                disabled={!newRoadmapName.trim() || create.isPending || add.isPending}
                className="project-sub-projects__create-btn"
              >
                {t('projectSubProjects.create')}
              </button>
              <button type="button" onClick={() => setCreatingRoadmap(false)} className="project-sub-projects__cancel-btn">
                {t('projectSubProjects.cancel')}
              </button>
            </div>
          ) : (
            <>
              {structure.candidates.roadmaps.length > 0 && (
                <select
                  value={selectedRoadmap}
                  onChange={(e) => {
                    const rid = parseInt(e.target.value, 10);
                    if (!isNaN(rid)) {
                      void add.mutate(rid, { onSuccess: () => setSelectedRoadmap('') });
                    }
                  }}
                  disabled={add.isPending}
                  className="project-sub-projects__select"
                >
                  <option value="">{t('projectSubProjects.addRoadmap')}</option>
                  {structure.candidates.roadmaps.map((r) => (
                    <option key={r.id} value={r.id}>
                      {r.name}
                    </option>
                  ))}
                  <option value="__create__">{t('projectSubProjects.createRoadmap')}</option>
                </select>
              )}
              {!structure.candidates.roadmaps.length && structure.canManage && (
                <button
                  type="button"
                  onClick={() => setCreatingRoadmap(true)}
                  className="project-sub-projects__select-button"
                >
                  {t('projectSubProjects.createRoadmap')}
                </button>
              )}
            </>
          )}
        </div>
      ) : null}
    </section>
  );
}
