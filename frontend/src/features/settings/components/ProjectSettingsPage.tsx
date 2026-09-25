/**
 * ProjectSettingsPage.tsx — 統合設定画面
 *
 * URL: /project/:projectKey/settings
 * タブ構成: General | Labels | Categories | Milestones | Members (Phase 2)
 *
 * Linear方式: フッターの Settings からアクセス。
 * General タブでグローバル設定（言語・テーマ）も管理。
 */

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate, Link, useParams } from 'react-router-dom';
import { useProjectByPrefix } from '@/shared/sync/repos/projectRepo';
import { ProjectTeamsSection } from '@/features/projects/components/ProjectTeamsSection';
import { useUIStore } from '@/shared/stores/uiStore';
import { useAuthStore } from '@/shared/stores/authStore';
import { useMutation } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { localUpdateProject } from '@/shared/sync/projectWrites';
import { runCycle } from '@/shared/sync/syncEngine';
import { useToastStore } from '@/shared/stores/toastStore';
import { LabelSettings } from './LabelSettings';
import { CategorySettings } from './CategorySettings';
import { MilestoneSettings } from './MilestoneSettings';
import { WorkflowSettings } from './WorkflowSettings';
import { IntegrationSettings } from './IntegrationSettings';
import { ChatIntegrationSettings } from './ChatIntegrationSettings';
import { SecuritySettings } from './SecuritySettings';
import { HolidaySettings } from './HolidaySettings';
import './ProjectSettings.css';

type TabKey = 'general' | 'workflow' | 'labels' | 'categories' | 'milestones' | 'holidays' | 'members' | 'integrations' | 'security';

interface TabDef {
  key: TabKey;
  label: string;
  icon: string;
  disabled?: boolean;
}

function getTabs(t: (key: string) => string): TabDef[] {
  return [
    { key: 'general', label: t('settings.general'), icon: '⚙️' },
    { key: 'workflow', label: t('settings.workflow'), icon: '🔄' },
    { key: 'labels', label: t('settings.labels'), icon: '🏷️' },
    { key: 'categories', label: t('settings.categories'), icon: '📂' },
    { key: 'milestones', label: t('settings.milestones'), icon: '🎯' },
    { key: 'holidays', label: t('settings.holidays'), icon: '🗓️' },
    { key: 'members', label: t('settings.members'), icon: '👥' },
    { key: 'integrations', label: t('settings.integrations'), icon: '🔗' },
    { key: 'security', label: t('settings.security'), icon: '🔒' },
  ];
}

const LANGUAGES = [
  { code: 'ja', label: '日本語', flag: '🇯🇵' },
  { code: 'en', label: 'English', flag: '🇺🇸' },
] as const;

