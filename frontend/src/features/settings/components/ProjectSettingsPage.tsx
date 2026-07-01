/**
 * ProjectSettingsPage.tsx — 統合設定画面
 *
 * URL: /p/:projectKey/settings
 * タブ構成: General | Labels | Categories | Milestones | Members (Phase 2)
 *
 * Linear方式: フッターの Settings からアクセス。
 * General タブでグローバル設定（言語・テーマ）も管理。
 */

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useProject } from '@/shared/hooks/useProject';
import { useUIStore } from '@/shared/stores/uiStore';
import { useAuthStore } from '@/shared/stores/authStore';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { useTeams } from '@/features/teams/hooks/useTeams';
import { useToastStore } from '@/shared/stores/toastStore';
import { LabelSettings } from './LabelSettings';
import { CategorySettings } from './CategorySettings';
import { MilestoneSettings } from './MilestoneSettings';
import { MemberSettings } from './MemberSettings';
import { WorkflowSettings } from './WorkflowSettings';
import { IntegrationSettings } from './IntegrationSettings';
import './ProjectSettings.css';

type TabKey = 'general' | 'workflow' | 'labels' | 'categories' | 'milestones' | 'members' | 'integrations';

interface TabDef {
  key: TabKey;
  label: string;
  icon: string;
  disabled?: boolean;
}

const TABS: TabDef[] = [
  { key: 'general', label: 'General', icon: '⚙️' },
  { key: 'workflow', label: 'Workflow', icon: '🔄' },
  { key: 'labels', label: 'Labels', icon: '🏷️' },
  { key: 'categories', label: 'Categories', icon: '📂' },
  { key: 'milestones', label: 'Milestones', icon: '🎯' },
  { key: 'members', label: 'Members', icon: '👥' },
  { key: 'integrations', label: 'Integrations', icon: '🔗' },
];

const LANGUAGES = [
  { code: 'ja', label: '日本語', flag: '🇯🇵' },
  { code: 'en', label: 'English', flag: '🇺🇸' },
] as const;

