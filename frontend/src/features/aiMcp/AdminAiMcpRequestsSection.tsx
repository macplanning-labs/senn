/**
 * AdminAiMcpRequestsSection.tsx — AI Agent を利用した MCP 操作の申請一覧と承認(管理 > AI)(DEMO-000192)
 */

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { useToast } from '@/shared/stores/toastStore';
import {
  AI_MCP_ADMIN_KEY,
  AI_MCP_PENDING_COUNT_KEY,
  apiErrorMessage,
  type AiMcpAdminList,
} from './types';
import '../admin/components/AdminPage.css';
import '@/features/settings/components/SettingsPage.css';

const STATUS_LABEL_KEY = {
  pending: 'aiMcp.statusPending',
  approved: 'aiMcp.statusApproved',
  rejected: 'aiMcp.statusRejected',
  issued: 'aiMcp.statusIssued',
  cancelled: 'aiMcp.statusCancelled',
  expired: 'aiMcp.statusExpired',
} as const;

export function AdminAiMcpRequestsSection() {
  const { t, i18n } = useTranslation();
  const toast = useToast();
  const queryClient = useQueryClient();
  const [pendingOnly, setPendingOnly] = useState(true);
  const [notes, setNotes] = useState<Record<number, string>>({});

  const { data } = useQuery<AiMcpAdminList>({
    queryKey: [...AI_MCP_ADMIN_KEY, pendingOnly],
    queryFn: async () =>
      (await apiClient.get('/system-admin/ai-mcp-requests/', { params: pendingOnly ? { status: 'pending' } : {} })).data,
  });

  const decide = useMutation({
    mutationFn: async (v: { id: number; action: 'approve' | 'reject' }) =>
      (await apiClient.post(`/system-admin/ai-mcp-requests/${v.id}/${v.action}/`, { note: notes[v.id] ?? '' })).data,
    onSuccess: (_d, v) => {
      toast.success(v.action === 'approve' ? t('aiMcp.approvedDone') : t('aiMcp.rejectedDone'));
      void queryClient.invalidateQueries({ queryKey: AI_MCP_ADMIN_KEY });
      void queryClient.invalidateQueries({ queryKey: AI_MCP_PENDING_COUNT_KEY });
    },
    onError: (e) => toast.error(apiErrorMessage(e, t('aiMcp.decideFailed'))),
  });

  const requests = data?.requests ?? [];

  return (
    <section className="settings__section" data-testid="admin-ai-mcp-requests-section">
      <h2 className="settings__section-title">{t('aiMcp.adminTitle')}</h2>
      <div className="settings__card">
        <p className="admin__hint" style={{ marginBottom: '1rem' }}>
          {t('aiMcp.adminHint')}
        </p>
        <div className="admin__actions" style={{ marginBottom: '1rem' }}>
          <button
            type="button"
            className={`admin__btn ${pendingOnly ? 'admin__btn--primary' : 'admin__btn--secondary'}`}
            onClick={() => setPendingOnly(true)}
          >
            {t('aiMcp.filterPending')}
            {data ? ` (${data.pendingCount})` : ''}
          </button>
          <button
            type="button"
            className={`admin__btn ${pendingOnly ? 'admin__btn--secondary' : 'admin__btn--primary'}`}
            onClick={() => setPendingOnly(false)}
          >
            {t('aiMcp.filterAll')}
          </button>
        </div>

        <table className="admin__table" style={{ width: '100%' }}>
          <thead>
            <tr>
              <th>{t('aiMcp.colApplicant')}</th>
              <th>{t('aiMcp.colRequestedAt')}</th>
              <th>{t('aiMcp.colStatus')}</th>
              <th>{t('aiMcp.colDecision')}</th>
            </tr>
          </thead>
          <tbody>
            {requests.length === 0 ? (
              <tr>
                <td colSpan={4}>{t('aiMcp.noRequests')}</td>
              </tr>
            ) : (
              requests.map((r) => (
                <tr key={r.id} data-testid={`ai-mcp-request-${r.id}`}>
                  <td>
                    {r.user.displayName || r.user.username}
                    {r.purpose && (
                      <div style={{ fontSize: '0.85em', opacity: 0.7 }} data-testid={`ai-mcp-device-${r.id}`}>
                        {r.purpose}
                      </div>
                    )}
                  </td>
                  <td>{new Date(r.createdAt).toLocaleString(i18n.language)}</td>
                  <td>{t(STATUS_LABEL_KEY[r.status])}</td>
                  <td>
                    {r.status === 'pending' ? (
                      <div>
                        <input
                          className="admin__input"
                          type="text"
                          aria-label={t('aiMcp.notePlaceholder')}
                          placeholder={t('aiMcp.notePlaceholder')}
                          value={notes[r.id] ?? ''}
                          onChange={(e) => setNotes((n) => ({ ...n, [r.id]: e.target.value }))}
                        />
                        <div className="admin__actions">
                          <button
                            type="button"
                            className="admin__btn admin__btn--primary"
                            disabled={decide.isPending}
                            onClick={() => decide.mutate({ id: r.id, action: 'approve' })}
                            data-testid={`ai-mcp-approve-${r.id}`}
                          >
                            {t('aiMcp.approve')}
                          </button>
                          <button
                            type="button"
                            className="admin__btn admin__btn--danger"
                            disabled={decide.isPending}
                            onClick={() => decide.mutate({ id: r.id, action: 'reject' })}
                            data-testid={`ai-mcp-reject-${r.id}`}
                          >
                            {t('aiMcp.reject')}
                          </button>
                        </div>
                      </div>
                    ) : (
                      <span>
                        {r.decidedBy
                          ? t('aiMcp.decidedBy', { name: r.decidedBy.displayName || r.decidedBy.username })
                          : '—'}
                        {r.decisionNote && ` (${r.decisionNote})`}
                      </span>
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
