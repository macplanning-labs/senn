/**
 * AiMcpAccessSection.tsx — AI Agent を利用した MCP 操作の申請・キー発行(設定 > AI)(DEMO-000192)
 *
 * 申請 → 管理者が承認 → 本人がここでキーを発行する。キーは発行した直後に一度だけ表示する。
 */

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { useToast } from '@/shared/stores/toastStore';
import {
  AI_MCP_MINE_KEY,
  apiErrorMessage,
  type AiMcpMine,
  type AiMcpRequest,
  type IssuedKey,
} from './types';
import '@/features/admin/components/AdminPage.css';
import '@/features/settings/components/SettingsPage.css';

const STATUS_LABEL_KEY: Record<AiMcpRequest['status'], string> = {
  pending: 'aiMcp.statusPending',
  approved: 'aiMcp.statusApproved',
  rejected: 'aiMcp.statusRejected',
  issued: 'aiMcp.statusIssued',
  cancelled: 'aiMcp.statusCancelled',
  expired: 'aiMcp.statusExpired',
};

export function AiMcpAccessSection() {
  const { t, i18n } = useTranslation();
  const toast = useToast();
  const queryClient = useQueryClient();
  const [label, setLabel] = useState('');
  const [issuedKey, setIssuedKey] = useState<IssuedKey | null>(null);

  const fmt = (v: string | null, empty = '—') =>
    v ? new Date(v).toLocaleString(i18n.language) : empty;

  const { data } = useQuery<AiMcpMine>({
    queryKey: AI_MCP_MINE_KEY,
    queryFn: async () => (await apiClient.get('/ai-mcp/requests/')).data,
  });

  const refresh = () => queryClient.invalidateQueries({ queryKey: AI_MCP_MINE_KEY });

  const requestMutation = useMutation({
    mutationFn: async () => (await apiClient.post('/ai-mcp/requests/', {})).data,
    onSuccess: () => {
      toast.success(t('aiMcp.requestSent'));
      void refresh();
    },
    onError: (e) => toast.error(apiErrorMessage(e, t('aiMcp.requestFailed'))),
  });

  const cancelMutation = useMutation({
    mutationFn: async (id: number) => {
      await apiClient.post(`/ai-mcp/requests/${id}/cancel/`);
    },
    onSuccess: () => {
      toast.success(t('aiMcp.cancelled'));
      void refresh();
    },
    onError: (e) => toast.error(apiErrorMessage(e, t('aiMcp.cancelFailed'))),
  });

  const issueMutation = useMutation({
    mutationFn: async (id: number) =>
      (await apiClient.post<IssuedKey>(`/ai-mcp/requests/${id}/issue/`, { label: label.trim() || undefined })).data,
    onSuccess: (key) => {
      setIssuedKey(key);
      setLabel('');
      void refresh();
    },
    onError: (e) => toast.error(apiErrorMessage(e, t('aiMcp.issueFailed'))),
  });

  const revokeMutation = useMutation({
    mutationFn: async (id: number) => {
      await apiClient.delete(`/ai-mcp/keys/${id}/`);
    },
    onSuccess: () => {
      toast.success(t('aiMcp.revokedDone'));
      void refresh();
    },
    onError: (e) => toast.error(apiErrorMessage(e, t('aiMcp.revokeFailed'))),
  });

  const requests = data?.requests ?? [];
  const keys = data?.keys ?? [];
  const latest = requests[0];
  // 申請中・承認済み(期限内)の申請がある間は、新しく申請できない
  const open = requests.find((r) => r.status === 'pending' || r.status === 'approved');
  const activeKeys = keys.filter((k) => !k.revokedAt);
  // 状態の1行(普通の利用者に見せるもの)
  const statusState = activeKeys.length > 0 ? 'connected' : (open?.status ?? 'none');
  const statusText =
    activeKeys.length > 0
      ? t('aiMcp.statusConnected', { count: activeKeys.length })
      : open?.status === 'pending'
        ? t('aiMcp.statusPending')
        : open?.status === 'approved'
          ? t('aiMcp.statusApproved')
          : t('aiMcp.statusNotConnected');
  const hintText =
    activeKeys.length > 0
      ? null
      : open?.status === 'pending'
        ? t('aiMcp.pendingHint')
        : t('aiMcp.hintConnect');

  return (
    <section className="settings__section" data-testid="ai-mcp-access-section">
      <h2 className="settings__section-title">{t('aiMcp.title')}</h2>
      <div className="settings__card">
        <p className="admin__hint" style={{ marginBottom: '1rem' }}>
          {t('aiMcp.intro')}
        </p>

        {/* 普通の利用者に見せるのは、状態の1行と案内だけ。キー・手動の操作は「詳細」に入れる */}
        <div data-testid="ai-mcp-status" data-state={statusState}>
          <strong>{statusText}</strong>
          {hintText && <p style={{ marginTop: 4 }}>{hintText}</p>}
        </div>

        <details className="ai-mcp-details" style={{ marginTop: '1.5rem' }} data-testid="ai-mcp-details">
          <summary style={{ cursor: 'pointer' }}>{t('aiMcp.detailsSummary')}</summary>
          <div style={{ marginTop: '1rem' }}>
        {!latest && <p data-testid="ai-mcp-status-none">{t('aiMcp.statusNone')}</p>}

        {latest && (
          <div data-testid="ai-mcp-latest" data-status={latest.status} style={{ marginBottom: '1rem' }}>
            <strong>{t(STATUS_LABEL_KEY[latest.status])}</strong>
            <span style={{ marginLeft: 8, opacity: 0.7 }}>{fmt(latest.createdAt)}</span>
            {latest.status === 'pending' && <p>{t('aiMcp.pendingHint')}</p>}
            {latest.status === 'approved' && (
              <p>{t('aiMcp.approvedHint', { date: fmt(latest.approvalExpiresAt) })}</p>
            )}
            {latest.status === 'rejected' && (
              <p>
                {t('aiMcp.rejectedHint')}
                {latest.decisionNote && ` ${t('aiMcp.reasonPrefix')}${latest.decisionNote}`}
              </p>
            )}
            {latest.status === 'expired' && <p>{t('aiMcp.expiredHint')}</p>}
          </div>
        )}

        {open && (
          <div className="admin__actions" style={{ marginBottom: '1rem' }}>
            {open.status === 'approved' && (
              <>
                <input
                  className="admin__input"
                  type="text"
                  aria-label={t('aiMcp.labelLabel')}
                  value={label}
                  onChange={(e) => setLabel(e.target.value)}
                  placeholder={t('aiMcp.labelPlaceholder')}
                  maxLength={100}
                />
                <button
                  type="button"
                  className="admin__btn admin__btn--primary"
                  disabled={issueMutation.isPending || activeKeys.length >= (data?.maxActiveKeys ?? 3)}
                  onClick={() => issueMutation.mutate(open.id)}
                  data-testid="ai-mcp-issue"
                >
                  {t('aiMcp.issueButton')}
                </button>
              </>
            )}
            <button
              type="button"
              className="admin__btn admin__btn--secondary"
              disabled={cancelMutation.isPending}
              onClick={() => cancelMutation.mutate(open.id)}
              data-testid="ai-mcp-cancel"
            >
              {t('aiMcp.cancelButton')}
            </button>
          </div>
        )}

        {!open && (
          <div className="admin__actions" style={{ marginBottom: '1rem' }}>
            <button
              type="button"
              className="admin__btn admin__btn--primary"
              disabled={requestMutation.isPending}
              onClick={() => requestMutation.mutate()}
              data-testid="ai-mcp-request"
            >
              {t('aiMcp.requestButton')}
            </button>
          </div>
        )}

        {issuedKey && (
          <div className="admin__key-issued-box" data-testid="ai-mcp-key-issued">
            <div className="admin__label" style={{ color: '#ca8a04' }}>
              {t('aiMcp.keyShownOnce')}
            </div>
            <code>{issuedKey.plainKey}</code>
            <p className="admin__hint">{t('aiMcp.keyUsageHint')}</p>
            <div className="admin__actions">
              <button
                type="button"
                className="admin__btn admin__btn--secondary"
                onClick={() => {
                  void navigator.clipboard.writeText(issuedKey.plainKey);
                  toast.success(t('aiMcp.copied'));
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

        <h3 className="settings__section-title" style={{ marginTop: '1.5rem' }}>
          {t('aiMcp.keysTitle')}
        </h3>
        <p className="admin__hint">{t('aiMcp.keyLimit', { max: data?.maxActiveKeys ?? 3 })}</p>
        <table className="admin__table" style={{ width: '100%' }}>
          <thead>
            <tr>
              <th>{t('aiMcp.colName')}</th>
              <th>{t('aiMcp.colKey')}</th>
              <th>{t('aiMcp.colIssued')}</th>
              <th>{t('aiMcp.colLastUsed')}</th>
              <th>{t('aiMcp.colStatus')}</th>
              <th aria-label={t('common.action')} />
            </tr>
          </thead>
          <tbody>
            {keys.length === 0 ? (
              <tr>
                <td colSpan={6}>{t('aiMcp.noKeys')}</td>
              </tr>
            ) : (
              keys.map((k) => (
                <tr key={k.id} data-testid={`ai-mcp-key-${k.id}`}>
                  <td>{k.label || '—'}</td>
                  <td>
                    <code>{k.keyPrefix}…</code>
                  </td>
                  <td>{new Date(k.createdAt).toLocaleDateString(i18n.language)}</td>
                  <td>{fmt(k.lastUsedAt, t('aiMcp.notUsed'))}</td>
                  <td>{k.revokedAt ? t('aiMcp.keyRevoked') : t('aiMcp.keyActive')}</td>
                  <td>
                    {!k.revokedAt && (
                      <button
                        type="button"
                        className="admin__btn admin__btn--danger"
                        disabled={revokeMutation.isPending}
                        onClick={() => {
                          if (window.confirm(t('aiMcp.revokeConfirm'))) revokeMutation.mutate(k.id);
                        }}
                      >
                        {t('aiMcp.revokeButton')}
                      </button>
                    )}
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>

        {requests.length > 1 && (
          <>
            <h3 className="settings__section-title" style={{ marginTop: '1.5rem' }}>
              {t('aiMcp.historyTitle')}
            </h3>
            <ul>
              {requests.map((r) => (
                <li key={r.id}>
                  {fmt(r.createdAt)} — {t(STATUS_LABEL_KEY[r.status])}
                  {r.decisionNote && ` (${r.decisionNote})`}
                </li>
              ))}
            </ul>
          </>
        )}
          </div>
        </details>
      </div>
    </section>
  );
}
