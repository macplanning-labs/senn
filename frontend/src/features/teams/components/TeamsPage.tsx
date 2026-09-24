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
import './TeamsPage.css';

export function TeamsPage() {
  const { t } = useTranslation();
  const { data: teams, isLoading } = useTeams();
  const [modalTeam, setModalTeam] = useState<Team | null | 'create'>(null);
  const [searchParams] = useSearchParams();
  const [archivedSectionOpen, setArchivedSectionOpen] = useState(
    searchParams.get('archived') === '1',
  );

  const activeTeamList = filterActiveTeams(teams);
  const archivedTeamList = filterArchivedTeams(teams);
  const hasAny = activeTeamList.length > 0 || archivedTeamList.length > 0;

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
          <button
            type="button"
            className="teams-page__create-btn"
            onClick={() => setModalTeam('create')}
            data-testid="create-team-btn"
          >
            + {t('team.createNew')}
          </button>
        </div>
      </header>

      {!hasAny ? (
        <div className="teams-page__empty" data-testid="teams-page-empty">
          <p>{t('team.noTeams')}</p>
          <button
            type="button"
            className="teams-page__create-btn"
            onClick={() => setModalTeam('create')}
            data-testid="teams-page-empty-create"
          >
            {t('team.createFirst')}
          </button>
        </div>
      ) : (
        <>
          <TeamsTable teams={activeTeamList} onEdit={(team) => setModalTeam(team)} />

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
