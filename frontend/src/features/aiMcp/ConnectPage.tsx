/**
 * ConnectPage.tsx — MCP ペアリング申請ページ(DEMO-000192)
 *
 * AI Agent を使うことを申請するシンプルなページ。
 * ルート: /connect?code=XXXX-XXXX
 */

import { useSearchParams } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { useToast } from '@/shared/stores/toastStore';
import { useAuthStore } from '@/shared/stores/authStore';
import { apiErrorMessage } from './types';
import '@/features/settings/components/SettingsPage.css';

interface PairingResponse {
  code: string;
  label: string;
  status: 'waiting' | 'requested' | 'approved' | 'rejected' | 'failed' | 'expired';
  message: string | null;
  createdAt: string;
}

export function ConnectPage() {
  const { t } = useTranslation();
  const toast = useToast();
  const [searchParams] = useSearchParams();
  const queryClient = useQueryClient();
  const code = searchParams.get('code');
  const user = useAuthStore((s) => s.user);
  const actingName = user ? user.displayName || user.username : null;

  // GET /ai-mcp/pairings/{code}/
  const { data, isLoading, error, isError } = useQuery<PairingResponse>({
    queryKey: ['ai-mcp-pairing', code],
    queryFn: async () => {
      if (!code) throw new Error('No code');
      return (await apiClient.get(`/ai-mcp/pairings/${code}/`)).data;
    },
    enabled: !!code,
    // requested ステータスのときだけ 10 秒ごとにポーリング
    refetchInterval: (query) => (query.state.data?.status === 'requested' ? 10000 : false),
  });

  // POST /ai-mcp/pairings/{code}/apply/
  const applyMutation = useMutation({
    mutationFn: async () => {
      if (!code) throw new Error('No code');
      return (await apiClient.post(`/ai-mcp/pairings/${code}/apply/`)).data;
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['ai-mcp-pairing', code] });
    },
    onError: (e) => {
      toast.error(apiErrorMessage(e, t('aiMcp.connectApplyFailed')));
    },
  });

  if (!code) {
    return (
      <div style={{ display: 'flex', justifyContent: 'center', alignItems: 'center', minHeight: '100vh' }}>
        <section className="settings__card" style={{ maxWidth: '500px', textAlign: 'center' }}>
          <h1 className="settings__section-title">{t('aiMcp.connectPageTitle')}</h1>
          <p>{t('aiMcp.connectInvalidLink')}</p>
        </section>
      </div>
    );
  }

  if (isLoading) {
    return (
      <div style={{ display: 'flex', justifyContent: 'center', alignItems: 'center', minHeight: '100vh' }}>
        <section className="settings__card" style={{ maxWidth: '500px', textAlign: 'center' }}>
          <p>{t('common.loading')}</p>
        </section>
      </div>
    );
  }

  // 404 または 期限切れの処理
  if (isError && error && (error as unknown as { response?: { status?: number } }).response?.status === 404) {
    return (
      <div style={{ display: 'flex', justifyContent: 'center', alignItems: 'center', minHeight: '100vh' }}>
        <section className="settings__card" style={{ maxWidth: '500px', textAlign: 'center' }}>
          <h1 className="settings__section-title">{t('aiMcp.connectPageTitle')}</h1>
          <p>{t('aiMcp.connectExpiredLink')}</p>
        </section>
      </div>
    );
  }

  if (isError || !data) {
    return (
      <div style={{ display: 'flex', justifyContent: 'center', alignItems: 'center', minHeight: '100vh' }}>
        <section className="settings__card" style={{ maxWidth: '500px', textAlign: 'center' }}>
          <h1 className="settings__section-title">{t('aiMcp.connectPageTitle')}</h1>
          <p>{t('common.error')}</p>
        </section>
      </div>
    );
  }

  return (
    <div style={{ display: 'flex', justifyContent: 'center', alignItems: 'center', minHeight: '100vh' }}>
      <section className="settings__card" style={{ maxWidth: '500px' }} data-testid="connect-page">
        <h1 className="settings__section-title">{t('aiMcp.connectPageTitle')}</h1>

        {/* ラベル(Mac の名前)を含む説明文 */}
        <p style={{ marginBottom: '1.5rem' }}>
          {t('aiMcp.connectDescription', { label: data.label })}
        </p>

        {/* 各ステータスごとの表示 */}
        {data.status === 'waiting' && (
          <>
            {actingName && <p>{t('aiMcp.connectActingAs', { name: actingName })}</p>}
          <div className="admin__actions">
            <button
              type="button"
              className="admin__btn admin__btn--primary"
              disabled={applyMutation.isPending}
              onClick={() => applyMutation.mutate()}
              data-testid="connect-apply-btn"
            >
              {t('aiMcp.connectRequestButton')}
            </button>
          </div>
          </>
        )}

        {data.status === 'requested' && (
          <div data-testid="connect-requested">
            <p>{t('aiMcp.connectRequestedMessage')}</p>
          </div>
        )}

        {data.status === 'approved' && (
          <div data-testid="connect-approved">
            <p>{t('aiMcp.connectApprovedMessage')}</p>
          </div>
        )}

        {data.status === 'rejected' && (
          <div data-testid="connect-rejected">
            <p>{t('aiMcp.connectRejectedMessage')}</p>
            {data.message && (
              <p>
                {t('aiMcp.connectReasonPrefix')}
                {data.message}
              </p>
            )}
          </div>
        )}

        {data.status === 'failed' && (
          <div data-testid="connect-failed">
            <p>{t('aiMcp.connectFailedMessage')}</p>
            {data.message && (
              <p>
                {t('aiMcp.connectReasonPrefix')}
                {data.message}
              </p>
            )}
          </div>
        )}

        {data.status === 'expired' && (
          <div data-testid="connect-expired">
            <p>{t('aiMcp.connectExpiredLink')}</p>
          </div>
        )}
      </section>
    </div>
  );
}
