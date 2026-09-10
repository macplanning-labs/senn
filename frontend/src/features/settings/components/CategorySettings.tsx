/**
 * CategorySettings.tsx — カテゴリーCRUD管理
 *
 * 2階層ツリー表示: Level 1（種別/フェーズ）→ Level 2（カテゴリー）
 * KI「マスタメンテナンス — モーダル編集方式」準拠
 */

import { useState, useCallback } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { useTranslation } from 'react-i18next';

// ─── 型定義 ─────────────────────────────────────
interface Category {
  id: number;
  name: string;
  slug: string;
  level: number;
  parent: number | null;
  sort_order: number;
  color: string;
}

interface CategoryFormData {
  name: string;
  level: number;
  parent: number | null;
  color: string;
  sort_order: number;
}

interface CategorySettingsProps {
  projectId: number;
}

// ─── カラーパレット ──────────────────────────────
const COLOR_PRESETS = [
  '#6366f1', '#3b82f6', '#0ea5e9', '#14b8a6',
  '#22c55e', '#84cc16', '#eab308', '#f97316',
  '#ef4444', '#ec4899', '#8b5cf6', '#64748b',
];

// ─── コンポーネント ──────────────────────────────

export function CategorySettings({ projectId: _projectId }: CategorySettingsProps) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [modalOpen, setModalOpen] = useState(false);
  const [editingCategory, setEditingCategory] = useState<Category | null>(null);
  const [deleteConfirm, setDeleteConfirm] = useState<Category | null>(null);
  const [formData, setFormData] = useState<CategoryFormData>({
    name: '', level: 1, parent: null, color: '#6366f1', sort_order: 0,
  });
  const [error, setError] = useState('');

  // --- データ取得 ---
  const { data, isLoading } = useQuery<Category[]>({
    queryKey: ['categories'],
    queryFn: async () => {
      const res = await apiClient.get<Category[]>('/categories/');
      return res.data;
    },
  });

  const categories = data ?? [];
  const level1Categories = categories.filter(c => c.level === 1);
  const level2Categories = categories.filter(c => c.level === 2);

  // --- 作成 ---
  const createMutation = useMutation({
    mutationFn: (data: CategoryFormData) =>
      apiClient.post('/categories/', data),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['categories'] });
      closeModal();
    },
    onError: () => setError(t('common.createFailed')),
  });

  // --- 更新 ---
  const updateMutation = useMutation({
    mutationFn: ({ id, data }: { id: number; data: CategoryFormData }) =>
      apiClient.put(`/categories/${id}/`, data),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['categories'] });
      closeModal();
    },
    onError: () => setError(t('common.updateFailed')),
  });

  // --- 削除 ---
  const deleteMutation = useMutation({
    mutationFn: (id: number) => apiClient.delete(`/categories/${id}/`),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['categories'] });
      setDeleteConfirm(null);
    },
    onError: () => setError(t('common.deleteFailed')),
  });

  // --- モーダル制御 ---
  const openCreateModal = useCallback((level: number, parentId: number | null = null) => {
    setEditingCategory(null);
    setFormData({ name: '', level, parent: parentId, color: '#6366f1', sort_order: 0 });
    setError('');
    setModalOpen(true);
  }, []);

  const openEditModal = useCallback((cat: Category) => {
    setEditingCategory(cat);
    setFormData({
      name: cat.name, level: cat.level, parent: cat.parent,
      color: cat.color, sort_order: cat.sort_order,
    });
    setError('');
    setModalOpen(true);
  }, []);

  const closeModal = useCallback(() => {
    setModalOpen(false);
    setEditingCategory(null);
    setError('');
  }, []);

  const handleSubmit = useCallback((e: React.FormEvent) => {
    e.preventDefault();
    if (!formData.name.trim()) {
      setError('カテゴリー名を入力してください');
      return;
    }
    if (editingCategory) {
      updateMutation.mutate({ id: editingCategory.id, data: formData });
    } else {
      createMutation.mutate(formData);
    }
  }, [formData, editingCategory, createMutation, updateMutation]);

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
            Categories
            <span className="settings-section__count">({categories.length})</span>
          </h2>
          <div style={{ fontSize: 'var(--font-size-sm)', color: 'var(--color-text-tertiary)', marginTop: 'var(--space-1)' }}>
            {t('settings.categoriesHelp')}
          </div>
        </div>
        <button
          className="settings-section__add-btn"
          onClick={() => openCreateModal(1)}
          data-testid="add-category-btn"
        >
          + フェーズ追加
        </button>
      </div>

      {/* ツリー表示 */}
      {level1Categories.length > 0 ? (
        <div>
          {level1Categories.map((parent) => {
            const children = level2Categories.filter(c => c.parent === parent.id);
            return (
              <div key={parent.id} className="settings-tree__group">
                {/* Level 1: フェーズ */}
                <div
                  className="settings-tree__parent"
                  onClick={() => openEditModal(parent)}
                >
                  <span
                    className="settings-table__color-dot"
                    style={{ background: parent.color }}
                  />
                  <span className="settings-tree__parent-name">{parent.name}</span>
                  <span className="settings-tree__parent-badge">
                    {children.length} カテゴリー
                  </span>
                  <div className="settings-table__actions" style={{ opacity: 1 }}>
                    <button
                      className="settings-table__action-btn"
                      onClick={(e) => { e.stopPropagation(); openCreateModal(2, parent.id); }}
                      title="子カテゴリー追加"
                    >
                      +
                    </button>
                    <button
                      className="settings-table__action-btn settings-table__action-btn--danger"
                      onClick={(e) => { e.stopPropagation(); setDeleteConfirm(parent); }}
                      title="削除"
                    >
                      🗑
                    </button>
                  </div>
                </div>

                {/* Level 2: カテゴリー */}
                {children.length > 0 && (
                  <div className="settings-tree__children">
                    {children.map((child) => (
                      <div
                        key={child.id}
                        className="settings-tree__child"
                        onClick={() => openEditModal(child)}
                      >
                        <span
                          className="settings-table__color-dot"
                          style={{ background: child.color }}
                        />
                        <span className="settings-tree__child-name">{child.name}</span>
                        <div className="settings-table__actions" style={{ opacity: 1 }}>
                          <button
                            className="settings-table__action-btn settings-table__action-btn--danger"
                            onClick={(e) => { e.stopPropagation(); setDeleteConfirm(child); }}
                            title="削除"
                          >
                            🗑
                          </button>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      ) : (
        <div className="settings-empty">
          <div className="settings-empty__icon">📂</div>
          <div className="settings-empty__text">
            カテゴリーがまだありません。「＋フェーズ追加」ボタンで作成してください。
          </div>
        </div>
      )}

      {/* 作成・編集モーダル */}
      {modalOpen && (
        <div className="settings-modal__overlay" onClick={closeModal}>
          <div className="settings-modal" onClick={(e) => e.stopPropagation()}>
            <div className="settings-modal__header">
              <h3 className="settings-modal__title">
                {editingCategory
                  ? `${editingCategory.level === 1 ? 'フェーズ' : 'カテゴリー'}を編集`
                  : formData.level === 1 ? '新しいフェーズ' : '新しいカテゴリー'
                }
              </h3>
              <button className="settings-modal__close" onClick={closeModal}>×</button>
            </div>

            <form onSubmit={handleSubmit}>
              <div className="settings-form__group">
                <label className="settings-form__label">名前</label>
                <input
                  className="settings-form__input"
                  value={formData.name}
                  onChange={(e) => setFormData(prev => ({ ...prev, name: e.target.value }))}
                  placeholder={formData.level === 1 ? '例: 要件定義, 基本設計' : '例: ログイン機能, 管理画面'}
                  autoFocus
                  data-testid="category-name-input"
                />
              </div>

              {formData.level === 2 && (
                <div className="settings-form__group">
                  <label className="settings-form__label">所属フェーズ</label>
                  <select
                    className="settings-form__select"
                    value={formData.parent ?? ''}
                    onChange={(e) => setFormData(prev => ({
                      ...prev, parent: e.target.value ? Number(e.target.value) : null,
                    }))}
                  >
                    <option value="">{t('common.selectPlaceholder')}</option>
                    {level1Categories.map((p) => (
                      <option key={p.id} value={p.id}>{p.name}</option>
                    ))}
                  </select>
                </div>
              )}

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
              </div>

              <div className="settings-form__group">
                <label className="settings-form__label">{t('settings.sortOrder')}</label>
                <input
                  className="settings-form__input"
                  type="number"
                  min={0}
                  value={formData.sort_order}
                  onChange={(e) => setFormData(prev => ({ ...prev, sort_order: Number(e.target.value) }))}
                  style={{ width: 100 }}
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
                  data-testid="category-save-btn"
                >
                  {isSaving ? '保存中...' : editingCategory ? '更新' : '作成'}
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
              <h3 className="settings-modal__title">
                {deleteConfirm.level === 1 ? 'フェーズ' : 'カテゴリー'}を削除
              </h3>
              <button className="settings-modal__close" onClick={() => setDeleteConfirm(null)}>×</button>
            </div>
            <div className="confirm-dialog__message">
              「<strong>{deleteConfirm.name}</strong>」を削除しますか？
            </div>
            {deleteConfirm.level === 1 && (
              <div className="confirm-dialog__warning">
                ⚠️ このフェーズに属する子カテゴリーも一緒に削除されます。
              </div>
            )}
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
                data-testid="category-delete-confirm-btn"
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