/** General タブ — 言語・テーマ・プロフィール（旧 SettingsPage の内容を統合） */
function GeneralSettings({ currentProject }: { currentProject: ReturnType<typeof useProjectByPrefix> }) {
  const { t, i18n } = useTranslation();
  const navigate = useNavigate();
  const { theme, toggleTheme } = useUIStore();
  const { user } = useAuthStore();
  const { addToast } = useToastStore();
  const [descriptionEdit, setDescriptionEdit] = useState(false);
  const [descriptionValue, setDescriptionValue] = useState(currentProject?.description ?? '');
  const [deleteConfirm, setDeleteConfirm] = useState(false);
  const [cycleAutoComplete, setCycleAutoComplete] = useState(currentProject?.cycleAutoComplete ?? true);
  const [cycleAutoCreateNext, setCycleAutoCreateNext] = useState(currentProject?.cycleAutoCreateNext ?? true);

  const canDeleteProject =
    !!user &&
    (user.isStaff || currentProject?.ownerId === user.id);


  const updateDescription = useMutation({
    mutationFn: async (description: string) => {
      await apiClient.patch(`/projects/${currentProject?.id}/`, {
        description,
      });
    },
    onSuccess: () => {
      // invalidateQueries は不要（端末内 DB は localUpdateProject() で更新される）
      setDescriptionEdit(false);
      addToast({ message: '説明を更新しました', type: 'success' });
    },
    onError: () => {
      addToast({ message: '説明の更新に失敗しました', type: 'error' });
    },
  });

  const updateCycleSettings = useMutation({
    mutationFn: async (payload: {
      cycleAutoComplete?: boolean;
      cycleAutoCreateNext?: boolean;
    }) => {
      await localUpdateProject(currentProject!.id, payload, payload);
    },
    onSuccess: () => {
      addToast({ message: 'サイクル設定を更新しました', type: 'success' });
    },
    onError: () => {
      addToast({ message: 'サイクル設定の更新に失敗しました', type: 'error' });
    },
  });

  const deleteProject = useMutation({
    mutationFn: async () => {
      await apiClient.delete(`/projects/${currentProject?.id}/`);
    },
    onSuccess: () => {
      addToast({ message: t('settings.projectDeleted'), type: 'success' });
      setDeleteConfirm(false);
      // invalidateQueries は不要（端末内 DB は自動更新される）
      // サーバー直のままにした理由：拒否された理由を画面で出しているため
      void runCycle();
      navigate('/my-issues');
    },
    onError: (error: unknown) => {
      const axiosErr = error as { response?: { status?: number; data?: { detail?: string } } };
      const detail = axiosErr.response?.data?.detail;
      if (detail) {
        addToast({ message: detail, type: 'error' });
      } else if (axiosErr.response?.status === 409) {
        addToast({ message: 'チケットが存在するプロジェクトは削除できません', type: 'error' });
      } else {
        addToast({ message: t('settings.deleteProjectFailed'), type: 'error' });
      }
    },
  });

  return (
    <div>
      {/* 参加チーム(追加・外す) */}
      {currentProject && <ProjectTeamsSection projectId={currentProject.id} />}

      {/* サイクル設定 */}
      {currentProject && (
        <>
          <div className="settings-section__header">
            <h2 className="settings-section__title">Cycle Settings</h2>
          </div>
          <div style={{ marginBottom: 'var(--space-6)' }}>
            <div style={{ marginBottom: 'var(--space-4)' }}>
              <label style={{
                display: 'flex', alignItems: 'center', gap: 'var(--space-3)',
                cursor: 'pointer',
                userSelect: 'none',
              }}>
                <input
                  type="checkbox"
                  checked={cycleAutoComplete}
                  onChange={(e) => {
                    const newVal = e.target.checked;
                    setCycleAutoComplete(newVal);
                    updateCycleSettings.mutate({ cycleAutoComplete: newVal });
                  }}
                  disabled={updateCycleSettings.isPending}
                  style={{ cursor: 'pointer', width: 18, height: 18 }}
                />
                <div>
                  <div style={{ fontWeight: 'var(--font-weight-semibold)', color: 'var(--color-text-primary)' }}>
                    {t('settings.cycleAutoComplete')}
                  </div>
                  <div style={{ fontSize: 'var(--font-size-sm)', color: 'var(--color-text-tertiary)', marginTop: 'var(--space-1)' }}>
                    {t('settings.cycleAutoCompleteDesc')}
                  </div>
                </div>
              </label>
            </div>
            <div>
              <label style={{
                display: 'flex', alignItems: 'center', gap: 'var(--space-3)',
                cursor: 'pointer',
                userSelect: 'none',
              }}>
                <input
                  type="checkbox"
                  checked={cycleAutoCreateNext}
                  onChange={(e) => {
                    const newVal = e.target.checked;
                    setCycleAutoCreateNext(newVal);
                    updateCycleSettings.mutate({ cycleAutoCreateNext: newVal });
                  }}
                  disabled={updateCycleSettings.isPending}
                  style={{ cursor: 'pointer', width: 18, height: 18 }}
                />
                <div>
                  <div style={{ fontWeight: 'var(--font-weight-semibold)', color: 'var(--color-text-primary)' }}>
                    {t('settings.cycleAutoCreateNext')}
                  </div>
                  <div style={{ fontSize: 'var(--font-size-sm)', color: 'var(--color-text-tertiary)', marginTop: 'var(--space-1)' }}>
                    {t('settings.cycleAutoCreateNextDesc')}
                  </div>
                </div>
              </label>
            </div>
          </div>
        </>
      )}

      {/* プロジェクト説明 */}
      {currentProject && (
        <>
          <div className="settings-section__header">
            <h2 className="settings-section__title">{t('settings.description')}</h2>
          </div>
          <div style={{ marginBottom: 'var(--space-6)' }}>
            {!descriptionEdit ? (
              <div
                onClick={() => {
                  setDescriptionValue(currentProject.description ?? '');
                  setDescriptionEdit(true);
                }}
                style={{
                  padding: 'var(--space-3)',
                  background: 'var(--color-bg-elevated)',
                  border: '1px solid var(--color-border-default)',
                  borderRadius: 'var(--radius-md)',
                  color: currentProject.description ? 'var(--color-text-primary)' : 'var(--color-text-tertiary)',
                  fontSize: 'var(--font-size-sm)',
                  minHeight: 80,
                  cursor: 'pointer',
                  whiteSpace: 'pre-wrap',
                  wordWrap: 'break-word',
                }}
                data-testid="description-display"
              >
                {currentProject.description || t('settings.descriptionPlaceholder')}
              </div>
            ) : (
              <div>
                <textarea
                  className="settings-form__textarea"
                  value={descriptionValue}
                  onChange={(e) => setDescriptionValue(e.target.value)}
                  placeholder={t('settings.descriptionPlaceholder')}
                  data-testid="description-input"
                  style={{
                    width: '100%',
                    minHeight: 120,
                    padding: 'var(--space-3)',
                    background: 'var(--color-bg-elevated)',
                    border: '1px solid var(--color-border-default)',
                    borderRadius: 'var(--radius-md)',
                    color: 'var(--color-text-primary)',
                    fontSize: 'var(--font-size-sm)',
                    fontFamily: 'inherit',
                  }}
                />
                <div
                  style={{
                    display: 'flex',
                    gap: 'var(--space-2)',
                    marginTop: 'var(--space-3)',
                  }}
                >
                  <button
                    className="settings-form__btn settings-form__btn--secondary"
                    onClick={() => setDescriptionEdit(false)}
                    disabled={updateDescription.isPending}
                  >
                    {t('common.cancel')}
                  </button>
                  <button
                    className="settings-form__btn settings-form__btn--primary"
                    onClick={() => updateDescription.mutate(descriptionValue)}
                    disabled={updateDescription.isPending}
                    data-testid="description-save-btn"
                  >
                    {updateDescription.isPending ? t('common.loading') : t('common.save')}
                  </button>
                </div>
              </div>
            )}
          </div>
        </>
      )}

      {/* プロフィール */}
      <div className="settings-section__header">
        <h2 className="settings-section__title">{t('common.profile')}</h2>
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

      {/* 危険エリア — プロジェクト削除 */}
      {currentProject && canDeleteProject && (
        <>
          <div
            style={{
              marginTop: 'var(--space-8)',
              paddingTop: 'var(--space-6)',
              borderTop: '1px solid var(--color-border-default)',
            }}
          >
            <div className="settings-section__header">
              <h2 className="settings-section__title" style={{ color: 'var(--color-error)' }}>
                ⚠️ Danger Zone
              </h2>
            </div>
            <div
              style={{
                padding: 'var(--space-4)',
                background: 'var(--color-bg-elevated)',
                border: '1px solid var(--color-error)',
                borderRadius: 'var(--radius-md)',
                marginBottom: 'var(--space-6)',
              }}
            >
              <div style={{ marginBottom: 'var(--space-3)' }}>
                <h3 style={{ color: 'var(--color-text-primary)', marginBottom: 'var(--space-1)' }}>
                  {t('settings.deleteProject')}
                </h3>
                <p style={{ fontSize: 'var(--font-size-sm)', color: 'var(--color-text-tertiary)' }}>
                  プロジェクトを完全に削除します。この操作は取り消せません。
                  {user?.isStaff && currentProject.ownerId !== user.id && (
                    <> 管理者権限で削除します。</>
                  )}
                </p>
              </div>
              <button
                className="settings-form__btn settings-form__btn--danger"
                onClick={() => setDeleteConfirm(true)}
                disabled={deleteProject.isPending}
                data-testid="delete-project-btn"
                style={{
                  background: 'var(--color-error)',
                  color: 'white',
                  border: 'none',
                  padding: 'var(--space-2) var(--space-3)',
                  borderRadius: 'var(--radius-md)',
                  cursor: deleteProject.isPending ? 'not-allowed' : 'pointer',
                  opacity: deleteProject.isPending ? 0.6 : 1,
                }}
              >
                {deleteProject.isPending ? '削除中...' : t('settings.deleteProject')}
              </button>
            </div>
          </div>
        </>
      )}

      {/* プロジェクト削除確認ダイアログ */}
      {deleteConfirm && currentProject && (
        <div className="settings-modal__overlay" onClick={() => setDeleteConfirm(false)}>
          <div className="settings-modal" onClick={(e) => e.stopPropagation()}>
            <div className="settings-modal__header">
              <h3 className="settings-modal__title">{t('settings.deleteProject')}</h3>
              <button className="settings-modal__close" onClick={() => setDeleteConfirm(false)}>×</button>
            </div>
            <div className="confirm-dialog__message">
              {t('settings.deleteProjectConfirm', { name: currentProject.name })}
            </div>
            <div className="confirm-dialog__warning">
              ⚠️ {t('settings.deleteProjectWarning')}
            </div>
            <div className="settings-form__actions">
              <button
                className="settings-form__btn settings-form__btn--secondary"
                onClick={() => setDeleteConfirm(false)}
              >
                {t('common.cancel')}
              </button>
              <button
                className="settings-form__btn settings-form__btn--danger"
                onClick={() => deleteProject.mutate()}
                disabled={deleteProject.isPending}
                data-testid="delete-project-confirm-btn"
              >
                {deleteProject.isPending ? '削除中...' : t('settings.deleteProject')}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export function ProjectSettingsPage() {
  const { t } = useTranslation();
  const { projectKey } = useParams<{ projectKey: string }>();
  const [activeTab, setActiveTab] = useState<TabKey>('general');
  const currentProject = useProjectByPrefix(projectKey);
  const tabs = getTabs(t);

  if (!currentProject) {
    return (
      <div className="settings-empty">
        <div className="settings-empty__icon">⚙️</div>
        <div className="settings-empty__text">{t('settings.noProject')}</div>
      </div>
    );
  }

  return (
    <div className="project-settings" data-testid="project-settings-page">
      {/* ヘッダー */}
      <div className="project-settings__header">
        <h1 className="project-settings__title">
          {t('nav.settings')}
          <span className="project-settings__project-name">{currentProject.name}</span>
        </h1>
      </div>

      {/* タブ */}
      <div className="project-settings__tabs" role="tablist">
        {tabs.map((tab) => (
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
        {activeTab === 'general' && <GeneralSettings currentProject={currentProject} />}
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
        {activeTab === 'holidays' && (
          <HolidaySettings />
        )}
        {activeTab === 'members' && (
          <div className="settings-empty">
            <div className="settings-empty__icon">👥</div>
            <div className="settings-empty__text">
              メンバー管理は Team 設定に統合されました。所属 Team のメンバー・
              Project 限定ゲストは<Link to="/teams">チーム設定</Link>
              から管理してください。
            </div>
          </div>
        )}
        {activeTab === 'integrations' && (
          <div>
            <IntegrationSettings projectId={currentProject.id} />
            <ChatIntegrationSettings projectId={currentProject.id} />
          </div>
        )}
        {activeTab === 'security' && <SecuritySettings />}
      </div>
    </div>
  );
}
