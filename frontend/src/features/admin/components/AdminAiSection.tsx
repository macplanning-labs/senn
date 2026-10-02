/**
 * AdminAiSection.tsx — AI 設定（Ollama + エージェント API キー）
 */

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { useToast } from '@/shared/stores/toastStore';
import { AiSettings } from '@/features/settings/components/AiSettings';
import { AdminAiPersonalKeysSection } from './AdminAiPersonalKeysSection';
import '../components/AdminPage.css';
import '@/features/settings/components/SettingsPage.css';

interface AiAgentKeySettings {
  configured: boolean;
  maskedTail?: string | null;
}

export function AdminAiSection() {
  const { t } = useTranslation();
  const toast = useToast();
  const queryClient = useQueryClient();
  const [agentKey, setAgentKey] = useState('');

  const { data } = useQuery<AiAgentKeySettings>({
    queryKey: ['system-admin-ai-agent-key'],
    queryFn: async () => (await apiClient.get('/system-admin/ai-agent-key/')).data,
  });

  const saveMutation = useMutation({
    mutationFn: async (key: string) => {
      const res = await apiClient.put<AiAgentKeySettings>('/system-admin/ai-agent-key/', {
        agentKey: key,
      });
      return res.data;
    },
    onSuccess: (updated) => {
      queryClient.setQueryData(['system-admin-ai-agent-key'], updated);
      setAgentKey('');
      toast.success(t('admin.keySaved'));
    },
    onError: () => {
      toast.error(t('admin.keySaveFailed'));
    },
  });

  return (
    <div data-testid="admin-ai-section">
      <section className="settings__section">
        <h2 className="settings__section-title">{t('admin.aiAgentKeyTitle')}</h2>
        <div className="settings__card">
          <p className="admin__hint" style={{ marginBottom: '1rem' }}>
            {t('admin.aiAgentKeyHint1Lead')} <code>X-AI-Api-Key</code> {t('admin.aiAgentKeyHint1Trail')}
            {' '}
            {t('admin.aiAgentKeyHint2Lead')} <code>SENN_AI_API_KEY</code> {t('admin.aiAgentKeyHint2Trail')}
          </p>
          <div className="admin__field">
            <div className="admin__label">{t('admin.currentStatusLabel')}</div>
            {data?.configured ? (
              <span className="admin__status-badge admin__status-badge--ok">
                {t('admin.configuredStatus', { tail: data.maskedTail ? `(${data.maskedTail})` : '' })}
              </span>
            ) : (
              <span className="admin__status-badge admin__status-badge--warn">{t('admin.notConfigured')}</span>
            )}
          </div>
          <div className="admin__field">
            <label className="admin__label" htmlFor="agent-key">{t('admin.newKeyLabel')}</label>
            <input
              id="agent-key"
              className="admin__input"
              type="password"
              value={agentKey}
              onChange={(e) => setAgentKey(e.target.value)}
              placeholder={t('admin.keyPlaceholderKeepExisting')}
              autoComplete="new-password"
            />
          </div>
          <div className="admin__actions">
            <button
              type="button"
              className="admin__btn admin__btn--primary"
              disabled={saveMutation.isPending || !agentKey.trim()}
              onClick={() => saveMutation.mutate(agentKey.trim())}
            >
              {t('admin.saveKey')}
            </button>
          </div>
        </div>
      </section>

      <AdminAiPersonalKeysSection />

      <AiSettings />
    </div>
  );
}
