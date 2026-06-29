/**
 * Sidebar.tsx — サイドバーナビゲーション
 *
 * Linearライクな折りたたみ可能サイドバー。
 * プロジェクト切替・ナビゲーション・ユーザーメニューを含む。
 */

import { useTranslation } from 'react-i18next';
import { NavLink } from 'react-router-dom';
import { useUIStore } from '@/shared/stores/uiStore';
import { useAuthStore } from '@/shared/stores/authStore';
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

function IconCollapse() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M11 4L7 8l4 4" />
    </svg>
  );
}

const navItems = [
  { path: '/dashboard', icon: IconDashboard, labelKey: 'nav.dashboard' },
  { path: '/tickets', icon: IconTicket, labelKey: 'nav.tickets' },
  { path: '/gantt', icon: IconGantt, labelKey: 'nav.gantt' },
  { path: '/wiki', icon: IconWiki, labelKey: 'nav.wiki' },
  { path: '/notifications', icon: IconNotification, labelKey: 'nav.notifications' },
] as const;

export function Sidebar() {
  const { t } = useTranslation();
  const { sidebarOpen, toggleSidebar } = useUIStore();
  const { user, logout } = useAuthStore();

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

      {/* ナビゲーション */}
      <nav className="sidebar__nav">
        {navItems.map(({ path, icon: Icon, labelKey }) => (
          <NavLink
            key={path}
            to={path}
            className={({ isActive }) =>
              `sidebar__link ${isActive ? 'sidebar__link--active' : ''}`
            }
            title={!sidebarOpen ? t(labelKey) : undefined}
            data-testid={`nav-${path.slice(1)}`}
          >
            <span className="sidebar__icon"><Icon /></span>
            {sidebarOpen && <span className="sidebar__label">{t(labelKey)}</span>}
          </NavLink>
        ))}
      </nav>

      {/* フッター：ユーザーメニュー + 設定 */}
      <div className="sidebar__footer">
        <NavLink
          to="/settings"
          className={({ isActive }) =>
            `sidebar__link ${isActive ? 'sidebar__link--active' : ''}`
          }
          data-testid="nav-settings"
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
