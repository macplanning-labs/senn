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

function buildNavItems(teamSlug: string | null, projectKey: string | null) {
  const items: Array<{ id: string; label: string; path: string; icon: string }> = [
    { id: 'my-issues', label: 'nav.myIssues', path: '/my-issues', icon: '📋' },
    { id: 'inbox', label: 'nav.inbox', path: '/notifications', icon: '📥' },
  ];
  if (teamSlug) {
    items.push(
      { id: 'team-home', label: 'nav.home', path: `/t/${teamSlug}/dashboard`, icon: '🏠' },
      { id: 'team-tickets', label: 'nav.tickets', path: `/t/${teamSlug}/tickets`, icon: '🎫' },
      { id: 'team-projects', label: 'nav.projects', path: `/t/${teamSlug}/projects`, icon: '📁' },
    );
  }
  if (projectKey) {
    items.push(
      { id: 'board', label: 'nav.board', path: `/p/${projectKey}/board`, icon: '🧱' },
      { id: 'cycles', label: 'nav.cycles', path: `/p/${projectKey}/cycles`, icon: '🔄' },
      { id: 'wiki', label: 'nav.wiki', path: `/p/${projectKey}/wiki`, icon: '📝' },
    );
  }
  items.push(
    { id: 'teams', label: 'nav.teams', path: '/teams', icon: '👥' },
    { id: 'dashboard', label: 'nav.dashboard', path: '/dashboard', icon: '📊' },
    { id: 'settings', label: 'nav.settings', path: '/settings', icon: '⚙️' },
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
    const match = location.pathname.match(/^\/t\/([^/]+)/);
    return match?.[1] ?? getLastTeamSlug();
  }, [location.pathname]);

  const projectKey = useMemo(() => {
    const match = location.pathname.match(/^\/p\/([^/]+)/);
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

  // API検索（debounce付き）
  useEffect(() => {
    if (!searchQuery || searchQuery.length < 2) {
      setSearchResults([]);
      return;
    }
    const timer = setTimeout(async () => {
      try {
        const { data } = await apiClient.get('/search/', {
          params: { q: searchQuery, limit: 8 },
        });
        setSearchResults(data.results ?? []);
      } catch {
        setSearchResults([]);
      }
    }, 300);
    return () => clearTimeout(timer);
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
      const teamMatch = window.location.pathname.match(/^\/t\/([^/]+)/);
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
    navigate(result.url);
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
                      navigate(`/t/${team.slug}/tickets`);
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
