/**
 * MemberSettings.tsx — プロジェクトメンバー管理
 *
 * メンバー一覧テーブル（アクティブ/期限切れ表示）＋追加モーダル。
 * KI「マスタメンテナンス — モーダル編集方式」準拠。
 */

import { useState, useCallback } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';

// ─── 型定義 ─────────────────────────────────────
interface UserSummary {
  id: number;
  username: string;
  displayName: string;
  email: string;
}

interface Membership {
  id: number;
  user: UserSummary;
  project: number;
  startDate: string;
  endDate: string | null;
  note: string;
  addedBy: UserSummary;
  createdAt: string;
  isActive: boolean;
  isInGracePeriod: boolean;
  daysUntilExpiry: number | null;
}

interface MemberFormData {
  user: number;
  project: number;
  start_date: string;
  end_date: string;
  note: string;
}

interface MemberSettingsProps {
  projectId: number;
}

// ─── ヘルパー ────────────────────────────────────

/** 日付フォーマット: YYYY-MM-DD → YYYY/MM/DD */
function formatDate(dateStr: string | null): string {
  if (!dateStr) return '無期限';
  return dateStr.replace(/-/g, '/');
}

/** ステータスバッジを返す */
function getStatusBadge(membership: Membership): { text: string; className: string } {
  if (!membership.isActive) {
    return { text: '期限切れ', className: 'member-status--expired' };
  }
  if (membership.isInGracePeriod) {
    return { text: '猶予期間中', className: 'member-status--grace' };
  }
  if (membership.daysUntilExpiry !== null && membership.daysUntilExpiry <= 30) {
    return { text: `残${membership.daysUntilExpiry}日`, className: 'member-status--warning' };
  }
  return { text: 'アクティブ', className: 'member-status--active' };
}

// ─── コンポーネント ──────────────────────────────

