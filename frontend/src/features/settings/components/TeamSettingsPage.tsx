/**
 * TeamSettingsPage.tsx — Team の統一設定画面
 *
 * URL: /team/:teamSlug/settings
 * メンバー / ゲスト / ルール / ワークフロー / ラベル / 連携 を1画面に統一。
 */

import { useState } from 'react';
import { Link } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { useTeam } from '@/shared/hooks/useTeam';
import { WorkflowSettings } from './WorkflowSettings';
import { LabelSettings } from './LabelSettings';
import { IntegrationSettings } from './IntegrationSettings';
import { ChatIntegrationSettings } from './ChatIntegrationSettings';
import { TeamMembersSection } from '@/features/teams/components/TeamMembersSection';
import { TeamGuestsSection } from '@/features/teams/components/TeamGuestsSection';
import { TeamRulesSection } from '@/features/teams/components/TeamRulesSection';
import './ProjectSettings.css';

type TabKey = 'members' | 'guests' | 'rules' | 'workflow' | 'labels' | 'integrations';

export function TeamSettingsPage() {
  const { t } = useTranslation();
  const { currentTeam, isLoading } = useTeam();
  const [activeTab, setActiveTab] = useState<TabKey>('members');

  if (isLoading) {
    return <p className="project-settings__empty">Loading...</p>;
  }
  if (!currentTeam) {
    return <p className="project-settings__empty">{t('settings.selectProject')}</p>;
  }

  return (
    <div className="project-settings">
      <header className="project-settings__header">
        <div>
          <h1 className="project-settings__title">{currentTeam.name}</h1>
          <p className="project-settings__desc">チームのメンバー・ルール・ワークフロー・連携を一元管理します</p>
        </div>
        <Link
          to="/teams"
          className="project-settings__back-link"
          style={{
            display: 'inline-block',
            padding: '0.5rem 1rem',
            background: 'var(--color-bg-secondary)',
            color: 'var(--color-text-secondary)',
            borderRadius: 'var(--radius-md)',
            textDecoration: 'none',
            fontSize: 'var(--font-size-sm)',
          }}
        >
          ← チーム一覧
        </Link>
      </header>

      <div className="project-settings__tabs">
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
