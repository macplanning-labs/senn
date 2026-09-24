/**
 * Sidebar.tsx — Linear 型 IA のサイドバー
 *
 * 自分のチケット / 通知 → 参加プロジェクト → 所属チーム → その他（Global）
 * UX は Linear 寄り、表示用語は SENN（Issue / Inbox 等は使わない）。
 */

import { useState, useEffect, useRef } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { Link, NavLink, useLocation, useNavigate } from 'react-router-dom';
import { useUIStore } from '@/shared/stores/uiStore';
import { useAuthStore } from '@/shared/stores/authStore';
import { useTeam, getLastTeamSlug } from '@/shared/hooks/useTeam';
import { isInternalChannel } from '@/shared/config/appChannel';
import { TeamDetailModal } from '@/features/teams/components/TeamDetailModal';
import {
  readPinnedSlugs,
  togglePinnedSlug,
  splitPinnedTeams,
} from '@/shared/utils/sidebarTeamPins';
import { TeamSidebarMoreMenu } from './TeamSidebarMoreMenu';
import { TeamSectionMenu } from './TeamSectionMenu';
import { IconPlus } from '@/shared/components/ui/icons';
import './Sidebar.css';

export function IconDashboard() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <rect x="1" y="1" width="6" height="6" rx="1" />
      <rect x="9" y="1" width="6" height="6" rx="1" />
      <rect x="1" y="9" width="6" height="6" rx="1" />
      <rect x="9" y="9" width="6" height="6" rx="1" />
    </svg>
  );
}

export function IconTicket() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="8" cy="8" r="6" />
      <path d="M8 5v3l2 2" />
    </svg>
  );
}

export function IconGantt() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M3 4h6" /><path d="M3 8h10" /><path d="M3 12h4" />
    </svg>
  );
}

export function IconDependency() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <rect x="1" y="2" width="5" height="4" rx="1" />
      <rect x="10" y="10" width="5" height="4" rx="1" />
      <path d="M6 4h3a2 2 0 012 2v6" />
    </svg>
  );
}

export function IconBoard() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <rect x="1" y="2" width="4" height="12" rx="1" />
      <rect x="6" y="2" width="4" height="8" rx="1" />
      <rect x="11" y="2" width="4" height="10" rx="1" />
    </svg>
  );
}

export function IconWiki() {
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

export function IconSettings() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="8" cy="8" r="2" />
      <path d="M8 1v2M8 13v2M1 8h2M13 8h2M2.9 2.9l1.4 1.4M11.7 11.7l1.4 1.4M2.9 13.1l1.4-1.4M11.7 4.3l1.4-1.4" />
    </svg>
  );
}

export function IconCycle() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M12 4a6 6 0 0 1-1.5 8.5" />
      <path d="M4 12A6 6 0 0 1 5.5 3.5" />
      <path d="M14 4l-2 0 0 2" />
      <path d="M2 12l2 0 0-2" />
    </svg>
  );
}

export function IconFolder() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M2 4h4l2 2h6v7a1 1 0 01-1 1H3a1 1 0 01-1-1V4z" />
    </svg>
  );
}

function IconTeam() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="5.5" cy="5" r="2" />
      <circle cx="11" cy="6.5" r="1.5" />
      <path d="M1.5 13c0-2.2 1.8-4 4-4s4 1.8 4 4" />
      <path d="M9.5 9.3c1.8.2 3 1.7 3 3.7" />
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

export function IconChevronDown() {
  return (
    <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M3 4.5l3 3 3-3" />
    </svg>
  );
}

/** 「すべて表示」トグル用（下向きシェブロン） */
function IconShowAll() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M4 5l4 3 4-3" />
      <path d="M4 9l4 3 4-3" />
    </svg>
  );
}

/** 「表示数を減らす」トグル用（上向きシェブロン） */
function IconShowFewer() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M4 7l4-3 4 3" />
      <path d="M4 11l4-3 4 3" />
    </svg>
  );
}
function IconMyIssues() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M3 3h10v10H3z" />
      <path d="M6 6h4M6 8.5h4M6 11h2" />
    </svg>
  );
}

