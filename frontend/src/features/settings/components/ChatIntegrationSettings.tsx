/**
 * ChatIntegrationSettings.tsx — チャット通知連携設定UI
 *
 * Project または Team 設定の Integrations タブ。
 * Slack/Google Chat/Chatwork/Teams Webhook連携の追加・削除・カテゴリ選択。
 */

import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import type { ChatIntegration, ChatProvider } from '@/shared/api/types';
import { useToast } from '@/shared/stores/toastStore';
import './IntegrationSettings.css';
import { useTranslation } from 'react-i18next';

interface ChatIntegrationSettingsProps {
  projectId?: number;
  teamId?: number;
}

const CATEGORY_META: { key: string; icon: string; labelKey: string; fallback: string }[] = [
  { key: 'assigned', icon: '👤', labelKey: 'settings.assignedToMe', fallback: '担当設定' },
  { key: 'commented', icon: '💬', labelKey: 'settings.commentsOnTickets', fallback: 'コメント' },
  { key: 'status_changed', icon: '🔄', labelKey: 'settings.statusChanges', fallback: 'ステータス変更' },
  { key: 'updated', icon: '📝', labelKey: 'settings.ticketUpdated', fallback: '更新' },
  { key: 'mentioned', icon: '📢', labelKey: 'settings.mentioned', fallback: 'メンション' },
  { key: 'review_requested', icon: '👁️', labelKey: 'settings.reviewRequested', fallback: 'レビュー依頼' },
  { key: 'due_soon', icon: '⏰', labelKey: 'settings.dueReminders', fallback: '期限間近' },
  { key: 'overdue', icon: '🔥', labelKey: 'settings.overdue', fallback: '期限超過' },
];

