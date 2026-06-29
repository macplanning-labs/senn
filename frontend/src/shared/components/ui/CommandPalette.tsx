/**
 * CommandPalette.tsx — コマンドパレット（⌘K）
 *
 * cmdk ライブラリベースのグローバル検索 + ナビゲーション。
 * Linearスタイルのキーボードファーストな操作体験。
 */

import { useEffect, useCallback } from 'react';
import { Command } from 'cmdk';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { useUIStore } from '@/shared/stores/uiStore';
import './CommandPalette.css';

const NAVIGATION_ITEMS = [
  { id: 'dashboard', label: 'nav.dashboard', path: '/dashboard', icon: '📊' },
  { id: 'tickets', label: 'nav.tickets', path: '/tickets', icon: '🎫' },
  { id: 'gantt', label: 'nav.gantt', path: '/gantt', icon: '📅' },
  { id: 'wiki', label: 'nav.wiki', path: '/wiki', icon: '📝' },
  { id: 'notifications', label: 'nav.notifications', path: '/notifications', icon: '🔔' },
  { id: 'settings', label: 'nav.settings', path: '/settings', icon: '⚙️' },
] as const;

const ACTION_ITEMS = [
  { id: 'new-ticket', label: 'ticket.create', action: 'navigate', path: '/tickets/new', icon: '➕' },
  { id: 'toggle-theme', label: 'Toggle Theme', action: 'theme', icon: '🌓' },
] as const;

export function CommandPalette() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { commandPaletteOpen, setCommandPaletteOpen, toggleTheme } = useUIStore();

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

  if (!commandPaletteOpen) return null;

  const handleSelect = (item: { action?: string; path?: string }) => {
    if (item.action === 'theme') {
      toggleTheme();
    } else if (item.path) {
      navigate(item.path);
    }
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
        <Command label="Command Palette" shouldFilter>
          <Command.Input
            className="command-palette__input"
            placeholder={t('common.search')}
            autoFocus
            data-testid="command-palette-search"
          />

          <Command.List className="command-palette__list">
            <Command.Empty className="command-palette__empty">
              {t('common.noResults')}
            </Command.Empty>

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
