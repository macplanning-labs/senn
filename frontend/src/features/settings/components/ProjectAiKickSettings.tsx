/**
 * ProjectAiKickSettings.tsx — プロジェクトの AI 自動実行の設定
 */

import { useState, useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { useMutation, useQuery } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { useToastStore } from '@/shared/stores/toastStore';
import { validateRepo } from '@/features/admin/aiKickFormat';
import '../components/ProjectAiKickSettings.css';
import '@/features/admin/aiKick.i18n';

interface AiKickSettings {
  enabled: boolean;
  repo: string;
  baseBranch: string;
  canEdit: boolean;
  systemEnabled: boolean;
  systemKilled: boolean;
  /** システム管理者が AI の作業を許可したリポジトリ(この中から選ぶ) */
  allowedRepos?: string[];
}

interface ProjectAiKickSettingsProps {
  projectId: string | number;
}

export function ProjectAiKickSettings({ projectId }: ProjectAiKickSettingsProps) {
  const { t } = useTranslation();
  const { addToast } = useToastStore();

  const [repoInput, setRepoInput] = useState('');
  const [branchInput, setBranchInput] = useState('');
  const [enabledInput, setEnabledInput] = useState(false);
  const [repoError, setRepoError] = useState('');
  // Anthropic 社に送られることの確認(有効にして保存するたびに必要。サーバーでも確かめる)
  const [ackInput, setAckInput] = useState(false);
  const initializedRef = useRef(false);

  const { data: settings, isLoading } = useQuery<AiKickSettings>({
    queryKey: ['project-ai-kick', projectId],
    queryFn: async () => {
      const res = await apiClient.get(`/projects/${projectId}/ai-kick/`);
      return res.data;
    },
  });

  // Initialize form from settings once loaded
  useEffect(() => {
    if (settings && !initializedRef.current) {
      initializedRef.current = true;
      setRepoInput(settings.repo);
      setBranchInput(settings.baseBranch);
      setEnabledInput(settings.enabled);
    }
  }, [settings]);

  const saveMutation = useMutation({
    mutationFn: async () => {
      if (!projectId) throw new Error('No projectId');

      // Validate
      if (enabledInput && !repoInput.trim()) {
        setRepoError(t('projectSettings.aiKickRepoRequired'));
        return;
      }

      const repoValidationError = validateRepo(repoInput);
      if (repoValidationError) {
        setRepoError(repoValidationError);
        return;
      }

      if (enabledInput && !ackInput) {
        setRepoError(t('projectSettings.aiKickDataSharingRequired'));
        return;
      }

      const res = await apiClient.put<AiKickSettings>(
        `/projects/${projectId}/ai-kick/`,
        {
          enabled: enabledInput,
          repo: repoInput.trim(),
          baseBranch: branchInput.trim(),
          dataSharingAcknowledged: enabledInput && ackInput,
        }
      );
      return res.data;
    },
    onSuccess: (data) => {
      if (data) {
        setRepoInput(data.repo);
        setBranchInput(data.baseBranch);
        setEnabledInput(data.enabled);
        setAckInput(false);
        addToast({ message: t('projectSettings.aiKickSaved'), type: 'success' });
      }
    },
    onError: (error: unknown) => {
      const axiosErr = error as { response?: { data?: { detail?: string } } };
      const detail = axiosErr.response?.data?.detail;
      if (detail) {
        setRepoError(detail);
      } else {
        addToast({ message: t('projectSettings.aiKickSaveFailed'), type: 'error' });
      }
    },
  });

  if (isLoading || !settings) {
    return <div>{t('common.loading')}</div>;
  }

  const allowedRepos = settings.allowedRepos ?? [];
  const currentNotAllowed =
    !!repoInput && !allowedRepos.some((r) => r.toLowerCase() === repoInput.toLowerCase());

  const hasChanged =
    repoInput !== settings.repo ||
    branchInput !== settings.baseBranch ||
    enabledInput !== settings.enabled ||
    (enabledInput && ackInput);

  return (
    <div className="project-ai-kick-settings" data-testid="project-ai-kick-settings">
      <div className="settings-section__header">
        <h2 className="settings-section__title">{t('projectSettings.aiKick')}</h2>
      </div>

      {!settings.canEdit && (
        <div style={{
          padding: 'var(--space-3)',
          background: 'var(--color-bg-secondary)',
          borderRadius: 'var(--radius-md)',
          fontSize: 'var(--font-size-sm)',
          color: 'var(--color-text-tertiary)',
          marginBottom: 'var(--space-4)',
        }}>
          {t('projectSettings.aiKickReadOnly')}
        </div>
      )}

      {!settings.systemEnabled && (
        <div style={{
          padding: 'var(--space-3)',
          background: 'rgba(234, 179, 8, 0.15)',
          border: '1px solid rgba(234, 179, 8, 0.3)',
          borderRadius: 'var(--radius-md)',
          fontSize: 'var(--font-size-sm)',
          color: '#ca8a04',
          marginBottom: 'var(--space-4)',
        }}>
          {t('projectSettings.aiKickSystemDisabled')}
        </div>
      )}

      {settings.systemKilled && (
        <div style={{
          padding: 'var(--space-3)',
          background: 'rgba(220, 38, 38, 0.15)',
          border: '1px solid rgba(220, 38, 38, 0.3)',
          borderRadius: 'var(--radius-md)',
          fontSize: 'var(--font-size-sm)',
          color: '#dc2626',
          marginBottom: 'var(--space-4)',
        }}>
          {t('projectSettings.aiKickSystemKilled')}
        </div>
      )}

      <fieldset
        disabled={!settings.canEdit}
        style={{
          border: 'none',
          padding: 0,
          margin: 0,
        }}
      >
        <div style={{ marginBottom: 'var(--space-6)' }}>
          {/* 有効にするスイッチ */}
          <div style={{ marginBottom: 'var(--space-4)' }}>
            <label style={{
              display: 'flex',
              alignItems: 'center',
              gap: 'var(--space-3)',
              cursor: 'pointer',
              userSelect: 'none',
            }}>
              <input
                type="checkbox"
                checked={enabledInput}
                onChange={(e) => {
                  setEnabledInput(e.target.checked);
                }}
                disabled={saveMutation.isPending}
                style={{ cursor: 'pointer', width: 18, height: 18 }}
              />
              <div>
                <div style={{
                  fontWeight: 'var(--font-weight-semibold)',
                  color: 'var(--color-text-primary)',
                }}>
                  {t('projectSettings.aiKickEnable')}
                </div>
              </div>
            </label>
          </div>

          {/* リポジトリ */}
          <div className="admin__field">
            <label className="admin__label" htmlFor="ai-kick-repo">
              {t('projectSettings.aiKickRepo')}
            </label>
            {/* 自由入力ではなく、システム管理者が許可した一覧から選ぶ(ほかのチームのリポジトリを指定させない) */}
            <select
              id="ai-kick-repo"
              className="admin__input"
              value={repoInput}
              onChange={(e) => {
                setRepoInput(e.target.value);
                setRepoError('');
              }}
              disabled={saveMutation.isPending}
            >
              <option value="">{t('projectSettings.aiKickRepoSelect')}</option>
              {currentNotAllowed && (
                <option value={repoInput}>
                  {repoInput} {t('projectSettings.aiKickRepoNotAllowed')}
                </option>
              )}
              {allowedRepos.map((r) => (
                <option key={r} value={r}>{r}</option>
              ))}
            </select>
            <div className="admin__hint">
              {allowedRepos.length === 0
                ? t('projectSettings.aiKickRepoNoneAllowed')
                : t('projectSettings.aiKickRepoHint')}
            </div>
            {repoError && (
              <div style={{
                fontSize: 'var(--font-size-sm)',
                color: '#dc2626',
                marginTop: 'var(--space-1)',
              }}>
                {repoError}
              </div>
            )}
          </div>

          {/* ベースブランチ */}
          <div className="admin__field">
            <label className="admin__label" htmlFor="ai-kick-branch">
              {t('projectSettings.aiKickBranch')}
            </label>
            <input
              id="ai-kick-branch"
              className="admin__input"
              type="text"
              value={branchInput}
              onChange={(e) => {
                setBranchInput(e.target.value);
              }}
              placeholder="develop"
              disabled={saveMutation.isPending}
            />
            <div className="admin__hint">{t('projectSettings.aiKickBranchDefault')}</div>
          </div>

          {/* 外部の AI(Anthropic)に送られることの確認(有効にするときは必須) */}
          {enabledInput && (
            <div className="admin__field">
              <label style={{ display: 'flex', alignItems: 'flex-start', gap: 'var(--space-2)', cursor: 'pointer' }}>
                <input
                  type="checkbox"
                  data-testid="ai-kick-data-sharing-ack"
                  checked={ackInput}
                  onChange={(e) => {
                    setAckInput(e.target.checked);
                    setRepoError('');
                  }}
                  disabled={saveMutation.isPending}
                  style={{ marginTop: 3, width: 16, height: 16, flexShrink: 0 }}
                />
                <span style={{ fontSize: 'var(--font-size-sm)', color: 'var(--color-text-secondary)' }}>
                  {t('projectSettings.aiKickDataSharing')}
                </span>
              </label>
            </div>
          )}

          {/* 保存ボタン */}
          <div className="admin__actions" style={{ marginTop: 'var(--space-4)' }}>
            <button
              type="button"
              className="admin__btn admin__btn--primary"
              disabled={!hasChanged || saveMutation.isPending || !settings.canEdit}
              onClick={() => saveMutation.mutate()}
            >
              {saveMutation.isPending ? t('common.saving') : t('common.save')}
            </button>
          </div>
        </div>
      </fieldset>
    </div>
  );
}
