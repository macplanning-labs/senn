/**
 * TeamGuestsSection.tsx — Projectゲスト管理セクション(L2)
 *
 * チームに所属せず、特定Projectだけに限定参加する外部パートナー等の管理。
 * 旧 tickets_project_membership(Projectメンバーシップ)の後継。
 */

import { useState } from 'react';
import { useTeamGuests, useAddTeamGuest, useRemoveTeamGuest } from '../hooks/useTeams';
import { useQuery } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { useProjects } from '@/shared/sync/repos/projectRepo';
import type { Team, UserSummary } from '@/shared/api/types';
import { useToastStore } from '@/shared/stores/toastStore';

interface Props {
  team: Team;
}

function formatDate(dateStr: string | null): string {
  if (!dateStr) return '無期限';
  return dateStr.replace(/-/g, '/');
}

export function TeamGuestsSection({ team }: Props) {
  const { data: guests, isLoading } = useTeamGuests(team.id);
  const addGuest = useAddTeamGuest();
  const removeGuest = useRemoveTeamGuest();
  const { addToast } = useToastStore();

  // ユーザー一覧(ゲスト追加候補)
  const { data: allUsers } = useQuery({
    queryKey: ['users'],
    queryFn: async () => {
      const res = await apiClient.get<UserSummary[]>('/users/');
      return res.data;
    },
  });

  // このチームが参加するProject一覧(限定先の選択肢、端末内 DB から取得)
  const { projects: allProjects } = useProjects();
  const teamProjects = allProjects.filter((p) => (p.teams ?? []).some((t) => t.id === team.id));

  const [showAddForm, setShowAddForm] = useState(false);
  const [selectedUserId, setSelectedUserId] = useState<number | ''>('');
  const [selectedProjectId, setSelectedProjectId] = useState<number | ''>('');
  const [endDate, setEndDate] = useState('');

  const availableUsers = allUsers ?? [];

  const resetForm = () => {
    setSelectedUserId('');
    setSelectedProjectId('');
    setEndDate('');
  };

  const handleAdd = async () => {
    if (!selectedUserId || !selectedProjectId) return;
    try {
      await addGuest.mutateAsync({
        teamId: team.id,
        userId: Number(selectedUserId),
        projectId: Number(selectedProjectId),
        endDate: endDate || null,
      });
      addToast({ message: 'Projectゲストを追加しました', type: 'success' });
      setShowAddForm(false);
      resetForm();
    } catch (err: unknown) {
      const axiosErr = err as { response?: { data?: { detail?: string } } };
      addToast({ message: axiosErr.response?.data?.detail || 'ゲストの追加に失敗しました', type: 'error' });
    }
  };

  const handleRemove = async (membershipId: number, name: string, projectPrefix: string) => {
    if (!confirm(`「${name}」を ${projectPrefix} のゲストから外しますか？`)) return;
    try {
      await removeGuest.mutateAsync({ teamId: team.id, membershipId });
      addToast({ message: 'ゲストを解除しました', type: 'success' });
    } catch {
      addToast({ message: 'ゲストの解除に失敗しました', type: 'error' });
    }
  };

  return (
    <div className="team-members" style={{ marginTop: 'var(--space-6)' }}>
      <div className="team-members__header">
        <h2 className="team-members__title">
          🔑 {team.name} の Project ゲスト
        </h2>
        <button
          className="team-members__add-btn"
          onClick={() => setShowAddForm(!showAddForm)}
          data-testid="add-guest-btn"
        >
          {showAddForm ? 'キャンセル' : '+ ゲスト追加'}
        </button>
      </div>
      <p className="team-members__hint" style={{ marginBottom: '1rem', color: 'var(--color-text-secondary)', fontSize: '0.875rem' }}>
        チームに所属しない外部パートナー等を、特定のProjectだけに限定して参加させます。有効期限（任意）を過ぎるとアクセスできなくなります。
      </p>

      {showAddForm && (
        <div className="team-members__add-form">
          <select
            className="team-members__select"
            value={selectedUserId}
            onChange={(e) => setSelectedUserId(e.target.value ? Number(e.target.value) : '')}
            data-testid="guest-user-select"
          >
            <option value="">ユーザーを選択</option>
            {availableUsers.map((u) => (
              <option key={u.id} value={u.id}>
                {u.displayName || u.username}
              </option>
            ))}
          </select>
          <select
            className="team-members__select"
            value={selectedProjectId}
            onChange={(e) => setSelectedProjectId(e.target.value ? Number(e.target.value) : '')}
            data-testid="guest-project-select"
          >
            <option value="">限定するProjectを選択</option>
            {teamProjects.map((p) => (
              <option key={p.id} value={p.id}>
                {p.prefix} — {p.name}
              </option>
            ))}
          </select>
          <input
            type="date"
            className="team-members__select"
            value={endDate}
            onChange={(e) => setEndDate(e.target.value)}
            title="有効期限(任意)"
            data-testid="guest-end-date-input"
          />
          <button
            className="team-members__submit-btn"
            onClick={() => void handleAdd()}
            disabled={!selectedUserId || !selectedProjectId || addGuest.isPending}
            data-testid="submit-add-guest"
          >
            追加
          </button>
        </div>
      )}

      {isLoading ? (
        <div className="team-members__loading">読み込み中...</div>
      ) : guests && guests.length > 0 ? (
        <div className="team-members__list">
          {guests.map((guest) => (
            <div key={guest.id} className="team-member-item">
              <div className="team-member-item__avatar">
                {guest.user.displayName?.[0]?.toUpperCase() ??
                  guest.user.username[0]?.toUpperCase() ?? '?'}
              </div>
              <div className="team-member-item__info">
                <span className="team-member-item__name">
                  {guest.user.displayName || guest.user.username}
                </span>
                <span style={{ fontSize: 'var(--font-size-xs)', color: 'var(--color-text-tertiary)' }}>
                  {guest.projectPrefix} 限定 ・ 期限: {formatDate(guest.endDate)}
                  {!guest.isActive && ' (期限切れ)'}
                  {guest.isActive && guest.isInGracePeriod && ' (猶予期間中)'}
                </span>
              </div>
              <button
                className="team-member-item__remove"
                onClick={() => void handleRemove(
                  guest.id,
                  guest.user.displayName || guest.user.username,
                  guest.projectPrefix,
                )}
                title="解除"
                data-testid={`remove-guest-${guest.id}`}
              >
                ✕
              </button>
            </div>
          ))}
        </div>
      ) : (
        <div className="team-members__empty">
          Projectゲストはいません。
        </div>
      )}
    </div>
  );
}
