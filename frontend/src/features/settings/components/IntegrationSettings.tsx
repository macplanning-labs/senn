/**
 * IntegrationSettings.tsx — Git連携設定UI
 *
 * プロジェクト設定の Integrations タブ。
 * GitHub/GitLab Webhook連携の追加・削除・シークレット表示。
 */

import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import type { GitIntegration, GitProvider } from '@/shared/api/types';
import { useToast } from '@/shared/stores/toastStore';
import './IntegrationSettings.css';
import { useTranslation } from 'react-i18next';

interface IntegrationSettingsProps {
  projectId: number;
}

export function IntegrationSettings({ projectId }: IntegrationSettingsProps) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const toast = useToast();
  const [showAddForm, setShowAddForm] = useState(false);
  const [formData, setFormData] = useState({
    provider: 'github' as GitProvider,
    repository_url: '',
    webhook_secret: '',
  });
  const [revealedSecrets, setRevealedSecrets] = useState<Set<number>>(new Set());

  const { data: integrations = [], isLoading } = useQuery<GitIntegration[]>({
    queryKey: ['integrations', projectId],
    queryFn: async () => {
      const res = await apiClient.get(`/integrations/?project=${projectId}`);
      return res.data;
    },
  });

  const createMutation = useMutation({
    mutationFn: async (data: typeof formData & { project: number }) => {
      const res = await apiClient.post('/integrations/', data);
      return res.data;
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['integrations', projectId] });
      setShowAddForm(false);
      setFormData({ provider: 'github', repository_url: '', webhook_secret: '' });
      toast.success(t('integration.added'));
    },
    onError: () => {
      toast.error(t('integration.addFailed'));
    },
  });

  const deleteMutation = useMutation({
    mutationFn: async (id: number) => {
      await apiClient.delete(`/integrations/${id}/`);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['integrations', projectId] });
      toast.success(t('integration.deleted'));
    },
  });

  const webhookUrl = `${window.location.origin}/api/v1/webhooks/github/`;

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    createMutation.mutate({ ...formData, project: projectId });
  };

  const handleCopy = async (text: string, label: string) => {
    await navigator.clipboard.writeText(text);
    toast.success(t('common.copied', { label }));
  };

  const toggleSecret = (id: number) => {
    setRevealedSecrets((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  if (isLoading) {
    return <div className="integration-loading">Loading...</div>;
  }

  return (
    <div className="integration-settings">
      <div className="settings-section__header">
        <h2 className="settings-section__title">Git Integrations</h2>
        <button
          className="settings-form__btn settings-form__btn--primary"
          onClick={() => setShowAddForm(!showAddForm)}
        >
          {showAddForm ? '✕ Cancel' : '+ Add Integration'}
        </button>
      </div>

      {/* Webhook URL 案内 */}
      <div className="integration-info">
        <div className="integration-info__label">Webhook URL</div>
        <div className="integration-info__value">
          <code>{webhookUrl}</code>
          <button
            className="integration-info__copy"
            onClick={() => void handleCopy(webhookUrl, 'Webhook URL')}
            title="Copy"
          >
            📋
          </button>
        </div>
        <p className="integration-info__help">
          {t('integration.webhookHint')}
          Content type は <code>application/json</code>、イベントは <code>push</code> と <code>pull_request</code> を選択してください。
        </p>
      </div>

      {/* 追加フォーム */}
      {showAddForm && (
        <form className="integration-form" onSubmit={handleSubmit}>
          <div className="integration-form__field">
            <label>Provider</label>
            <select
              value={formData.provider}
              onChange={(e) => setFormData({ ...formData, provider: e.target.value as GitProvider })}
            >
              <option value="github">GitHub</option>
              <option value="gitlab">GitLab</option>
            </select>
          </div>
          <div className="integration-form__field">
            <label>Repository URL</label>
            <input
              type="url"
              placeholder="https://github.com/org/repo"
              value={formData.repository_url}
              onChange={(e) => setFormData({ ...formData, repository_url: e.target.value })}
              required
            />
          </div>
          <div className="integration-form__field">
            <label>Webhook Secret</label>
            <input
              type="text"
              placeholder="your-webhook-secret"
              value={formData.webhook_secret}
              onChange={(e) => setFormData({ ...formData, webhook_secret: e.target.value })}
              required
            />
          </div>
          <button
            type="submit"
            className="settings-form__btn settings-form__btn--primary"
            disabled={createMutation.isPending}
          >
            {createMutation.isPending ? 'Adding...' : 'Add Integration'}
          </button>
        </form>
      )}

      {/* 連携一覧 */}
      {integrations.length === 0 && !showAddForm ? (
        <div className="integration-empty">
          <div className="integration-empty__icon">🔗</div>
          <p>{t('integration.noIntegrations')}</p>
          <p className="integration-empty__hint">
            {t('integration.noIntegrationsHint')}
          </p>
        </div>
      ) : (
        <div className="integration-list">
          {integrations.map((integration) => (
            <div key={integration.id} className="integration-card">
              <div className="integration-card__header">
                <span className="integration-card__provider">
                  {integration.provider === 'github' ? '🐙' : '🦊'}
                  {integration.provider.charAt(0).toUpperCase() + integration.provider.slice(1)}
                </span>
                <span className={`integration-card__status ${integration.isActive ? 'integration-card__status--active' : ''}`}>
                  {integration.isActive ? '● Active' : '○ Inactive'}
                </span>
              </div>

              <div className="integration-card__url">
                <a href={integration.repositoryUrl} target="_blank" rel="noopener noreferrer">
                  {integration.repositoryUrl.replace('https://', '')}
                </a>
              </div>

              <div className="integration-card__secret">
                <span className="integration-card__secret-label">Secret:</span>
                <code>
                  {revealedSecrets.has(integration.id)
                    ? integration.webhookSecret
                    : '••••••••••••'}
                </code>
                <button
                  className="integration-card__secret-toggle"
                  onClick={() => toggleSecret(integration.id)}
                >
                  {revealedSecrets.has(integration.id) ? '🙈' : '👁️'}
                </button>
                <button
                  className="integration-card__secret-toggle"
                  onClick={() => void handleCopy(integration.webhookSecret, 'Secret')}
                >
                  📋
                </button>
              </div>

              <div className="integration-card__footer">
                <span className="integration-card__events">
                  {integration.eventCount} events linked
                </span>
                <button
                  className="integration-card__delete"
                  onClick={() => {
                    if (window.confirm(t('integration.deleteConfirm'))) {
                      deleteMutation.mutate(integration.id);
                    }
                  }}
                >
                  🗑️ Delete
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
