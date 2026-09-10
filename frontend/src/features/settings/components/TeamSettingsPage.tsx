/**
 * TeamSettingsPage.tsx — Team のワークフロー／ラベル設定
 *
 * URL: /t/:teamSlug/settings
 * Project 設定は残し、ここでは Team マスタだけを触る。
 */

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useTeam } from '@/shared/hooks/useTeam';
import { WorkflowSettings } from './WorkflowSettings';
import { LabelSettings } from './LabelSettings';
import { IntegrationSettings } from './IntegrationSettings';
import './ProjectSettings.css';

type TabKey = 'workflow' | 'labels' | 'integrations';

export function TeamSettingsPage() {
  const { t } = useTranslation();
  const { currentTeam, isLoading } = useTeam();
  const [activeTab, setActiveTab] = useState<TabKey>('workflow');

  if (isLoading) {
    return <p className="project-settings__empty">Loading...</p>;
  }
  if (!currentTeam) {
    return <p className="project-settings__empty">{t('settings.selectProject')}</p>;
  }

  return (
    <div className="project-settings">
      <header className="project-settings__header">
        <h1 className="project-settings__title">{currentTeam.name}</h1>
        <p className="project-settings__desc">チームのワークフロー・ラベル・Git連携</p>
      </header>

      <div className="project-settings__tabs">
        <button
          type="button"
          className={`project-settings__tab ${activeTab === 'workflow' ? 'project-settings__tab--active' : ''}`}
          onClick={() => setActiveTab('workflow')}
        >
          🔄 {t('settings.workflow')}
        </button>
        <button
          type="button"
          className={`project-settings__tab ${activeTab === 'labels' ? 'project-settings__tab--active' : ''}`}
          onClick={() => setActiveTab('labels')}
        >
          🏷️ {t('settings.labels')}
        </button>
        <button
          type="button"
          className={`project-settings__tab ${activeTab === 'integrations' ? 'project-settings__tab--active' : ''}`}
          onClick={() => setActiveTab('integrations')}
        >
          🔗 {t('settings.integrations')}
        </button>
      </div>

      <div className="project-settings__body">
        {activeTab === 'workflow' && <WorkflowSettings teamId={currentTeam.id} />}
        {activeTab === 'labels' && <LabelSettings teamId={currentTeam.id} />}
        {activeTab === 'integrations' && <IntegrationSettings teamId={currentTeam.id} />}
      </div>
    </div>
  );
}
