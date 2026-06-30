/**
 * SettingsPage.tsx — 設定画面
 *
 * 言語切替・テーマ切替・プロフィール情報表示・メール通知設定。
 */

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useUIStore } from '@/shared/stores/uiStore';
import { useAuthStore } from '@/shared/stores/authStore';
import { useMutation } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { useToast } from '@/shared/stores/toastStore';
import './SettingsPage.css';

const LANGUAGES = [
  { code: 'ja', label: '日本語' },
  { code: 'en', label: 'English' },
] as const;

const NOTIFICATION_CATEGORIES = [
  { key: 'assigned', label: 'Assigned to me', icon: '👤' },
  { key: 'commented', label: 'Comments on my tickets', icon: '💬' },
  { key: 'status_changed', label: 'Status changes', icon: '🔄' },
  { key: 'due_soon', label: 'Due date reminders', icon: '⏰' },
] as const;

export function SettingsPage() {
  const { t, i18n } = useTranslation();
  const { theme, toggleTheme } = useUIStore();
  const { user } = useAuthStore();
  const toast = useToast();

  const [emailEnabled, setEmailEnabled] = useState(true);

  const toggleEmailMutation = useMutation({
    mutationFn: async (enabled: boolean) => {
      await apiClient.patch('/api/v1/auth/me/', {
        email_notifications_enabled: enabled,
      });
      return enabled;
    },
    onSuccess: (enabled) => {
      setEmailEnabled(enabled);
      toast.success(enabled ? 'メール通知を有効にしました' : 'メール通知を無効にしました');
    },
    onError: () => {
      toast.error('設定の更新に失敗しました');
    },
  });

  return (
    <div className="settings" data-testid="settings-page">
      <h1 className="settings__title">{t('nav.settings', 'Settings')}</h1>

      {/* プロフィール */}
      <section className="settings__section">
        <h2 className="settings__section-title">Profile</h2>
        <div className="settings__card">
          <div className="settings__profile">
            <span className="settings__avatar">
              {(user?.firstName || user?.username || '?')[0]?.toUpperCase()}
            </span>
            <div className="settings__profile-info">
              <span className="settings__profile-name">
                {user?.firstName ? `${user.firstName} ${user.lastName}` : user?.username}
              </span>
              <span className="settings__profile-email">{user?.email}</span>
            </div>
          </div>
        </div>
      </section>

      {/* 言語設定 */}
      <section className="settings__section">
        <h2 className="settings__section-title">{t('settings.language', 'Language')}</h2>
        <div className="settings__card">
          <div className="settings__option-group">
            {LANGUAGES.map((lang) => (
              <button
                key={lang.code}
                className={`settings__lang-btn ${
                  i18n.language === lang.code ? 'settings__lang-btn--active' : ''
                }`}
                onClick={() => void i18n.changeLanguage(lang.code)}
              >
                <span className="settings__lang-flag">
                  {lang.code === 'ja' ? '🇯🇵' : '🇺🇸'}
                </span>
                {lang.label}
              </button>
            ))}
          </div>
        </div>
      </section>

      {/* テーマ設定 */}
      <section className="settings__section">
        <h2 className="settings__section-title">{t('settings.theme', 'Theme')}</h2>
        <div className="settings__card">
          <div className="settings__option-group">
            <button
              className={`settings__theme-btn ${theme === 'dark' ? 'settings__theme-btn--active' : ''}`}
              onClick={() => { if (theme !== 'dark') toggleTheme(); }}
            >
              🌙 Dark
            </button>
            <button
              className={`settings__theme-btn ${theme === 'light' ? 'settings__theme-btn--active' : ''}`}
              onClick={() => { if (theme !== 'light') toggleTheme(); }}
            >
              ☀️ Light
            </button>
          </div>
        </div>
      </section>

      {/* メール通知設定 */}
      <section className="settings__section">
        <h2 className="settings__section-title">📧 Email Notifications</h2>
        <div className="settings__card">
          <div className="settings__notification-master">
            <div className="settings__notification-row">
              <div>
                <div className="settings__notification-label">Email notifications</div>
                <div className="settings__notification-desc">
                  Receive email notifications for ticket updates
                </div>
              </div>
              <label className="settings__toggle">
                <input
                  type="checkbox"
                  checked={emailEnabled}
                  onChange={(e) => toggleEmailMutation.mutate(e.target.checked)}
                />
                <span className="settings__toggle-slider" />
              </label>
            </div>
          </div>

          {emailEnabled && (
            <div className="settings__notification-categories">
              {NOTIFICATION_CATEGORIES.map((cat) => (
                <div key={cat.key} className="settings__notification-row">
                  <div>
                    <div className="settings__notification-label">
                      {cat.icon} {cat.label}
                    </div>
                  </div>
                  <label className="settings__toggle">
                    <input type="checkbox" defaultChecked />
                    <span className="settings__toggle-slider" />
                  </label>
                </div>
              ))}
            </div>
          )}
        </div>
      </section>

      {/* キーボードショートカット */}
      <section className="settings__section">
        <h2 className="settings__section-title">Keyboard Shortcuts</h2>
        <div className="settings__card">
          <div className="settings__shortcuts">
            <div className="settings__shortcut">
              <kbd>⌘K</kbd>
              <span>Command Palette</span>
            </div>
            <div className="settings__shortcut">
              <kbd>J</kbd> / <kbd>K</kbd>
              <span>Navigate tickets</span>
            </div>
            <div className="settings__shortcut">
              <kbd>Enter</kbd>
              <span>Open ticket detail</span>
            </div>
            <div className="settings__shortcut">
              <kbd>Esc</kbd>
              <span>Close panel</span>
            </div>
            <div className="settings__shortcut">
              <kbd>C</kbd>
              <span>Create ticket</span>
            </div>
          </div>
        </div>
      </section>
    </div>
  );
}

