/**
 * MainLayout.tsx — メインレイアウト
 *
 * サイドバー + ヘッダー + メインコンテンツエリア。
 * 認証済みユーザーのみ表示。
 * デモモードバナー対応。
 */

import { Outlet, useNavigate, useLocation } from 'react-router-dom';
import { Sidebar } from './Sidebar';
import { NotificationDropdown } from '@/features/notifications/components/NotificationDropdown';
import { useUIStore } from '@/shared/stores/uiStore';
import { useOfflineStatus } from '@/shared/hooks/useOfflineStatus';
import { useGoToHotkeys } from '@/shared/hooks/useGoToHotkeys';
import { useHistoryNav } from '@/shared/hooks/useHistoryNav';
import { startSync, stopSync } from '@/shared/sync/syncEngine';
import { onKeyRemap } from '@/shared/sync/push';
import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { isDemoMode, DEMO_ACCESS_TOKEN } from '@/features/demo/demoMode';
import { useAuthStore } from '@/shared/stores/authStore';
import { getAccessToken } from '@/shared/api/client';
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

function ThemeToggle() {
  const { t } = useTranslation();
  const { theme, toggleTheme } = useUIStore();
  const isDark = theme === 'dark';
  // アイコンは「今のモード」(ライト=太陽 / ダーク=三日月)。ラベルは押した時に切り替わる先
  const label = isDark ? t('sidebar.lightMode') : t('sidebar.darkMode');
  return (
    <button
      className="layout__theme-toggle"
      onClick={toggleTheme}
      data-testid="theme-toggle"
      title={label}
      aria-label={label}
    >
      {/* 太陽と月を重ねて描画し、CSS で回転+フェードして入れ替える(data-mode で切替) */}
      <span className="layout__theme-icons" data-mode={isDark ? 'dark' : 'light'} aria-hidden="true">
        <svg className="layout__theme-icon layout__theme-icon--sun" width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
          <circle cx="8" cy="8" r="3" />
          <path d="M8 1v2M8 13v2M1 8h2M13 8h2M3.05 3.05l1.41 1.41M11.54 11.54l1.41 1.41M3.05 12.95l1.41-1.41M11.54 4.46l1.41-1.41" />
        </svg>
        <svg className="layout__theme-icon layout__theme-icon--moon" width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
          <path d="M13.5 8.5a5.5 5.5 0 1 1-6-6 4.5 4.5 0 0 0 6 6z" />
        </svg>
      </span>
    </button>
  );
}

export function MainLayout() {
  const { sidebarOpen, setCommandPaletteOpen } = useUIStore();
  const { canGoBack, canGoForward, goBack, goForward } = useHistoryNav();
  const { isOffline } = useOfflineStatus();
  const navigate = useNavigate();
  const location = useLocation();
  const logout = useAuthStore((s) => s.logout);
  const user = useAuthStore((s) => s.user);
  const { t } = useTranslation();
  const demoMode = isDemoMode();

  // Go to ショートカット（G then キー）を有効化
  useGoToHotkeys();

  // デモ TTL 切れでフラグだけ消えたあと、デモトークンが残っている場合は logout
  useEffect(() => {
    if (getAccessToken() === DEMO_ACCESS_TOKEN && !isDemoMode()) {
      void logout({ force: true }).then(() => {
        navigate('/login');
      });
    }
  }, [logout, navigate]);

  // デモモード TTL チェック：30秒ごとに再評価して期限切れで logout
  useEffect(() => {
    if (!demoMode) {
      return;
    }

    const intervalId = setInterval(() => {
      if (!isDemoMode()) {
        void logout({ force: true }).then(() => {
          navigate('/login');
        });
      }
    }, 30 * 1000);

    return () => clearInterval(intervalId);
  }, [demoMode, logout, navigate]);

  // 認証済みでマウント時に同期開始（デモでも同期する）
  useEffect(() => {
    if (!user?.id) {
      return;
    }
    startSync(user.id);
    return () => stopSync();
  }, [user?.id]);

  // キーの付け替え（仮キー → 本キー）を監視
  useEffect(() => {
    const unsubscribe = onKeyRemap((tempKey, realKey) => {
      if (location.pathname.includes(tempKey)) {
        navigate(location.pathname.replace(tempKey, realKey) + location.search, { replace: true });
      }
    });
    return unsubscribe as () => void;
  }, [location, navigate]);

  async function handleLogout() {
    await logout();
    navigate('/login');
  }

  async function handleCreateAccount() {
    await logout();
    navigate('/register');
  }

  return (
    <div className="layout" data-testid="main-layout">
      <Sidebar />
      <div
        className={`layout__main ${sidebarOpen ? 'layout__main--sidebar-open' : 'layout__main--sidebar-collapsed'}`}
      >
        {/* ヘッダーバー */}
        <header className="layout__header" data-testid="header-bar">
          <div className="layout__history-nav">
            <button
              type="button"
              className="layout__history-btn"
              onClick={goBack}
              disabled={!canGoBack}
              aria-label={t('nav.historyBack')}
              title={`${t('nav.historyBack')} (⌘[)`}
              data-testid="history-back"
            >
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M15 18l-6-6 6-6" /></svg>
            </button>
            <button
              type="button"
              className="layout__history-btn"
              onClick={goForward}
              disabled={!canGoForward}
              aria-label={t('nav.historyForward')}
              title={`${t('nav.historyForward')} (⌘])`}
              data-testid="history-forward"
            >
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M9 18l6-6-6-6" /></svg>
            </button>
          </div>
          <button
            className="layout__search-trigger"
            onClick={() => setCommandPaletteOpen(true)}
            data-testid="header-search"
          >
            <span>{t('commandPalette.searchTrigger')}</span>
          </button>
          <div className="layout__header-actions">
            <ThemeToggle />
            <LanguageToggle />
            <NotificationDropdown />
          </div>
        </header>

        {/* デモモードバナー */}
        {demoMode && (
          <div className="layout__demo-banner" data-testid="demo-banner">
            <div className="layout__demo-banner-content">
              <span>
                👨‍💻 {t('demo.banner', 'Demo mode — registered not required. Changes are not saved.')}
              </span>
              <div className="layout__demo-banner-actions">
                <button
                  className="layout__demo-banner-btn"
                  onClick={() => { void handleCreateAccount(); }}
                >
                  {t('auth.register', 'Sign up')}
                </button>
                <button
                  className="layout__demo-banner-btn layout__demo-banner-btn--logout"
                  onClick={() => { void handleLogout(); }}
                >
                  {t('auth.logout', 'Log out')}
                </button>
              </div>
            </div>
          </div>
        )}

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
