/**
 * AdminAiKickSection.tsx — AI 自動実行の管理画面
 */

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { useToast } from '@/shared/stores/toastStore';
import {
  formatTokens,
  usageBarClass,
  validateRunnerName,
  validateLimitsDraft,
} from '../aiKickFormat';
import '../components/AdminPage.css';
import '@/features/settings/components/SettingsPage.css';
import '../components/AdminAiKickSection.css';
import '../aiKick.i18n';

interface Runner {
  id: string;
  name: string;
  tokenPrefix: string;
  createdAt: string;
  revokedAt: string | null;
  lastSeenAt: string | null;
  online: boolean;
}

interface Usage {
  dayUsed: number;
  weekUsed: number;
  dayBudget: number | null;
  weekBudget: number | null;
  dayPercent: number | null;
  weekPercent: number | null;
}

interface Limits {
  maxTurns: number;
  maxMinutes: number;
  maxWeightedTokens: number;
  dailyBudgetSystem: number;
  dailyBudgetProject: number;
  weeklyBudgetSystem: number;
  perUserHour: number;
  perUserDay: number;
  perTicket24h: number;
  maxQueue: number;
  burnRatePerMin: number;
  maxConsecutiveFailures: number;
  hardCeilingPercent: number;
  emergencyCapPercent: number;
  overrideAdminOnly: boolean;
}

interface AiKickData {
  enabled: boolean;
  killed: boolean;
  limits: Limits | null;
  defaultLimits: Limits;
  /** AI が作業してよいリポジトリ(owner/name) */
  allowedRepos?: string[];
  runners: Runner[];
  usage: Usage;
}

interface IssuedRunner {
  id: string;
  name: string;
  token: string;
  signingSecret: string;
  notice: string;
}

function formatDateTime(value: string | null): string {
  if (!value) return '—';
  return new Date(value).toLocaleString('ja-JP');
}

