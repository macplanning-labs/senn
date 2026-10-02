/**
 * MilestoneSettings.tsx — マイルストーンCRUD管理
 *
 * 一覧テーブル（期限・進捗率表示）＋モーダルでCRUD完結。
 * KI「マスタメンテナンス — モーダル編集方式」準拠
 */

import { useState, useCallback } from 'react';
import { DateInput } from '@/shared/components/ui/DateInput';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { useTranslation } from 'react-i18next';

// ─── 型定義 ─────────────────────────────────────
interface Milestone {
  id: number;
  name: string;
  dueDate: string | null;
  description: string;
  project: number;
  openTicketCount: number;
  closedTicketCount: number;
  createdAt: string;
}

interface MilestoneFormData {
  name: string;
  due_date: string;
  description: string;
}

interface MilestoneSettingsProps {
  projectId: number;
}

// ─── ヘルパー ────────────────────────────────────

/** 日付フォーマット: YYYY-MM-DD → M/D 表示 */
function formatDate(dateStr: string | null, noDueDateLabel: string): string {
  if (!dateStr) return noDueDateLabel;
  const d = new Date(dateStr + 'T00:00:00');
  return `${d.getMonth() + 1}/${d.getDate()}`;
}

/** 期限超過チェック */
function isOverdue(dateStr: string | null): boolean {
  if (!dateStr) return false;
  return new Date(dateStr + 'T23:59:59') < new Date();
}

/** 進捗率を計算 */
function calcProgress(open: number, closed: number): number {
  const total = open + closed;
  if (total === 0) return 0;
  return Math.round((closed / total) * 100);
}

// ─── コンポーネント ──────────────────────────────