/** General タブ — 言語・テーマ・プロフィール（旧 SettingsPage の内容を統合） */
function GeneralSettings() {
  const { t, i18n } = useTranslation();
  const { theme, toggleTheme } = useUIStore();
  const { user } = useAuthStore();
  const { currentProject } = useProject();
  const { data: teams } = useTeams();
  const { addToast } = useToastStore();
  const queryClient = useQueryClient();

  const updateOwnerTeam = useMutation({
    mutationFn: async (ownerTeamId: number | null) => {
      await apiClient.patch(`/projects/${currentProject?.id}/`, {
        owner_team: ownerTeamId,
      });
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['projects'] });
      addToast({ message: 'オーナーチームを更新しました', type: 'success' });
    },
    onError: () => {
      addToast({ message: 'オーナーチームの更新に失敗しました', type: 'error' });
    },
  });

  return (
    <div>
      {/* オーナーチーム */}
      {currentProject && (
        <>
          <div className="settings-section__header">
            <h2 className="settings-section__title">Owner Team</h2>
          </div>
          <div style={{ marginBottom: 'var(--space-6)' }}>
            <select
              className="settings-form__select"
              value={(currentProject as any).ownerTeam?.id ?? ''}
              onChange={(e) => {
                const val = e.target.value;
                updateOwnerTeam.mutate(val ? Number(val) : null);
              }}
              data-testid="owner-team-select"
              style={{
                width: '100%',
                maxWidth: 320,
                padding: 'var(--space-2) var(--space-3)',
                background: 'var(--color-bg-elevated)',
                border: '1px solid var(--color-border-default)',
                borderRadius: 'var(--radius-md)',
                color: 'var(--color-text-primary)',
                fontSize: 'var(--font-size-sm)',
              }}
            >
              <option value="">— None</option>
              {(teams ?? []).map((team) => (
                <option key={team.id} value={team.id}>
                  {team.icon} {team.name}
                </option>
              ))}
            </select>
          </div>
        </>
      )}

      {/* プロフィール */}
      <div className="settings-section__header">
        <h2 className="settings-section__title">Profile</h2>
      </div>
      <div style={{ marginBottom: 'var(--space-6)' }}>
        <div className="settings__card" style={{
          background: 'var(--color-bg-elevated)',
          border: '1px solid var(--color-border-default)',
          borderRadius: 'var(--radius-lg)',
          padding: 'var(--space-4)',
        }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-3)' }}>
            <span style={{
              width: 48, height: 48, borderRadius: '50%',
              background: 'linear-gradient(135deg, var(--color-accent-primary), #a78bfa)',
              color: 'white', display: 'flex', alignItems: 'center', justifyContent: 'center',
              fontSize: 'var(--font-size-lg)', fontWeight: 'var(--font-weight-bold)',
            }}>
              {(user?.firstName || user?.username || '?')[0]?.toUpperCase()}
            </span>
            <div>
              <div style={{ fontWeight: 'var(--font-weight-semibold)', color: 'var(--color-text-primary)' }}>
                {user?.firstName ? `${user.firstName} ${user.lastName}` : user?.username}
              </div>
              <div style={{ fontSize: 'var(--font-size-sm)', color: 'var(--color-text-tertiary)' }}>
                {user?.email}
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* 言語 */}
      <div className="settings-section__header">
        <h2 className="settings-section__title">{t('settings.language', 'Language')}</h2>
      </div>
      <div style={{ display: 'flex', gap: 'var(--space-2)', marginBottom: 'var(--space-6)' }}>
        {LANGUAGES.map((lang) => (
          <button
            key={lang.code}
            className={`settings-form__btn ${
              i18n.language === lang.code ? 'settings-form__btn--primary' : 'settings-form__btn--secondary'
            }`}
            onClick={() => void i18n.changeLanguage(lang.code)}
            style={{ flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center', gap: 8 }}
          >
            <span>{lang.flag}</span> {lang.label}
          </button>
        ))}
      </div>

      {/* テーマ */}
      <div className="settings-section__header">
        <h2 className="settings-section__title">{t('settings.theme', 'Theme')}</h2>
      </div>
      <div style={{ display: 'flex', gap: 'var(--space-2)', marginBottom: 'var(--space-6)' }}>
        <button
          className={`settings-form__btn ${theme === 'dark' ? 'settings-form__btn--primary' : 'settings-form__btn--secondary'}`}
          onClick={() => { if (theme !== 'dark') toggleTheme(); }}
          style={{ flex: 1 }}
        >
          🌙 Dark
        </button>
        <button
          className={`settings-form__btn ${theme === 'light' ? 'settings-form__btn--primary' : 'settings-form__btn--secondary'}`}
          onClick={() => { if (theme !== 'light') toggleTheme(); }}
          style={{ flex: 1 }}
        >
          ☀️ Light
        </button>
      </div>

      {/* キーボードショートカット */}
      <div className="settings-section__header">
        <h2 className="settings-section__title">Keyboard Shortcuts</h2>
      </div>
      <table className="settings-table">
        <tbody>
          {[
            ['⌘K', 'Command Palette'],
            ['J / K', 'Navigate tickets'],
            ['Enter', 'Open ticket detail'],
            ['Esc', 'Close panel'],
            ['C', 'Create ticket'],
          ].map(([key, desc]) => (
            <tr key={key}>
              <td style={{ width: 80 }}>
                <kbd style={{
                  display: 'inline-flex', alignItems: 'center', justifyContent: 'center',
                  minWidth: 24, height: 24, padding: '0 6px',
                  background: 'var(--color-bg-tertiary)', border: '1px solid var(--color-border-default)',
                  borderRadius: 'var(--radius-sm)', fontFamily: 'var(--font-family-mono)',
                  fontSize: 'var(--font-size-xs)', color: 'var(--color-text-primary)',
                }}>{key}</kbd>
              </td>
              <td>{desc}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export function ProjectSettingsPage() {
  const [activeTab, setActiveTab] = useState<TabKey>('general');
  const { currentProject } = useProject();

  if (!currentProject) {
    return (
      <div className="settings-empty">
        <div className="settings-empty__icon">⚙️</div>
        <div className="settings-empty__text">プロジェクトが選択されていません</div>
      </div>
    );
  }

  return (
    <div className="project-settings" data-testid="project-settings-page">
      {/* ヘッダー */}
      <div className="project-settings__header">
        <h1 className="project-settings__title">
          Settings
          <span className="project-settings__project-name">{currentProject.name}</span>
        </h1>
      </div>

      {/* タブ */}
      <div className="project-settings__tabs" role="tablist">
        {TABS.map((tab) => (
          <button
            key={tab.key}
            role="tab"
            aria-selected={activeTab === tab.key}
            className={`project-settings__tab ${activeTab === tab.key ? 'project-settings__tab--active' : ''}`}
            onClick={() => !tab.disabled && setActiveTab(tab.key)}
            disabled={tab.disabled}
            data-testid={`tab-${tab.key}`}
            style={tab.disabled ? { opacity: 0.4, cursor: 'not-allowed' } : undefined}
          >
            <span style={{ marginRight: 6 }}>{tab.icon}</span>
            {tab.label}
            {tab.disabled && <span style={{ fontSize: '0.7em', marginLeft: 4 }}>(soon)</span>}
          </button>
        ))}
      </div>

      {/* タブコンテンツ */}
      <div role="tabpanel">
        {activeTab === 'general' && <GeneralSettings />}
        {activeTab === 'workflow' && <WorkflowSettings />}
        {activeTab === 'labels' && (
          <LabelSettings projectId={currentProject.id} />
        )}
        {activeTab === 'categories' && (
          <CategorySettings projectId={currentProject.id} />
        )}
        {activeTab === 'milestones' && (
          <MilestoneSettings projectId={currentProject.id} />
        )}
        {activeTab === 'members' && (
          <MemberSettings projectId={currentProject.id} />
        )}
        {activeTab === 'integrations' && (
          <IntegrationSettings projectId={currentProject.id} />
        )}
      </div>
    </div>
  );
}