export function AdminAiKickSection() {
  const { t } = useTranslation();
  const toast = useToast();
  const queryClient = useQueryClient();

  // State for modal
  const [issuedRunner, setIssuedRunner] = useState<IssuedRunner | null>(null);
  const [savedModal, setSavedModal] = useState(false);

  // State for runner creation
  const [runnerNameInput, setRunnerNameInput] = useState('');
  const [runnerNameError, setRunnerNameError] = useState('');

  // State for limits form
  const [limitsDraft, setLimitsDraft] = useState<Partial<Limits> | null>(null);
  const [limitsError, setLimitsError] = useState('');

  // State for allowed repos (1 行に 1 つ。保存するまで反映しない)
  const [reposDraft, setReposDraft] = useState<string | null>(null);

  // State for enable switch
  const [confirmKill, setConfirmKill] = useState(false);
  const [confirmResume, setConfirmResume] = useState(false);
  const [confirmEnable, setConfirmEnable] = useState(false);

  // Queries
  const { data: aiKickData, isLoading } = useQuery<AiKickData>({
    queryKey: ['system-admin-ai-kick'],
    queryFn: async () => (await apiClient.get('/system-admin/ai-kick/')).data,
  });

  // Mutations
  const updateAiKickMutation = useMutation({
    mutationFn: async (payload: Partial<AiKickData>) => {
      const res = await apiClient.put<AiKickData>('/system-admin/ai-kick/', payload);
      return res.data;
    },
    onSuccess: (data) => {
      queryClient.setQueryData(['system-admin-ai-kick'], data);
      toast.success(t('admin.aiKickSaved'));
      setConfirmKill(false);
      setConfirmResume(false);
      setConfirmEnable(false);
      setLimitsDraft(null);
      setReposDraft(null);
    },
    onError: (error: unknown) => {
      const axiosErr = error as { response?: { data?: { detail?: string } } };
      const detail = axiosErr.response?.data?.detail;
      if (detail) {
        setLimitsError(detail);
      } else {
        toast.error(t('admin.aiKickSaveFailed'));
      }
    },
  });

  const createRunnerMutation = useMutation({
    mutationFn: async (name: string) => {
      const res = await apiClient.post<IssuedRunner>('/system-admin/ai-kick/runners/', {
        name,
      });
      return res.data;
    },
    onSuccess: (data) => {
      setIssuedRunner(data);
      setRunnerNameInput('');
      setRunnerNameError('');
      void queryClient.invalidateQueries({ queryKey: ['system-admin-ai-kick'] });
    },
    onError: (error: unknown) => {
      const axiosErr = error as { response?: { status?: number; data?: { detail?: string } } };
      if (axiosErr.response?.status === 409) {
        setRunnerNameError(t('admin.aiKickRunnerNameDuplicate'));
      } else if (axiosErr.response?.data?.detail) {
        setRunnerNameError(axiosErr.response.data.detail);
      } else {
        toast.error(t('admin.aiKickRunnerCreateFailed'));
      }
    },
  });

  const revokeRunnerMutation = useMutation({
    mutationFn: async (id: string) => {
      await apiClient.delete(`/system-admin/ai-kick/runners/${id}/`);
    },
    onSuccess: () => {
      toast.success(t('admin.aiKickRunnerRevokedMessage'));
      void queryClient.invalidateQueries({ queryKey: ['system-admin-ai-kick'] });
    },
    onError: () => {
      toast.error(t('admin.aiKickRunnerRevokeFailed'));
    },
  });

  if (isLoading || !aiKickData) {
    return (
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', height: '50vh', color: 'var(--color-text-tertiary)' }}>
        Loading...
      </div>
    );
  }

  const hasLimits = aiKickData.limits !== null;
  const onlineCount = aiKickData.runners.filter((r) => r.online && !r.revokedAt).length;

  const handleRunnerAdd = () => {
    const error = validateRunnerName(runnerNameInput);
    if (error) {
      setRunnerNameError(error);
      return;
    }
    createRunnerMutation.mutate(runnerNameInput);
  };

  const handleLimitsLoad = () => {
    setLimitsDraft(aiKickData.defaultLimits);
    setLimitsError('');
  };

  // 上限が未保存のときは、画面に見えている初期値のまま保存できる(「初期値を入れる」を押さなくてよい)
  const draftToSave = limitsDraft ?? (aiKickData.limits ? null : aiKickData.defaultLimits);

  const handleLimitsSave = () => {
    if (!draftToSave) return;
    const errors = validateLimitsDraft(draftToSave);
    if (errors.length > 0) {
      setLimitsError(errors.join(', '));
      return;
    }
    updateAiKickMutation.mutate({ limits: draftToSave as Limits });
  };

  const handleEnableToggle = () => {
    if (!hasLimits) return;
    if (!aiKickData.enabled) {
      setConfirmEnable(true);
    } else {
      updateAiKickMutation.mutate({ enabled: false });
    }
  };

  return (
    <div data-testid="admin-ai-kick-section">
      <section className="settings__section">
        <h2 className="settings__section-title">{t('admin.aiKickTitle')}</h2>

        {/* 状態カード */}
        <div className="settings__card ai-kick__status-card">
          <div className="ai-kick__status-row">
            <div className="ai-kick__status-item">
              <span className="ai-kick__label">{t('admin.aiKickStatus')}</span>
              <span
                className={`admin__status-badge ${
                  aiKickData.enabled ? 'admin__status-badge--ok' : 'admin__status-badge--warn'
                }`}
              >
                {aiKickData.enabled ? t('admin.aiKickEnabled') : t('admin.aiKickDisabled')}
              </span>
              {aiKickData.killed && (
                <span className="admin__status-badge ai-kick__status-badge--error">
                  {t('admin.aiKickKilled')}
                </span>
              )}
            </div>

            <div className="ai-kick__status-item">
              <span className="ai-kick__label">{t('admin.aiKickRunnersOnline')}</span>
              <span className="ai-kick__runner-count">{onlineCount}</span>
            </div>
          </div>

          {/* 使用量 */}
          {aiKickData.usage && (
            <div className="ai-kick__usage-section">
              <div className="ai-kick__usage-row">
                <div className="ai-kick__usage-item">
                  <div className="ai-kick__usage-label">{t('admin.aiKickUsageToday')}</div>
                  <div className="ai-kick__usage-bar-container">
                    <div
                      className={`ai-kick__usage-bar ${usageBarClass(aiKickData.usage.dayPercent)}`}
                      style={{
                        width: `${Math.min(aiKickData.usage.dayPercent || 0, 100)}%`,
                      }}
                    />
                  </div>
                  <div className="ai-kick__usage-text">
                    {formatTokens(aiKickData.usage.dayUsed)} /{' '}
                    {formatTokens(aiKickData.usage.dayBudget)}
                    {aiKickData.usage.dayPercent !== null && (
                      <> ({Math.round(aiKickData.usage.dayPercent)}%)</>
                    )}
                  </div>
                </div>

                <div className="ai-kick__usage-item">
                  <div className="ai-kick__usage-label">{t('admin.aiKickUsageWeek')}</div>
                  <div className="ai-kick__usage-bar-container">
                    <div
                      className={`ai-kick__usage-bar ${usageBarClass(aiKickData.usage.weekPercent)}`}
                      style={{
                        width: `${Math.min(aiKickData.usage.weekPercent || 0, 100)}%`,
                      }}
                    />
                  </div>
                  <div className="ai-kick__usage-text">
                    {formatTokens(aiKickData.usage.weekUsed)} /{' '}
                    {formatTokens(aiKickData.usage.weekBudget)}
                    {aiKickData.usage.weekPercent !== null && (
                      <> ({Math.round(aiKickData.usage.weekPercent)}%)</>
                    )}
                  </div>
                </div>
              </div>
            </div>
          )}
        </div>

        {/* 緊急停止ボタン */}
        {aiKickData.killed === false ? (
          <>
            <button
              className="admin__btn ai-kick__btn--emergency"
              onClick={() => setConfirmKill(true)}
              disabled={updateAiKickMutation.isPending}
            >
              🛑 {t('admin.aiKickEmergencyStop')}
            </button>
            {confirmKill && (
              <div className="ai-kick__confirm-overlay" onClick={() => setConfirmKill(false)}>
                <div className="ai-kick__confirm-modal" onClick={(e) => e.stopPropagation()}>
                  <div className="ai-kick__confirm-title">{t('admin.aiKickEmergencyStop')}</div>
                  <div className="ai-kick__confirm-message">
                    {t('admin.aiKickEmergencyConfirmStop')}
                  </div>
                  <div className="ai-kick__confirm-actions">
                    <button
                      className="admin__btn admin__btn--secondary"
                      onClick={() => setConfirmKill(false)}
                    >
                      {t('common.cancel')}
                    </button>
                    <button
                      className="admin__btn ai-kick__btn--emergency"
                      onClick={() => updateAiKickMutation.mutate({ killed: true })}
                      disabled={updateAiKickMutation.isPending}
                    >
                      {t('admin.aiKickEmergencyStop')}
                    </button>
                  </div>
                </div>
              </div>
            )}
          </>
        ) : (
          <>
            <button
              className="admin__btn admin__btn--primary"
              onClick={() => setConfirmResume(true)}
              disabled={updateAiKickMutation.isPending}
            >
              ✓ {t('admin.aiKickEmergencyResume')}
            </button>
            {confirmResume && (
              <div className="ai-kick__confirm-overlay" onClick={() => setConfirmResume(false)}>
                <div className="ai-kick__confirm-modal" onClick={(e) => e.stopPropagation()}>
                  <div className="ai-kick__confirm-title">{t('admin.aiKickEmergencyResume')}</div>
                  <div className="ai-kick__confirm-message">
                    {t('admin.aiKickEmergencyConfirmResume')}
                  </div>
                  <div className="ai-kick__confirm-actions">
                    <button
                      className="admin__btn admin__btn--secondary"
                      onClick={() => setConfirmResume(false)}
                    >
                      {t('common.cancel')}
                    </button>
                    <button
                      className="admin__btn admin__btn--primary"
                      onClick={() => updateAiKickMutation.mutate({ killed: false })}
                      disabled={updateAiKickMutation.isPending}
                    >
                      {t('admin.aiKickEmergencyResume')}
                    </button>
                  </div>
                </div>
              </div>
            )}
          </>
        )}

        {/* 有効にするスイッチ */}
        <div className="admin__field" style={{ marginTop: 'var(--space-4)' }}>
          <label style={{
            display: 'flex',
            alignItems: 'center',
            gap: 'var(--space-3)',
            cursor: hasLimits ? 'pointer' : 'not-allowed',
            userSelect: 'none',
          }}>
            <input
              type="checkbox"
              checked={aiKickData.enabled}
              onChange={handleEnableToggle}
              disabled={!hasLimits || updateAiKickMutation.isPending}
              style={{ cursor: 'pointer', width: 18, height: 18 }}
            />
            <div>
              <div style={{ fontWeight: 'var(--font-weight-semibold)', color: 'var(--color-text-primary)' }}>
                {t('admin.aiKickEnable')}
              </div>
              {!hasLimits && (
                <div style={{ fontSize: 'var(--font-size-sm)', color: 'var(--color-text-tertiary)', marginTop: 'var(--space-1)' }}>
                  {t('admin.aiKickEnableDisabledReason')}
                </div>
              )}
            </div>
          </label>
        </div>

        {/* 確認ダイアログ（有効化） */}
        {confirmEnable && (
          <div className="ai-kick__confirm-overlay" onClick={() => setConfirmEnable(false)}>
            <div className="ai-kick__confirm-modal" onClick={(e) => e.stopPropagation()}>
              <div className="ai-kick__confirm-title">{t('admin.aiKickEnable')}</div>
              <div className="ai-kick__confirm-message">
                {t('admin.aiKickEnableConfirm')}
              </div>
              <div className="ai-kick__confirm-actions">
                <button
                  className="admin__btn admin__btn--secondary"
                  onClick={() => setConfirmEnable(false)}
                >
                  {t('common.cancel')}
                </button>
                <button
                  className="admin__btn admin__btn--primary"
                  onClick={() => updateAiKickMutation.mutate({ enabled: true })}
                  disabled={updateAiKickMutation.isPending}
                >
                  {t('admin.aiKickEnable')}
                </button>
              </div>
            </div>
          </div>
        )}
      </section>

      {/* 上限フォーム */}
      <section className="settings__section">
        <h2 className="settings__section-title">{t('admin.aiKickLimitsTitle')}</h2>
        <div className="settings__card">
          {limitsError && (
            <div style={{
              padding: 'var(--space-2) var(--space-3)',
              background: 'rgba(220, 38, 38, 0.1)',
              border: '1px solid rgba(220, 38, 38, 0.3)',
              borderRadius: 'var(--radius-md)',
              color: '#dc2626',
              fontSize: 'var(--font-size-sm)',
              marginBottom: 'var(--space-4)',
            }}>
              {limitsError}
            </div>
          )}

          <LimitsForm
            draft={limitsDraft}
            defaults={aiKickData.defaultLimits}
            onUpdate={setLimitsDraft}
          />

          <div className="admin__actions" style={{ marginTop: 'var(--space-6)' }}>
            <button
              type="button"
              className="admin__btn admin__btn--secondary"
              onClick={handleLimitsLoad}
              disabled={updateAiKickMutation.isPending}
            >
              {t('admin.aiKickDefaultButton')}
            </button>
            <button
              type="button"
              className="admin__btn admin__btn--primary"
              disabled={updateAiKickMutation.isPending || !draftToSave}
              onClick={handleLimitsSave}
            >
              {t('admin.aiKickSaveButton')}
            </button>
          </div>
        </div>
      </section>

      {/* AI が作業してよいリポジトリ(プロジェクトのオーナーは、この中からしか選べない) */}
      <section className="settings__section" data-testid="ai-kick-allowed-repos">
        <h2 className="settings__section-title">{t('admin.aiKickAllowedReposTitle')}</h2>
        <div className="settings__card">
          <div className="admin__hint" style={{ marginBottom: 'var(--space-2)' }}>
            {t('admin.aiKickAllowedReposHint')}
          </div>
          <textarea
            className="admin__input"
            rows={4}
            aria-label={t('admin.aiKickAllowedReposTitle')}
            placeholder="owner/name"
            value={reposDraft ?? (aiKickData.allowedRepos ?? []).join('\n')}
            onChange={(e) => {
              setReposDraft(e.target.value);
              setLimitsError('');
            }}
            disabled={updateAiKickMutation.isPending}
          />
          <div className="admin__actions" style={{ marginTop: 'var(--space-4)' }}>
            <button
              type="button"
              className="admin__btn admin__btn--primary"
              disabled={updateAiKickMutation.isPending || reposDraft === null}
              onClick={() =>
                updateAiKickMutation.mutate({
                  allowedRepos: (reposDraft ?? '').split('\n').map((r) => r.trim()).filter(Boolean),
                })
              }
            >
              {t('admin.aiKickAllowedReposSave')}
            </button>
          </div>
        </div>
      </section>

      {/* ランナー */}
      <section className="settings__section">
        <h2 className="settings__section-title">{t('admin.aiKickRunnersTitle')}</h2>
        <div className="settings__card">
          <div className="ai-kick__runner-list">
            {aiKickData.runners.length === 0 ? (
              <p style={{ color: 'var(--color-text-tertiary)' }}>
                {t('admin.aiKickNoRunners')}
              </p>
            ) : (
              <table className="admin__table">
                <thead>
                  <tr>
                    <th>{t('admin.aiKickRunnerName')}</th>
                    <th>{t('admin.aiKickRunnerToken')}</th>
                    <th>{t('admin.aiKickRunnerOnline')}</th>
                    <th>{t('admin.aiKickRunnerLastSeen')}</th>
                    <th></th>
                  </tr>
                </thead>
                <tbody>
                  {aiKickData.runners.map((runner) => (
                    <tr
                      key={runner.id}
                      style={{
                        opacity: runner.revokedAt ? 0.5 : 1,
                      }}
                    >
                      <td>
                        {runner.name}
                        {runner.revokedAt && (
                          <span style={{
                            marginLeft: 'var(--space-2)',
                            fontSize: 'var(--font-size-xs)',
                            color: 'var(--color-text-tertiary)',
                          }}>
                            ({t('admin.aiKickRunnerRevoked')})
                          </span>
                        )}
                      </td>
                      <td>
                        <code style={{
                          fontSize: 'var(--font-size-xs)',
                          background: 'var(--color-bg-secondary)',
                          padding: '2px 6px',
                          borderRadius: 'var(--radius-sm)',
                        }}>
                          {runner.tokenPrefix}...
                        </code>
                      </td>
                      <td>
                        {runner.revokedAt ? (
                          <span style={{ color: 'var(--color-text-tertiary)' }}>—</span>
                        ) : (
                          <span style={{
                            display: 'inline-flex',
                            alignItems: 'center',
                            gap: '4px',
                          }}>
                            <span
                              style={{
                                width: 8,
                                height: 8,
                                borderRadius: '50%',
                                background: runner.online ? '#10b981' : '#9ca3af',
                              }}
                            />
                            {runner.online ? t('admin.aiKickRunnerOnline') : t('admin.aiKickRunnerOffline')}
                          </span>
                        )}
                      </td>
                      <td>{formatDateTime(runner.lastSeenAt)}</td>
                      <td>
                        {!runner.revokedAt && (
                          <button
                            className="admin__btn admin__btn--danger"
                            style={{ fontSize: 'var(--font-size-xs)', padding: '4px 8px' }}
                            onClick={() => {
                              if (confirm(t('admin.aiKickRunnerRevokeConfirm'))) {
                                revokeRunnerMutation.mutate(runner.id);
                              }
                            }}
                            disabled={revokeRunnerMutation.isPending}
                          >
                            {t('admin.aiKickRunnerRevokeButton')}
                          </button>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>

          {/* ランナー追加 */}
          <div style={{ marginTop: 'var(--space-6)', paddingTop: 'var(--space-4)', borderTop: '1px solid var(--color-border-default)' }}>
            <div className="admin__field">
              <label className="admin__label" htmlFor="runner-name">
                {t('admin.aiKickRunnerName')}
              </label>
              <input
                id="runner-name"
                className="admin__input"
                type="text"
                value={runnerNameInput}
                onChange={(e) => {
                  setRunnerNameInput(e.target.value);
                  setRunnerNameError('');
                }}
                placeholder="my-runner"
                disabled={createRunnerMutation.isPending}
              />
              {runnerNameError && (
                <div style={{
                  fontSize: 'var(--font-size-sm)',
                  color: '#dc2626',
                  marginTop: 'var(--space-1)',
                }}>
                  {runnerNameError}
                </div>
              )}
            </div>
            <div className="admin__actions">
              <button
                type="button"
                className="admin__btn admin__btn--primary"
                onClick={handleRunnerAdd}
                disabled={createRunnerMutation.isPending || !runnerNameInput.trim()}
              >
                {t('admin.aiKickAddRunner')}
              </button>
            </div>
          </div>
        </div>
      </section>

      {/* セットアップ手順 */}
      <section className="settings__section">
        <details style={{ cursor: 'pointer' }}>
          <summary style={{
            fontSize: 'var(--font-size-sm)',
            fontWeight: 'var(--font-weight-semibold)',
            color: 'var(--color-text-primary)',
            padding: 'var(--space-2) 0',
            userSelect: 'none',
          }}>
            {t('admin.aiKickSetupTitle')}
          </summary>
          <div style={{
            marginTop: 'var(--space-4)',
            paddingTop: 'var(--space-4)',
            borderTop: '1px solid var(--color-border-default)',
          }}>
            <ol style={{
              fontSize: 'var(--font-size-sm)',
              lineHeight: 1.6,
              paddingLeft: '1.5rem',
              color: 'var(--color-text-secondary)',
            }}>
              <li style={{ marginBottom: 'var(--space-3)' }}>
                {t('admin.aiKickSetupStep1')}
              </li>
              <li style={{ marginBottom: 'var(--space-3)' }}>
                {t('admin.aiKickSetupStep2')}
              </li>
              <li>
                {t('admin.aiKickSetupStep3')}
              </li>
            </ol>
            <div style={{
              marginTop: 'var(--space-4)',
              padding: 'var(--space-3)',
              background: 'var(--color-bg-secondary)',
              borderRadius: 'var(--radius-md)',
              fontSize: 'var(--font-size-xs)',
              color: 'var(--color-text-tertiary)',
            }}>
              📌 {t('admin.aiKickSetupNote')}
            </div>
          </div>
        </details>
      </section>

      {/* モーダル：ランナー作成後の認証情報表示 */}
      {issuedRunner && (
        <div className="ai-kick__modal-overlay">
          <div className="ai-kick__modal">
            <div className="ai-kick__modal-header">
              <h3 className="ai-kick__modal-title">{t('admin.aiKickRunnerCreatedModalTitle')}</h3>
            </div>

            <div style={{ marginBottom: 'var(--space-4)' }}>
              <div className="admin__field">
                <label className="admin__label">{t('admin.aiKickRunnerCreatedModalToken')}</label>
                <div style={{
                  display: 'flex',
                  gap: 'var(--space-2)',
                  alignItems: 'stretch',
                }}>
                  <input
                    type="text"
                    className="admin__input"
                    value={issuedRunner.token}
                    readOnly
                    style={{ fontFamily: 'var(--font-family-mono)' }}
                  />
                  <button
                    type="button"
                    className="admin__btn admin__btn--secondary"
                    onClick={() => {
                      void navigator.clipboard.writeText(issuedRunner.token);
                      toast.success(t('common.copied'));
                    }}
                  >
                    {t('admin.aiKickRunnerCreatedModalCopy')}
                  </button>
                </div>
              </div>

              <div className="admin__field">
                <label className="admin__label">{t('admin.aiKickRunnerCreatedModalSecret')}</label>
                <div style={{
                  display: 'flex',
                  gap: 'var(--space-2)',
                  alignItems: 'stretch',
                }}>
                  <input
                    type="text"
                    className="admin__input"
                    value={issuedRunner.signingSecret}
                    readOnly
                    style={{ fontFamily: 'var(--font-family-mono)' }}
                  />
                  <button
                    type="button"
                    className="admin__btn admin__btn--secondary"
                    onClick={() => {
                      void navigator.clipboard.writeText(issuedRunner.signingSecret);
                      toast.success(t('common.copied'));
                    }}
                  >
                    {t('admin.aiKickRunnerCreatedModalCopy')}
                  </button>
                </div>
              </div>
            </div>

            <div style={{
              padding: 'var(--space-3)',
              background: 'rgba(234, 179, 8, 0.15)',
              border: '1px solid rgba(234, 179, 8, 0.3)',
              borderRadius: 'var(--radius-md)',
              fontSize: 'var(--font-size-sm)',
              color: '#ca8a04',
              marginBottom: 'var(--space-4)',
            }}>
              ⚠️ {t('admin.aiKickRunnerCreatedModalWarning')}
            </div>

            <div>
              <label style={{
                display: 'flex',
                alignItems: 'center',
                gap: 'var(--space-2)',
                cursor: 'pointer',
                userSelect: 'none',
              }}>
                <input
                  type="checkbox"
                  checked={savedModal}
                  onChange={(e) => setSavedModal(e.target.checked)}
                  style={{ cursor: 'pointer' }}
                />
                <span>{t('admin.aiKickRunnerCreatedModalSaved')}</span>
              </label>
            </div>

            <div style={{
              marginTop: 'var(--space-4)',
              display: 'flex',
              gap: 'var(--space-2)',
              justifyContent: 'flex-end',
            }}>
              <button
                type="button"
                className="admin__btn admin__btn--primary"
                onClick={() => {
                  setIssuedRunner(null);
                  setSavedModal(false);
                }}
                disabled={!savedModal}
              >
                {t('common.close')}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

/**
 * LimitsForm - 上限値の入力フォーム
 */
interface LimitsFormProps {
  draft: Partial<Limits> | null;
  defaults: Limits;
  onUpdate: (draft: Partial<Limits>) => void;
}

function LimitsForm({ draft, defaults, onUpdate }: LimitsFormProps) {
  const { t } = useTranslation();
  const current = draft || defaults;

  const handleChange = (key: keyof Limits, value: number | boolean) => {
    onUpdate({ ...current, [key]: value });
  };

  return (
    <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 'var(--space-6)' }}>
      {/* 1 実行の上限 */}
      <div>
        <h3 style={{
          fontSize: 'var(--font-size-sm)',
          fontWeight: 'var(--font-weight-semibold)',
          color: 'var(--color-text-primary)',
          marginBottom: 'var(--space-3)',
        }}>
          {t('admin.aiKickLimitsPerExecution')}
        </h3>
        <NumberInput
          label={t('admin.aiKickMaxTurns')}
          value={current.maxTurns}
          onChange={(v) => handleChange('maxTurns', v)}
        />
        <NumberInput
          label={t('admin.aiKickMaxMinutes')}
          value={current.maxMinutes}
          onChange={(v) => handleChange('maxMinutes', v)}
        />
        <NumberInput
          label={t('admin.aiKickMaxTokens')}
          value={current.maxWeightedTokens}
          onChange={(v) => handleChange('maxWeightedTokens', v)}
          hint={t('admin.aiKickTokenDesc')}
        />
      </div>

      {/* 予算 */}
      <div>
        <h3 style={{
          fontSize: 'var(--font-size-sm)',
          fontWeight: 'var(--font-weight-semibold)',
          color: 'var(--color-text-primary)',
          marginBottom: 'var(--space-3)',
        }}>
          {t('admin.aiKickBudget')}
        </h3>
        <NumberInput
          label={t('admin.aiKickDailyBudgetSystem')}
          value={current.dailyBudgetSystem}
          onChange={(v) => handleChange('dailyBudgetSystem', v)}
        />
        <NumberInput
          label={t('admin.aiKickDailyBudgetProject')}
          value={current.dailyBudgetProject}
          onChange={(v) => handleChange('dailyBudgetProject', v)}
        />
        <NumberInput
          label={t('admin.aiKickWeeklyBudgetSystem')}
          value={current.weeklyBudgetSystem}
          onChange={(v) => handleChange('weeklyBudgetSystem', v)}
        />
        <div style={{
          fontSize: 'var(--font-size-xs)',
          color: 'var(--color-text-tertiary)',
          marginTop: 'var(--space-2)',
        }}>
          {t('admin.aiKickBudgetDesc')}
        </div>
      </div>

      {/* 回数 */}
      <div>
        <h3 style={{
          fontSize: 'var(--font-size-sm)',
          fontWeight: 'var(--font-weight-semibold)',
          color: 'var(--color-text-primary)',
          marginBottom: 'var(--space-3)',
        }}>
          {t('admin.aiKickFrequency')}
        </h3>
        <NumberInput
          label={t('admin.aiKickPerUserHour')}
          value={current.perUserHour}
          onChange={(v) => handleChange('perUserHour', v)}
        />
        <NumberInput
          label={t('admin.aiKickPerUserDay')}
          value={current.perUserDay}
          onChange={(v) => handleChange('perUserDay', v)}
        />
        <NumberInput
          label={t('admin.aiKickPerTicket24h')}
          value={current.perTicket24h}
          onChange={(v) => handleChange('perTicket24h', v)}
        />
        <NumberInput
          label={t('admin.aiKickMaxQueue')}
          value={current.maxQueue}
          onChange={(v) => handleChange('maxQueue', v)}
        />
      </div>

      {/* 暴走検知 */}
      <div>
        <h3 style={{
          fontSize: 'var(--font-size-sm)',
          fontWeight: 'var(--font-weight-semibold)',
          color: 'var(--color-text-primary)',
          marginBottom: 'var(--space-3)',
        }}>
          {t('admin.aiKickRunaway')}
        </h3>
        <NumberInput
          label={t('admin.aiKickBurnRate')}
          value={current.burnRatePerMin}
          onChange={(v) => handleChange('burnRatePerMin', v)}
        />
        <NumberInput
          label={t('admin.aiKickMaxFailures')}
          value={current.maxConsecutiveFailures}
          onChange={(v) => handleChange('maxConsecutiveFailures', v)}
        />
      </div>

      {/* 上限の線 */}
      <div>
        <h3 style={{
          fontSize: 'var(--font-size-sm)',
          fontWeight: 'var(--font-weight-semibold)',
          color: 'var(--color-text-primary)',
          marginBottom: 'var(--space-3)',
        }}>
          {t('admin.aiKickThresholds')}
        </h3>
        <NumberInput
          label={t('admin.aiKickHardCeiling')}
          value={current.hardCeilingPercent}
          onChange={(v) => handleChange('hardCeilingPercent', v)}
        />
        <NumberInput
          label={t('admin.aiKickEmergencyCap')}
          value={current.emergencyCapPercent}
          onChange={(v) => handleChange('emergencyCapPercent', v)}
        />
        <div style={{
          display: 'flex',
          alignItems: 'center',
          gap: 'var(--space-2)',
          marginTop: 'var(--space-3)',
        }}>
          <input
            type="checkbox"
            checked={current.overrideAdminOnly}
            onChange={(e) => handleChange('overrideAdminOnly', e.target.checked)}
            style={{ cursor: 'pointer' }}
          />
          <label style={{ cursor: 'pointer', fontSize: 'var(--font-size-sm)' }}>
            {t('admin.aiKickOverrideAdminOnly')}
          </label>
        </div>
      </div>
    </div>
  );
}

/**
 * NumberInput - 数値入力欄
 */
interface NumberInputProps {
  label: string;
  value: number | undefined;
  onChange: (v: number) => void;
  hint?: string;
}

function NumberInput({ label, value, onChange, hint }: NumberInputProps) {
  return (
    <div className="admin__field">
      <label className="admin__label">
        {label}
        {value !== undefined && value >= 1000 && (
          <span className="ai-kick__number-grouped"> ({value.toLocaleString('ja-JP')})</span>
        )}
      </label>
      <input
        type="number"
        className="admin__input"
        value={value ?? ''}
        onChange={(e) => onChange(Number(e.target.value))}
        min="0"
        step="1"
      />
      {hint && (
        <div className="admin__hint">{hint}</div>
      )}
    </div>
  );
}
