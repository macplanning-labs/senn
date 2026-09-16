/**
 * AdminAiSection.tsx — AI 設定（Ollama + エージェント API キー）
 */

import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { useToast } from '@/shared/stores/toastStore';
import { AiSettings } from '@/features/settings/components/AiSettings';
import '../components/AdminPage.css';
import '@/features/settings/components/SettingsPage.css';

interface AiAgentKeySettings {
  configured: boolean;
  maskedTail?: string | null;
}

export function AdminAiSection() {
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
      toast.success('AIエージェントキーを保存しました');
    },
    onError: () => {
      toast.error('AIエージェントキーの保存に失敗しました');
    },
  });

  return (
    <div data-testid="admin-ai-section">
      <section className="settings__section">
        <h2 className="settings__section-title">AI エージェント API キー</h2>
        <div className="settings__card">
          <p className="admin__hint" style={{ marginBottom: '1rem' }}>
            外部 MCP / Claude Code 等が SENN を操作する際の <code>X-AI-Api-Key</code> です。
            DB に暗号化保存され、未設定時は従来の <code>SENN_AI_API_KEY</code> 環境変数にフォールバックします。
          </p>
          <div className="admin__field">
            <div className="admin__label">現在の状態</div>
            {data?.configured ? (
              <span className="admin__status-badge admin__status-badge--ok">
                設定済み {data.maskedTail ? `(${data.maskedTail})` : ''}
              </span>
            ) : (
              <span className="admin__status-badge admin__status-badge--warn">未設定</span>
            )}
          </div>
          <div className="admin__field">
            <label className="admin__label" htmlFor="agent-key">新しいキー（上書き）</label>
            <input
              id="agent-key"
              className="admin__input"
              type="password"
              value={agentKey}
              onChange={(e) => setAgentKey(e.target.value)}
              placeholder="空欄のまま保存すると既存キーを維持"
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
              キーを保存
            </button>
          </div>
        </div>
      </section>

      <AiSettings />
    </div>
  );
}
