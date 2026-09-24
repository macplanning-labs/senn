/**
 * ProjectLayout.tsx - プロジェクト詳細画面のレイアウト
 *
 * タブベースのナビゲーション（Overview/Tickets/Dependencies/Gantt/Activity/Projects）
 * チケットタブがアクティブの時のみ、リスト/ボード切替を表示
 */

import { useParams, useLocation, Outlet, NavLink } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { useProject } from '@/shared/hooks/useProject';
import { BackLink } from '@/shared/components/ui/BackLink';
import './ProjectLayout.css';

export function ProjectLayout() {
  const { t } = useTranslation();
  const { projectKey } = useParams<{ projectKey: string }>();
  const { pathname } = useLocation();
  const { currentProject, isLoading } = useProject();

  if (isLoading) {
    return <div className="project-layout__loading">{t('common.loading')}</div>;
  }

  if (!currentProject) {
    return <div className="project-layout__error">{t('projectTabs.notFound')}</div>;
  }

  // ステータスバッジ表示判定
  const statusBadge = currentProject.status ? (
    <span className="project-layout__status-badge" data-status={currentProject.status}>
      {currentProject.status}
    </span>
  ) : null;

  // チケットタブ（リスト/ボード）がアクティブか。プロジェクトキー以降のパスだけで判定する
  const subPath = pathname.replace(/^\/project\/[^/]+/, '');
  const isTicketTabActive = /^\/(tickets|board)(\/|$)/.test(subPath);

  return (
    <div className="project-layout">
      {/* プロジェクトヘッダー */}
      <div className="project-layout__header">
        <BackLink to="/projects" label={t('nav.backTo.projects')} testId="project-back-to-list" />
        <h1 className="project-layout__title">
          {currentProject.name}
          {statusBadge}
        </h1>
      </div>

      {/* メインタブバー */}
      <nav
        className="project-layout__tabs"
        role="navigation"
        aria-label={t('projectTabs.navLabel')}
      >
        <NavLink
          to={`/project/${projectKey}`}
          end
          className={({ isActive }) =>
            `project-layout__tab ${isActive ? 'project-layout__tab--active' : ''}`
          }
          data-testid="project-tab-overview"
        >
          {t('projectTabs.overview')}
        </NavLink>

        <NavLink
          to={`/project/${projectKey}/tickets`}
          className={() =>
            `project-layout__tab ${isTicketTabActive ? 'project-layout__tab--active' : ''}`
          }
          data-testid="project-tab-tickets"
        >
          {t('projectTabs.tickets')}
        </NavLink>

        <NavLink
          to={`/project/${projectKey}/dependencies`}
          className={({ isActive }) =>
            `project-layout__tab ${isActive ? 'project-layout__tab--active' : ''}`
          }
          data-testid="project-tab-dependencies"
        >
          {t('projectTabs.dependencies')}
        </NavLink>

        <NavLink
          to={`/project/${projectKey}/gantt`}
          className={({ isActive }) =>
            `project-layout__tab ${isActive ? 'project-layout__tab--active' : ''}`
          }
          data-testid="project-tab-gantt"
        >
          {t('projectTabs.gantt')}
        </NavLink>

        <NavLink
          to={`/project/${projectKey}/reports`}
          className={({ isActive }) =>
            `project-layout__tab ${isActive ? 'project-layout__tab--active' : ''}`
          }
          data-testid="project-tab-reports"
        >
          {t('projectTabs.reports')}
        </NavLink>

        <NavLink
          to={`/project/${projectKey}/activity`}
          className={({ isActive }) =>
            `project-layout__tab ${isActive ? 'project-layout__tab--active' : ''}`
          }
          data-testid="project-tab-activity"
        >
          {t('projectTabs.activity')}
        </NavLink>

        <NavLink
          to={`/project/${projectKey}/projects`}
          className={({ isActive }) =>
            `project-layout__tab ${isActive ? 'project-layout__tab--active' : ''}`
          }
          data-testid="project-tab-projects"
        >
          {t('projectTabs.projects')}
        </NavLink>

        {/* 補助リンク（右端） */}
        <div className="project-layout__aux-links">
          <NavLink
            to={`/project/${projectKey}/cycles`}
            className={({ isActive }) =>
              `project-layout__aux-link ${isActive ? 'project-layout__aux-link--active' : ''}`
            }
            data-testid="project-aux-cycles"
          >
            {t('projectTabs.cycles')}
          </NavLink>
          <NavLink
            to={`/project/${projectKey}/wiki`}
            className={({ isActive }) =>
              `project-layout__aux-link ${isActive ? 'project-layout__aux-link--active' : ''}`
            }
            data-testid="project-aux-wiki"
          >
            {t('projectTabs.wiki')}
          </NavLink>
          <NavLink
            to={`/project/${projectKey}/settings`}
            className={({ isActive }) =>
              `project-layout__aux-link ${isActive ? 'project-layout__aux-link--active' : ''}`
            }
            data-testid="project-aux-settings"
          >
            {t('projectTabs.settings')}
          </NavLink>
        </div>
      </nav>

      {/* チケットタブの時だけサブ切替を表示 */}
      {isTicketTabActive && (
        <nav
          className="project-layout__view-switch"
          role="navigation"
          aria-label={t('projectTabs.viewSwitchLabel')}
        >
          <NavLink
            to={`/project/${projectKey}/tickets`}
            className={({ isActive }) =>
              `project-layout__view-option ${isActive ? 'project-layout__view-option--active' : ''}`
            }
            data-testid="project-view-list"
          >
            {t('projectTabs.viewList')}
          </NavLink>
          <NavLink
            to={`/project/${projectKey}/board`}
            className={({ isActive }) =>
              `project-layout__view-option ${isActive ? 'project-layout__view-option--active' : ''}`
            }
            data-testid="project-view-board"
          >
            {t('projectTabs.viewBoard')}
          </NavLink>
        </nav>
      )}

      {/* コンテンツ領域 */}
      <div className="project-layout__content">
        <Outlet />
      </div>
    </div>
  );
}
