/**
 * CommandPalette.tsx — コマンドパレット（⌘K）
 *
 * cmdk ライブラリベースのグローバル検索 + ナビゲーション。
 * Linearスタイルのキーボードファーストな操作体験。
 * API検索によるチケット・Wiki・プロジェクト横断検索。
 */

import { useEffect, useCallback, useState, useMemo } from 'react';
import { Command } from 'cmdk';
import { useNavigate, useLocation } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { useUIStore } from '@/shared/stores/uiStore';
import { useAuthStore } from '@/shared/stores/authStore';
import { getLastProjectKey } from '@/shared/hooks/useProject';
import { getLastTeamSlug } from '@/shared/hooks/useTeam';
import { useTeams } from '@/features/teams/hooks/useTeams';
import { apiClient } from '@/shared/api/client';
import { searchTicketsLocal } from '@/shared/sync/repos/ticketRepo';
import { isTempTicketKey } from '@/shared/sync/ticketWrites';
import { buildTicketDetailPath } from '@/features/tickets/utils/ticketNavigation';
import './CommandPalette.css';

interface SearchResult {
  type: 'ticket' | 'wiki' | 'project';
  id: number;
  key?: string;
  title: string;
  status?: string;
  projectKey?: string;
  url: string;
  icon: string;
}

export function buildNavItems(teamSlug: string | null, projectKey: string | null) {
  const items: Array<{ id: string; label: string; path: string; icon: string }> = [
    { id: 'my-issues', label: 'nav.myIssues', path: '/my-issues', icon: '📋' },
    { id: 'inbox', label: 'nav.inbox', path: '/notifications', icon: '📥' },
  ];
  if (teamSlug) {
    items.push(
      { id: 'team-home', label: 'nav.home', path: `/team/${teamSlug}/dashboard`, icon: '🏠' },
      { id: 'team-tickets', label: 'nav.tickets', path: `/team/${teamSlug}/tickets`, icon: '🎫' },
      { id: 'team-gantt', label: 'nav.gantt', path: `/team/${teamSlug}/gantt`, icon: '📈' },
      { id: 'team-dependencies', label: 'nav.dependencies', path: `/team/${teamSlug}/dependencies`, icon: '🔗' },
      { id: 'team-projects', label: 'nav.projects', path: `/team/${teamSlug}/projects`, icon: '📁' },
      { id: 'team-triage', label: 'nav.triage', path: `/team/${teamSlug}/triage`, icon: '📥' },
      { id: 'team-wiki', label: 'nav.wiki', path: `/team/${teamSlug}/wiki`, icon: '📝' },
      { id: 'team-settings', label: 'nav.teamSettings', path: `/team/${teamSlug}/settings`, icon: '⚙️' },
    );
  }
  if (projectKey) {
    items.push(
      { id: 'project-tickets', label: 'nav.tickets', path: `/project/${projectKey}/tickets`, icon: '🎫' },
      { id: 'board', label: 'nav.board', path: `/project/${projectKey}/board`, icon: '🧱' },
      { id: 'cycles', label: 'nav.cycles', path: `/project/${projectKey}/cycles`, icon: '🔄' },
      { id: 'wiki', label: 'nav.wiki', path: `/project/${projectKey}/wiki`, icon: '📝' },
      { id: 'gantt', label: 'nav.gantt', path: `/project/${projectKey}/gantt`, icon: '📈' },
      { id: 'dependencies', label: 'nav.dependencies', path: `/project/${projectKey}/dependencies`, icon: '🔗' },
      { id: 'project-settings', label: 'nav.projectSettings', path: `/project/${projectKey}/settings`, icon: '⚙️' },
    );
  }
  items.push(
    { id: 'teams', label: 'nav.teams', path: '/teams', icon: '👥' },
    { id: 'workspace-wiki', label: 'nav.wiki', path: '/wiki', icon: '📝' },
    { id: 'dashboard', label: 'nav.dashboard', path: '/dashboard', icon: '📊' },
    { id: 'settings', label: 'nav.personalSettings', path: '/settings', icon: '⚙️' },
  );
  return items;
}

const ACTION_ITEMS = [
  { id: 'new-ticket', label: 'ticket.create', action: 'create-ticket', icon: '➕' },
  { id: 'toggle-theme', label: 'commandPalette.toggleTheme', action: 'theme', icon: '🌓' },
] as const;