export function MemberSettings({ projectId }: MemberSettingsProps) {
  const queryClient = useQueryClient();
  const [modalOpen, setModalOpen] = useState(false);
  const [deleteConfirm, setDeleteConfirm] = useState<Membership | null>(null);
  const [formData, setFormData] = useState<MemberFormData>({
    user: 0, project: projectId, start_date: new Date().toISOString().slice(0, 10),
    end_date: '', note: '',
  });
  const [error, setError] = useState('');

  // --- メンバー一覧取得 ---
  const { data, isLoading } = useQuery<{ results: Membership[] }>({
    queryKey: ['memberships', projectId],
    queryFn: async () => {
      const res = await apiClient.get<{ results: Membership[] }>('/memberships/', {
        params: { project: projectId },
      });
      return res.data;
    },
  });

  const members = data?.results ?? [];

  // --- ユーザー一覧取得（追加候補） ---
  const { data: usersData } = useQuery<UserSummary[]>({
    queryKey: ['users'],
    queryFn: async () => {
      const res = await apiClient.get<UserSummary[]>('/users/');
      return res.data;
    },
  });

  const allUsers = usersData ?? [];
  const existingUserIds = new Set(members.map(m => m.user.id));
  const availableUsers = allUsers.filter(u => !existingUserIds.has(u.id));

  // --- 追加 ---
  const addMutation = useMutation({
    mutationFn: (data: MemberFormData) =>
      apiClient.post('/memberships/', {
        ...data,
        end_date: data.end_date || null,
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['memberships', projectId] });
      closeModal();
    },
    onError: (err: unknown) => {
      const axiosErr = err as { response?: { data?: Record<string, string[]> } };
      const detail = axiosErr.response?.data;
      if (detail) {
        const messages = Object.values(detail).flat().join(', ');
        setError(messages || 'メンバーの追加に失敗しました');
      } else {
        setError('メンバーの追加に失敗しました');
      }
    },
  });

  // --- 削除 ---
  const deleteMutation = useMutation({
    mutationFn: (id: number) => apiClient.delete(`/memberships/${id}/`),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['memberships', projectId] });
      setDeleteConfirm(null);
    },
    onError: () => setError('メンバーの削除に失敗しました'),
  });

  // --- モーダル制御 ---
  const openAddModal = useCallback(() => {
    setFormData({
      user: availableUsers[0]?.id ?? 0,
      project: projectId,
      start_date: new Date().toISOString().slice(0, 10),
      end_date: '',
      note: '',
    });
    setError('');
    setModalOpen(true);
  }, [availableUsers, projectId]);

  const closeModal = useCallback(() => {
    setModalOpen(false);
    setError('');
  }, []);

  const handleSubmit = useCallback((e: React.FormEvent) => {
    e.preventDefault();
    if (!formData.user) {
      setError('ユーザーを選択してください');
      return;
    }
    addMutation.mutate(formData);
  }, [formData, addMutation]);

  if (isLoading) {
    return <div className="settings-empty"><div className="settings-empty__text">読み込み中...</div></div>;
  }

  return (
    <div>
      {/* ヘッダー */}
      <div className="settings-section__header">
        <h2 className="settings-section__title">
          Members
          <span className="settings-section__count">({members.length})</span>
        </h2>
        <button
          className="settings-section__add-btn"
          onClick={openAddModal}
          disabled={availableUsers.length === 0}
          style={availableUsers.length === 0 ? { opacity: 0.5, cursor: 'not-allowed' } : undefined}
          data-testid="add-member-btn"
        >
          + メンバー追加
        </button>
      </div>

      {/* テーブル */}
      {members.length > 0 ? (
        <table className="settings-table">
          <thead>
            <tr>
              <th>ユーザー</th>
              <th>ステータス</th>
              <th>参画日</th>
              <th>終了日</th>
              <th style={{ width: 100, textAlign: 'right' }}>アクション</th>
            </tr>
          </thead>
          <tbody>
            {members.map((m) => {
              const status = getStatusBadge(m);
              return (
                <tr key={m.id}>
                  <td>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-2)' }}>
                      <span style={{
                        width: 28, height: 28, borderRadius: '50%',
                        background: 'linear-gradient(135deg, var(--color-accent-primary), #a78bfa)',
                        color: 'white', display: 'flex', alignItems: 'center', justifyContent: 'center',
                        fontSize: '11px', fontWeight: 'bold', flexShrink: 0,
                      }}>
                        {(m.user.displayName || m.user.username)[0]?.toUpperCase()}
                      </span>
                      <div>
                        <div className="settings-table__color-name">
                          {m.user.displayName || m.user.username}
                        </div>
                        <div style={{ fontSize: 'var(--font-size-xs)', color: 'var(--color-text-tertiary)' }}>
                          {m.user.email}
                        </div>
                      </div>
                    </div>
                  </td>
                  <td>
                    <span className={`settings-label-badge ${status.className}`}>
                      {status.text}
                    </span>
                  </td>
                  <td style={{ color: 'var(--color-text-tertiary)', fontSize: 'var(--font-size-sm)' }}>
                    {formatDate(m.startDate)}
                  </td>
                  <td style={{ color: 'var(--color-text-tertiary)', fontSize: 'var(--font-size-sm)' }}>
                    {formatDate(m.endDate)}
                  </td>
                  <td>
                    <div className="settings-table__actions" style={{ opacity: 1 }}>
                      <button
                        className="settings-table__action-btn settings-table__action-btn--danger"
                        onClick={() => setDeleteConfirm(m)}
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
          <div className="settings-empty__icon">👥</div>
          <div className="settings-empty__text">
            メンバーがまだいません。「＋メンバー追加」ボタンで追加してください。
          </div>
        </div>
      )}

      {/* 追加モーダル */}
      {modalOpen && (
        <div className="settings-modal__overlay" onClick={closeModal}>
          <div className="settings-modal" onClick={(e) => e.stopPropagation()}>
            <div className="settings-modal__header">
              <h3 className="settings-modal__title">メンバーを追加</h3>
              <button className="settings-modal__close" onClick={closeModal}>×</button>
            </div>

            <form onSubmit={handleSubmit}>
              <div className="settings-form__group">
                <label className="settings-form__label">ユーザー</label>
                <select
                  className="settings-form__select"
                  value={formData.user}
                  onChange={(e) => setFormData(prev => ({ ...prev, user: Number(e.target.value) }))}
                  data-testid="member-user-select"
                >
                  <option value={0}>選択してください</option>
                  {availableUsers.map((u) => (
                    <option key={u.id} value={u.id}>
                      {u.displayName || u.username} ({u.email})
                    </option>
                  ))}
                </select>
              </div>

              <div className="settings-form__row">
                <div className="settings-form__group">
                  <label className="settings-form__label">参画開始日</label>
                  <input
                    className="settings-form__input"
                    type="date"
                    value={formData.start_date}
                    onChange={(e) => setFormData(prev => ({ ...prev, start_date: e.target.value }))}
                    required
                    data-testid="member-start-input"
                  />
                </div>
                <div className="settings-form__group">
                  <label className="settings-form__label">契約終了日（任意）</label>
                  <input
                    className="settings-form__input"
                    type="date"
                    value={formData.end_date}
                    onChange={(e) => setFormData(prev => ({ ...prev, end_date: e.target.value }))}
                    data-testid="member-end-input"
                  />
                </div>
              </div>

              <div className="settings-form__group">
                <label className="settings-form__label">備考</label>
                <textarea
                  className="settings-form__textarea"
                  value={formData.note}
                  onChange={(e) => setFormData(prev => ({ ...prev, note: e.target.value }))}
                  placeholder="役割やメモを記載"
                  data-testid="member-note-input"
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
                  disabled={addMutation.isPending}
                  data-testid="member-save-btn"
                >
                  {addMutation.isPending ? '追加中...' : '追加'}
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
              <h3 className="settings-modal__title">メンバーを削除</h3>
              <button className="settings-modal__close" onClick={() => setDeleteConfirm(null)}>×</button>
            </div>
            <div className="confirm-dialog__message">
              「<strong>{deleteConfirm.user.displayName || deleteConfirm.user.username}</strong>」をプロジェクトから削除しますか？
            </div>
            <div className="confirm-dialog__warning">
              ⚠️ このメンバーに割り当てられているチケットの担当者は変更されません。
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
                data-testid="member-delete-confirm-btn"
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
