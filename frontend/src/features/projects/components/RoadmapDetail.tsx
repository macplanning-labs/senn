/**
 * RoadmapDetail.tsx — ロードマップ詳細・編集・削除
 *
 * GET /roadmaps/{id}/ で詳細を取得、PUT で編集、DELETE で削除
 * 削除は画面内2段階確認
 */

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useParams, useNavigate } from 'react-router-dom';
import { BackLink } from '@/shared/components/ui/BackLink';
import { useRoadmapDetail, useUpdateRoadmap, useDeleteRoadmap } from '../hooks/useProjectStructure';
import { formatProgress } from '../utils/projectProgress';
import { projectStatusLabelKey } from '../utils/projectStatus';
import './RoadmapDetail.css';

export function RoadmapDetail() {
  const { t } = useTranslation();
  const { id: roadmapIdStr } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const roadmapId = roadmapIdStr ? parseInt(roadmapIdStr, 10) : 0;

  const { data, isLoading, isError } = useRoadmapDetail(roadmapId);
  const update = useUpdateRoadmap(roadmapId);
  const deleteRoadmap = useDeleteRoadmap(roadmapId);

  const [editMode, setEditMode] = useState(false);
  const [name, setName] = useState(data?.name ?? '');
  const [description, setDescription] = useState(data?.description ?? '');
  const [deleteConfirm, setDeleteConfirm] = useState(false);

  // データが更新されたときに form をリセット
  if (data && (name !== data.name || description !== data.description)) {
    setName(data.name);
    setDescription(data.description);
  }

  const handleUpdate = async () => {
    if (!name.trim()) return;
    try {
      await update.mutateAsync({ name, description });
      setEditMode(false);
    } catch {
      // Error handled by mutation callback
    }
  };

  const handleDelete = async () => {
    try {
      await deleteRoadmap.mutateAsync();
      navigate('/roadmaps/');
    } catch {
      // Error handled by mutation callback
    }
  };

  if (isLoading) {
    return <div className="roadmap-detail__state">{t('common.loading')}</div>;
  }

  if (isError || !data) {
    return <div className="roadmap-detail__state roadmap-detail__state--error">{t('roadmaps.notFound')}</div>;
  }

  return (
    <div className="roadmap-detail" data-testid="roadmap-detail">
      <BackLink to="/roadmaps" label={t('nav.backTo.roadmaps')} testId="roadmap-back-to-list" />
      <div className="roadmap-detail__header">
        <div className="roadmap-detail__title-area">
          {editMode ? (
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              className="roadmap-detail__title-input"
              data-testid="roadmap-edit-name"
            />
          ) : (
            <h1 className="roadmap-detail__title">{data.name}</h1>
          )}
        </div>
        {data.canManage && !editMode && (
          <div className="roadmap-detail__actions">
            <button
              type="button"
              onClick={() => setEditMode(true)}
              className="roadmap-detail__edit-btn"
              data-testid="roadmap-edit-btn"
            >
              {t('roadmaps.edit')}
            </button>
            <button
              type="button"
              onClick={() => setDeleteConfirm(true)}
              className="roadmap-detail__delete-btn"
              data-testid="roadmap-delete-btn"
            >
              {t('roadmaps.delete')}
            </button>
          </div>
        )}
      </div>

      {editMode && data.canManage && (
        <div className="roadmap-detail__edit-form">
          <label className="roadmap-detail__label">{t('roadmaps.description')}</label>
          <textarea
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            className="roadmap-detail__description-input"
            data-testid="roadmap-edit-description"
          />
          <div className="roadmap-detail__edit-actions">
            <button
              type="button"
              onClick={() => void handleUpdate()}
              disabled={!name.trim() || update.isPending}
              className="roadmap-detail__save-btn"
            >
              {t('roadmaps.save')}
            </button>
            <button
              type="button"
              onClick={() => {
                setEditMode(false);
                setName(data.name);
                setDescription(data.description);
              }}
              className="roadmap-detail__cancel-btn"
            >
              {t('roadmaps.cancel')}
            </button>
          </div>
        </div>
      )}

      {deleteConfirm && data.canManage && (
        <div className="roadmap-detail__delete-confirm">
          <p>{t('roadmaps.deleteConfirm')}</p>
          <div className="roadmap-detail__delete-confirm-actions">
            <button
              type="button"
              onClick={() => void handleDelete()}
              disabled={deleteRoadmap.isPending}
              className="roadmap-detail__confirm-delete-btn"
              data-testid="roadmap-confirm-delete"
            >
              {t('roadmaps.deleteConfirm')}
            </button>
            <button
              type="button"
              onClick={() => setDeleteConfirm(false)}
              className="roadmap-detail__cancel-btn"
            >
              {t('roadmaps.cancel')}
            </button>
          </div>
        </div>
      )}

      <section className="roadmap-detail__projects">
        <h2 className="roadmap-detail__projects-title">{t('roadmaps.projects')}</h2>
        {data.projects.length === 0 ? (
          <div className="roadmap-detail__empty" data-testid="roadmap-projects-empty">
            {t('roadmaps.projectsEmpty')}
          </div>
        ) : (
          <table className="roadmap-detail__table">
            <thead>
              <tr>
                <th>{t('roadmaps.projectName')}</th>
                <th>{t('roadmaps.status')}</th>
                <th>{t('roadmaps.progress')}</th>
                <th>{t('roadmaps.tickets')}</th>
              </tr>
            </thead>
            <tbody>
              {data.projects.map((proj) => (
                <tr key={proj.id}>
                  <td className="roadmap-detail__project-name">
                    <a href={`/project/${proj.prefix}/`}>{proj.name}</a>
                  </td>
                  <td className="roadmap-detail__project-status">{projectStatusLabelKey(proj.status) ? t(projectStatusLabelKey(proj.status) as string) : proj.status}</td>
                  <td className="roadmap-detail__project-progress">{formatProgress(proj.progress)}</td>
                  <td className="roadmap-detail__project-tickets">
                    {proj.completedCount} / {proj.ticketCount}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>
    </div>
  );
}
