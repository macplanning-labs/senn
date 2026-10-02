/**
 * AdminAiPersonalKeysSection.tsx — 人ごとのAIエージェント用APIキー管理 (DEMO-000100)
 *
 * 全社共有の1本のキーとは別に、staffが特定ユーザー向けにAPIキーを発行できる。
 * 発行されたキーで認証されたAIエージェント経由の操作は、「実際に誰が行ったか」が
 * サーバー側で記録される(表示・編集権限への反映は tickets 側の実装を参照)。
 */

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { useToast } from '@/shared/stores/toastStore';

interface UserSummary {
  id: number;
  username: string;
  email: string;
  displayName: string;
}

interface AiAgentPersonalKey {
  id: number;
  user: UserSummary;
  keyPrefix: string;
  label: string | null;
  createdAt: string;
  lastUsedAt: string | null;
  revokedAt: string | null;
  createdBy: UserSummary | null;
}

interface UserListItem {
  id: number;
  username: string;
  displayName: string;
}

interface IssuedKeyResult {
  id: number;
  plainKey: string;
  keyPrefix: string;
}

function formatDateTime(value: string | null, notUsedLabel: string): string {
  if (!value) return notUsedLabel;
  return new Date(value).toLocaleString('ja-JP');
}

export function AdminAiPersonalKeysSection() {
  const { t } = useTranslation();
  const toast = useToast();
  const queryClient = useQueryClient();
  const [selectedUserId, setSelectedUserId] = useState('');
  const [label, setLabel] = useState('');
  const [issuedKey, setIssuedKey] = useState<IssuedKeyResult | null>(null);

  const { data: keys } = useQuery<AiAgentPersonalKey[]>({
    queryKey: ['system-admin-ai-agent-personal-keys'],
    queryFn: async () => (await apiClient.get('/system-admin/ai-agent-personal-keys/')).data,
  });

  const { data: users } = useQuery<UserListItem[]>({
    queryKey: ['users-for-ai-key-issue'],
    queryFn: async () => (await apiClient.get('/users/')).data,
  });

  const issueMutation = useMutation({
    mutationFn: async () => {
      const res = await apiClient.post<IssuedKeyResult>('/system-admin/ai-agent-personal-keys/', {
        userId: Number(selectedUserId),
        label: label.trim() || undefined,
      });
      return res.data;
    },
    onSuccess: (data) => {
      setIssuedKey(data);
      setSelectedUserId('');
      setLabel('');
      void queryClient.invalidateQueries({ queryKey: ['system-admin-ai-agent-personal-keys'] });
    },
    onError: () => {
      toast.error(t('admin.issueKeyFailed'));
    },
  });

  const revokeMutation = useMutation({
    mutationFn: async (id: number) => {
      await apiClient.delete(`/system-admin/ai-agent-personal-keys/${id}/`);
    },
    onSuccess: () => {
      toast.success(t('admin.keyRevoked'));
      void queryClient.invalidateQueries({ queryKey: ['system-admin-ai-agent-personal-keys'] });
    },
    onError: () => {
      toast.error(t('admin.revokeFailed'));
    },
  });

  return (
    <section className="settings__section" data-testid="admin-ai-personal-keys-section">
      <h2 className="settings__section-title">{t('admin.aiPersonalKeysTitle')}</h2>
      <div className="settings__card">
        <p className="admin__hint" style={{ marginBottom: '1rem' }}>
          {t('admin.aiPersonalKeysHint')}
        </p>

        <div className="admin__field">
          <label className="admin__label" htmlFor="ai-personal-key-user">{t('admin.targetUser')}</label>
          <select
            id="ai-personal-key-user"
            className="admin__input"
            value={selectedUserId}
            onChange={(e) => setSelectedUserId(e.target.value)}
          >
            <option value="">{t('common.selectPlaceholder')}</option>
            {(users ?? []).map((u) => (
              <option key={u.id} value={u.id}>
                {u.displayName || u.username}
              </option>
            ))}
          </select>
        </div>

        <div className="admin__field">
          <label className="admin__label" htmlFor="ai-personal-key-label">{t('admin.labelOptional')}</label>
          <input
            id="ai-personal-key-label"
            className="admin__input"
            type="text"
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            placeholder={t('admin.labelPlaceholderExample')}
          />
        </div>

        <div className="admin__actions">
          <button
            type="button"
            className="admin__btn admin__btn--primary"
            disabled={!selectedUserId || issueMutation.isPending}
            onClick={() => issueMutation.mutate()}
          >
            {t('admin.issueKey')}
          </button>
        </div>

        {issuedKey && (
          <div className="admin__key-issued-box" data-testid="ai-personal-key-issued">
            <div className="admin__label" style={{ color: '#ca8a04' }}>
              {t('admin.keyShownOnce')}
            </div>
            <code>{issuedKey.plainKey}</code>
            <div className="admin__actions">
              <button
                type="button"
                className="admin__btn admin__btn--secondary"
                onClick={() => {
                  void navigator.clipboard.writeText(issuedKey.plainKey);
                  toast.success(t('admin.copied'));
                }}
              >
                {t('common.copy')}
              </button>
              <button type="button" className="admin__btn admin__btn--secondary" onClick={() => setIssuedKey(null)}>
                {t('common.close')}
              </button>
            </div>
          </div>
        )}

        <table className="admin__table" style={{ marginTop: '1.5rem', width: '100%' }}>
          <thead>
            <tr>
              <th>{t('admin.colUser')}</th>
              <th>{t('admin.colLabel')}</th>
              <th>{t('admin.colKey')}</th>
              <th>{t('admin.colIssuedAt')}</th>
              <th>{t('admin.colLastUsed')}</th>
              <th>{t('admin.colStatus')}</th>
              <th aria-label={t('common.action')} />
            </tr>
          </thead>
          <tbody>
            {(keys ?? []).length === 0 ? (
              <tr>
                <td colSpan={7}>{t('admin.noKeysIssued')}</td>
              </tr>
            ) : (
              (keys ?? []).map((k) => (
                <tr key={k.id}>
                  <td>{k.user.displayName || k.user.username}</td>
                  <td>{k.label || '—'}</td>
                  <td>
                    <code>{k.keyPrefix}…</code>
                  </td>
                  <td>{new Date(k.createdAt).toLocaleDateString('ja-JP')}</td>
                  <td>{formatDateTime(k.lastUsedAt, t('admin.notUsed'))}</td>
                  <td>
                    {k.revokedAt ? (
                      <span className="admin__status-badge admin__status-badge--warn">{t('admin.revoked')}</span>
                    ) : (
                      <span className="admin__status-badge admin__status-badge--ok">{t('common.enabled')}</span>
                    )}
                  </td>
                  <td>
                    {!k.revokedAt && (
                      <button
                        type="button"
                        className="admin__btn admin__btn--danger"
                        disabled={revokeMutation.isPending}
                        onClick={() => {
                          if (window.confirm(t('admin.revokeConfirm'))) {
                            revokeMutation.mutate(k.id);
                          }
                        }}
                      >
                        {t('admin.revoke')}
                      </button>
                    )}
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </section>
  );
}