export type NavItem = {
  path: string;
  icon: React.FC;
  label: string;
  testId?: string;
};

export function NavItemLink({
  path,
  icon: Icon,
  label,
  sidebarOpen,
  testId,
  nested,
}: NavItem & { sidebarOpen: boolean; nested?: boolean }) {
  return (
    <NavLink
      to={path}
      className={({ isActive }) =>
        `sidebar__link ${nested ? 'sidebar__link--nested' : ''} ${isActive ? 'sidebar__link--active' : ''}`
      }
      title={!sidebarOpen ? label : undefined}
      data-testid={testId}
    >
      <span className="sidebar__icon"><Icon /></span>
      {sidebarOpen && <span className="sidebar__label">{label}</span>}
    </NavLink>
  );
}

const DEFAULT_SIDEBAR_WIDTH = 272;
const MIN_SIDEBAR_WIDTH = 200;
const MAX_SIDEBAR_WIDTH = 400;

function clampSidebarWidth(width: number): number {
  return Math.max(MIN_SIDEBAR_WIDTH, Math.min(MAX_SIDEBAR_WIDTH, width));
}

/** チーム／プロジェクト共通: 行クリックでネスト開閉＋チームへ */
export function handleAxisNestToggle(args: {
  key: string;
  isExpanded: boolean;
  axisPathPrefix: string;
  ticketsPath: string;
  storageKey: string;
  pathname: string;
  teamSlug: string;
  setFocused: (key: string) => void;
  setExpanded: (key: string | null) => void;
  navigate: (to: string) => void;
}) {
  try {
    localStorage.setItem(args.storageKey, args.key);
  } catch {
    /* ignore */
  }
  args.setFocused(args.key);
  if (args.isExpanded) {
    // 別軸にいるときは閉じずにその対象のチームへ戻る
    if (!args.pathname.startsWith(args.axisPathPrefix)) {
      args.navigate(args.ticketsPath);
      return;
    }
    args.setExpanded(null);
    return;
  }
  args.setExpanded(args.key);
  args.navigate(args.ticketsPath);
}