export function MilestoneSettings({ projectId }: MilestoneSettingsProps) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [modalOpen, setModalOpen] = useState(false);
  const [editingMilestone, setEditingMilestone] = useState<Milestone | null>(null);
  const [deleteConfirm, setDeleteConfirm] = useState<Milestone | null>(null);
  const [formData, setFormData] = useState<MilestoneFormData>({
    name: '', due_date: '', description: '',
  });
  const [error, setError] = useState('');

  // --- データ取得 ---
  const { data, isLoading } = useQuery<{ results: Milestone[] }>({
    queryKey: ['milestones', projectId],
    queryFn: async () => {
      const res = await apiClient.get<{ results: Milestone[] }>('/milestones/', {
        params: { project: projectId },
      });
      return res.data;
    },
  });

  const milestones = data?.results ?? [];

  // --- 作成 ---
  const createMutation = useMutation({
    mutationFn: (data: MilestoneFormData) =>
      apiClient.post('/milestones/', {
        ...data,
        project: projectId,
        due_date: data.due_date || null,
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['milestones', projectId] });
      closeModal();
    },
    onError: () => setError(t('common.createFailed')),
  });

  // --- 更新 ---
  const updateMutation = useMutation({
    mutationFn: ({ id, data }: { id: number; data: MilestoneFormData }) =>
      apiClient.put(`/milestones/${id}/`, {
        ...data,
        project: projectId,
        due_date: data.due_date || null,
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['milestones', projectId] });
      closeModal();
    },
    onError: () => setError(t('common.updateFailed')),
  });

  // --- 削除 ---
  const deleteMutation = useMutation({
    mutationFn: (id: number) => apiClient.delete(`/milestones/${id}/`),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['milestones', projectId] });
      setDeleteConfirm(null);
    },
    onError: () => setError(t('common.deleteFailed')),
  });

  // --- モーダル制御 ---
  const openCreateModal = useCallback(() => {
    setEditingMilestone(null);
    setFormData({ name: '', due_date: '', description: '' });
    setError('');
    setModalOpen(true);
  }, []);

  const openEditModal = useCallback((ms: Milestone) => {
    setEditingMilestone(ms);
    setFormData({
      name: ms.name,
      due_date: ms.dueDate ?? '',
      description: ms.description,
    });
    setError('');
    setModalOpen(true);
  }, []);

  const closeModal = useCallback(() => {
    setModalOpen(false);
    setEditingMilestone(null);
    setError('');
  }, []);

  const handleSubmit = useCallback((e: React.FormEvent) => {
    e.preventDefault();
    if (!formData.name.trim()) {
      setError(t('settings.milestoneNameRequired'));
      return;
    }
    if (editingMilestone) {
      updateMutation.mutate({ id: editingMilestone.id, data: formData });
    } else {
      createMutation.mutate(formData);
    }
  }, [formData, editingMilestone, createMutation, updateMutation]);

  const isSaving = createMutation.isPending || updateMutation.isPending;

  if (isLoading) {
    return <div className="settings-empty"><div className="settings-empty__text">{t('common.loading')}</div></div>;
  }

  return (
    <div>
      {/* ヘッダー */}
      <div className="settings-section__header">
        <div>
          <h2 className="settings-section__title">
            Milestones
            <span className="settings-section__count">({milestones.length})</span>
          </h2>
          <div style={{ fontSize: 'var(--font-size-sm)', color: 'var(--color-text-tertiary)', marginTop: 'var(--space-1)' }}>
            {t('settings.milestonesHelp')}
          </div>
        </div>
        <button
          className="settings-section__add-btn"
          onClick={openCreateModal}
          data-testid="add-milestone-btn"
        >
          {t('settings.addNew')}
        </button>
      </div>

      {/* テーブル */}
      {milestones.length > 0 ? (
        <table className="settings-table">
          <thead>
            <tr>
              <th>{t('settings.milestoneCol')}</th>
              <th>{t('settings.dueDateCol')}</th>
              <th>{t('settings.progressCol')}</th>
              <th style={{ width: 100, textAlign: 'right' }}>{t('common.action')}</th>
            </tr>
          </thead>
          <tbody>
            {milestones.map((ms) => {
              const progress = calcProgress(ms.openTicketCount, ms.closedTicketCount);
              const total = ms.openTicketCount + ms.closedTicketCount;
              return (
                <tr key={ms.id} onClick={() => openEditModal(ms)}>
                  <td>
                    <span className="settings-table__color-name">{ms.name}</span>
                  </td>
                  <td>
                    <span className={`milestone-due ${isOverdue(ms.dueDate) ? 'milestone-due--overdue' : ''}`}>
                      {formatDate(ms.dueDate, t('settings.noDueDate'))}
                      {isOverdue(ms.dueDate) && ' ⚠️'}
                    </span>
                  </td>
                  <td>
                    <div className="milestone-progress">
                      <div className="milestone-progress__bar">
                        <div
                          className="milestone-progress__fill"
                          style={{ width: `${progress}%` }}
                        />
                      </div>
                      <span className="milestone-progress__text">
                        {total > 0 ? `${ms.closedTicketCount}/${total}` : '-'}
                      </span>
                    </div>
                  </td>
                  <td>
                    <div className="settings-table__actions">
                      <button
                        className="settings-table__action-btn settings-table__action-btn--danger"
                        onClick={(e) => { e.stopPropagation(); setDeleteConfirm(ms); }}
                        title={t('common.delete')}
                      >
                        🗑
                      </button>
                    </div>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      ) : (
        <div className="settings-empty">
          <div className="settings-empty__icon">🎯</div>
          <div className="settings-empty__text">
            {t('settings.noMilestonesHint', { button: t('settings.addNew') })}
          </div>
        </div>
      )}

      {/* 作成・編集モーダル */}
      {modalOpen && (
        <div className="settings-modal__overlay" onClick={closeModal}>
          <div className="settings-modal" onClick={(e) => e.stopPropagation()}>
            <div className="settings-modal__header">
              <h3 className="settings-modal__title">
                {editingMilestone ? t('settings.editMilestone') : t('settings.newMilestone')}
              </h3>
              <button className="settings-modal__close" onClick={closeModal}>×</button>
            </div>

            <form onSubmit={handleSubmit}>
              <div className="settings-form__group">
                <label className="settings-form__label">{t('settings.name')}</label>
                <input
                  className="settings-form__input"
                  value={formData.name}
                  onChange={(e) => setFormData(prev => ({ ...prev, name: e.target.value }))}
                  placeholder={t('settings.milestoneNamePlaceholder')}
                  autoFocus
                  data-testid="milestone-name-input"
                />
              </div>

              <div className="settings-form__group">
                <label className="settings-form__label">{t('settings.dueDate')}</label>
                <DateInput
                  className="settings-form__input"
                  value={formData.due_date || null}
                  onChange={(value) => setFormData(prev => ({ ...prev, due_date: value ?? '' }))}
                  testId="milestone-due-input"
                />
              </div>

              <div className="settings-form__group">
                <label className="settings-form__label">{t('settings.description')}</label>
                <textarea
                  className="settings-form__textarea"
                  value={formData.description}
                  onChange={(e) => setFormData(prev => ({ ...prev, description: e.target.value }))}
                  placeholder={t("settings.milestonePlaceholder")}
                  data-testid="milestone-desc-input"
                />
              </div>

              {error && <div className="settings-form__error">{error}</div>}

              <div className="settings-form__actions">
                <button
                  type="button"
                  className="settings-form__btn settings-form__btn--secondary"
                  onClick={closeModal}
                >
                  {t('common.cancel')}
                </button>
                <button
                  type="submit"
                  className="settings-form__btn settings-form__btn--primary"
                  disabled={isSaving}
                  data-testid="milestone-save-btn"
                >
                  {isSaving ? t('common.saving') : editingMilestone ? t('common.update') : t('common.create')}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {/* 削除確認ダイアログ */}
      {deleteConfirm && (
        <div className="settings-modal__overlay" onClick={() => setDeleteConfirm(null)}>
          <div className="settings-modal" onClick={(e) => e.stopPropagation()}>
            <div className="settings-modal__header">
              <h3 className="settings-modal__title">{t('settings.deleteMilestone')}</h3>
              <button className="settings-modal__close" onClick={() => setDeleteConfirm(null)}>×</button>
            </div>
            <div className="confirm-dialog__message">
              {t('common.deleteConfirm', { name: deleteConfirm.name })}
            </div>
            <div className="confirm-dialog__warning">
              ⚠️ {t('settings.deleteMilestoneWarning')}
            </div>
            <div className="settings-form__actions">
              <button
                className="settings-form__btn settings-form__btn--secondary"
                onClick={() => setDeleteConfirm(null)}
              >
                {t('common.cancel')}
              </button>
              <button
                className="settings-form__btn settings-form__btn--danger"
                onClick={() => deleteMutation.mutate(deleteConfirm.id)}
                disabled={deleteMutation.isPending}
                data-testid="milestone-delete-confirm-btn"
              >
                {deleteMutation.isPending ? t('common.deleting') : t('common.delete')}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