export function ChatIntegrationSettings({ projectId, teamId }: ChatIntegrationSettingsProps) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const toast = useToast();
  const [showAddForm, setShowAddForm] = useState(false);
  const [formData, setFormData] = useState({
    provider: 'slack' as ChatProvider,
    webhook_url: '',
    api_token: '',
    room_id: '',
    enabled_categories: ['assigned'],
  });
  const [revealedSecrets, setRevealedSecrets] = useState<Set<number>>(new Set());

  const scopeKey = projectId != null ? `chat-project:${projectId}` : `chat-team:${teamId}`;
  const listQuery =
    projectId != null
      ? `/chat-integrations/?project=${projectId}`
      : `/chat-integrations/?team=${teamId}`;

  const { data: integrations = [], isLoading } = useQuery<ChatIntegration[]>({
    queryKey: ['chat-integrations', scopeKey],
    queryFn: async () => {
      const res = await apiClient.get(listQuery);
      return res.data;
    },
    enabled: projectId != null || teamId != null,
  });

  const createMutation = useMutation({
    mutationFn: async (data: typeof formData & { project?: number; team?: number }) => {
      const res = await apiClient.post('/chat-integrations/', data);
      return res.data;
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['chat-integrations', scopeKey] });
      setShowAddForm(false);
      setFormData({
        provider: 'slack',
        webhook_url: '',
        api_token: '',
        room_id: '',
        enabled_categories: ['assigned'],
      });
      toast.success(t('integration.added'));
    },
    onError: (error: any) => {
      toast.error(error.response?.data?.detail || t('integration.addFailed'));
    },
  });

  const deleteMutation = useMutation({
    mutationFn: async (id: number) => {
      await apiClient.delete(`/chat-integrations/${id}/`);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['chat-integrations', scopeKey] });
      toast.success(t('integration.deleted'));
    },
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (projectId != null) {
      createMutation.mutate({ ...formData, project: projectId });
    } else if (teamId != null) {
      createMutation.mutate({ ...formData, team: teamId });
    }
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

  const toggleCategory = (key: string) => {
    setFormData((prev) => {
      const updated = prev.enabled_categories.includes(key)
        ? prev.enabled_categories.filter((c) => c !== key)
        : [...prev.enabled_categories, key];
      return { ...prev, enabled_categories: updated };
    });
  };

  if (isLoading) {
    return <div className="integration-loading">Loading...</div>;
  }

  return (
    <div className="integration-settings">
      <div className="settings-section__header">
        <h2 className="settings-section__title">Chat Integrations</h2>
        <button
          className="settings-form__btn settings-form__btn--primary"
          onClick={() => setShowAddForm(!showAddForm)}
        >
          {showAddForm ? '✕ Cancel' : '+ Add Integration'}
        </button>
      </div>

      {/* 追加フォーム */}
      {showAddForm && (
        <form className="integration-form" onSubmit={handleSubmit}>
          <div className="integration-form__field">
            <label>Provider</label>
            <select
              value={formData.provider}
              onChange={(e) => setFormData({ ...formData, provider: e.target.value as ChatProvider })}
            >
              <option value="slack">Slack</option>
              <option value="google_chat">Google Chat</option>
              <option value="teams">Microsoft Teams</option>
              <option value="chatwork">Chatwork</option>
            </select>
          </div>

          {/* Provider依存の入力欄 */}
          {formData.provider === 'chatwork' ? (
            <>
              <div className="integration-form__field">
                <label>API Token</label>
                <input
                  type="password"
                  placeholder="your-api-token"
                  value={formData.api_token}
                  onChange={(e) => setFormData({ ...formData, api_token: e.target.value })}
                  required
                />
              </div>
              <div className="integration-form__field">
                <label>Room ID</label>
                <input
                  type="text"
                  placeholder="123456789"
                  value={formData.room_id}
                  onChange={(e) => setFormData({ ...formData, room_id: e.target.value })}
                  required
                />
              </div>
            </>
          ) : (
            <div className="integration-form__field">
              <label>Webhook URL</label>
              <input
                type="url"
                placeholder="https://hooks.slack.com/..."
                value={formData.webhook_url}
                onChange={(e) => setFormData({ ...formData, webhook_url: e.target.value })}
                required
              />
            </div>
          )}

          {/* カテゴリ選択 */}
          <div className="integration-form__field">
            <label>通知対象イベント</label>
            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 'var(--space-3)', marginTop: 'var(--space-2)' }}>
              {CATEGORY_META.map((cat) => (
                <label key={cat.key} style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-2)', cursor: 'pointer' }}>
                  <input
                    type="checkbox"
                    checked={formData.enabled_categories.includes(cat.key)}
                    onChange={() => toggleCategory(cat.key)}
                    style={{ width: 18, height: 18, cursor: 'pointer' }}
                  />
                  <span>{cat.icon} {t(cat.labelKey, cat.fallback)}</span>
                </label>
              ))}
            </div>
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
          <div className="integration-empty__icon">💬</div>
          <p>{t('integration.noIntegrations')}</p>
          <p className="integration-empty__hint">
            Slack、Google Chat、Chatwork、Microsoft Teamsへの通知設定
          </p>
        </div>
      ) : (
        <div className="integration-list">
          {integrations.map((integration) => (
            <div key={integration.id} className="integration-card">
              <div className="integration-card__header">
                <span className="integration-card__provider">
                  {integration.provider === 'slack' && '💜'}
                  {integration.provider === 'google_chat' && '💙'}
                  {integration.provider === 'teams' && '🔷'}
                  {integration.provider === 'chatwork' && '🟠'}
                  {integration.provider.charAt(0).toUpperCase() + integration.provider.slice(1).replace('_', ' ')}
                </span>
                <span className={`integration-card__status ${integration.isActive ? 'integration-card__status--active' : ''}`}>
                  {integration.isActive ? '● Active' : '○ Inactive'}
                </span>
              </div>

              {/* Webhook URL または API Token + Room ID */}
              {integration.provider === 'chatwork' ? (
                <>
                  <div className="integration-card__secret">
                    <span className="integration-card__secret-label">API Token:</span>
                    <code>
                      {revealedSecrets.has(integration.id)
                        ? integration.apiToken
                        : '••••••••••••'}
                    </code>
                    <button
                      className="integration-card__secret-toggle"
                      onClick={() => toggleSecret(integration.id)}
                    >
                      {revealedSecrets.has(integration.id) ? '🙈' : '👁️'}
                    </button>
                    {integration.apiToken && (
                      <button
                        className="integration-card__secret-toggle"
                        onClick={() => void handleCopy(integration.apiToken || '', 'API Token')}
                      >
                        📋
                      </button>
                    )}
                  </div>
                  <div className="integration-card__secret">
                    <span className="integration-card__secret-label">Room ID:</span>
                    <code>{integration.roomId}</code>
                    {integration.roomId && (
                      <button
                        className="integration-card__secret-toggle"
                        onClick={() => void handleCopy(integration.roomId || '', 'Room ID')}
                      >
                        📋
                      </button>
                    )}
                  </div>
                </>
              ) : (
                <div className="integration-card__secret">
                  <span className="integration-card__secret-label">Webhook URL:</span>
                  <code>
                    {revealedSecrets.has(integration.id)
                      ? integration.webhookUrl
                      : '••••••••••••'}
                  </code>
                  <button
                    className="integration-card__secret-toggle"
                    onClick={() => toggleSecret(integration.id)}
                  >
                    {revealedSecrets.has(integration.id) ? '🙈' : '👁️'}
                  </button>
                  {integration.webhookUrl && (
                    <button
                      className="integration-card__secret-toggle"
                      onClick={() => void handleCopy(integration.webhookUrl || '', 'Webhook URL')}
                    >
                      📋
                    </button>
                  )}
                </div>
              )}

              {/* 通知対象イベント */}
              <div style={{ marginTop: 'var(--space-4)', marginBottom: 'var(--space-4)' }}>
                <div style={{ fontWeight: 'var(--font-weight-semibold)', color: 'var(--color-text-primary)', marginBottom: 'var(--space-2)' }}>
                  通知対象イベント
                </div>
                <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 'var(--space-2)', fontSize: 'var(--font-size-sm)' }}>
                  {CATEGORY_META.map((cat) => (
                    <div key={cat.key} style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-2)' }}>
                      <span style={{ opacity: integration.enabledCategories.includes(cat.key) ? 1 : 0.5 }}>
                        {integration.enabledCategories.includes(cat.key) ? '☑️' : '☐'}
                      </span>
                      <span style={{ opacity: integration.enabledCategories.includes(cat.key) ? 1 : 0.5 }}>
                        {cat.icon} {t(cat.labelKey, cat.fallback)}
                      </span>
                    </div>
                  ))}
                </div>
              </div>

              <div className="integration-card__footer">
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
