/**
 * AdminAccessSection.tsx — システム管理者のアクセス管理(アクセス制御の再設計 G-4・G-6)
 *
 * - メンバーのいない Private チーム(孤立)と、Owner のいない Private チームの一覧と、Owner の指名(救済)。
 *   中身は表示しない(名前とメンバー数だけ)。Owner 不在のチームは、サーバーが返す今のメンバー(candidates)からだけ選ぶ。
 * - 移行の確認の一覧(どのチームを Private にするか等の判断材料)を JSON でダウンロードする。
 */
import { InvitePanel } from '@/features/teams/components/InvitePanel';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import type { UserSummary } from '@/shared/api/types';
import { useToastStore } from '@/shared/stores/toastStore';

interface OrphanTeam {
  id: number;
  name: string;
  memberCount: number;
  /** no_members = 孤立(誰でも指名できる)/ no_owner = Owner 不在(今のメンバーからだけ) */
  kind?: 'no_members' | 'no_owner';
  candidates?: { id: number; displayName: string }[];
}

const orphanKey = ['system-admin', 'orphan-teams'] as const;

function OrphanRow({ team, users }: { team: OrphanTeam; users: UserSummary[] }) {
  const { t } = useTranslation();
  const { addToast } = useToastStore();
  const queryClient = useQueryClient();
  const [userId, setUserId] = useState<number | ''>('');
  const rescue = useMutation({
    mutationFn: async () => {
      await apiClient.post(`/system-admin/orphan-teams/${team.id}/rescue/`, { userId });
    },
    onSuccess: () => {
      addToast({ message: t('teamAccess.rescueDone'), type: 'success' });
      void queryClient.invalidateQueries({ queryKey: orphanKey });
      void queryClient.invalidateQueries({ queryKey: ['teams'] });
    },
  });
  return (
    <li className="admin-access__row" data-testid={`orphan-team-${team.id}`}>
      <span className="admin-access__name">🔒 {team.name}</span>
      <span className="admin-access__meta">
        {t('teamAccess.memberCount', { count: team.memberCount })}
        {team.kind === 'no_owner' && ` · ${t('teamAccess.orphanKindNoOwner')}`}
      </span>
      <select
        className="admin-access__select"
        value={userId}
        onChange={(e) => setUserId(e.target.value ? Number(e.target.value) : '')}
        data-testid={`orphan-user-select-${team.id}`}
      >
        <option value="">{t('teamAccess.rescueSelect')}</option>
        {(team.candidates ?? users.map((u) => ({ id: u.id, displayName: u.displayName || u.username }))).map((u) => (
          <option key={u.id} value={u.id}>
            {u.displayName}
          </option>
        ))}
      </select>
      <button
        type="button"
        className="admin__btn admin__btn--primary"
        disabled={!userId || rescue.isPending}
        onClick={() => rescue.mutate()}
        data-testid={`orphan-rescue-${team.id}`}
      >
        {t('teamAccess.rescue')}
      </button>
    </li>
  );
}

export function AdminAccessSection() {
  const { t } = useTranslation();
  const { data: orphans, isLoading } = useQuery({
    queryKey: orphanKey,
    queryFn: async () => (await apiClient.get<OrphanTeam[]>('/system-admin/orphan-teams/')).data,
  });
  const { data: users } = useQuery({
    queryKey: ['users'],
    queryFn: async () => (await apiClient.get<UserSummary[]>('/users/')).data,
  });
  const download = useMutation({
    mutationFn: async () => {
      const res = await apiClient.get<unknown>('/system-admin/access-migration-report/');
      const blob = new Blob([JSON.stringify(res.data, null, 2)], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `senn_access_migration_report_${new Date().toISOString().slice(0, 10)}.json`;
      document.body.appendChild(a);
      a.click();
      a.remove();
      URL.revokeObjectURL(url);
    },
  });

  return (
    <>
      <section className="settings__section" data-testid="admin-orphan-teams">
        <h2 className="settings__section-title">{t('teamAccess.orphanHeading')}</h2>
        <div className="settings__card">
          <p className="admin__hint">{t('teamAccess.orphanDescription')}</p>
          {isLoading ? (
            <p className="admin__hint">{t('common.loading')}</p>
          ) : orphans && orphans.length > 0 ? (
            <ul className="admin-access__list">
              {orphans.map((team) => (
                <OrphanRow key={team.id} team={team} users={users ?? []} />
              ))}
            </ul>
          ) : (
            <p className="admin__hint" data-testid="admin-orphan-empty">{t('teamAccess.orphanEmpty')}</p>
          )}
        </div>
      </section>

      <section className="settings__section" data-testid="admin-invite">
        <h2 className="settings__section-title">{t('invite.adminHeading')}</h2>
        <div className="settings__card">
          <InvitePanel teamId={null} />
        </div>
      </section>

      <section className="settings__section" data-testid="admin-access-report">
        <h2 className="settings__section-title">{t('teamAccess.reportHeading')}</h2>
        <div className="settings__card">
          <p className="admin__hint" style={{ marginBottom: '1rem' }}>{t('teamAccess.reportDescription')}</p>
          <button
            type="button"
            className="admin__btn admin__btn--primary"
            disabled={download.isPending}
            onClick={() => download.mutate()}
            data-testid="admin-access-report-download"
          >
            {t('teamAccess.reportDownload')}
          </button>
        </div>
      </section>
    </>
  );
}
