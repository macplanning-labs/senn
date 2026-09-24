/**
 * AdminAiPersonalKeysSection.tsx — 人ごとのAIエージェント用APIキー管理 (WIPAPPDEV-000100)
 *
 * 全社共有の1本のキーとは別に、staffが特定ユーザー向けにAPIキーを発行できる。
 * 発行されたキーで認証されたAIエージェント経由の操作は、「実際に誰が行ったか」が
 * サーバー側で記録される(表示・編集権限への反映は tickets 側の実装を参照)。
 */

import { useState } from 'react';
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

function formatDateTime(value: string | null): string {
  if (!value) return '未使用';
  return new Date(value).toLocaleString('ja-JP');
}

export function AdminAiPersonalKeysSection() {
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
      toast.error('キーの発行に失敗しました');
    },
  });

  const revokeMutation = useMutation({
    mutationFn: async (id: number) => {
      await apiClient.delete(`/system-admin/ai-agent-personal-keys/${id}/`);
    },
    onSuccess: () => {
      toast.success('キーを失効させました');
      void queryClient.invalidateQueries({ queryKey: ['system-admin-ai-agent-personal-keys'] });
    },
    onError: () => {
      toast.error('失効に失敗しました');
    },
  });

  return (
    <section className="settings__section" data-testid="admin-ai-personal-keys-section">
      <h2 className="settings__section-title">個人別 AIエージェント API キー</h2>
      <div className="settings__card">
        <p className="admin__hint" style={{ marginBottom: '1rem' }}>
          特定のメンバー用にAPIキーを発行します。このキーで認証されたAIエージェント経由の操作は、
          「誰が実際に行ったか(実行者)」として記録され、コメントの投稿者表示にも反映されます。
        </p>

        <div className="admin__field">
          <label className="admin__label" htmlFor="ai-personal-key-user">対象ユーザー</label>
          <select
            id="ai-personal-key-user"
            className="admin__input"
            value={selectedUserId}
            onChange={(e) => setSelectedUserId(e.target.value)}
          >
            <option value="">選択してください</option>
            {(users ?? []).map((u) => (
              <option key={u.id} value={u.id}>
                {u.displayName || u.username}
              </option>
            ))}
          </select>
        </div>

        <div className="admin__field">
          <label className="admin__label" htmlFor="ai-personal-key-label">ラベル（任意）</label>
          <input
            id="ai-personal-key-label"
            className="admin__input"
            type="text"
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            placeholder="例: MacBook Pro (Cursor)"
          />
        </div>

        <div className="admin__actions">
          <button
            type="button"
            className="admin__btn admin__btn--primary"
            disabled={!selectedUserId || issueMutation.isPending}
            onClick={() => issueMutation.mutate()}
          >
            キーを発行
          </button>
        </div>

        {issuedKey && (
          <div className="admin__key-issued-box" data-testid="ai-personal-key-issued">
            <div className="admin__label" style={{ color: '#ca8a04' }}>
              このキーは二度と表示されません。今すぐコピーしてください。
            </div>
            <code>{issuedKey.plainKey}</code>
            <div className="admin__actions">
              <button
                type="button"
                className="admin__btn admin__btn--secondary"
                onClick={() => {
                  void navigator.clipboard.writeText(issuedKey.plainKey);
                  toast.success('コピーしました');
                }}
              >
                コピー
              </button>
              <button type="button" className="admin__btn admin__btn--secondary" onClick={() => setIssuedKey(null)}>
                閉じる
              </button>
            </div>
          </div>
        )}

        <table className="admin__table" style={{ marginTop: '1.5rem', width: '100%' }}>
          <thead>
            <tr>
              <th>ユーザー</th>
              <th>ラベル</th>
              <th>キー</th>
              <th>発行日</th>
              <th>最終使用</th>
              <th>状態</th>
              <th aria-label="操作" />
            </tr>
          </thead>
          <tbody>
            {(keys ?? []).length === 0 ? (
              <tr>
                <td colSpan={7}>発行済みのキーはありません</td>
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
                  <td>{formatDateTime(k.lastUsedAt)}</td>
                  <td>
                    {k.revokedAt ? (
                      <span className="admin__status-badge admin__status-badge--warn">失効済み</span>
                    ) : (
                      <span className="admin__status-badge admin__status-badge--ok">有効</span>
                    )}
                  </td>
                  <td>
                    {!k.revokedAt && (
                      <button
                        type="button"
                        className="admin__btn admin__btn--danger"
                        disabled={revokeMutation.isPending}
                        onClick={() => {
                          if (window.confirm('このキーを失効させますか？以後使用できなくなります。')) {
                            revokeMutation.mutate(k.id);
                          }
                        }}
                      >
                        失効
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
