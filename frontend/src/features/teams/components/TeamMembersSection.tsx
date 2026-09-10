/**
 * TeamMembersSection.tsx — チームメンバー管理セクション
 *
 * 選択されたチームのメンバー一覧・追加・削除を行う。
 */

import { useState } from 'react';
import { useTeamMembers, useAddTeamMember, useRemoveTeamMember } from '../hooks/useTeams';
import { useQuery } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import type { Team, UserSummary, TeamRole } from '@/shared/api/types';
import { useToastStore } from '@/shared/stores/toastStore';
import { useTranslation } from 'react-i18next';

interface Props {
  team: Team;
}

export function TeamMembersSection({ team }: Props) {
  const { t } = useTranslation();
  const { data: members, isLoading } = useTeamMembers(team.id);
  const addMember = useAddTeamMember();
  const removeMember = useRemoveTeamMember();
  const { addToast } = useToastStore();

  // ユーザー一覧（メンバー追加用）
  const { data: allUsers } = useQuery({
    queryKey: ['users'],
    queryFn: async () => {
      const res = await apiClient.get<UserSummary[]>('/users/');
      return res.data;
    },
  });

  const [showAddForm, setShowAddForm] = useState(false);
  const [selectedUserId, setSelectedUserId] = useState<number | ''>('');
  const [selectedRole, setSelectedRole] = useState<TeamRole>('member');

  // メンバーでないユーザーのみを候補に出す
  const memberUserIds = new Set(members?.map((m) => m.user.id) ?? []);
  const availableUsers = allUsers?.filter((u) => !memberUserIds.has(u.id)) ?? [];

  const handleAdd = async () => {
    if (!selectedUserId) return;
    try {
      await addMember.mutateAsync({
        teamId: team.id,
        userId: Number(selectedUserId),
        role: selectedRole,
      });
      addToast({ message: 'メンバーを追加しました', type: 'success' });
      setShowAddForm(false);
      setSelectedUserId('');
      setSelectedRole('member');
    } catch {
      addToast({ message: 'メンバーの追加に失敗しました', type: 'error' });
    }
  };

  const handleRemove = async (userId: number, name: string) => {
    if (!confirm(t('team.removeMemberConfirm', { name }))) return;
    try {
      await removeMember.mutateAsync({ teamId: team.id, userId });
      addToast({ message: t('team.memberRemoved', { name }), type: 'success' });
    } catch {
      addToast({ message: 'メンバーの削除に失敗しました', type: 'error' });
    }
  };

  return (
    <div className="team-members">
      <div className="team-members__header">
        <h2 className="team-members__title">
          <span style={{ color: team.color }}>{team.icon}</span>
          {' '}{team.name} のメンバー
        </h2>
        <button
          className="team-members__add-btn"
          onClick={() => setShowAddForm(!showAddForm)}
          data-testid="add-member-btn"
        >
          {showAddForm ? 'キャンセル' : '+ メンバー追加'}
        </button>
      </div>
      <p className="team-members__hint" style={{ marginBottom: '1rem', color: 'var(--color-text-secondary)', fontSize: '0.875rem' }}>
        Role は管理者（admin）とメンバーのみ。所属 Team が主な権限の入口です。
      </p>

      {/* メンバー追加フォーム */}
      {showAddForm && (
        <div className="team-members__add-form">
          <select
            className="team-members__select"
            value={selectedUserId}
            onChange={(e) => setSelectedUserId(e.target.value ? Number(e.target.value) : '')}
            data-testid="member-user-select"
          >
            <option value="">{t('team.selectUser')}</option>
            {availableUsers.map((u) => (
              <option key={u.id} value={u.id}>
                {u.displayName || u.username}
              </option>
            ))}
          </select>
          <select
            className="team-members__select team-members__select--role"
            value={selectedRole}
            onChange={(e) => setSelectedRole(e.target.value as TeamRole)}
            data-testid="member-role-select"
          >
            <option value="member">メンバー</option>
            <option value="admin">管理者</option>
          </select>
          <button
            className="team-members__submit-btn"
            onClick={() => void handleAdd()}
            disabled={!selectedUserId || addMember.isPending}
            data-testid="submit-add-member"
          >
            追加
          </button>
        </div>
      )}

      {/* メンバー一覧 */}
      {isLoading ? (
        <div className="team-members__loading">{t('common.loading')}</div>
      ) : members && members.length > 0 ? (
        <div className="team-members__list">
          {members.map((membership) => (
            <div key={membership.id} className="team-member-item">
              <div className="team-member-item__avatar">
                {membership.user.displayName?.[0]?.toUpperCase() ??
                  membership.user.username[0]?.toUpperCase() ?? '?'}
              </div>
              <div className="team-member-item__info">
                <span className="team-member-item__name">
                  {membership.user.displayName || membership.user.username}
                </span>
                <span className={`team-member-item__role team-member-item__role--${membership.role}`}>
                  {membership.role === 'admin' ? '管理者' : 'メンバー'}
                </span>
              </div>
              <button
                className="team-member-item__remove"
                onClick={() => void handleRemove(
                  membership.user.id,
                  membership.user.displayName || membership.user.username,
                )}
                title="削除"
                data-testid={`remove-member-${membership.user.id}`}
              >
                ✕
              </button>
            </div>
          ))}
        </div>
      ) : (
        <div className="team-members__empty">
          メンバーがいません。「メンバー追加」から追加してください。
        </div>
      )}
    </div>
  );
}
