/**
 * MainLayout.tsx — メインレイアウト
 *
 * サイドバー + ヘッダー + メインコンテンツエリア。
 * 認証済みユーザーのみ表示。
 */

import { Outlet } from 'react-router-dom';
import { Sidebar } from './Sidebar';
import { NotificationDropdown } from '@/features/notifications/components/NotificationDropdown';
import { useUIStore } from '@/shared/stores/uiStore';
import { useOfflineStatus } from '@/shared/hooks/useOfflineStatus';
import { useGoToHotkeys } from '@/shared/hooks/useGoToHotkeys';
import { startSync, stopSync } from '@/shared/sync/syncEngine';
import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import './MainLayout.css';

function LanguageToggle() {
  const { i18n } = useTranslation();
  const isJa = i18n.language === 'ja';
  return (
    <button
      className="layout__lang-toggle"
      onClick={() => void i18n.changeLanguage(isJa ? 'en' : 'ja')}
      title={isJa ? 'Switch to English' : '日本語に切替'}
    >
      {isJa ? '🇯🇵' : '🇺🇸'}
    </button>
  );
}

export function MainLayout() {
  const { sidebarOpen, setCommandPaletteOpen } = useUIStore();
  const { isOffline } = useOfflineStatus();

  // Go to ショートカット（G then キー）を有効化
  useGoToHotkeys();

  // 認証済みでマウント時に同期開始
  useEffect(() => {
    startSync();
    return () => stopSync();
  }, []);

  return (
    <div className="layout" data-testid="main-layout">
      <Sidebar />
      <div
        className={`layout__main ${sidebarOpen ? 'layout__main--sidebar-open' : 'layout__main--sidebar-collapsed'}`}
      >
        {/* ヘッダーバー */}
        <header className="layout__header" data-testid="header-bar">
          <button
            className="layout__search-trigger"
            onClick={() => setCommandPaletteOpen(true)}
            data-testid="header-search"
          >
            <span>⌘K to search...</span>
          </button>
          <div className="layout__header-actions">
            <LanguageToggle />
            <NotificationDropdown />
          </div>
        </header>

        {/* オフラインバナー */}
        {isOffline && (
          <div className="layout__offline-banner" data-testid="offline-banner">
            ⚡ You're offline — changes will sync when reconnected
          </div>
        )}

        {/* メインコンテンツ */}
        <main className="layout__content" data-testid="main-content">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
