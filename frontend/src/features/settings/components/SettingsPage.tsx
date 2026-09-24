/**
 * SettingsPage.tsx — 設定画面
 *
 * 言語切替・テーマ切替・プロフィール情報表示・メール通知設定。
 */

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router-dom';
import { useUIStore } from '@/shared/stores/uiStore';
import { useAuthStore } from '@/shared/stores/authStore';
import { useMutation } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { useToast } from '@/shared/stores/toastStore';
import { SecuritySettings } from './SecuritySettings';
import { AiSettings } from './AiSettings';
import { NotificationSettings } from './NotificationSettings';
import './SettingsPage.css';

const LANGUAGES = [
  { code: 'ja', label: '日本語' },
  { code: 'en', label: 'English' },
] as const;

type TabKey = 'profile' | 'notifications' | 'security' | 'ai' | 'shortcuts';

interface TabDef {
  key: TabKey;
  label: string;
  icon: string;
}

function getTabs(t: ReturnType<typeof useTranslation>['t']): TabDef[] {
  return [
    { key: 'profile', label: t('settings.tabProfile', 'プロフィール'), icon: '👤' },
    { key: 'notifications', label: t('settings.tabNotifications', '通知'), icon: '🔔' },
    { key: 'security', label: t('settings.tabSecurity', 'セキュリティ'), icon: '🔒' },
    { key: 'ai', label: t('settings.tabAi', 'AI'), icon: '🤖' },
    { key: 'shortcuts', label: t('settings.tabShortcuts', 'ショートカット'), icon: '⌨️' },
  ];
}

