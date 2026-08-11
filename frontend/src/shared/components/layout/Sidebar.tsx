/**
 * Sidebar.tsx — サイドバーナビゲーション
 *
 * Linearライクな折りたたみ可能サイドバー。
 * プロジェクト切替セレクタ・ナビゲーション・ユーザーメニューを含む。
 */

import { useState, useRef, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { NavLink } from 'react-router-dom';
import { useUIStore } from '@/shared/stores/uiStore';
import { useAuthStore } from '@/shared/stores/authStore';
import { useProject, useProjectSwitch, getLastProjectKey } from '@/shared/hooks/useProject';
import { apiClient } from '@/shared/api/client';
import { useQueryClient } from '@tanstack/react-query';
import './Sidebar.css';

// アイコンはSVGインラインで実装（外部依存なし）
function IconDashboard() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <rect x="1" y="1" width="6" height="6" rx="1" />
      <rect x="9" y="1" width="6" height="6" rx="1" />
      <rect x="1" y="9" width="6" height="6" rx="1" />
      <rect x="9" y="9" width="6" height="6" rx="1" />
    </svg>
  );
}

function IconTicket() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="8" cy="8" r="6" />
      <path d="M8 5v3l2 2" />
    </svg>
  );
}

function IconGantt() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M3 4h6" /><path d="M3 8h10" /><path d="M3 12h4" />
    </svg>
  );
}

function IconDependency() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <rect x="1" y="2" width="5" height="4" rx="1" />
      <rect x="10" y="10" width="5" height="4" rx="1" />
      <path d="M6 4h3a2 2 0 012 2v6" />
    </svg>
  );
}

function IconBoard() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <rect x="1" y="2" width="4" height="12" rx="1" />
      <rect x="6" y="2" width="4" height="8" rx="1" />
      <rect x="11" y="2" width="4" height="10" rx="1" />
    </svg>
  );
}

function IconWiki() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M4 2h8a1 1 0 011 1v10a1 1 0 01-1 1H4a1 1 0 01-1-1V3a1 1 0 011-1z" />
      <path d="M6 5h4" /><path d="M6 8h4" /><path d="M6 11h2" />
    </svg>
  );
}

function IconNotification() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M4 6a4 4 0 018 0c0 4 2 5 2 5H2s2-1 2-5" />
      <path d="M6.5 13a1.5 1.5 0 003 0" />
    </svg>
  );
}

function IconSettings() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="8" cy="8" r="2" />
      <path d="M8 1v2M8 13v2M1 8h2M13 8h2M2.9 2.9l1.4 1.4M11.7 11.7l1.4 1.4M2.9 13.1l1.4-1.4M11.7 4.3l1.4-1.4" />
    </svg>
  );
}

function IconCycle() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M12 4a6 6 0 0 1-1.5 8.5" />
      <path d="M4 12A6 6 0 0 1 5.5 3.5" />
      <path d="M14 4l-2 0 0 2" />
      <path d="M2 12l2 0 0-2" />
    </svg>
  );
}

function IconTeam() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="6" cy="5" r="2.5" />
      <path d="M1.5 14c0-2.5 2-4 4.5-4s4.5 1.5 4.5 4" />
      <circle cx="12" cy="5" r="1.5" />
      <path d="M12 9c1.5 0 3 1 3 3" />
    </svg>
  );
}

function IconCollapse() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M11 4L7 8l4 4" />
    </svg>
  );
}

function IconChevronDown() {
  return (
    <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M3 4.5l3 3 3-3" />
    </svg>
  );
}

