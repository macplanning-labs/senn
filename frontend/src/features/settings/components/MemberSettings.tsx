/**
 * MemberSettings.tsx — プロジェクトメンバー管理
 *
 * メンバー一覧テーブル（アクティブ/期限切れ表示）＋追加モーダル。
 * ユーザー作成機能付き。
 * KI「マスタメンテナンス — モーダル編集方式」準拠。
 */

import { useState, useCallback } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { useTranslation } from 'react-i18next';

// ─── 型定義 ─────────────────────────────────────
interface UserSummary {
  id: number;
  username: string;
  displayName: string;
  email: string;
}

interface RegisterRequest {
  username: string;
  email: string;
  password: string;
  first_name?: string;
  last_name?: string;
}

interface UserResponse {
  id: number;
  username: string;
  email: string;
  firstName: string;
  lastName: string;
  displayName: string;
  isStaff: boolean;
}

interface RegisterResponse {
  user: UserResponse;
  tokens: {
    access: string;
    refresh: string;
  };
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

interface MemberUpdateData {
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
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [modalOpen, setModalOpen] = useState(false);
  const [editingMember, setEditingMember] = useState<Membership | null>(null);
  const [deleteConfirm, setDeleteConfirm] = useState<Membership | null>(null);
  const [creatingNewUser, setCreatingNewUser] = useState(false);
  const [newlyCreatedUserId, setNewlyCreatedUserId] = useState<number | null>(null);
  const [formData, setFormData] = useState<MemberFormData>({
    user: 0, project: projectId, start_date: new Date().toISOString().slice(0, 10),
    end_date: '', note: '',
  });
  const [registrationData, setRegistrationData] = useState({
    username: '',
    email: '',
    password: '',
    first_name: '',
    last_name: '',
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

  // --- 更新 ---
  const updateMutation = useMutation({
    mutationFn: ({ id, data }: { id: number; data: MemberUpdateData }) =>
      apiClient.patch(`/memberships/${id}/`, {
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
        setError(messages || 'メンバーの更新に失敗しました');
      } else {
        setError('メンバーの更新に失敗しました');
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
    onError: () => setError(t('settings.memberDeleteFailed')),
  });

  // --- ユーザー登録 ---
  const registerMutation = useMutation({
    mutationFn: (data: RegisterRequest) =>
      apiClient.post<RegisterResponse>('/auth/register/', data),
    onSuccess: (response) => {
      // 新規作成ユーザーのIDを保持して、メンバーシップ追加へ進む
      setNewlyCreatedUserId(response.data.user.id);
      setCreatingNewUser(false);
      setRegistrationData({
        username: '',
        email: '',
        password: '',
        first_name: '',
        last_name: '',
      });
    },
    onError: (err: unknown) => {
      const axiosErr = err as { response?: { data?: Record<string, unknown> } };
      const detail = axiosErr.response?.data?.detail;
      if (typeof detail === 'string') {
        setError(detail);
      } else {
        setError(t('settings.userCreationFailed'));
      }
    },
  });

  // --- モーダル制御 ---
  const openAddModal = useCallback(() => {
    setEditingMember(null);
    setCreatingNewUser(false);
    setNewlyCreatedUserId(null);
    setFormData({
      user: availableUsers[0]?.id ?? 0,
      project: projectId,
      start_date: new Date().toISOString().slice(0, 10),
      end_date: '',
      note: '',
    });
    setRegistrationData({
      username: '',
      email: '',
      password: '',
      first_name: '',
      last_name: '',
    });
    setError('');
    setModalOpen(true);
  }, [availableUsers, projectId]);

  const openEditModal = useCallback((m: Membership) => {
    setEditingMember(m);
    setFormData({
      user: m.user.id,
      project: projectId,
      start_date: m.startDate,
      end_date: m.endDate ?? '',
      note: m.note,
    });
    setError('');
    setModalOpen(true);
  }, [projectId]);

  const closeModal = useCallback(() => {
    setModalOpen(false);
    setEditingMember(null);
    setCreatingNewUser(false);
    setNewlyCreatedUserId(null);
    setError('');
    setRegistrationData({
      username: '',
      email: '',
      password: '',
      first_name: '',
      last_name: '',
    });
  }, []);

  const handleRegistrationSubmit = useCallback((e: React.FormEvent) => {
    e.preventDefault();
    if (!registrationData.username || !registrationData.email || !registrationData.password) {
      setError(t('common.error'));
      return;
    }
    if (registrationData.password.length < 8) {
      setError(t('settings.passwordTooShort'));
      return;
    }
    registerMutation.mutate({
      username: registrationData.username,
      email: registrationData.email,
      password: registrationData.password,
      first_name: registrationData.first_name || undefined,
      last_name: registrationData.last_name || undefined,
    });
  }, [registrationData, registerMutation, t]);

  const handleSubmit = useCallback((e: React.FormEvent) => {
    e.preventDefault();
    if (editingMember) {
      updateMutation.mutate({
        id: editingMember.id,
        data: {
          start_date: formData.start_date,
          end_date: formData.end_date,
          note: formData.note,
        },
      });
      return;
    }
    const userToAdd = newlyCreatedUserId || formData.user;
    if (!userToAdd) {
      setError('ユーザーを選択してください');
      return;
    }
    addMutation.mutate({ ...formData, user: userToAdd });
  }, [formData, newlyCreatedUserId, editingMember, addMutation, updateMutation]);

  const isSaving = addMutation.isPending || updateMutation.isPending || registerMutation.isPending;

  if (isLoading) {
    return <div className="settings-empty"><div className="settings-empty__text">{t('common.loading')}</div></div>;
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
                <tr key={m.id} onClick={() => openEditModal(m)}>
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
                        onClick={(e) => { e.stopPropagation(); setDeleteConfirm(m); }}
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

      {/* 追加・編集モーダル */}
      {modalOpen && (
        <div className="settings-modal__overlay" onClick={closeModal}>
          <div className="settings-modal" onClick={(e) => e.stopPropagation()}>
            <div className="settings-modal__header">
              <h3 className="settings-modal__title">
                {editingMember ? 'メンバーを編集' : t('settings.addMember')}
              </h3>
              <button className="settings-modal__close" onClick={closeModal}>×</button>
            </div>

            <form onSubmit={creatingNewUser ? handleRegistrationSubmit : handleSubmit}>
              {!editingMember && (
                <div className="settings-form__group">
                  <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 'var(--space-2)' }}>
                    <label className="settings-form__label" style={{ margin: 0 }}>
                      {creatingNewUser || newlyCreatedUserId
                        ? t('settings.createNewUser')
                        : t('settings.selectExistingUser')}
                    </label>
                    {!newlyCreatedUserId && (
                      <button
                        type="button"
                        className="settings-form__btn settings-form__btn--secondary"
                        style={{ padding: 'var(--space-1) var(--space-2)', fontSize: 'var(--font-size-sm)' }}
                        onClick={() => {
                          setCreatingNewUser(!creatingNewUser);
                          setError('');
                        }}
                        data-testid="toggle-create-user-btn"
                      >
                        {creatingNewUser ? t('settings.selectExistingUser') : t('settings.createNewUser')}
                      </button>
                    )}
                  </div>

                  {creatingNewUser ? (
                    // User creation form
                    <>
                      <div className="settings-form__group">
                        <label className="settings-form__label">{t('settings.username')}</label>
                        <input
                          className="settings-form__input"
                          type="text"
                          value={registrationData.username}
                          onChange={(e) => setRegistrationData(prev => ({ ...prev, username: e.target.value }))}
                          required
                          data-testid="registration-username-input"
                        />
                      </div>

                      <div className="settings-form__group">
                        <label className="settings-form__label">{t('settings.email')}</label>
                        <input
                          className="settings-form__input"
                          type="email"
                          value={registrationData.email}
                          onChange={(e) => setRegistrationData(prev => ({ ...prev, email: e.target.value }))}
                          required
                          data-testid="registration-email-input"
                        />
                      </div>

                      <div className="settings-form__group">
                        <label className="settings-form__label">{t('settings.password')}</label>
                        <input
                          className="settings-form__input"
                          type="password"
                          value={registrationData.password}
                          onChange={(e) => setRegistrationData(prev => ({ ...prev, password: e.target.value }))}
                          required
                          placeholder="8+ characters"
                          data-testid="registration-password-input"
                        />
                      </div>

                      <div className="settings-form__row">
                        <div className="settings-form__group">
                          <label className="settings-form__label">{t('settings.firstName')}</label>
                          <input
                            className="settings-form__input"
                            type="text"
                            value={registrationData.first_name}
                            onChange={(e) => setRegistrationData(prev => ({ ...prev, first_name: e.target.value }))}
                            data-testid="registration-first-name-input"
                          />
                        </div>
                        <div className="settings-form__group">
                          <label className="settings-form__label">{t('settings.lastName')}</label>
                          <input
                            className="settings-form__input"
                            type="text"
                            value={registrationData.last_name}
                            onChange={(e) => setRegistrationData(prev => ({ ...prev, last_name: e.target.value }))}
                            data-testid="registration-last-name-input"
                          />
                        </div>
                      </div>
                    </>
                  ) : (
                    // User selection dropdown
                    <select
                      className="settings-form__select"
                      value={formData.user}
                      onChange={(e) => setFormData(prev => ({ ...prev, user: Number(e.target.value) }))}
                      data-testid="member-user-select"
                    >
                      <option value={0}>{t('settings.selectUser')}</option>
                      {availableUsers.map((u) => (
                        <option key={u.id} value={u.id}>
                          {u.displayName || u.username} ({u.email})
                        </option>
                      ))}
                    </select>
                  )}
                </div>
              )}

              {editingMember && (
                <div className="settings-form__group">
                  <label className="settings-form__label">ユーザー</label>
                  <div className="settings-form__input" style={{ display: 'flex', alignItems: 'center', background: 'var(--color-bg-secondary)' }}>
                    {editingMember.user.displayName || editingMember.user.username} ({editingMember.user.email})
                  </div>
                </div>
              )}

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
                  placeholder={t("settings.rolePlaceholder")}
                  data-testid="member-note-input"
                />
              </div>

              {error && <div className="settings-form__error">{error}</div>}

              <div className="settings-form__actions">
                <button
                  type="button"
                  className="settings-form__btn settings-form__btn--secondary"
                  onClick={() => {
                    if (creatingNewUser && !newlyCreatedUserId) {
                      setCreatingNewUser(false);
                      setError('');
                    } else {
                      closeModal();
                    }
                  }}
                >
                  {creatingNewUser && !newlyCreatedUserId ? t('common.cancel') : t('common.cancel')}
                </button>
                <button
                  type="submit"
                  className="settings-form__btn settings-form__btn--primary"
                  disabled={isSaving}
                  data-testid="member-save-btn"
                >
                  {isSaving ? '保存中...' : creatingNewUser && !newlyCreatedUserId ? 'ユーザーを作成' : editingMember ? '更新' : '追加'}
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
              <h3 className="settings-modal__title">{t('settings.deleteMember')}</h3>
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
