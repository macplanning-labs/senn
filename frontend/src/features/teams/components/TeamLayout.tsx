/**
 * TeamLayout.tsx - チーム詳細画面のレイアウト
 *
 * タブベースのナビゲーション（Tickets/Cycles/Board/Gantt/Dependencies/Projects/Triage/Wiki/Settings）
 */

import { useParams, useLocation, Outlet, NavLink } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { useTeam } from '@/shared/hooks/useTeam';
import './TeamLayout.css';

export function TeamLayout() {
  const { t } = useTranslation();
  const { teamSlug } = useParams<{ teamSlug: string }>();
  const { pathname } = useLocation();
  const { currentTeam, isLoading } = useTeam();

  if (isLoading) {
    return <div className="team-layout__loading">{t('common.loading')}</div>;
  }

  if (!currentTeam) {
    return <div className="team-layout__error">{t('teamTabs.notFound')}</div>;
  }

  const archiveBadge = currentTeam.archivedAt ? (
    <span className="team-layout__status-badge">{t('teamArchive.badge')}</span>
  ) : null;

  const subPath = pathname.replace(/^\/team\/[^/]+/, '');
  const isTicketsTabActive = /^\/tickets(\/|$)/.test(subPath);
  const isBoardTabActive = /^\/board(\/|$)/.test(subPath);

  return (
    <div className="team-layout">
      <div className="team-layout__header">
        <h1 className="team-layout__title">
          {currentTeam.name}
          {archiveBadge}
        </h1>
      </div>

      <nav
        className="team-layout__tabs"
        role="navigation"
        aria-label={t('teamTabs.navLabel')}
      >
        <NavLink
          to={`/team/${teamSlug}/tickets`}
          className={() =>
            `team-layout__tab ${isTicketsTabActive ? 'team-layout__tab--active' : ''}`
          }
          data-testid="team-tab-tickets"
        >
          {t('teamTabs.tickets')}
        </NavLink>

        <NavLink
          to={`/team/${teamSlug}/cycles`}
          className={({ isActive }) =>
            `team-layout__tab ${isActive ? 'team-layout__tab--active' : ''}`
          }
          data-testid="team-tab-cycles"
        >
          {t('teamTabs.cycles')}
        </NavLink>

        <NavLink
          to={`/team/${teamSlug}/board`}
          className={() =>
            `team-layout__tab ${isBoardTabActive ? 'team-layout__tab--active' : ''}`
          }
          data-testid="team-tab-board"
        >
          {t('teamTabs.board')}
        </NavLink>

        <NavLink
          to={`/team/${teamSlug}/gantt`}
          className={({ isActive }) =>
            `team-layout__tab ${isActive ? 'team-layout__tab--active' : ''}`
          }
          data-testid="team-tab-gantt"
        >
          {t('teamTabs.gantt')}
        </NavLink>

        <NavLink
          to={`/team/${teamSlug}/reports`}
          className={({ isActive }) =>
            `team-layout__tab ${isActive ? 'team-layout__tab--active' : ''}`
          }
          data-testid="team-tab-reports"
        >
          {t('teamTabs.reports')}
        </NavLink>

        <NavLink
          to={`/team/${teamSlug}/dependencies`}
          className={({ isActive }) =>
            `team-layout__tab ${isActive ? 'team-layout__tab--active' : ''}`
          }
          data-testid="team-tab-dependencies"
        >
          {t('teamTabs.dependencies')}
        </NavLink>

        <NavLink
          to={`/team/${teamSlug}/projects`}
          className={({ isActive }) =>
            `team-layout__tab ${isActive ? 'team-layout__tab--active' : ''}`
          }
          data-testid="team-tab-projects"
        >
          {t('teamTabs.projects')}
        </NavLink>

        <NavLink
          to={`/team/${teamSlug}/triage`}
          className={({ isActive }) =>
            `team-layout__tab ${isActive ? 'team-layout__tab--active' : ''}`
          }
          data-testid="team-tab-triage"
        >
          {t('teamTabs.triage')}
        </NavLink>

        <NavLink
          to={`/team/${teamSlug}/wiki`}
          className={({ isActive }) =>
            `team-layout__tab ${isActive ? 'team-layout__tab--active' : ''}`
          }
          data-testid="team-tab-wiki"
        >
          {t('teamTabs.wiki')}
        </NavLink>

        <NavLink
          to={`/team/${teamSlug}/settings`}
          className={({ isActive }) =>
            `team-layout__tab ${isActive ? 'team-layout__tab--active' : ''}`
          }
          data-testid="team-tab-settings"
        >
          {t('teamTabs.settings')}
        </NavLink>
      </nav>

      <div className="team-layout__content">
        <Outlet />
      </div>
    </div>
  );
}
