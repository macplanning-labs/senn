/**
 * ProjectUpdatesSection.tsx — Overview に表示する進捗報告(Project Updates)
 *
 * 直近の進捗を表示し、プロジェクトのメンバー(または管理者)は投稿できる。
 * 編集・削除は投稿者と管理者に表示する(最終的な権限判定はサーバー側)。
 */

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useAuthStore } from '@/shared/stores/authStore';
import {
  useProjectUpdates,
  usePostProjectUpdate,
  useEditProjectUpdate,
  useDeleteProjectUpdate,
  type ProjectUpdate,
} from '../hooks/useProjectActivity';
import { ProjectUpdateForm } from './ProjectUpdateForm';
import './ProjectUpdates.css';

interface Props {
  projectId: number;
  isMember: boolean;
}

export function ProjectUpdatesSection({ projectId, isMember }: Props) {
  const { t } = useTranslation();
  const user = useAuthStore((s) => s.user);
  const { data, isLoading, isError } = useProjectUpdates(projectId);
  const post = usePostProjectUpdate(projectId);
  const edit = useEditProjectUpdate(projectId);
  const remove = useDeleteProjectUpdate(projectId);

  const [posting, setPosting] = useState(false);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [confirmingId, setConfirmingId] = useState<number | null>(null);

  const canPost = isMember || !!user?.isStaff;
  const canModify = (u: ProjectUpdate) => !!user && (u.authorId === user.id || user.isStaff);
  const updates = data?.results ?? [];

  return (
    <section className="project-updates" data-testid="project-updates">
      <div className="project-updates__header">
        <h2 className="project-updates__title">{t('projectUpdates.title')}</h2>
        {canPost && !posting && (
          <button
            type="button"
            className="project-updates__post-btn"
            onClick={() => setPosting(true)}
            data-testid="project-update-open"
          >
            {t('projectUpdates.post')}
          </button>
        )}
      </div>

      {!canPost && (
        <div className="project-updates__hint" data-testid="project-updates-member-hint">
          {t('projectUpdates.memberOnlyHint')}
        </div>
      )}

      {posting && (
        <ProjectUpdateForm
          submitLabel={t('projectUpdates.submit')}
          submitting={post.isPending}
          onCancel={() => setPosting(false)}
          onSubmit={(input) => post.mutate(input, { onSuccess: () => setPosting(false) })}
        />
      )}

      {isLoading && <div className="project-updates__empty">{t('projectActivity.loading')}</div>}
      {isError && <div className="project-updates__error">{t('projectUpdates.loadFailed')}</div>}
      {!isLoading && !isError && updates.length === 0 && !posting && (
        <div className="project-updates__empty" data-testid="project-updates-empty">
          {t('projectUpdates.empty')}
        </div>
      )}

      <ul className="project-updates__list">
        {updates.map((u) => (
          <li key={u.id} className="project-updates__item" data-testid={`project-update-${u.id}`}>
            {editingId === u.id ? (
              <ProjectUpdateForm
                initial={{ health: u.health, body: u.body }}
                submitLabel={t('projectUpdates.save')}
                submitting={edit.isPending}
                onCancel={() => setEditingId(null)}
                onSubmit={(input) => edit.mutate({ id: u.id, ...input }, { onSuccess: () => setEditingId(null) })}
              />
            ) : (
              <>
                <div className="project-updates__item-head">
                  <span className={`project-updates__badge project-updates__badge--${u.health}`}>
                    {t(`projectUpdates.health.${u.health}`)}
                  </span>
                  <span className="project-updates__meta">
                    {u.authorName ?? '—'} · {new Date(u.createdAt).toLocaleString()}
                    {u.updatedAt !== u.createdAt && ` · ${t('projectUpdates.edited')}`}
                  </span>
                  {canModify(u) && (
                    <span className="project-updates__actions">
                      <button type="button" onClick={() => setEditingId(u.id)}>
                        {t('projectUpdates.edit')}
                      </button>
                      {confirmingId === u.id ? (
                        <>
                          <span className="project-updates__confirm">{t('projectUpdates.deleteConfirm')}</span>
                          <button
                            type="button"
                            className="project-updates__danger"
                            disabled={remove.isPending}
                            onClick={() => remove.mutate(u.id, { onSettled: () => setConfirmingId(null) })}
                            data-testid={`project-update-delete-confirm-${u.id}`}
                          >
                            {t('projectUpdates.delete')}
                          </button>
                          <button type="button" onClick={() => setConfirmingId(null)}>
                            {t('projectUpdates.cancel')}
                          </button>
                        </>
                      ) : (
                        <button type="button" onClick={() => setConfirmingId(u.id)}>
                          {t('projectUpdates.delete')}
                        </button>
                      )}
                    </span>
                  )}
                </div>
                {u.body && <div className="project-updates__body">{u.body}</div>}
              </>
            )}
          </li>
        ))}
      </ul>
    </section>
  );
}