export function Sidebar() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { sidebarOpen, toggleSidebar } = useUIStore();
  const { user, logout } = useAuthStore();
  const { teamList, activeTeams: activeTeamList, isLoading } = useTeam();
  const location = useLocation();

  // focused* = サイドバー枠に出す対象（折りたたみでは消さない）
  // expanded* = ネスト展開中の slug/prefix（null = 閉じている）
  const [focusedTeamSlug, setFocusedTeamSlug] = useState<string | null>(() => {
    try {
      return localStorage.getItem('wip-sidebar-team-expanded')
        ?? getLastTeamSlug()
        ?? null;
    } catch {
      return null;
    }
  });

  const [expandedTeam, setExpandedTeam] = useState<string | null>(() => {
    try {
      return localStorage.getItem('wip-sidebar-team-expanded')
        ?? getLastTeamSlug()
        ?? null;
    } catch {
      return null;
    }
  });

  const [sidebarWidth, setSidebarWidth] = useState<number>(() => {
    try {
      const stored = localStorage.getItem('wip-sidebar-width');
      if (stored) {
        return clampSidebarWidth(parseInt(stored, 10));
      }
    } catch {
      /* ignore */
    }
    return DEFAULT_SIDEBAR_WIDTH;
  });

  const [collapsedTeamsOpen, setCollapsedTeamsOpen] = useState(false);
  const [pinnedSlugs, setPinnedSlugs] = useState<string[]>(() => readPinnedSlugs());
  const [createTeamModalOpen, setCreateTeamModalOpen] = useState(false);

  const [userMenuOpen, setUserMenuOpen] = useState(false);
  const [userMenuPosition, setUserMenuPosition] = useState<{ left: number; bottom: number } | null>(null);
  const userMenuTriggerRef = useRef<HTMLButtonElement>(null);
  const userMenuRef = useRef<HTMLDivElement>(null);

  const showAddAttention = !isLoading && teamList.length === 0;

  useEffect(() => {
    if (focusedTeamSlug) {
      localStorage.setItem('wip-sidebar-team-expanded', focusedTeamSlug);
    }
  }, [focusedTeamSlug]);

  useEffect(() => {
    // URL でチーム画面に入ったとき、枠の対象と展開を合わせる。
    // expandedTeam を依存に入れると、折りたたみ直後に再展開されて矢印が効かなくなる。
    const match = location.pathname.match(/^\/team\/([^/]+)/);
    if (match?.[1]) {
      setFocusedTeamSlug(match[1]);
      setExpandedTeam(match[1]);
    }
  }, [location.pathname]);

  useEffect(() => {
    document.documentElement.style.setProperty('--sidebar-width', `${sidebarWidth}px`);
  }, [sidebarWidth]);

  // メニューを開いた時、アバターボタン基準で位置を計算する(document.bodyへportalするため)
  useEffect(() => {
    if (!userMenuOpen || !userMenuTriggerRef.current) return;
    const rect = userMenuTriggerRef.current.getBoundingClientRect();
    setUserMenuPosition({
      left: rect.left,
      bottom: window.innerHeight - rect.top + 8,
    });
  }, [userMenuOpen]);

  // 外側クリックで閉じる(portalでdocument.body直下にあるためuserMenuRefも確認)
  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      const target = e.target as Node;
      if (
        userMenuTriggerRef.current && !userMenuTriggerRef.current.contains(target) &&
        (!userMenuRef.current || !userMenuRef.current.contains(target))
      ) {
        setUserMenuOpen(false);
      }
    }
    if (userMenuOpen) {
      document.addEventListener('mousedown', handleClickOutside);
      return () => document.removeEventListener('mousedown', handleClickOutside);
    }
  }, [userMenuOpen]);

  const handleResizeStart = (e: React.MouseEvent<HTMLDivElement>) => {
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = sidebarWidth;

    const handleMouseMove = (moveEvent: MouseEvent) => {
      const deltaX = moveEvent.clientX - startX;
      const newWidth = clampSidebarWidth(startWidth + deltaX);
      document.documentElement.style.setProperty('--sidebar-width', `${newWidth}px`);
    };

    const handleMouseUp = () => {
      const computed = getComputedStyle(document.documentElement).getPropertyValue('--sidebar-width').trim();
      const widthValue = parseInt(computed, 10) || sidebarWidth;
      const clampedWidth = clampSidebarWidth(widthValue);
      setSidebarWidth(clampedWidth);
      localStorage.setItem('wip-sidebar-width', String(clampedWidth));
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };

    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);
  };

  const moreGlobalItems: NavItem[] = [
    { path: '/reports', icon: IconDashboard, label: t('nav.reports'), testId: 'nav-reports' },
    { path: '/wiki', icon: IconWiki, label: t('nav.wiki'), testId: 'nav-wiki' },
  ];

  return (
    <aside
      className={`sidebar ${sidebarOpen ? 'sidebar--open' : 'sidebar--collapsed'}`}
      data-testid="sidebar"
    >
      {sidebarOpen && (
        <div
          className="sidebar__resize-handle"
          onMouseDown={handleResizeStart}
          aria-label="Resize sidebar"
        />
      )}
      <div className="sidebar__header">
        {sidebarOpen && (
          <span className="sidebar__logo" data-testid="sidebar-logo">SENN</span>
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

      <nav className="sidebar__nav">
        <NavItemLink
          path="/my-issues"
          icon={IconMyIssues}
          label={t('nav.myIssues')}
          sidebarOpen={sidebarOpen}
          testId="nav-my-issues"
        />
        <NavItemLink
          path="/notifications"
          icon={IconNotification}
          label={t('nav.inbox')}
          sidebarOpen={sidebarOpen}
          testId="nav-inbox"
        />

        <NavItemLink
          path="/projects"
          icon={IconFolder}
          label={t('nav.projects')}
          sidebarOpen={sidebarOpen}
          testId="nav-projects"
        />

        <NavItemLink
          path="/tickets"
          icon={IconTicket}
          label={t('nav.tickets')}
          sidebarOpen={sidebarOpen}
          testId="nav-tickets"
        />

        <div className="sidebar__divider" />

        {sidebarOpen && (
          <div className="sidebar__section-header">
            <Link
              to="/teams"
              className="sidebar__section-label-link"
              data-testid="teams-section-link"
            >
              <span className="sidebar__nav-label">{t('sidebar.yourTeams')}</span>
            </Link>
            <div className="sidebar__section-actions">
              <button
                type="button"
                className={`sidebar__section-add-btn${showAddAttention ? ' sidebar__section-add-btn--attention' : ''}`}
                onClick={() => setCreateTeamModalOpen(true)}
                title={t('team.createNew')}
                aria-label={t('team.createNew')}
                data-testid="team-create-btn"
              >
                <IconPlus />
              </button>
              <TeamSectionMenu teams={activeTeamList} />
            </div>
          </div>
        )}
        {createTeamModalOpen && (
          <TeamDetailModal
            team={null}
            onClose={() => setCreateTeamModalOpen(false)}
            onCreated={(team) => {
              setExpandedTeam(team.slug);
            }}
          />
        )}

        <>
          {activeTeamList.length === 0 && (
          <button
            type="button"
            className="sidebar__empty-cta"
            onClick={() => setCreateTeamModalOpen(true)}
            title={!sidebarOpen ? t('team.createFirst') : undefined}
            data-testid="sidebar-create-team-cta"
          >
            <span className="sidebar__icon"><IconTeam /></span>
            {sidebarOpen && <span className="sidebar__label">{t('team.createFirst')}</span>}
          </button>
        )}

        {(() => {
          const { pinned, other } = splitPinnedTeams(activeTeamList, pinnedSlugs);

          // 閲覧中の未 PIN チームは常時表示側へ持ち上げ、ネスト／アクティブ行を維持する
          const routeTeamMatch = location.pathname.match(/^\/team\/([^/]+)/);
          const routeSlug = routeTeamMatch?.[1]?.toLowerCase() ?? null;
          const hoistSlug = routeSlug
            ?? (focusedTeamSlug ? focusedTeamSlug.toLowerCase() : null)
            ?? (expandedTeam ? expandedTeam.toLowerCase() : null);

          let visibleTeams = pinned;
          let hiddenTeams = other;
          if (hoistSlug) {
            const hoistIdx = hiddenTeams.findIndex((t) => t.slug.toLowerCase() === hoistSlug);
            if (hoistIdx >= 0) {
              const hoisted = hiddenTeams[hoistIdx]!;
              hiddenTeams = [...hiddenTeams.slice(0, hoistIdx), ...hiddenTeams.slice(hoistIdx + 1)];
              visibleTeams = [...pinned, hoisted];
            }
          }

          const renderTeamItem = (team: (typeof activeTeamList)[number]) => {
            const isExpanded = expandedTeam?.toLowerCase() === team.slug.toLowerCase();
            const teamPath = `/team/${team.slug}`;
            const teamTicketsPath = `/team/${team.slug}/tickets`;
            return (
              <div key={team.id} className="sidebar__team-block" data-testid={`team-nav-${team.slug}`}>
                <div className="sidebar__team-row">
                  <button
                    type="button"
                    className={`sidebar__team-toggle ${
                      location.pathname.startsWith(`/team/${team.slug}/`) ? 'sidebar__team-toggle--active' : ''
                    }`}
                    aria-expanded={isExpanded}
                    onClick={() => {
                      handleAxisNestToggle({
                        key: team.slug,
                        isExpanded,
                        axisPathPrefix: `/team/${team.slug}/`,
                        ticketsPath: teamPath,
                        storageKey: 'wip-last-team-slug',
                        pathname: location.pathname,
                        teamSlug: team.slug,
                        setFocused: setFocusedTeamSlug,
                        setExpanded: setExpandedTeam,
                        navigate,
                      });
                    }}
                    title={!sidebarOpen ? team.name : undefined}
                    data-testid={`team-select-${team.slug}`}
                  >
                    <span className="sidebar__project-icon">
                      {team.name[0]?.toUpperCase() ?? 'T'}
                    </span>
                    {sidebarOpen && <span className="sidebar__label">{team.name}</span>}
                  </button>
                  {sidebarOpen && (
                    <TeamSidebarMoreMenu
                      teamSlug={team.slug}
                      pinnedSlugs={pinnedSlugs}
                      onTogglePin={(slug) => setPinnedSlugs(togglePinnedSlug(slug))}
                    />
                  )}
                </div>
                {isExpanded && (
                                    <div className="sidebar__nested">
                    <NavItemLink
                      path={teamTicketsPath}
                      icon={IconTicket}
                      label={t('nav.tickets')}
                      sidebarOpen={sidebarOpen}
                      nested
                      testId={`nav-issues-${team.slug}`}
                    />
                    <NavItemLink
                      path={`/team/${team.slug}/cycles`}
                      icon={IconCycle}
                      label={t('nav.cycles')}
                      sidebarOpen={sidebarOpen}
                      nested
                      testId={`nav-cycles-${team.slug}`}
                    />
                    <NavItemLink
                      path={`/team/${team.slug}/board`}
                      icon={IconBoard}
                      label={t('nav.board')}
                      sidebarOpen={sidebarOpen}
                      nested
                      testId={`nav-board-${team.slug}`}
                    />
                    <NavItemLink
                      path={`/team/${team.slug}/gantt`}
                      icon={IconGantt}
                      label={t('nav.gantt')}
                      sidebarOpen={sidebarOpen}
                      nested
                      testId={`nav-gantt-${team.slug}`}
                    />
                    <NavItemLink
                      path={`/team/${team.slug}/reports`}
                      icon={IconDashboard}
                      label={t('nav.reports')}
                      sidebarOpen={sidebarOpen}
                      nested
                      testId={`nav-reports-${team.slug}`}
                    />
                    <NavItemLink
                      path={`/team/${team.slug}/dependencies`}
                      icon={IconDependency}
                      label={t('nav.dependencies')}
                      sidebarOpen={sidebarOpen}
                      nested
                      testId={`nav-dependencies-${team.slug}`}
                    />
                    <NavItemLink
                      path={`/team/${team.slug}/projects`}
                      icon={IconFolder}
                      label={t('nav.projects')}
                      sidebarOpen={sidebarOpen}
                      nested
                      testId={`nav-projects-${team.slug}`}
                    />
                    <NavItemLink
                      path={`/team/${team.slug}/triage`}
                      icon={IconTicket}
                      label={t('nav.triage')}
                      sidebarOpen={sidebarOpen}
                      nested
                      testId={`nav-triage-${team.slug}`}
                    />
                    <NavItemLink
                      path={`/team/${team.slug}/wiki`}
                      icon={IconWiki}
                      label={t('nav.wiki')}
                      sidebarOpen={sidebarOpen}
                      nested
                      testId={`nav-wiki-${team.slug}`}
                    />
                    <NavItemLink
                      path={`/team/${team.slug}/settings`}
                      icon={IconSettings}
                      label={t('nav.settings')}
                      sidebarOpen={sidebarOpen}
                      nested
                      testId={`nav-team-settings-${team.slug}`}
                    />
                  </div>
                )}
              </div>
            );
          };

          return (
            <>
              {visibleTeams.map((team) => renderTeamItem(team))}

              {hiddenTeams.length > 0 && (
                <>
                  {/* 展開時はリストを上に出し、「すべて表示」は常に一番下 */}
                  {collapsedTeamsOpen && (
                    <div className="sidebar__collapsed-teams">
                      {hiddenTeams.map((team) => renderTeamItem(team))}
                    </div>
                  )}

                  <div className="sidebar__team-block">
                    <div className="sidebar__team-row">
                      <button
                        type="button"
                        className="sidebar__team-toggle sidebar__team-toggle--collapsed-group"
                        aria-expanded={collapsedTeamsOpen}
                        onClick={() => setCollapsedTeamsOpen(!collapsedTeamsOpen)}
                        data-testid="team-collapsed-toggle"
                        title={!sidebarOpen ? (collapsedTeamsOpen ? t('sidebar.showFewerTeams') : t('sidebar.showAllTeams')) : undefined}
                      >
                        <span className="sidebar__project-icon sidebar__project-icon--show-all">
                          {collapsedTeamsOpen ? <IconShowFewer /> : <IconShowAll />}
                        </span>
                        {sidebarOpen && (
                          <span className="sidebar__label">{collapsedTeamsOpen ? t('sidebar.showFewerTeams') : t('sidebar.showAllTeams')}</span>
                        )}
                      </button>
                    </div>
                  </div>
                </>
              )}
            </>
          );
        })()}
        </>

        <div className="sidebar__divider" />

        {moreGlobalItems.map((item) => (
          <NavItemLink key={item.path} {...item} sidebarOpen={sidebarOpen} nested={false} />
        ))}
      </nav>

      <div className="sidebar__footer">
        <NavLink
          to="/settings"
          className={({ isActive }) =>
            `sidebar__link ${isActive ? 'sidebar__link--active' : ''}`
          }
          data-testid="nav-settings"
          title={!sidebarOpen ? t('nav.settings') : undefined}
        >
          <span className="sidebar__icon"><IconSettings /></span>
          {sidebarOpen && <span className="sidebar__label">{t('nav.settings')}</span>}
        </NavLink>

        {user?.isStaff && isInternalChannel() && (
          <NavLink
            to="/admin"
            className={({ isActive }) =>
              `sidebar__link ${isActive ? 'sidebar__link--active' : ''}`
            }
            data-testid="nav-admin"
            title={!sidebarOpen ? 'システム管理' : undefined}
          >
            <span className="sidebar__icon">🏢</span>
            {sidebarOpen && <span className="sidebar__label">システム管理</span>}
          </NavLink>
        )}

        {user && (
          <div className="sidebar__user" data-testid="sidebar-user">
            <button
              ref={userMenuTriggerRef}
              type="button"
              className="sidebar__avatar sidebar__avatar--button"
              onClick={() => setUserMenuOpen((v) => !v)}
              title="個人設定"
              aria-haspopup="menu"
              aria-expanded={userMenuOpen}
              data-testid="sidebar-user-menu-trigger"
            >
              {user.firstName?.[0] ?? user.username[0]?.toUpperCase() ?? '?'}
            </button>
            {sidebarOpen && (
              <div className="sidebar__user-info">
                <button
                  type="button"
                  className="sidebar__user-name sidebar__user-name--button"
                  onClick={() => setUserMenuOpen((v) => !v)}
                >
                  {user.firstName || user.username}
                </button>
              </div>
            )}

            {userMenuOpen && userMenuPosition && createPortal(
              <div
                ref={userMenuRef}
                className="sidebar__user-menu"
                role="menu"
                style={{ left: userMenuPosition.left, bottom: userMenuPosition.bottom }}
                data-testid="sidebar-user-menu"
              >
                <NavLink
                  to="/settings"
                  role="menuitem"
                  className="sidebar__user-menu-item"
                  onClick={() => setUserMenuOpen(false)}
                  data-testid="nav-my-settings"
                >
                  <IconSettings /> {t('nav.settings')}
                </NavLink>
                <button
                  type="button"
                  role="menuitem"
                  className="sidebar__user-menu-item"
                  onClick={() => { setUserMenuOpen(false); void logout(); }}
                  data-testid="logout-button"
                >
                  🚪 {t('auth.logout')}
                </button>
              </div>,
              document.body
            )}
          </div>
        )}
      </div>
    </aside>
  );
}
