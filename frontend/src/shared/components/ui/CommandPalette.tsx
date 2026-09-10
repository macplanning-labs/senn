/**
 * CommandPalette.tsx — コマンドパレット（⌘K）
 *
 * cmdk ライブラリベースのグローバル検索 + ナビゲーション。
 * Linearスタイルのキーボードファーストな操作体験。
 * API検索によるチケット・Wiki・プロジェクト横断検索。
 */

import { useEffect, useCallback, useState } from 'react';
import { Command } from 'cmdk';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { useUIStore } from '@/shared/stores/uiStore';
import { getLastProjectKey } from '@/shared/hooks/useProject';
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

const NAVIGATION_ITEMS = [
  { id: 'dashboard', label: 'nav.dashboard', path: '/dashboard', icon: '📊' },
  { id: 'tickets', label: 'nav.tickets', path: '/tickets', icon: '🎫' },
  { id: 'gantt', label: 'nav.gantt', path: '/gantt', icon: '📅' },
  { id: 'wiki', label: 'nav.wiki', path: '/wiki', icon: '📝' },
  { id: 'notifications', label: 'nav.notifications', path: '/notifications', icon: '🔔' },
  { id: 'settings', label: 'nav.settings', path: '/settings', icon: '⚙️' },
] as const;

const ACTION_ITEMS = [
  { id: 'new-ticket', label: 'ticket.create', action: 'create-ticket', icon: '➕' },
  { id: 'toggle-theme', label: 'Toggle Theme', action: 'theme', icon: '🌓' },
] as const;

export function CommandPalette() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { commandPaletteOpen, setCommandPaletteOpen, toggleTheme, openTicketFormModal } = useUIStore();
  const [searchResults, setSearchResults] = useState<SearchResult[]>([]);
  const [searchQuery, setSearchQuery] = useState('');

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
      const lastKey = getLastProjectKey();
      if (lastKey) {
        openTicketFormModal(lastKey);
      } else {
        navigate('/tickets/new');
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
                heading="検索結果"
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
              heading="Navigation"
              className="command-palette__group"
            >
              {NAVIGATION_ITEMS.map((item) => (
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

            {/* アクション */}
            <Command.Group
              heading="Actions"
              className="command-palette__group"
            >
              {ACTION_ITEMS.map((item) => (
                <Command.Item
                  key={item.id}
                  value={`${item.id} ${'label' in item && item.label.startsWith('ticket') ? t(item.label) : item.label}`}
                  onSelect={() => handleSelect(item)}
                  className="command-palette__item"
                  data-testid={`cmd-${item.id}`}
                >
                  <span className="command-palette__item-icon">{item.icon}</span>
                  <span className="command-palette__item-label">
                    {'label' in item && item.label.startsWith('ticket') ? t(item.label) : item.label}
                  </span>
                </Command.Item>
              ))}
            </Command.Group>
          </Command.List>
        </Command>

        <div className="command-palette__footer">
          <span className="command-palette__shortcut">↑↓ Navigate</span>
          <span className="command-palette__shortcut">↵ Select</span>
          <span className="command-palette__shortcut">Esc Close</span>
        </div>
      </div>
    </>
  );
}
