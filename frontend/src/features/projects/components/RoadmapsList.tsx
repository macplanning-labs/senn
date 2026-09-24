/**
 * RoadmapsList.tsx — ロードマップ一覧・作成
 *
 * GET /roadmaps/ で一覧を取得、POST /roadmaps/ で作成
 */

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Link } from 'react-router-dom';
import { BackLink } from '@/shared/components/ui/BackLink';
import { useRoadmaps, useCreateRoadmap } from '../hooks/useProjectStructure';
import './RoadmapsList.css';

export function RoadmapsList() {
  const { t } = useTranslation();
  const { data, isLoading, isError } = useRoadmaps();
  const create = useCreateRoadmap();

  const [formOpen, setFormOpen] = useState(false);
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');

  const handleSubmit = async () => {
    if (!name.trim()) return;
    try {
      await create.mutateAsync({ name, description });
      setName('');
      setDescription('');
      setFormOpen(false);
    } catch {
      // Error handled by mutation callback
    }
  };

  if (isLoading) {
    return <div className="roadmaps-list__state">{t('common.loading')}</div>;
  }

  if (isError) {
    return <div className="roadmaps-list__state roadmaps-list__state--error">{t('roadmaps.loadFailed')}</div>;
  }

  return (
    <div className="roadmaps-list" data-testid="roadmaps-list">
      <BackLink to="/projects" label={t('nav.backTo.projects')} testId="roadmaps-back-to-projects" />
      <div className="roadmaps-list__header">
        <h1 className="roadmaps-list__title">{t('roadmaps.title')}</h1>
        {!formOpen && (
          <button
            type="button"
            onClick={() => setFormOpen(true)}
            className="roadmaps-list__create-btn"
            data-testid="roadmaps-create-btn"
          >
            {t('roadmaps.create')}
          </button>
        )}
      </div>

      {formOpen && (
        <div className="roadmaps-list__form">
          <input
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={t('roadmaps.namePlaceholder')}
            className="roadmaps-list__input"
            data-testid="roadmaps-name-input"
          />
          <textarea
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder={t('roadmaps.descriptionPlaceholder')}
            className="roadmaps-list__textarea"
            data-testid="roadmaps-description-input"
          />
          <div className="roadmaps-list__form-actions">
            <button
              type="button"
              onClick={() => void handleSubmit()}
              disabled={!name.trim() || create.isPending}
              className="roadmaps-list__submit-btn"
              data-testid="roadmaps-submit-btn"
            >
              {t('roadmaps.create')}
            </button>
            <button
              type="button"
              onClick={() => setFormOpen(false)}
              className="roadmaps-list__cancel-btn"
            >
              {t('roadmaps.cancel')}
            </button>
          </div>
        </div>
      )}

      {data && data.length === 0 ? (
        <div className="roadmaps-list__empty" data-testid="roadmaps-empty">
          {t('roadmaps.empty')}
        </div>
      ) : (
        <div className="roadmaps-list__items">
          {data?.map((roadmap) => (
            <Link
              key={roadmap.id}
              to={`/roadmaps/${roadmap.id}`}
              className="roadmaps-list__item"
              data-testid={`roadmap-item-${roadmap.id}`}
            >
              <div className="roadmaps-list__item-name">{roadmap.name}</div>
              <div className="roadmaps-list__item-count">{t('roadmaps.projectCount', { count: roadmap.projectCount ?? 0 })}</div>
            </Link>
          ))}
        </div>
      )}
    </div>
  );
}