export function CommandPalette() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const location = useLocation();
  const { commandPaletteOpen, setCommandPaletteOpen, toggleTheme, openTicketFormModal } = useUIStore();
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated);
  // App 直下に常駐するため、未ログインの /login 等ではチーム API を叩かない
  //（401 → window.location=/login の無限リロードを防ぐ）
  const { data: teams = [] } = useTeams({ enabled: isAuthenticated });
  const [searchResults, setSearchResults] = useState<SearchResult[]>([]);
  const [searchQuery, setSearchQuery] = useState('');

  const teamSlug = useMemo(() => {
    const match = location.pathname.match(/^\/team\/([^/]+)/);
    return match?.[1] ?? getLastTeamSlug();
  }, [location.pathname]);

  const projectKey = useMemo(() => {
    const match = location.pathname.match(/^\/project\/([^/]+)/);
    return match?.[1] ?? getLastProjectKey();
  }, [location.pathname]);

  const navigationItems = useMemo(
    () => buildNavItems(teamSlug, projectKey),
    [teamSlug, projectKey],
  );

  // ⌘K / Ctrl+K でトグル
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault();
        setCommandPaletteOpen(!commandPaletteOpen);
      }
      if (e.key === 'Escape') {
        setCommandPaletteOpen(false);
      }
    },
    [commandPaletteOpen, setCommandPaletteOpen],
  );

  useEffect(() => {
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [handleKeyDown]);

  // 検索: チケットは端末内 DB から入力と同時に出し、サーバー検索（debounce）の結果からはチケット以外を後ろに足す
  useEffect(() => {
    if (!searchQuery || searchQuery.length < 2) {
      setSearchResults([]);
      return;
    }
    let cancelled = false;
    let localResults: SearchResult[] = [];
    void searchTicketsLocal(searchQuery, 8).then((tickets) => {
      if (cancelled) return;
      localResults = tickets
        .filter((t) => !isTempTicketKey(t.ticketKey))
        .map((t) => ({
          type: 'ticket' as const,
          id: t.id,
          key: t.ticketKey,
          title: t.title,
          status: t.status,
          projectKey: t.projectPrefix ?? '',
          // 画面のルート（/project/{prefix}/tickets/{key} または /team/{slug}/tickets/{key}）
          url: buildTicketDetailPath(t.projectPrefix, t.ticketKey, undefined, t.projectPrefix ? null : (t.team?.slug ?? null)),
          icon: '🎫',
        }));
      setSearchResults(localResults);
    });
    const timer = setTimeout(async () => {
      try {
        const { data } = await apiClient.get<{ results?: SearchResult[] }>('/search/', {
          params: { q: searchQuery, limit: 8 },
        });
        if (cancelled) return;
        const serverResults = data.results ?? [];
        const localKeys = new Set(localResults.map((r) => r.key));
        // 端末内に無いチケット（同期範囲外など）はサーバーの結果を使う
        const extraTickets = serverResults
          .filter((r) => r.type === 'ticket' && !localKeys.has(r.key))
          .slice(0, Math.max(0, 8 - localResults.length));
        const others = serverResults.filter((r) => r.type !== 'ticket');
        setSearchResults([...localResults, ...extraTickets, ...others]);
      } catch {
        // サーバー検索に失敗しても端末内の結果は残す
      }
    }, 300);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [searchQuery]);

  // パレットを閉じるときにクリア
  useEffect(() => {
    if (!commandPaletteOpen) {
      setSearchQuery('');
      setSearchResults([]);
    }
  }, [commandPaletteOpen]);

  if (!commandPaletteOpen) return null;

  const handleSelect = (item: { action?: string; path?: string }) => {
    if (item.action === 'theme') {
      toggleTheme();
    } else if (item.action === 'create-ticket') {
      const teamMatch = window.location.pathname.match(/^\/team\/([^/]+)/);
      if (teamMatch?.[1]) {
        openTicketFormModal(null, teamMatch[1]);
      } else {
        const lastKey = getLastProjectKey();
        if (lastKey) {
          openTicketFormModal(lastKey);
        } else {
          navigate('/my-issues');
        }
      }
    } else if (item.path) {
      navigate(item.path);
    }
    setCommandPaletteOpen(false);
  };

  const handleSearchSelect = (result: SearchResult) => {
    // サーバー検索の URL は古い形（/p/{prefix}/...）のことがあるので、画面のルートへ直す
    navigate(result.url.replace(/^\/p\//, '/project/'));
    setCommandPaletteOpen(false);
  };

  return (
    <>
      {/* オーバーレイ */}
      <div
        className="command-palette__overlay"
        onClick={() => setCommandPaletteOpen(false)}
        data-testid="command-palette-overlay"
      />

      {/* パレット本体 */}
      <div className="command-palette" data-testid="command-palette">
        <Command label="Command Palette" shouldFilter={searchResults.length === 0}>
          <Command.Input
            className="command-palette__input"
            placeholder={t('common.search')}
            autoFocus
            value={searchQuery}
            onValueChange={setSearchQuery}
            data-testid="command-palette-search"
          />

          <Command.List className="command-palette__list">
            <Command.Empty className="command-palette__empty">
              {t('common.noResults')}
            </Command.Empty>

            {/* API検索結果 */}
            {searchResults.length > 0 && (
              <Command.Group
                heading={t('commandPalette.searchResults')}
                className="command-palette__group"
              >
                {searchResults.map((result) => (
                  <Command.Item
                    key={`${result.type}-${result.id}`}
                    value={`${result.key || ''} ${result.title}`}
                    onSelect={() => handleSearchSelect(result)}
                    className="command-palette__item"
                  >
                    <span className="command-palette__item-icon">{result.icon}</span>
                    <span className="command-palette__item-label">
                      {result.key && (
                        <span className="command-palette__item-key">{result.key} </span>
                      )}
                      {result.title}
                    </span>
                    {result.status && (
                      <span className="command-palette__item-badge">{result.status}</span>
                    )}
                  </Command.Item>
                ))}
              </Command.Group>
            )}

            {/* ナビゲーション */}
            <Command.Group
              heading={t('commandPalette.navigation')}
              className="command-palette__group"
            >
              {navigationItems.map((item) => (
                <Command.Item
                  key={item.id}
                  value={`${item.id} ${t(item.label)}`}
                  onSelect={() => handleSelect({ path: item.path })}
                  className="command-palette__item"
                  data-testid={`cmd-${item.id}`}
                >
                  <span className="command-palette__item-icon">{item.icon}</span>
                  <span className="command-palette__item-label">{t(item.label)}</span>
                </Command.Item>
              ))}
            </Command.Group>

            {/* チーム切替 */}
            {teams.length > 0 && (
              <Command.Group
                heading={t('commandPalette.openTeam')}
                className="command-palette__group"
              >
                {teams.map((team) => (
                  <Command.Item
                    key={`team-${team.id}`}
                    value={`team ${team.name}`}
                    onSelect={() => {
                      navigate(`/team/${team.slug}/tickets`);
                      setCommandPaletteOpen(false);
                    }}
                    className="command-palette__item"
                  >
                    <span className="command-palette__item-icon">👥</span>
                    <span className="command-palette__item-label">{team.name}</span>
                  </Command.Item>
                ))}
              </Command.Group>
            )}

            {/* アクション */}
            <Command.Group
              heading={t('commandPalette.actions')}
              className="command-palette__group"
            >
              {ACTION_ITEMS.map((item) => (
                <Command.Item
                  key={item.id}
                  value={`${item.id} ${t(item.label)}`}
                  onSelect={() => handleSelect(item)}
                  className="command-palette__item"
                  data-testid={`cmd-${item.id}`}
                >
                  <span className="command-palette__item-icon">{item.icon}</span>
                  <span className="command-palette__item-label">{t(item.label)}</span>
                </Command.Item>
              ))}
            </Command.Group>
          </Command.List>
        </Command>

        <div className="command-palette__footer">
          <span className="command-palette__shortcut">↑↓ {t('commandPalette.navigateHint')}</span>
          <span className="command-palette__shortcut">↵ {t('commandPalette.selectHint')}</span>
          <span className="command-palette__shortcut">Esc {t('commandPalette.closeHint')}</span>
        </div>
      </div>
    </>
  );
}
