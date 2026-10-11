/**
 * IntegrationsTab.tsx — プロジェクト/チーム設定の「連携」タブの中身
 *
 * 縦に長くなりすぎないよう、目的ごとに小さく切り替える:
 *   GitHub リポジトリ(つなぐ・連携の一覧) / ステータスの自動更新(規則の表) / チャット通知
 */
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { IntegrationSettings } from './IntegrationSettings';
import { GitStatusRules } from './GitStatusRules';
import { ChatIntegrationSettings } from './ChatIntegrationSettings';
import './IntegrationsTab.css';

type SubTab = 'repos' | 'rules' | 'chat';

interface IntegrationsTabProps {
  projectId?: number;
  teamId?: number;
}

export function IntegrationsTab({ projectId, teamId }: IntegrationsTabProps) {
  const { t } = useTranslation();
  const [tab, setTab] = useState<SubTab>('repos');
  const tabs: SubTab[] = ['repos', 'rules', 'chat'];

  return (
    <div className="integrations-tab">
      <div className="integrations-tab__nav" role="tablist">
        {tabs.map((key) => (
          <button
            key={key}
            type="button"
            role="tab"
            aria-selected={tab === key}
            className={`integrations-tab__button ${tab === key ? 'integrations-tab__button--active' : ''}`}
            onClick={() => setTab(key)}
          >
            {t(`integration.tabs.${key}`)}
          </button>
        ))}
      </div>

      <div className="integrations-tab__body">
        {tab === 'repos' && <IntegrationSettings projectId={projectId} teamId={teamId} />}
        {tab === 'rules' && <GitStatusRules projectId={projectId} teamId={teamId} />}
        {tab === 'chat' && <ChatIntegrationSettings projectId={projectId} teamId={teamId} />}
      </div>
    </div>
  );
}
