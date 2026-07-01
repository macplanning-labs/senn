/**
 * LabelSettings.tsx — ラベルCRUD管理
 *
 * KI「マスタメンテナンス — モーダル編集方式」準拠
 * 一覧テーブル＋モーダルでCRUD完結。
 */

import { useState, useCallback } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { useTranslation } from 'react-i18next';

// ─── 型定義 ─────────────────────────────────────
interface Label {
  id: number;
  name: string;
  color: string;
  project: number;
  createdAt: string;
}

interface LabelFormData {
  name: string;
  color: string;
}

interface LabelSettingsProps {
  projectId: number;
}

// ─── カラーパレット ──────────────────────────────
const COLOR_PRESETS = [
  '#ef4444', '#f97316', '#f59e0b', '#eab308',
  '#84cc16', '#22c55e', '#10b981', '#14b8a6',
  '#06b6d4', '#0ea5e9', '#3b82f6', '#6366f1',
  '#8b5cf6', '#a855f7', '#d946ef', '#ec4899',
  '#f43f5e', '#78716c', '#64748b', '#475569',
];

/** ラベルの背景色からテキスト色を計算 */
function getTextColor(bgColor: string): string {
  const hex = bgColor.replace('#', '');
  const r = parseInt(hex.substring(0, 2), 16);
  const g = parseInt(hex.substring(2, 4), 16);
  const b = parseInt(hex.substring(4, 6), 16);
  const luminance = (0.299 * r + 0.587 * g + 0.114 * b) / 255;
  return luminance > 0.5 ? '#1a1a2e' : '#ffffff';
}

// ─── コンポーネント ──────────────────────────────

