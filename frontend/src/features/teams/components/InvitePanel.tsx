/**
 * InvitePanel.tsx — 招待の作成と、未使用の招待の一覧(アクセス制御の再設計 A-4)
 *
 * - teamId あり: チームの設定を管理できる人が、Full Member(チームに所属)か Guest(チーム全体 / プロジェクト単位)を招待する
 * - teamId なし: システム管理者が、チームの無い Full Member を招待する(管理画面)
 * 権限の最終判定はサーバー。メールが送れない環境(SMTP 未設定の本番以外)では、受諾のリンクを表示して手で渡せるようにする。
 */
import { useMemo, useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import type { AxiosError } from 'axios';
import { useProjects } from '@/shared/sync/repos/projectRepo';
import { useToastStore } from '@/shared/stores/toastStore';
import {
  useCreateInvitation,
  useInvitations,
  useRevokeInvitation,
  type InviteRole,
} from '../hooks/useInvitations';
import './InvitePanel.css';

interface Props {
  /** 招待先のチーム。null はチームの無い Full Member の招待(システム管理者) */
  teamId: number | null;
}

function errorDetail(err: unknown): string | undefined {
  return (err as AxiosError<{ detail?: string }>)?.response?.data?.detail;
}

export function InvitePanel({ teamId }: Props) {
  const { t } = useTranslation();
  const { addToast } = useToastStore();
  const { data: pending = [] } = useInvitations(teamId);
  const create = useCreateInvitation(teamId);
  const revoke = useRevokeInvitation(teamId);
  const { projects } = useProjects();

  const [email, setEmail] = useState('');
  const [role, setRole] = useState<InviteRole>(teamId === null ? 'full_member' : 'guest');
  const [projectId, setProjectId] = useState('');
  const [endDate, setEndDate] = useState('');
  const [manualUrl, setManualUrl] = useState<string | null>(null);

  // Guest のプロジェクト単位の招待は、このチームが参加しているプロジェクトだけ
  const teamProjects = useMemo(
    () => (teamId === null ? [] : projects.filter((p) => (p.teams ?? []).some((tm) => tm.id === teamId))),
    [projects, teamId],
  );

  function handleSubmit(e: FormEvent) {
    e.preventDefault();
    setManualUrl(null);
    create.mutate(
      {
        email: email.trim(),
        role,
        ...(teamId !== null ? { teamId } : {}),
        ...(role === 'guest' && projectId ? { scopedProjectId: Number(projectId) } : {}),
        ...(role === 'guest' && endDate ? { endDate } : {}),
      },
      {
        onSuccess: (res) => {
          setEmail('');
          setProjectId('');
          setEndDate('');
          if (res.emailSent) {
            addToast({ message: t('invite.sent'), type: 'success' });
          } else if (res.inviteUrl) {
            setManualUrl(`${window.location.origin}${res.inviteUrl}`);
            addToast({ message: t('invite.createdNoMail'), type: 'info' });
          } else {
            addToast({ message: t('invite.createdNoMailProd'), type: 'info' });
          }
        },
        onError: (err) => addToast({ message: errorDetail(err) ?? t('invite.failed'), type: 'error' }),
      },
    );
  }

  return (
    <div className="invite-panel" data-testid="invite-panel">
      <h3 className="invite-panel__title">{t('invite.heading')}</h3>
      <p className="invite-panel__hint">
        {teamId === null ? t('invite.hintAdmin') : t('invite.hintTeam')}
      </p>
      <form className="invite-panel__form" onSubmit={handleSubmit}>
        <input
          type="email"
          required
          className="invite-panel__input"
          placeholder={t('invite.emailPlaceholder')}
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          data-testid="invite-email"
        />
        {teamId !== null && (
          <div className="invite-panel__roles" role="radiogroup" aria-label={t('invite.role')}>
            <label>
              <input
                type="radio"
                name={`invite-role-${teamId}`}
                checked={role === 'full_member'}
                onChange={() => setRole('full_member')}
                data-testid="invite-role-full"
              />
              {t('invite.roleFullMember')}
            </label>
            <label>
              <input
                type="radio"
                name={`invite-role-${teamId}`}
                checked={role === 'guest'}
                onChange={() => setRole('guest')}
                data-testid="invite-role-guest"
              />
              {t('invite.roleGuest')}
            </label>
          </div>
        )}
        {teamId !== null && role === 'guest' && (
          <div className="invite-panel__guest">
            <select
              className="invite-panel__input"
              value={projectId}
              onChange={(e) => setProjectId(e.target.value)}
              aria-label={t('invite.scope')}
              data-testid="invite-project"
            >
              <option value="">{t('invite.scopeWholeTeam')}</option>
              {teamProjects.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
            <label className="invite-panel__date">
              {t('invite.endDate')}
              <input
                type="date"
                className="invite-panel__input"
                value={endDate}
                onChange={(e) => setEndDate(e.target.value)}
                data-testid="invite-end-date"
              />
            </label>
          </div>
        )}
        <button
          type="submit"
          className="invite-panel__submit"
          disabled={create.isPending || !email.trim()}
          data-testid="invite-submit"
        >
          {t('invite.send')}
        </button>
      </form>

      {manualUrl && (
        <div className="invite-panel__manual" data-testid="invite-manual-url">
          <p>{t('invite.manualUrlHint')}</p>
          <code>{manualUrl}</code>
        </div>
      )}

      {pending.length > 0 && (
        <ul className="invite-panel__list" data-testid="invite-pending">
          {pending.map((inv) => (
            <li key={inv.id} className="invite-panel__item">
              <span className="invite-panel__email">{inv.email}</span>
              <span className="invite-panel__badge">
                {inv.role === 'guest' ? t('invite.roleGuest') : t('invite.roleFullMember')}
              </span>
              <span className="invite-panel__expires">
                {t('invite.expires', { date: new Date(inv.expiresAt).toLocaleDateString() })}
              </span>
              <button
                type="button"
                className="invite-panel__revoke"
                disabled={revoke.isPending}
                onClick={() =>
                  revoke.mutate(inv.id, {
                    onSuccess: () => addToast({ message: t('invite.revoked'), type: 'success' }),
                    onError: (err) => addToast({ message: errorDetail(err) ?? t('invite.failed'), type: 'error' }),
                  })
                }
                data-testid={`invite-revoke-${inv.id}`}
              >
                {t('invite.revoke')}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