export function SettingsPage() {
  const { t, i18n } = useTranslation();
  const { theme, toggleTheme } = useUIStore();
  const { user, setUser, logout } = useAuthStore();
  const toast = useToast();
  const navigate = useNavigate();

  const [activeTab, setActiveTab] = useState<TabKey>('profile');
  const [isEditingProfile, setIsEditingProfile] = useState(false);
  const [showDeactivateModal, setShowDeactivateModal] = useState(false);
  const [deactivatePassword, setDeactivatePassword] = useState('');
  const [profileFormData, setProfileFormData] = useState({
    username: user?.username || '',
    firstName: user?.firstName || '',
    lastName: user?.lastName || '',
    email: user?.email || '',
    displayName: user?.displayName || '',
    alias: user?.alias || '',
  });

  const updateProfileMutation = useMutation({
    mutationFn: async (data: typeof profileFormData) => {
      const { data: updated } = await apiClient.patch('/auth/me/', {
        username: data.username || undefined,
        first_name: data.firstName || undefined,
        last_name: data.lastName || undefined,
        email: data.email || undefined,
        display_name: data.displayName || undefined,
        alias: data.alias || undefined,
      });
      return updated;
    },
    onSuccess: (updated) => {
      if (updated && typeof updated === 'object') {
        setUser({
          ...user,
          ...updated,
        });
      }
      setIsEditingProfile(false);
      toast.success(t('settings.profileUpdated', 'プロフィール更新完了'));
    },
    onError: (error: any) => {
      const errorMessage = error?.response?.data?.detail || t('settings.updateFailed');
      toast.error(errorMessage);
    },
  });

  const deactivateAccountMutation = useMutation({
    mutationFn: async (password: string) => {
      const { data } = await apiClient.post('/auth/me/deactivate/', {
        current_password: password,
      });
      return data;
    },
    onSuccess: () => {
      toast.success(t('settings.accountDeactivated', 'アカウントを削除しました'));
      setShowDeactivateModal(false);
      setDeactivatePassword('');
      // ログアウトして、ログイン画面に遷移
      logout();
      navigate('/');
    },
    onError: (error: any) => {
      const errorMessage = error?.response?.data?.detail || t('settings.deactivateFailed', 'アカウント削除に失敗しました');
      toast.error(errorMessage);
    },
  });

  const tabs = getTabs(t);

  return (
    <div className="settings" data-testid="settings-page">
      <h1 className="settings__title">{t('nav.settings', 'Settings')}</h1>

      <div className="settings__tabs" role="tablist">
        {tabs.map((tab) => (
          <button
            key={tab.key}
            type="button"
            role="tab"
            aria-selected={activeTab === tab.key}
            className={`settings__tab ${activeTab === tab.key ? 'settings__tab--active' : ''}`}
            onClick={() => setActiveTab(tab.key)}
            data-testid={`settings-tab-${tab.key}`}
          >
            <span style={{ marginRight: 6 }}>{tab.icon}</span>
            {tab.label}
          </button>
        ))}
      </div>

      <div role="tabpanel">
        {activeTab === 'profile' && (
          <>
            {/* プロフィール */}
            <section className="settings__section">
        <h2 className="settings__section-title">{t('common.profile')}</h2>
        <div className="settings__card">
          {isEditingProfile ? (
            <div className="settings__profile-form">
              <div className="settings__form-group">
                <label className="settings__form-label">{t('common.username', 'Username')}</label>
                <input
                  type="text"
                  className="settings__form-input"
                  value={profileFormData.username}
                  onChange={(e) => setProfileFormData({ ...profileFormData, username: e.target.value })}
                />
              </div>
              <div className="settings__form-group">
                <label className="settings__form-label">{t('common.firstName', 'First Name')}</label>
                <input
                  type="text"
                  className="settings__form-input"
                  value={profileFormData.firstName}
                  onChange={(e) => setProfileFormData({ ...profileFormData, firstName: e.target.value })}
                />
              </div>
              <div className="settings__form-group">
                <label className="settings__form-label">{t('common.lastName', 'Last Name')}</label>
                <input
                  type="text"
                  className="settings__form-input"
                  value={profileFormData.lastName}
                  onChange={(e) => setProfileFormData({ ...profileFormData, lastName: e.target.value })}
                />
              </div>
              <div className="settings__form-group">
                <label className="settings__form-label">{t('common.email', 'Email')}</label>
                <input
                  type="email"
                  className="settings__form-input"
                  value={profileFormData.email}
                  onChange={(e) => setProfileFormData({ ...profileFormData, email: e.target.value })}
                />
              </div>
              <div className="settings__form-group">
                <label className="settings__form-label">{t('common.displayName', 'Display Name')}</label>
                <input
                  type="text"
                  className="settings__form-input"
                  value={profileFormData.displayName}
                  onChange={(e) => setProfileFormData({ ...profileFormData, displayName: e.target.value })}
                />
              </div>
              <div className="settings__form-group">
                <label className="settings__form-label">{t('common.alias', 'ニックネーム（エイリアス）')}</label>
                <input
                  type="text"
                  className="settings__form-input"
                  value={profileFormData.alias}
                  onChange={(e) => setProfileFormData({ ...profileFormData, alias: e.target.value })}
                  placeholder="未設定"
                />
                <small style={{ marginTop: 'var(--space-1)', color: 'var(--color-text-tertiary)', display: 'block' }}>
                  コメント欄の@メンションで、表示名・ログイン名に加えてこのニックネームでも指定できます
                </small>
              </div>
              <div className="settings__form-actions">
                <button
                  className="settings__btn settings__btn--primary"
                  onClick={() => updateProfileMutation.mutate(profileFormData)}
                  disabled={updateProfileMutation.isPending}
                >
                  {updateProfileMutation.isPending ? t('common.saving', 'Saving...') : t('common.save', 'Save')}
                </button>
                <button
                  className="settings__btn settings__btn--secondary"
                  onClick={() => {
                    setIsEditingProfile(false);
                    setProfileFormData({
                      username: user?.username || '',
                      firstName: user?.firstName || '',
                      lastName: user?.lastName || '',
                      email: user?.email || '',
                      displayName: user?.displayName || '',
                      alias: user?.alias || '',
                    });
                  }}
                  disabled={updateProfileMutation.isPending}
                >
                  {t('common.cancel', 'Cancel')}
                </button>
              </div>
            </div>
          ) : (
            <>
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
              <button
                className="settings__btn settings__btn--secondary"
                onClick={() => setIsEditingProfile(true)}
              >
                {t('common.edit', 'Edit')}
              </button>
            </>
          )}
        </div>
      </section>

      {/* アカウント削除 */}
      <section className="settings__section settings__danger-zone">
        <h2 className="settings__section-title" style={{ color: 'var(--color-danger, #ef4444)' }}>
          ⚠️ {t('settings.dangerZone', 'Danger Zone')}
        </h2>
        <div className="settings__card settings__card--danger">
          <div className="settings__danger-content">
            <div>
              <h3 className="settings__danger-title">
                {t('settings.deleteAccount', 'アカウントを削除')}
              </h3>
              <p className="settings__danger-description">
                {t('settings.deleteAccountWarning', 'このアクションは取り消せません。あなたのアカウントは論理削除され、ログインできなくなります。')}
              </p>
            </div>
            <button
              className="settings__btn settings__btn--danger"
              onClick={() => setShowDeactivateModal(true)}
              disabled={deactivateAccountMutation.isPending}
            >
              {t('settings.deleteMyAccount', 'アカウントを削除する')}
            </button>
          </div>
        </div>
      </section>

      {/* アカウント削除確認モーダル */}
      {showDeactivateModal && (
        <div className="settings__modal-overlay" onClick={() => !deactivateAccountMutation.isPending && setShowDeactivateModal(false)}>
          <div className="settings__modal" onClick={(e) => e.stopPropagation()}>
            <div className="settings__modal-header">
              <h3 className="settings__modal-title">
                {t('settings.confirmDelete', 'アカウント削除の確認')}
              </h3>
              <button
                className="settings__modal-close"
                onClick={() => !deactivateAccountMutation.isPending && setShowDeactivateModal(false)}
                disabled={deactivateAccountMutation.isPending}
                aria-label="Close"
              >
                ✕
              </button>
            </div>
            <div className="settings__modal-body">
              <p className="settings__modal-warning">
                {t('settings.deleteWarningFinal', '本当にアカウントを削除しますか？このアクションは取り消せません。')}
              </p>
              <div className="settings__form-group">
                <label className="settings__form-label">
                  {t('settings.currentPassword', '現在のパスワード')}
                </label>
                <input
                  type="password"
                  className="settings__form-input"
                  placeholder={t('settings.enterPassword', 'パスワードを入力してください')}
                  value={deactivatePassword}
                  onChange={(e) => setDeactivatePassword(e.target.value)}
                  disabled={deactivateAccountMutation.isPending}
                />
              </div>
            </div>
            <div className="settings__modal-footer">
              <button
                className="settings__btn settings__btn--secondary"
                onClick={() => setShowDeactivateModal(false)}
                disabled={deactivateAccountMutation.isPending}
              >
                {t('common.cancel', 'キャンセル')}
              </button>
              <button
                className="settings__btn settings__btn--danger"
                onClick={() => deactivateAccountMutation.mutate(deactivatePassword)}
                disabled={!deactivatePassword || deactivateAccountMutation.isPending}
              >
                {deactivateAccountMutation.isPending
                  ? t('common.deleting', '削除中...')
                  : t('settings.deleteMyAccount', 'アカウントを削除する')}
              </button>
            </div>
          </div>
        </div>
      )}

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
                    🌙 {t('common.dark')}
                  </button>
                  <button
                    className={`settings__theme-btn ${theme === 'light' ? 'settings__theme-btn--active' : ''}`}
                    onClick={() => { if (theme !== 'light') toggleTheme(); }}
                  >
                    ☀️ {t('common.light')}
                  </button>
                </div>
              </div>
            </section>
          </>
        )}

        {activeTab === 'notifications' && <NotificationSettings />}

        {activeTab === 'security' && <SecuritySettings />}

        {activeTab === 'ai' && <AiSettings />}

        {activeTab === 'shortcuts' && (
          <>
            {/* キーボードショートカット */}
            <section className="settings__section">
              <h2 className="settings__section-title">{t('shortcuts.title')}</h2>
              <div className="settings__card">
                <div className="settings__shortcuts">
                  <div className="settings__shortcut">
                    <kbd>⌘K</kbd>
                    <span>{t('shortcuts.commandPalette')}</span>
                  </div>
                  <div className="settings__shortcut">
                    <kbd>J</kbd> / <kbd>K</kbd>
                    <span>{t('shortcuts.navigateTickets')}</span>
                  </div>
                  <div className="settings__shortcut">
                    <kbd>Enter</kbd>
                    <span>{t('shortcuts.openDetail')}</span>
                  </div>
                  <div className="settings__shortcut">
                    <kbd>Esc</kbd>
                    <span>{t('shortcuts.closePanel')}</span>
                  </div>
                  <div className="settings__shortcut">
                    <kbd>C</kbd>
                    <span>{t('shortcuts.createTicket')}</span>
                  </div>
                  <div className="settings__shortcut">
                    <kbd>G</kbd> then <kbd>M</kbd>
                    <span>{t('shortcuts.goToMyTickets')}</span>
                  </div>
                  <div className="settings__shortcut">
                    <kbd>G</kbd> then <kbd>I</kbd>
                    <span>{t('shortcuts.goToTickets')}</span>
                  </div>
                  <div className="settings__shortcut">
                    <kbd>G</kbd> then <kbd>H</kbd>
                    <span>{t('shortcuts.goToDashboard')}</span>
                  </div>
                  <div className="settings__shortcut">
                    <kbd>G</kbd> then <kbd>P</kbd>
                    <span>{t('shortcuts.goToProjects')}</span>
                  </div>
                  <div className="settings__shortcut">
                    <kbd>G</kbd> then <kbd>N</kbd>
                    <span>{t('shortcuts.goToNotifications')}</span>
                  </div>
                  <div className="settings__shortcut">
                    <kbd>G</kbd> then <kbd>S</kbd>
                    <span>{t('shortcuts.goToSettings')}</span>
                  </div>
                  <div className="settings__shortcut">
                    <kbd>S</kbd>
                    <span>{t('shortcuts.status')}</span>
                  </div>
                  <div className="settings__shortcut">
                    <kbd>A</kbd>
                    <span>{t('shortcuts.assignee')}</span>
                  </div>
                  <div className="settings__shortcut">
                    <kbd>P</kbd>
                    <span>{t('shortcuts.priority')}</span>
                  </div>
                  <div className="settings__shortcut">
                    <kbd>C</kbd>
                    <span>{t('shortcuts.cycle')}</span>
                  </div>
                  <div className="settings__shortcut">
                    <kbd>Shift</kbd> + <kbd>P</kbd>
                    <span>{t('shortcuts.project')}</span>
                  </div>
                  <div className="settings__shortcut">
                    <kbd>L</kbd>
                    <span>{t('shortcuts.labels')}</span>
                  </div>
                  <div className="settings__shortcut">
                    <kbd>E</kbd>
                    <span>{t('shortcuts.estimate')}</span>
                  </div>
                  <div className="settings__shortcut">
                    <kbd>Shift</kbd> + <kbd>D</kbd>
                    <span>{t('shortcuts.dueDate')}</span>
                  </div>
                  <div className="settings__shortcut">
                    <kbd>I</kbd>
                    <span>{t('shortcuts.focusTitle')}</span>
                  </div>
                  <div className="settings__shortcut">
                    <kbd>D</kbd>
                    <span>{t('shortcuts.focusDescription')}</span>
                  </div>
                </div>
              </div>
            </section>
          </>
        )}
      </div>
    </div>
  );
}

