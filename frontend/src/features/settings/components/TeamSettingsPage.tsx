/**
 * TeamSettingsPage.tsx — Team の統一設定画面
 *
 * URL: /team/:teamSlug/settings
 * メンバー / ゲスト / ルール / ワークフロー / ラベル / 連携 を1画面に統一。
 */

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useTeam } from '@/shared/hooks/useTeam';
import { WorkflowSettings } from './WorkflowSettings';
import { LabelSettings } from './LabelSettings';
import { IntegrationSettings } from './IntegrationSettings';
import { ChatIntegrationSettings } from './ChatIntegrationSettings';
import { TeamMembersSection } from '@/features/teams/components/TeamMembersSection';
import { TeamGuestsSection } from '@/features/teams/components/TeamGuestsSection';
import { TeamRulesSection } from '@/features/teams/components/TeamRulesSection';
import { TeamGeneralSection } from './TeamGeneralSection';
import './ProjectSettings.css';
import { TeamTabPageHeader } from '@/features/teams/components/TeamTabPageHeader';
import { IconSettings } from '@/shared/components/layout/Sidebar';

type TabKey = 'general' | 'members' | 'guests' | 'rules' | 'workflow' | 'labels' | 'integrations';

export function TeamSettingsPage() {
  const { t } = useTranslation();
  const { currentTeam, isLoading } = useTeam();
  const [activeTab, setActiveTab] = useState<TabKey>('general');

  if (isLoading) {
    return <p className="project-settings__empty">Loading...</p>;
  }
  if (!currentTeam) {
    return <p className="project-settings__empty">{t('settings.selectProject')}</p>;
  }

  return (
    <div className="project-settings">
      <TeamTabPageHeader
        icon={IconSettings}
        title={t('nav.settings')}
        subtitle={<p className="project-settings__desc">{currentTeam.name}</p>}
      />

      <div className="project-settings__tabs">
        <button
          type="button"
          className={`project-settings__tab ${activeTab === 'general' ? 'project-settings__tab--active' : ''}`}
          onClick={() => setActiveTab('general')}
          data-testid="tab-general"
        >
          ⚙️ {t('settings.general')}
        </button>
        <button
          type="button"
          className={`project-settings__tab ${activeTab === 'members' ? 'project-settings__tab--active' : ''}`}
          onClick={() => setActiveTab('members')}
          data-testid="tab-members"
        >
          👤 メンバー
        </button>
        <button
          type="button"
          className={`project-settings__tab ${activeTab === 'guests' ? 'project-settings__tab--active' : ''}`}
          onClick={() => setActiveTab('guests')}
          data-testid="tab-guests"
        >
          🔑 ゲスト
        </button>
        <button
          type="button"
          className={`project-settings__tab ${activeTab === 'rules' ? 'project-settings__tab--active' : ''}`}
          onClick={() => setActiveTab('rules')}
          data-testid="tab-rules"
        >
          📖 ルール
        </button>
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
        {activeTab === 'general' && <TeamGeneralSection team={currentTeam} />}
        {activeTab === 'members' && <TeamMembersSection team={currentTeam} />}
        {activeTab === 'guests' && <TeamGuestsSection team={currentTeam} />}
        {activeTab === 'rules' && <TeamRulesSection teamId={currentTeam.id} />}
        {activeTab === 'workflow' && <WorkflowSettings teamId={currentTeam.id} />}
        {activeTab === 'labels' && <LabelSettings teamId={currentTeam.id} />}
        {activeTab === 'integrations' && (
          <div>
            <IntegrationSettings teamId={currentTeam.id} />
            <ChatIntegrationSettings teamId={currentTeam.id} />
          </div>
        )}
      </div>
    </div>
  );
}
