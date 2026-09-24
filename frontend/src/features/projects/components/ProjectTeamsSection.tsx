/**
 * ProjectTeamsSection.tsx — プロジェクト設定の「参加チーム」
 *
 * 参加チームの一覧と、追加・外す操作。誰が変更できるか・外せるか(できない理由)・
 * 追加できるチームは、サーバーが判定して返すので、画面はそれに従う。
 */

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  useProjectTeams,
  useAddProjectTeam,
  useRemoveProjectTeam,
} from '../hooks/useProjectTeams';
import './ProjectTeamsSection.css';

interface Props {
  projectId: number;
}

export function ProjectTeamsSection({ projectId }: Props) {
  const { t } = useTranslation();
  const { data, isLoading, isError } = useProjectTeams(projectId);
  const add = useAddProjectTeam(projectId);
  const remove = useRemoveProjectTeam(projectId);

  const [selectedTeam, setSelectedTeam] = useState('');
  const [confirmingId, setConfirmingId] = useState<number | null>(null);

  if (isLoading) {
    return <div className="project-teams__state">{t('common.loading')}</div>;
  }
  if (isError || !data) {
    return <div className="project-teams__state project-teams__state--error">{t('projectTeams.loadFailed')}</div>;
  }

  const { canManage, teams, addableTeams } = data;

  return (
    <section className="project-teams" data-testid="project-teams">
      <div className="settings-section__header">
        <h2 className="settings-section__title">{t('projectTeams.title')}</h2>
      </div>
      <p className="project-teams__description">{t('projectTeams.description')}</p>

      <ul className="project-teams__list">
        {teams.map((team) => (
          <li key={team.id} className="project-teams__item" data-testid={`project-team-${team.id}`}>
            <span className="project-teams__icon" style={{ backgroundColor: team.color }} aria-hidden="true">
              {team.icon}
            </span>
            <div className="project-teams__info">
              <div className="project-teams__name">
                {team.name}
                {team.archived && (
                  <span className="project-teams__badge" data-testid={`project-team-archived-${team.id}`}>
                    {t('teamArchive.badge')}
                  </span>
                )}
              </div>
              <div className="project-teams__meta">
                {team.slug} · {t('projectTeams.tickets', { count: team.ticketCount })} ·{' '}
                {t('projectTeams.cycles', { count: team.cycleCount })}
              </div>
              {canManage && team.removeBlockedReason && (
                <div className="project-teams__blocked" data-testid={`project-team-blocked-${team.id}`}>
                  {team.removeBlockedReason}
                </div>
              )}
            </div>
            {canManage && team.removable && (
              <div className="project-teams__actions">
                {confirmingId === team.id ? (
                  <>
                    <span className="project-teams__confirm">{t('projectTeams.removeConfirm')}</span>
                    <button
                      type="button"
                      className="project-teams__danger"
                      disabled={remove.isPending}
                      onClick={() => remove.mutate(team.id, { onSettled: () => setConfirmingId(null) })}
                      data-testid={`project-team-remove-confirm-${team.id}`}
                    >
                      {t('projectTeams.remove')}
                    </button>
                    <button type="button" onClick={() => setConfirmingId(null)}>
                      {t('projectTeams.cancel')}
                    </button>
                  </>
                ) : (
                  <button
                    type="button"
                    onClick={() => setConfirmingId(team.id)}
                    data-testid={`project-team-remove-${team.id}`}
                  >
                    {t('projectTeams.remove')}
                  </button>
                )}
              </div>
            )}
          </li>
        ))}
      </ul>

      {canManage ? (
        <div className="project-teams__add" data-testid="project-teams-add">
          {addableTeams.length > 0 ? (
            <>
              <select
                className="project-teams__select"
                value={selectedTeam}
                onChange={(e) => setSelectedTeam(e.target.value)}
                aria-label={t('projectTeams.addLabel')}
                data-testid="project-teams-select"
              >
                <option value="">{t('projectTeams.addPlaceholder')}</option>
                {addableTeams.map((team) => (
                  <option key={team.id} value={team.id}>
                    {team.icon} {team.name}
                  </option>
                ))}
              </select>
              <button
                type="button"
                className="project-teams__add-btn"
                disabled={!selectedTeam || add.isPending}
                onClick={() =>
                  add.mutate(Number(selectedTeam), { onSuccess: () => setSelectedTeam('') })
                }
                data-testid="project-teams-add-btn"
              >
                {t('projectTeams.add')}
              </button>
            </>
          ) : (
            <span className="project-teams__hint">{t('projectTeams.noAddable')}</span>
          )}
        </div>
      ) : (
        <p className="project-teams__hint" data-testid="project-teams-readonly-hint">
          {t('projectTeams.readOnlyHint')}
        </p>
      )}
    </section>
  );
}
