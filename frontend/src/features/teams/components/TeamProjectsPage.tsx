/**
 * TeamProjectsPage.tsx — チーム配下の Projects 一覧
 */

import { Link, useParams } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { useProject } from '@/shared/hooks/useProject';
import { useTeam } from '@/shared/hooks/useTeam';
import './TeamProjectsPage.css';

export function TeamProjectsPage() {
  const { t } = useTranslation();
  const { teamSlug } = useParams<{ teamSlug: string }>();
  const { currentTeam } = useTeam();
  const { projectList, isLoading } = useProject();

  const teamId = currentTeam?.id;
  const projects = projectList.filter((p) => {
    if (!teamId) return false;
    return (p.teams ?? []).some((t) => t.id === teamId);
  });

  return (
    <div className="team-projects" data-testid="team-projects-page">
      <header className="team-projects__header">
        <h1 className="team-projects__title">{t('nav.projects')}</h1>
        <p className="team-projects__subtitle">
          {currentTeam?.name ?? teamSlug}
        </p>
      </header>

      {isLoading ? (
        <div className="team-projects__empty">{t('common.loading')}</div>
      ) : projects.length === 0 ? (
        <div className="team-projects__empty" data-testid="team-projects-empty">
          <p>{t('teamProjects.empty')}</p>
          <p className="team-projects__hint">{t('teamProjects.emptyHint')}</p>
          <Link to="/teams" className="team-projects__link">
            {t('nav.teams')}
          </Link>
        </div>
      ) : (
        <ul className="team-projects__list">
          {projects.map((p) => (
            <li key={p.id}>
              <Link
                to={`/project/${p.prefix}/tickets`}
                className="team-projects__row"
                data-testid={`team-project-${p.prefix}`}
              >
                <span className="team-projects__icon">
                  {p.prefix[0]?.toUpperCase() ?? '?'}
                </span>
                <span className="team-projects__name">{p.name}</span>
                <span className="team-projects__prefix">{p.prefix}</span>
              </Link>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
