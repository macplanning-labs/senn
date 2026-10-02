/**
 * TeamAccessSection.tsx — チームの公開区分・設定の方針・退出(アクセス制御の再設計 G-2)
 *
 * 変更できるかは、サーバーが返す `viewerCanManageOwners`(Owner の操作)で決める(フロントで権限を判定しない)。
 * Private → Public と Public → Private は、影響を説明して確認してから送る。
 * 最後のメンバー・最後の Owner などの拒否(409)は、サーバーの文言をそのまま出す(共通のエラー表示)。
 */
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { useLeaveTeam, useUpdateTeamAccess } from '@/features/teams/hooks/useTeamAccess';
import { useToastStore } from '@/shared/stores/toastStore';
import type { Team, TeamSettingsPolicy, TeamVisibility } from '@/shared/api/types';

interface Props {
  team: Team;
}

export function TeamAccessSection({ team }: Props) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { addToast } = useToastStore();
  const updateAccess = useUpdateTeamAccess();
  const leaveTeam = useLeaveTeam();
  const visibility: TeamVisibility = team.visibility ?? 'public';
  const policy: TeamSettingsPolicy = team.settingsPolicy ?? 'members';
  const canManage = !!team.viewerCanManageOwners && !team.archivedAt;
  const busy = updateAccess.isPending || leaveTeam.isPending;

  const changeVisibility = (next: TeamVisibility) => {
    if (next === visibility) return;
    const message = next === 'public' ? t('teamAccess.confirmPublic') : t('teamAccess.confirmPrivate');
    if (!window.confirm(message)) return;
    updateAccess.mutate(
      { teamId: team.id, visibility: next, confirm: next === 'public' },
      { onSuccess: () => addToast({ message: t('teamAccess.saved'), type: 'success' }) },
    );
  };

  const changePolicy = (next: TeamSettingsPolicy) => {
    if (next === policy) return;
    updateAccess.mutate(
      { teamId: team.id, settingsPolicy: next },
      { onSuccess: () => addToast({ message: t('teamAccess.saved'), type: 'success' }) },
    );
  };

  const leave = () => {
    if (!window.confirm(t('teamAccess.leaveConfirm'))) return;
    leaveTeam.mutate(team.id, { onSuccess: () => navigate('/teams') });
  };

  return (
    <div className="team-general-section__archive-section" data-testid="team-access-section">
      <h3 className="team-general-section__archive-heading">{t('teamAccess.accessHeading')}</h3>

      <fieldset className="team-access__group" disabled={!canManage || busy}>
        <legend className="team-access__legend">{t('teamAccess.visibility')}</legend>
        <label className="team-access__option">
          <input
            type="radio"
            name="team-access-visibility"
            checked={visibility === 'public'}
            onChange={() => changeVisibility('public')}
            data-testid="team-access-public"
          />
          {t('teamAccess.visibilityPublic')}
        </label>
        <label className="team-access__option">
          <input
            type="radio"
            name="team-access-visibility"
            checked={visibility === 'private'}
            onChange={() => changeVisibility('private')}
            data-testid="team-access-private"
          />
          🔒 {t('teamAccess.visibilityPrivate')}
        </label>
      </fieldset>

      <fieldset className="team-access__group" disabled={!canManage || busy}>
        <legend className="team-access__legend">{t('teamAccess.settingsPolicy')}</legend>
        <label className="team-access__option">
          <input
            type="radio"
            name="team-access-policy"
            checked={policy === 'members'}
            onChange={() => changePolicy('members')}
            data-testid="team-access-policy-members"
          />
          {t('teamAccess.policyMembers')}
        </label>
        <label className="team-access__option">
          <input
            type="radio"
            name="team-access-policy"
            checked={policy === 'owners'}
            onChange={() => changePolicy('owners')}
            data-testid="team-access-policy-owners"
          />
          {t('teamAccess.policyOwners')}
        </label>
      </fieldset>

      {team.viewerIsMember && (
        <div>
          <button
            type="button"
            className="team-general-section__confirm-btn team-general-section__confirm-btn--cancel"
            onClick={leave}
            disabled={busy}
            data-testid="team-leave-btn"
          >
            {t('teamAccess.leave')}
          </button>
        </div>
      )}
    </div>
  );
}
