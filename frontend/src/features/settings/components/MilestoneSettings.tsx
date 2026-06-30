/**
 * MilestoneSettings.tsx — マイルストーンCRUD管理
 *
 * 一覧テーブル（期限・進捗率表示）＋モーダルでCRUD完結。
 * KI「マスタメンテナンス — モーダル編集方式」準拠
 */

import { useState, useCallback } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';

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
function formatDate(dateStr: string | null): string {
  if (!dateStr) return '期限なし';
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
    onError: () => setError('作成に失敗しました'),
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
    onError: () => setError('更新に失敗しました'),
  });

  // --- 削除 ---
  const deleteMutation = useMutation({
    mutationFn: (id: number) => apiClient.delete(`/milestones/${id}/`),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['milestones', projectId] });
      setDeleteConfirm(null);
    },
    onError: () => setError('削除に失敗しました'),
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
      setError('マイルストーン名を入力してください');
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
    return <div className="settings-empty"><div className="settings-empty__text">読み込み中...</div></div>;
  }

  return (
    <div>
      {/* ヘッダー */}
      <div className="settings-section__header">
        <h2 className="settings-section__title">
          Milestones
          <span className="settings-section__count">({milestones.length})</span>
        </h2>
        <button
          className="settings-section__add-btn"
          onClick={openCreateModal}
          data-testid="add-milestone-btn"
        >
          + 追加
        </button>
      </div>

      {/* テーブル */}
      {milestones.length > 0 ? (
        <table className="settings-table">
          <thead>
            <tr>
              <th>マイルストーン</th>
              <th>期限</th>
              <th>進捗</th>
              <th style={{ width: 100, textAlign: 'right' }}>アクション</th>
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
                      {formatDate(ms.dueDate)}
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
                        title="削除"
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
            マイルストーンがまだありません。「＋追加」ボタンで作成してください。
          </div>
        </div>
      )}

      {/* 作成・編集モーダル */}
      {modalOpen && (
        <div className="settings-modal__overlay" onClick={closeModal}>
          <div className="settings-modal" onClick={(e) => e.stopPropagation()}>
            <div className="settings-modal__header">
              <h3 className="settings-modal__title">
                {editingMilestone ? 'マイルストーンを編集' : '新しいマイルストーン'}
              </h3>
              <button className="settings-modal__close" onClick={closeModal}>×</button>
            </div>

            <form onSubmit={handleSubmit}>
              <div className="settings-form__group">
                <label className="settings-form__label">マイルストーン名</label>
                <input
                  className="settings-form__input"
                  value={formData.name}
                  onChange={(e) => setFormData(prev => ({ ...prev, name: e.target.value }))}
                  placeholder="例: v1.0 リリース, Sprint 1"
                  autoFocus
                  data-testid="milestone-name-input"
                />
              </div>

              <div className="settings-form__group">
                <label className="settings-form__label">期限日</label>
                <input
                  className="settings-form__input"
                  type="date"
                  value={formData.due_date}
                  onChange={(e) => setFormData(prev => ({ ...prev, due_date: e.target.value }))}
                  data-testid="milestone-due-input"
                />
              </div>

              <div className="settings-form__group">
                <label className="settings-form__label">説明</label>
                <textarea
                  className="settings-form__textarea"
                  value={formData.description}
                  onChange={(e) => setFormData(prev => ({ ...prev, description: e.target.value }))}
                  placeholder="マイルストーンの目標・スコープを記載"
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
                  キャンセル
                </button>
                <button
                  type="submit"
                  className="settings-form__btn settings-form__btn--primary"
                  disabled={isSaving}
                  data-testid="milestone-save-btn"
                >
                  {isSaving ? '保存中...' : editingMilestone ? '更新' : '作成'}
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
              <h3 className="settings-modal__title">マイルストーンを削除</h3>
              <button className="settings-modal__close" onClick={() => setDeleteConfirm(null)}>×</button>
            </div>
            <div className="confirm-dialog__message">
              マイルストーン「<strong>{deleteConfirm.name}</strong>」を削除しますか？
            </div>
            <div className="confirm-dialog__warning">
              ⚠️ このマイルストーンに紐付いているチケットのマイルストーンが未設定になります。
            </div>
            <div className="settings-form__actions">
              <button
                className="settings-form__btn settings-form__btn--secondary"
                onClick={() => setDeleteConfirm(null)}
              >
                キャンセル
              </button>
              <button
                className="settings-form__btn settings-form__btn--danger"
                onClick={() => deleteMutation.mutate(deleteConfirm.id)}
                disabled={deleteMutation.isPending}
                data-testid="milestone-delete-confirm-btn"
              >
                {deleteMutation.isPending ? '削除中...' : '削除'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
