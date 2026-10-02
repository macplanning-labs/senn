/**
 * TeamsPage.tsx — チーム一覧（管理: 作成・編集・アーカイブ／復元）
 *
 * 名前クリック → チーム詳細で作業。個別設定（メンバー等）は /team/:slug/settings。
 */

import { useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import type { Team } from '@/shared/api/types';
import { useTeams } from '../hooks/useTeams';
import { TeamDetailModal } from './TeamDetailModal';
import { TeamsTable } from './TeamsTable';
import { activeTeams as filterActiveTeams, archivedTeams as filterArchivedTeams } from '../utils/archivedTeams';
import { discoverableTeams, joinedTeams } from '../utils/teamAccess';
import { useAuthStore } from '@/shared/stores/authStore';
import './TeamsPage.css';

export function TeamsPage() {
  const { t } = useTranslation();
  const { data: teams, isLoading } = useTeams();
  const [modalTeam, setModalTeam] = useState<Team | null | 'create'>(null);
  const [searchParams] = useSearchParams();
  const [archivedSectionOpen, setArchivedSectionOpen] = useState(
    searchParams.get('archived') === '1',
  );

  const isGuest = useAuthStore((s) => s.user?.isGuest ?? false);
  // 参加しているチームと、参加できる Public チームを分けて出す(アクセス制御の再設計 G-2)。
  // Guest には「参加できるチーム」を出さない(招待されたチームだけ)
  const activeTeamList = joinedTeams(filterActiveTeams(teams));
  const joinableTeamList = isGuest ? [] : discoverableTeams(teams);
  const archivedTeamList = filterArchivedTeams(teams);
  const hasAny = activeTeamList.length > 0 || archivedTeamList.length > 0 || joinableTeamList.length > 0;

  if (isLoading) {
    return (
      <div className="teams-page">
        <div className="teams-page__empty">{t('common.loading')}</div>
      </div>
    );
  }

  return (
    <div className="teams-page">
      <header className="teams-page__header">
        <h1 className="teams-page__title">{t('nav.teams')}</h1>
        <div className="teams-page__header-actions">
          {!isGuest && (
            <button
              type="button"
              className="teams-page__create-btn"
              onClick={() => setModalTeam('create')}
              data-testid="create-team-btn"
            >
              + {t('team.createNew')}
            </button>
          )}
        </div>
      </header>

      {!hasAny ? (
        <div className="teams-page__empty" data-testid="teams-page-empty">
          {isGuest ? (
            // Guest はチームを作れない。招待されたプロジェクトへの道を案内する
            <p>{t('teamAccess.guestNoTeams')}</p>
          ) : (
            <>
              <p>{t('team.noTeams')}</p>
              <button
                type="button"
                className="teams-page__create-btn"
                onClick={() => setModalTeam('create')}
                data-testid="teams-page-empty-create"
              >
                {t('team.createFirst')}
              </button>
            </>
          )}
        </div>
      ) : (
        <>
          <h2 className="teams-page__section-title">{t('teamAccess.yourTeamsHeading')}</h2>
          <TeamsTable teams={activeTeamList} onEdit={(team) => setModalTeam(team)} />

          {!isGuest && (
            <section className="teams-page__explore" data-testid="teams-page-explore">
              <h2 className="teams-page__section-title">{t('teamAccess.exploreHeading')}</h2>
              {joinableTeamList.length > 0 ? (
                <TeamsTable teams={joinableTeamList} />
              ) : (
                <p className="teams-page__explore-empty">{t('teamAccess.exploreEmpty')}</p>
              )}
            </section>
          )}

          {archivedTeamList.length > 0 && (
            <details className="teams-page__archived-section" open={archivedSectionOpen}>
              <summary
                className="teams-page__archived-summary"
                onClick={(e) => {
                  e.preventDefault();
                  setArchivedSectionOpen(!archivedSectionOpen);
                }}
              >
                {t('teamArchive.archivedSection', { count: archivedTeamList.length })}
              </summary>
              <div className="teams-page__archived-list">
                <TeamsTable teams={archivedTeamList} archived />
              </div>
            </details>
          )}
        </>
      )}

      {modalTeam !== null && (
        <TeamDetailModal
          team={modalTeam === 'create' ? null : modalTeam}
          onClose={() => setModalTeam(null)}
        />
      )}
    </div>
  );
}
