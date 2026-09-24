/**
 * ProjectUpdateForm.tsx — 進捗報告の投稿・編集フォーム
 */

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { HEALTH_VALUES, type ProjectHealth, type ProjectUpdateInput } from '../hooks/useProjectActivity';
import './ProjectUpdates.css';

interface Props {
  initial?: ProjectUpdateInput;
  submitLabel: string;
  submitting: boolean;
  onSubmit: (input: ProjectUpdateInput) => void;
  onCancel: () => void;
}

export function ProjectUpdateForm({ initial, submitLabel, submitting, onSubmit, onCancel }: Props) {
  const { t } = useTranslation();
  const [health, setHealth] = useState<ProjectHealth>(initial?.health ?? 'on_track');
  const [body, setBody] = useState(initial?.body ?? '');

  return (
    <form
      className="project-update-form"
      data-testid="project-update-form"
      onSubmit={(e) => {
        e.preventDefault();
        onSubmit({ health, body: body.trim() });
      }}
    >
      <fieldset className="project-update-form__health">
        <legend className="project-update-form__label">{t('projectUpdates.healthLabel')}</legend>
        {HEALTH_VALUES.map((h) => (
          <label
            key={h}
            className={`project-update-form__health-option project-update-form__health-option--${h}${
              health === h ? ' project-update-form__health-option--selected' : ''
            }`}
          >
            <input
              type="radio"
              name="project-update-health"
              value={h}
              checked={health === h}
              onChange={() => setHealth(h)}
              data-testid={`project-update-health-${h}`}
            />
            {t(`projectUpdates.health.${h}`)}
          </label>
        ))}
      </fieldset>

      <label className="project-update-form__label" htmlFor="project-update-body">
        {t('projectUpdates.bodyLabel')}
      </label>
      <textarea
        id="project-update-body"
        className="project-update-form__body"
        value={body}
        maxLength={5000}
        rows={4}
        placeholder={t('projectUpdates.bodyPlaceholder')}
        onChange={(e) => setBody(e.target.value)}
        data-testid="project-update-body"
      />

      <div className="project-update-form__actions">
        <button type="button" className="project-update-form__cancel" onClick={onCancel} disabled={submitting}>
          {t('projectUpdates.cancel')}
        </button>
        <button
          type="submit"
          className="project-update-form__submit"
          disabled={submitting}
          data-testid="project-update-submit"
        >
          {submitting ? t('projectUpdates.saving') : submitLabel}
        </button>
      </div>
    </form>
  );
}
