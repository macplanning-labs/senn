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
import { startSync, stopSync } from '@/shared/sync/syncEngine';
import { useEffect } from 'react';
import './MainLayout.css';

export function MainLayout() {
  const { sidebarOpen, setCommandPaletteOpen } = useUIStore();
  const { isOffline } = useOfflineStatus();

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
