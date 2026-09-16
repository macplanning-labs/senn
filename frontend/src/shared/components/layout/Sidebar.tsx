/**
 * Sidebar.tsx — Linear 型 IA のサイドバー
 *
 * 自分のチケット / 通知 → 所属チーム → 参加プロジェクト → その他（Global）
 * UX は Linear 寄り、表示用語は SENN（Issue / Inbox 等は使わない）。
 */

import { useState, useEffect, useRef } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { NavLink, useLocation, useNavigate } from 'react-router-dom';
import { useUIStore } from '@/shared/stores/uiStore';
import { useAuthStore } from '@/shared/stores/authStore';
import { getLastProjectKey, useProject } from '@/shared/hooks/useProject';
import { useTeam, getLastTeamSlug } from '@/shared/hooks/useTeam';
import './Sidebar.css';

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

function IconFolder() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M2 4h4l2 2h6v7a1 1 0 01-1 1H3a1 1 0 01-1-1V4z" />
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

function IconMyIssues() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M3 3h10v10H3z" />
      <path d="M6 6h4M6 8.5h4M6 11h2" />
    </svg>
  );
}

type NavItem = {
  path: string;
  icon: React.FC;
  label: string;
  testId?: string;
};

function NavItemLink({
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

/** チーム／プロジェクト共通: 行クリックでネスト開閉＋チケットへ */
function handleAxisNestToggle(args: {
  key: string;
  isExpanded: boolean;
  axisPathPrefix: string;
  ticketsPath: string;
  storageKey: string;
  pathname: string;
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
    // 別軸にいるときは閉じずにその対象のチケットへ戻る
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
  const { sidebarOpen, toggleSidebar, theme, toggleTheme } = useUIStore();
  const { user, logout } = useAuthStore();
  const { teamList } = useTeam();
  const { projectList } = useProject();
  const location = useLocation();

  const resolvedProjectKey = (() => {
    const match = location.pathname.match(/^\/project\/([^/]+)/);
    return match?.[1] ?? getLastProjectKey() ?? null;
  })();

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

  const [focusedProjectPrefix, setFocusedProjectPrefix] = useState<string | null>(() => {
    try {
      return localStorage.getItem('wip-sidebar-project-expanded')
        ?? resolvedProjectKey
        ?? null;
    } catch {
      return null;
    }
  });

  const [expandedProject, setExpandedProject] = useState<string | null>(() => {
    try {
      return localStorage.getItem('wip-sidebar-project-expanded')
        ?? resolvedProjectKey
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
  const [collapsedProjectsOpen, setCollapsedProjectsOpen] = useState(false);
  const [userMenuOpen, setUserMenuOpen] = useState(false);
  const [userMenuPosition, setUserMenuPosition] = useState<{ left: number; bottom: number } | null>(null);
  const userMenuTriggerRef = useRef<HTMLButtonElement>(null);
  const userMenuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (focusedTeamSlug) {
      localStorage.setItem('wip-sidebar-team-expanded', focusedTeamSlug);
    }
  }, [focusedTeamSlug]);

  useEffect(() => {
    if (focusedProjectPrefix) {
      localStorage.setItem('wip-sidebar-project-expanded', focusedProjectPrefix);
    }
  }, [focusedProjectPrefix]);


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
    // URL でプロジェクト画面に入ったとき、枠の対象と展開を合わせる。
    const match = location.pathname.match(/^\/project\/([^/]+)/);
    if (match?.[1]) {
      setFocusedProjectPrefix(match[1]);
      setExpandedProject(match[1]);
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
    { path: '/triage', icon: IconTicket, label: t('nav.triage'), testId: 'nav-triage' },
    { path: '/reports', icon: IconDashboard, label: t('nav.reports'), testId: 'nav-reports' },
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

        {sidebarOpen && (
          <div className="sidebar__nav-label">{t('sidebar.yourTeams')}</div>
        )}

        {(() => {
          const TEAM_COLLAPSE_THRESHOLD = 6;
          const shouldCollapse = teamList.length > TEAM_COLLAPSE_THRESHOLD;

          if (!shouldCollapse) {
            // 6件以下: 現行どおり全チーム表示
            return teamList.map((team) => {
              const isExpanded = expandedTeam?.toLowerCase() === team.slug.toLowerCase();
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
                          ticketsPath: teamTicketsPath,
                          storageKey: 'wip-last-team-slug',
                          pathname: location.pathname,
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
                      {sidebarOpen && (
                        <span
                          className="sidebar__team-chevron"
                          style={{ marginLeft: 'auto', transform: isExpanded ? 'rotate(0deg)' : 'rotate(-90deg)' }}
                          data-testid={`team-chevron-${team.slug}`}
                        >
                          <IconChevronDown />
                        </span>
                      )}
                    </button>
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
                        path={`/team/${team.slug}/projects`}
                        icon={IconFolder}
                        label={t('nav.projects')}
                        sidebarOpen={sidebarOpen}
                        nested
                        testId={`nav-projects-${team.slug}`}
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
            });
          }

          // 6件超: 「枠に出しているチーム」と「他のチーム」に分ける。
          // 枠の対象は focusedTeamSlug（折りたたみでは消さない）。
          const urlTeamSlug = location.pathname.match(/^\/team\/([^/]+)/)?.[1];
          const slotTeamSlug = focusedTeamSlug ?? urlTeamSlug ?? getLastTeamSlug();
          const visibleTeam = teamList.find(
            (t) => t.slug.toLowerCase() === slotTeamSlug?.toLowerCase()
          ) ?? teamList[0];
          const hiddenTeams = teamList.filter((t) => t.id !== visibleTeam?.id);

          const renderTeamItem = (team: typeof visibleTeam) => {
            if (!team) return null;
            const isExpanded = expandedTeam?.toLowerCase() === team.slug.toLowerCase();
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
                        ticketsPath: teamTicketsPath,
                        storageKey: 'wip-last-team-slug',
                        pathname: location.pathname,
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
                    {sidebarOpen && (
                      <span
                        className="sidebar__team-chevron"
                        style={{ marginLeft: 'auto', transform: isExpanded ? 'rotate(0deg)' : 'rotate(-90deg)' }}
                        data-testid={`team-chevron-${team.slug}`}
                      >
                        <IconChevronDown />
                      </span>
                    )}
                  </button>
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
                      path={`/team/${team.slug}/projects`}
                      icon={IconFolder}
                      label={t('nav.projects')}
                      sidebarOpen={sidebarOpen}
                      nested
                      testId={`nav-projects-${team.slug}`}
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
              {/* 表示中のチーム */}
              {renderTeamItem(visibleTeam)}

              {/* 「他のチーム」折りたたみボタン */}
              {hiddenTeams.length > 0 && (
                <>
                  <div className="sidebar__team-block">
                    <div className="sidebar__team-row">
                      <button
                        type="button"
                        className="sidebar__team-toggle sidebar__team-toggle--collapsed-group"
                        aria-expanded={collapsedTeamsOpen}
                        onClick={() => setCollapsedTeamsOpen(!collapsedTeamsOpen)}
                        data-testid="team-collapsed-toggle"
                      >
                        <span className="sidebar__project-icon">···</span>
                        {sidebarOpen && (
                          <span className="sidebar__label">
                            {t('sidebar.otherTeams', { count: hiddenTeams.length })}
                          </span>
                        )}
                        {sidebarOpen && (
                          <span
                            className="sidebar__team-chevron"
                            style={{ marginLeft: 'auto', transform: collapsedTeamsOpen ? 'rotate(0deg)' : 'rotate(-90deg)' }}
                            data-testid="team-collapsed-chevron"
                          >
                            <IconChevronDown />
                          </span>
                        )}
                      </button>
                    </div>
                  </div>

                  {/* 隠れたチーム一覧 */}
                  {collapsedTeamsOpen && (
                    <div className="sidebar__collapsed-teams">
                      {hiddenTeams.map((team) => {
                        const teamTicketsPath = `/team/${team.slug}/tickets`;
                        return (
                          <button
                            key={team.id}
                            type="button"
                            className={`sidebar__team-collapsed-item ${
                              location.pathname.startsWith(`/team/${team.slug}/`) ? 'sidebar__team-collapsed-item--active' : ''
                            }`}
                            onClick={() => {
                              setFocusedTeamSlug(team.slug);
                              setExpandedTeam(team.slug);
                              setCollapsedTeamsOpen(false);
                              try {
                                localStorage.setItem('wip-last-team-slug', team.slug);
                              } catch {
                                /* ignore */
                              }
                              navigate(teamTicketsPath);
                            }}
                            title={!sidebarOpen ? team.name : undefined}
                            data-testid={`team-collapsed-select-${team.slug}`}
                          >
                            <span className="sidebar__project-icon">
                              {team.name[0]?.toUpperCase() ?? 'T'}
                            </span>
                            {sidebarOpen && <span className="sidebar__label">{team.name}</span>}
                          </button>
                        );
                      })}
                    </div>
                  )}
                </>
              )}
            </>
          );
        })()}

        {projectList.length > 0 && (
          <>
            <div className="sidebar__divider" />

            {sidebarOpen && (
              <div className="sidebar__nav-label">{t('sidebar.projects')}</div>
            )}

            {(() => {
              const PROJECT_COLLAPSE_THRESHOLD = 6;
              const shouldCollapse = projectList.length > PROJECT_COLLAPSE_THRESHOLD;

              if (!shouldCollapse) {
                // 6件以下: 全プロジェクト表示
                return projectList.map((project) => {
                  const isExpanded = expandedProject?.toLowerCase() === project.prefix.toLowerCase();
                  return (
                    <div key={project.id} className="sidebar__team-block" data-testid={`project-nav-${project.prefix}`}>
                      <div className="sidebar__team-row">
                        <button
                          type="button"
                          className={`sidebar__team-toggle ${
                            location.pathname.startsWith(`/project/${project.prefix}/`) ? 'sidebar__team-toggle--active' : ''
                          }`}
                          aria-expanded={isExpanded}
                          onClick={() => {
                            const projectTicketsPath = `/project/${project.prefix}/tickets`;
                            handleAxisNestToggle({
                              key: project.prefix,
                              isExpanded,
                              axisPathPrefix: `/project/${project.prefix}/`,
                              ticketsPath: projectTicketsPath,
                              storageKey: 'wip-last-project-key',
                              pathname: location.pathname,
                              setFocused: setFocusedProjectPrefix,
                              setExpanded: setExpandedProject,
                              navigate,
                            });
                          }}
                          title={!sidebarOpen ? project.name : undefined}
                          data-testid={`project-select-${project.prefix}`}
                        >
                          <span className="sidebar__project-icon">
                            {project.prefix[0]?.toUpperCase() ?? 'P'}
                          </span>
                          {sidebarOpen && <span className="sidebar__label">{project.name}</span>}
                          {sidebarOpen && (
                            <span
                              className="sidebar__team-chevron"
                              style={{ marginLeft: 'auto', transform: isExpanded ? 'rotate(0deg)' : 'rotate(-90deg)' }}
                              data-testid={`project-chevron-${project.prefix}`}
                            >
                              <IconChevronDown />
                            </span>
                          )}
                        </button>
                      </div>
                      {isExpanded && (
                                                <div className="sidebar__nested">
                          <NavItemLink
                            path={`/project/${project.prefix}/tickets`}
                            icon={IconTicket}
                            label={t('nav.tickets')}
                            sidebarOpen={sidebarOpen}
                            nested
                            testId={`nav-issues-${project.prefix}`}
                          />
                          <NavItemLink
                            path={`/project/${project.prefix}/gantt`}
                            icon={IconGantt}
                            label={t('nav.gantt')}
                            sidebarOpen={sidebarOpen}
                            nested
                            testId={`nav-gantt-${project.prefix}`}
                          />
                          <NavItemLink
                            path={`/project/${project.prefix}/dependencies`}
                            icon={IconDependency}
                            label={t('nav.dependencies')}
                            sidebarOpen={sidebarOpen}
                            nested
                            testId={`nav-dependencies-${project.prefix}`}
                          />
                          <NavItemLink
                            path={`/project/${project.prefix}/wiki`}
                            icon={IconWiki}
                            label={t('nav.wiki')}
                            sidebarOpen={sidebarOpen}
                            nested
                            testId={`nav-wiki-${project.prefix}`}
                          />
                          <NavItemLink
                            path={`/project/${project.prefix}/settings`}
                            icon={IconSettings}
                            label={t('nav.settings')}
                            sidebarOpen={sidebarOpen}
                            nested
                            testId={`nav-settings-${project.prefix}`}
                          />
                        </div>
                      )}
                    </div>
                  );
                });
              }

              // 6件超: 「枠に出しているプロジェクト」と「他のプロジェクト」に分ける。
              const urlProjectPrefix = location.pathname.match(/^\/project\/([^/]+)/)?.[1];
              const slotProjectPrefix = focusedProjectPrefix ?? urlProjectPrefix ?? getLastProjectKey();
              const visibleProject = projectList.find(
                (p) => p.prefix.toLowerCase() === slotProjectPrefix?.toLowerCase()
              ) ?? projectList[0];
              const hiddenProjects = projectList.filter((p) => p.id !== visibleProject?.id);

              const renderProjectItem = (project: typeof visibleProject) => {
                if (!project) return null;
                const isExpanded = expandedProject?.toLowerCase() === project.prefix.toLowerCase();
                return (
                  <div key={project.id} className="sidebar__team-block" data-testid={`project-nav-${project.prefix}`}>
                    <div className="sidebar__team-row">
                      <button
                        type="button"
                        className={`sidebar__team-toggle ${
                          location.pathname.startsWith(`/project/${project.prefix}/`) ? 'sidebar__team-toggle--active' : ''
                        }`}
                        aria-expanded={isExpanded}
                        onClick={() => {
                          const projectTicketsPath = `/project/${project.prefix}/tickets`;
                          handleAxisNestToggle({
                            key: project.prefix,
                            isExpanded,
                            axisPathPrefix: `/project/${project.prefix}/`,
                            ticketsPath: projectTicketsPath,
                            storageKey: 'wip-last-project-key',
                            pathname: location.pathname,
                            setFocused: setFocusedProjectPrefix,
                            setExpanded: setExpandedProject,
                            navigate,
                          });
                        }}
                        title={!sidebarOpen ? project.name : undefined}
                        data-testid={`project-select-${project.prefix}`}
                      >
                        <span className="sidebar__project-icon">
                          {project.prefix[0]?.toUpperCase() ?? 'P'}
                        </span>
                        {sidebarOpen && <span className="sidebar__label">{project.name}</span>}
                        {sidebarOpen && (
                          <span
                            className="sidebar__team-chevron"
                            style={{ marginLeft: 'auto', transform: isExpanded ? 'rotate(0deg)' : 'rotate(-90deg)' }}
                            data-testid={`project-chevron-${project.prefix}`}
                          >
                            <IconChevronDown />
                          </span>
                        )}
                      </button>
                    </div>
                    {isExpanded && (
                                            <div className="sidebar__nested">
                        <NavItemLink
                          path={`/project/${project.prefix}/tickets`}
                          icon={IconTicket}
                          label={t('nav.tickets')}
                          sidebarOpen={sidebarOpen}
                          nested
                          testId={`nav-issues-${project.prefix}`}
                        />
                        <NavItemLink
                          path={`/project/${project.prefix}/gantt`}
                          icon={IconGantt}
                          label={t('nav.gantt')}
                          sidebarOpen={sidebarOpen}
                          nested
                          testId={`nav-gantt-${project.prefix}`}
                        />
                        <NavItemLink
                          path={`/project/${project.prefix}/dependencies`}
                          icon={IconDependency}
                          label={t('nav.dependencies')}
                          sidebarOpen={sidebarOpen}
                          nested
                          testId={`nav-dependencies-${project.prefix}`}
                        />
                        <NavItemLink
                          path={`/project/${project.prefix}/wiki`}
                          icon={IconWiki}
                          label={t('nav.wiki')}
                          sidebarOpen={sidebarOpen}
                          nested
                          testId={`nav-wiki-${project.prefix}`}
                        />
                        <NavItemLink
                          path={`/project/${project.prefix}/settings`}
                          icon={IconSettings}
                          label={t('nav.settings')}
                          sidebarOpen={sidebarOpen}
                          nested
                          testId={`nav-settings-${project.prefix}`}
                        />
                      </div>
                    )}
                  </div>
                );
              };

              return (
                <>
                  {/* 表示中のプロジェクト */}
                  {renderProjectItem(visibleProject)}

                  {/* 「他のプロジェクト」折りたたみボタン */}
                  {hiddenProjects.length > 0 && (
                    <>
                      <div className="sidebar__team-block">
                        <div className="sidebar__team-row">
                          <button
                            type="button"
                            className="sidebar__team-toggle sidebar__team-toggle--collapsed-group"
                            aria-expanded={collapsedProjectsOpen}
                            onClick={() => setCollapsedProjectsOpen(!collapsedProjectsOpen)}
                            data-testid="project-collapsed-toggle"
                          >
                            <span className="sidebar__project-icon">···</span>
                            {sidebarOpen && (
                              <span className="sidebar__label">
                                {t('sidebar.otherProjects', { count: hiddenProjects.length })}
                              </span>
                            )}
                            {sidebarOpen && (
                              <span
                                className="sidebar__team-chevron"
                                style={{ marginLeft: 'auto', transform: collapsedProjectsOpen ? 'rotate(0deg)' : 'rotate(-90deg)' }}
                                data-testid="project-collapsed-chevron"
                              >
                                <IconChevronDown />
                              </span>
                            )}
                          </button>
                        </div>
                      </div>

                      {/* 隠れたプロジェクト一覧 */}
                      {collapsedProjectsOpen && (
                        <div className="sidebar__collapsed-teams">
                          {hiddenProjects.map((project) => (
                            <button
                              key={project.id}
                              type="button"
                              className={`sidebar__team-collapsed-item ${
                                location.pathname.startsWith(`/project/${project.prefix}/`) ? 'sidebar__team-collapsed-item--active' : ''
                              }`}
                              onClick={() => {
                                setFocusedProjectPrefix(project.prefix);
                                setExpandedProject(project.prefix);
                                setCollapsedProjectsOpen(false);
                                navigate(`/project/${project.prefix}/tickets`);
                              }}
                              title={!sidebarOpen ? project.name : undefined}
                              data-testid={`project-collapsed-select-${project.prefix}`}
                            >
                              <span className="sidebar__project-icon">
                                {project.prefix[0]?.toUpperCase() ?? 'P'}
                              </span>
                              {sidebarOpen && <span className="sidebar__label">{project.name}</span>}
                            </button>
                          ))}
                        </div>
                      )}
                    </>
                  )}
                </>
              );
            })()}
          </>
        )}

        <div className="sidebar__divider" />

        {moreGlobalItems.map((item) => (
          <NavItemLink key={item.path} {...item} sidebarOpen={sidebarOpen} nested={false} />
        ))}
      </nav>

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

        {user?.isStaff && (
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