export function Sidebar() {
  const { t } = useTranslation();
  const { sidebarOpen, toggleSidebar, theme, toggleTheme } = useUIStore();
  const { user, logout } = useAuthStore();
  const { projectKey: routeProjectKey, currentProject, projectList, isLoading: projectsLoading } = useProject();
  const { switchProject } = useProjectSwitch();
  const queryClient = useQueryClient();

  // projectKey: useParams → URL解析 → localStorage のフォールバック
  const resolvedProjectKey = routeProjectKey
    ?? (() => {
      const match = window.location.pathname.match(/^\/p\/([^/]+)/);
      return match?.[1] ?? getLastProjectKey() ?? null;
    })();

  // プロジェクト表示用：URL由来の currentProject を優先、なければ resolvedProjectKey で projectList から探す
  const displayProject = currentProject
    ?? (resolvedProjectKey
      ? projectList.find(p => p.prefix.toLowerCase() === resolvedProjectKey.toLowerCase())
      : null);

  // プロジェクトセレクタのドロップダウン開閉
  const [projectDropdownOpen, setProjectDropdownOpen] = useState(false);
  const [showCreateForm, setShowCreateForm] = useState(false);
  const [newProjectName, setNewProjectName] = useState('');
  const [newProjectPrefix, setNewProjectPrefix] = useState('');
  const [createError, setCreateError] = useState('');
  const dropdownRef = useRef<HTMLDivElement>(null);

  // 外側クリックで閉じる
  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      if (dropdownRef.current && !dropdownRef.current.contains(e.target as Node)) {
        setProjectDropdownOpen(false);
      }
    }
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  // プロジェクトスコープのベースパス
  const projectBase = resolvedProjectKey ? `/p/${resolvedProjectKey}` : '';

  const projectNavItems = resolvedProjectKey
    ? [
        { path: `${projectBase}/tickets`, icon: IconTicket, label: t('nav.tickets') },
        { path: `${projectBase}/board`, icon: IconBoard, label: t('nav.board') },
        { path: `${projectBase}/cycles`, icon: IconCycle, label: t('nav.cycles') },
        { path: `${projectBase}/gantt`, icon: IconGantt, label: t('nav.gantt') },
        { path: `${projectBase}/dependencies`, icon: IconDependency, label: t('nav.dependencies') },
        { path: `${projectBase}/wiki`, icon: IconWiki, label: t('nav.wiki') },
      ]
    : [];

  // グローバルナビ項目
  const globalNavItems = [
    { path: '/dashboard', icon: IconDashboard, label: t('nav.dashboard') },
    { path: '/teams', icon: IconTeam, label: t('nav.teams') },
    { path: '/triage', icon: '📋', label: t('nav.triage') },
    { path: '/reports', icon: '📊', label: t('nav.reports') },
    { path: '/notifications', icon: IconNotification, label: t('nav.notifications') },
  ];

  return (
    <aside
      className={`sidebar ${sidebarOpen ? 'sidebar--open' : 'sidebar--collapsed'}`}
      data-testid="sidebar"
    >
      {/* ヘッダー：プロダクト名 + 折りたたみボタン */}
      <div className="sidebar__header">
        {sidebarOpen && (
          <span className="sidebar__logo" data-testid="sidebar-logo">
            WIP
          </span>
        )}
        <button
          className="sidebar__toggle"
          onClick={toggleSidebar}
          aria-label={sidebarOpen ? 'Collapse sidebar' : 'Expand sidebar'}
          data-testid="sidebar-toggle"
        >
          <IconCollapse />
        </button>
      </div>

      {/* プロジェクトセレクタ */}
      <div className="sidebar__project-selector" ref={dropdownRef}>
        <button
          className="sidebar__project-btn"
          onClick={() => setProjectDropdownOpen(!projectDropdownOpen)}
          data-testid="project-selector"
          title={!sidebarOpen ? (displayProject?.name ?? 'Select project') : undefined}
        >
          <span className="sidebar__project-icon">
            {displayProject?.prefix?.[0]?.toUpperCase() ?? '?'}
          </span>
          {sidebarOpen && (
            <>
              <span className="sidebar__project-name">
                {projectsLoading
                  ? '...'
                  : displayProject?.name ?? t('sidebar.selectProject')}
              </span>
              <IconChevronDown />
            </>
          )}
        </button>

        {projectDropdownOpen && (
          <div className="sidebar__project-dropdown" data-testid="project-dropdown">
            {projectList.map((p) => (
              <button
                key={p.id}
                className={`sidebar__project-option ${p.prefix.toLowerCase() === resolvedProjectKey?.toLowerCase() ? 'sidebar__project-option--active' : ''}`}
                onClick={() => {
                  switchProject(p.prefix);
                  setProjectDropdownOpen(false);
                }}
              >
                <span className="sidebar__project-option-icon">
                  {p.prefix[0]?.toUpperCase() ?? '?'}
                </span>
                <div className="sidebar__project-option-info">
                  <span className="sidebar__project-option-name">{p.name}</span>
                  <span className="sidebar__project-option-prefix">{p.prefix}</span>
                </div>
              </button>
            ))}
            {projectList.length === 0 && !projectsLoading && (
              <div className="sidebar__project-empty">{t('sidebar.noProjects')}</div>
            )}

            {/* 新規プロジェクト作成 */}
            <div className="sidebar__project-create">
              {showCreateForm ? (
                <form
                  className="sidebar__create-form"
                  onSubmit={async (e) => {
                    e.preventDefault();
                    setCreateError('');
                    if (!newProjectName.trim() || !newProjectPrefix.trim()) {
                      setCreateError(t('sidebar.createValidation'));
                      return;
                    }
                    try {
                      await apiClient.post('/projects/', {
                        name: newProjectName.trim(),
                        prefix: newProjectPrefix.trim().toUpperCase(),
                      });
                      void queryClient.invalidateQueries({ queryKey: ['projects'] });
                      switchProject(newProjectPrefix.trim().toUpperCase());
                      setShowCreateForm(false);
                      setNewProjectName('');
                      setNewProjectPrefix('');
                      setProjectDropdownOpen(false);
                    } catch {
                      setCreateError(t('sidebar.createError'));
                    }
                  }}
                >
                  <input
                    className="sidebar__create-input"
                    placeholder={t('sidebar.projectName')}
                    value={newProjectName}
                    onChange={(e) => setNewProjectName(e.target.value)}
                    autoFocus
                  />
                  <input
                    className="sidebar__create-input sidebar__create-input--prefix"
                    placeholder="KEY (例: PROJ)"
                    value={newProjectPrefix}
                    onChange={(e) => setNewProjectPrefix(e.target.value)}
                    maxLength={10}
                  />
                  {createError && (
                    <div className="sidebar__create-error">{createError}</div>
                  )}
                  <div className="sidebar__create-actions">
                    <button type="submit" className="sidebar__create-submit">{t('common.save')}</button>
                    <button
                      type="button"
                      className="sidebar__create-cancel"
                      onClick={() => { setShowCreateForm(false); setCreateError(''); }}
                    >
                      {t('common.cancel')}
                    </button>
                  </div>
                </form>
              ) : (
                <button
                  className="sidebar__project-add"
                  onClick={() => setShowCreateForm(true)}
                  data-testid="create-project-btn"
                >
                   <span className="sidebar__project-add-icon">+</span>
                   {t('sidebar.newProject')}
                </button>
              )}
            </div>
          </div>
        )}
      </div>

      {/* ナビゲーション */}
      <nav className="sidebar__nav">
        {/* プロジェクトスコープナビ */}
        {projectNavItems.length > 0 && (
          <>
            {sidebarOpen && (
              <div className="sidebar__nav-label">{t('sidebar.project')}</div>
            )}
            {projectNavItems.map(({ path, icon: Icon, label }) => (
              <NavLink
                key={path}
                to={path}
                className={({ isActive }) =>
                  `sidebar__link ${isActive ? 'sidebar__link--active' : ''}`
                }
                title={!sidebarOpen ? label : undefined}
                data-testid={`nav-${path.split('/').pop()}`}
              >
                <span className="sidebar__icon"><Icon /></span>
                {sidebarOpen && <span className="sidebar__label">{label}</span>}
              </NavLink>
            ))}
          </>
        )}

        {/* 区切り線 */}
        {projectNavItems.length > 0 && <div className="sidebar__divider" />}

        {/* グローバルナビ */}
        {sidebarOpen && (
          <div className="sidebar__nav-label">{t('sidebar.global')}</div>
        )}
        {globalNavItems.map(({ path, icon: Icon, label }) => (
          <NavLink
            key={path}
            to={path}
            className={({ isActive }) =>
              `sidebar__link ${isActive ? 'sidebar__link--active' : ''}`
            }
            title={!sidebarOpen ? label : undefined}
            data-testid={`nav-${path.slice(1)}`}
          >
            <span className="sidebar__icon"><Icon /></span>
            {sidebarOpen && <span className="sidebar__label">{label}</span>}
          </NavLink>
        ))}
      </nav>

      {/* フッター：テーマ切替 + 設定 + ユーザーメニュー */}
      <div className="sidebar__footer">
        <button
          className="sidebar__link sidebar__theme-toggle"
          onClick={toggleTheme}
          data-testid="theme-toggle"
          title={!sidebarOpen ? (theme === 'dark' ? 'Light mode' : 'Dark mode') : undefined}
        >
          <span className="sidebar__icon">
            {theme === 'dark' ? (
              <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                <circle cx="8" cy="8" r="3" />
                <path d="M8 1v2M8 13v2M1 8h2M13 8h2M3.05 3.05l1.41 1.41M11.54 11.54l1.41 1.41M3.05 12.95l1.41-1.41M11.54 4.46l1.41-1.41" />
              </svg>
            ) : (
              <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                <path d="M13.5 8.5a5.5 5.5 0 1 1-6-6 4.5 4.5 0 0 0 6 6z" />
              </svg>
            )}
          </span>
          {sidebarOpen && (
            <span className="sidebar__label">
              {theme === 'dark' ? t('sidebar.lightMode') : t('sidebar.darkMode')}
            </span>
          )}
        </button>

        <NavLink
          to={resolvedProjectKey ? `${projectBase}/settings` : '/settings'}
          className={({ isActive }) =>
            `sidebar__link ${isActive ? 'sidebar__link--active' : ''}`
          }
          data-testid="nav-settings"
          title={!sidebarOpen ? t('nav.settings') : undefined}
        >
          <span className="sidebar__icon"><IconSettings /></span>
          {sidebarOpen && <span className="sidebar__label">{t('nav.settings')}</span>}
        </NavLink>

        {user && (
          <div className="sidebar__user" data-testid="sidebar-user">
            <div className="sidebar__avatar">
              {user.firstName?.[0] ?? user.username[0]?.toUpperCase() ?? '?'}
            </div>
            {sidebarOpen && (
              <div className="sidebar__user-info">
                <span className="sidebar__user-name">
                  {user.firstName || user.username}
                </span>
                <button
                  className="sidebar__logout"
                  onClick={() => { void logout(); }}
                  data-testid="logout-button"
                >
                  {t('auth.logout')}
                </button>
              </div>
            )}
          </div>
        )}
      </div>
    </aside>
  );
}