export function LabelSettings({ projectId }: LabelSettingsProps) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [modalOpen, setModalOpen] = useState(false);
  const [editingLabel, setEditingLabel] = useState<Label | null>(null);
  const [deleteConfirm, setDeleteConfirm] = useState<Label | null>(null);
  const [formData, setFormData] = useState<LabelFormData>({ name: '', color: '#6366f1' });
  const [error, setError] = useState('');

  // --- データ取得 ---
  const { data, isLoading } = useQuery<{ results: Label[] }>({
    queryKey: ['labels', projectId],
    queryFn: async () => {
      const res = await apiClient.get<{ results: Label[] }>('/labels/', {
        params: { project: projectId },
      });
      return res.data;
    },
  });

  const labels = data?.results ?? [];

  // --- 作成 ---
  const createMutation = useMutation({
    mutationFn: (data: LabelFormData) =>
      apiClient.post('/labels/', { ...data, project: projectId }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['labels', projectId] });
      closeModal();
    },
    onError: () => setError(t('common.createFailed')),
  });

  // --- 更新 ---
  const updateMutation = useMutation({
    mutationFn: ({ id, data }: { id: number; data: LabelFormData }) =>
      apiClient.put(`/labels/${id}/`, { ...data, project: projectId }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['labels', projectId] });
      closeModal();
    },
    onError: () => setError(t('common.updateFailed')),
  });

  // --- 削除 ---
  const deleteMutation = useMutation({
    mutationFn: (id: number) => apiClient.delete(`/labels/${id}/`),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['labels', projectId] });
      setDeleteConfirm(null);
    },
    onError: () => setError(t('common.deleteFailed')),
  });

  // --- モーダル制御 ---
  const openCreateModal = useCallback(() => {
    setEditingLabel(null);
    setFormData({ name: '', color: '#6366f1' });
    setError('');
    setModalOpen(true);
  }, []);

  const openEditModal = useCallback((label: Label) => {
    setEditingLabel(label);
    setFormData({ name: label.name, color: label.color });
    setError('');
    setModalOpen(true);
  }, []);

  const closeModal = useCallback(() => {
    setModalOpen(false);
    setEditingLabel(null);
    setError('');
  }, []);

  const handleSubmit = useCallback((e: React.FormEvent) => {
    e.preventDefault();
    if (!formData.name.trim()) {
      setError('ラベル名を入力してください');
      return;
    }
    if (editingLabel) {
      updateMutation.mutate({ id: editingLabel.id, data: formData });
    } else {
      createMutation.mutate(formData);
    }
  }, [formData, editingLabel, createMutation, updateMutation]);

  const isSaving = createMutation.isPending || updateMutation.isPending;

  if (isLoading) {
    return <div className="settings-empty"><div className="settings-empty__text">{t('common.loading')}</div></div>;
  }

  return (
    <div>
      {/* ヘッダー */}
      <div className="settings-section__header">
        <h2 className="settings-section__title">
          Labels
          <span className="settings-section__count">({labels.length})</span>
        </h2>
        <button
          className="settings-section__add-btn"
          onClick={openCreateModal}
          data-testid="add-label-btn"
        >
          + 追加
        </button>
      </div>

      {/* テーブル */}
      {labels.length > 0 ? (
        <table className="settings-table">
          <thead>
            <tr>
              <th>ラベル</th>
              <th>カラー</th>
              <th style={{ width: 100, textAlign: 'right' }}>アクション</th>
            </tr>
          </thead>
          <tbody>
            {labels.map((label) => (
              <tr key={label.id} onClick={() => openEditModal(label)}>
                <td>
                  <span
                    className="settings-label-badge"
                    style={{
                      background: label.color,
                      color: getTextColor(label.color),
                    }}
                  >
                    {label.name}
                  </span>
                </td>
                <td>
                  <div className="settings-table__color-cell">
                    <span
                      className="settings-table__color-dot"
                      style={{ background: label.color }}
                    />
                    <span>{label.color}</span>
                  </div>
                </td>
                <td>
                  <div className="settings-table__actions">
                    <button
                      className="settings-table__action-btn settings-table__action-btn--danger"
                      onClick={(e) => { e.stopPropagation(); setDeleteConfirm(label); }}
                      title="削除"
                    >
                      🗑
                    </button>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : (
        <div className="settings-empty">
          <div className="settings-empty__icon">🏷️</div>
          <div className="settings-empty__text">
            ラベルがまだありません。「＋追加」ボタンで作成してください。
          </div>
        </div>
      )}

      {/* 作成・編集モーダル */}
      {modalOpen && (
        <div className="settings-modal__overlay" onClick={closeModal}>
          <div className="settings-modal" onClick={(e) => e.stopPropagation()}>
            <div className="settings-modal__header">
              <h3 className="settings-modal__title">
                {editingLabel ? 'ラベルを編集' : '新しいラベル'}
              </h3>
              <button className="settings-modal__close" onClick={closeModal}>×</button>
            </div>

            <form onSubmit={handleSubmit}>
              <div className="settings-form__group">
                <label className="settings-form__label">ラベル名</label>
                <input
                  className="settings-form__input"
                  value={formData.name}
                  onChange={(e) => setFormData(prev => ({ ...prev, name: e.target.value }))}
                  placeholder="例: bug, feature, urgent"
                  autoFocus
                  data-testid="label-name-input"
                />
              </div>

              <div className="settings-form__group">
                <label className="settings-form__label">カラー</label>
                <div className="settings-color-picker">
                  {COLOR_PRESETS.map((color) => (
                    <button
                      key={color}
                      type="button"
                      className={`settings-color-picker__swatch ${formData.color === color ? 'settings-color-picker__swatch--selected' : ''}`}
                      style={{ background: color }}
                      onClick={() => setFormData(prev => ({ ...prev, color }))}
                    />
                  ))}
                </div>
                <div className="settings-color-picker__custom">
                  <input
                    type="color"
                    className="settings-color-picker__custom-input"
                    value={formData.color}
                    onChange={(e) => setFormData(prev => ({ ...prev, color: e.target.value }))}
                  />
                  <input
                    className="settings-form__input"
                    value={formData.color}
                    onChange={(e) => setFormData(prev => ({ ...prev, color: e.target.value }))}
                    placeholder="#6366f1"
                    style={{ width: 100 }}
                  />
                  <span
                    className="settings-label-badge"
                    style={{
                      background: formData.color,
                      color: getTextColor(formData.color),
                    }}
                  >
                    {formData.name || 'プレビュー'}
                  </span>
                </div>
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
                  data-testid="label-save-btn"
                >
                  {isSaving ? '保存中...' : editingLabel ? '更新' : '作成'}
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
              <h3 className="settings-modal__title">{t('settings.deleteLabel')}</h3>
              <button className="settings-modal__close" onClick={() => setDeleteConfirm(null)}>×</button>
            </div>
            <div className="confirm-dialog__message">
              ラベル「<strong>{deleteConfirm.name}</strong>」を削除しますか？
            </div>
            <div className="confirm-dialog__warning">
              このラベルが付いているチケットからラベルが除去されます。
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
                data-testid="label-delete-confirm-btn"
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
