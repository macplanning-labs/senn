/**
 * AiKickStatus.tsx - AI Kick 実行状態表示コンポーネント
 *
 * 最新の実行 1 件と、実行履歴を表示
 */

import { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { useToast } from '@/shared/stores/toastStore';
import {
  getAiKickRuns,
  cancelAiKickRun,
  parseAiKickError,
  type AiKickRun,
} from '../api';
import {
  isActive,
  statusLabelKey,
  formatTokens,
  stopReasonLabelKey,
  runResultLabelKey,
  priorityBadgeClass,
} from '../format';
import '../styles/AiKickStatus.css';
import '../i18n';

interface AiKickStatusProps {
  ticketKey: string;
}

export function AiKickStatus({ ticketKey }: AiKickStatusProps) {
  const { t } = useTranslation();
  const toast = useToast();
  const [runs, setRuns] = useState<AiKickRun[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [isCancelling, setIsCancelling] = useState(false);
  const [showHistory, setShowHistory] = useState(false);

  // 実行一覧を取得
  const loadRuns = useCallback(async () => {
    try {
      setIsLoading(true);
      const response = await getAiKickRuns(ticketKey);
      setRuns(response.runs || []);
    } catch (error) {
      console.error('Failed to load AI runs:', error);
    } finally {
      setIsLoading(false);
    }
  }, [ticketKey]);

  // 初回マウント時とticketKey変更時に取得
  useEffect(() => {
    loadRuns();
  }, [loadRuns]);

  // アクティブな実行がある場合のみ自動更新
  const activeRun = runs.find((r) => isActive(r));
  useEffect(() => {
    if (!activeRun) return;

    const interval = setInterval(() => {
      loadRuns();
    }, 10_000); // 10秒ごと

    return () => clearInterval(interval);
  }, [activeRun, loadRuns]);

  // 実行を取り消す
  const handleCancel = async () => {
    if (!activeRun || !confirm(t('aiKick.statusPanel.cancelConfirm'))) {
      return;
    }

    setIsCancelling(true);
    try {
      await cancelAiKickRun(ticketKey, activeRun.runUuid);
      toast.success(t('aiKick.statusPanel.cancelSuccess'));
      loadRuns();
    } catch (error) {
      const parsed = parseAiKickError(error);
      toast.error(parsed.detail || t('aiKick.statusPanel.cancelError'));
    } finally {
      setIsCancelling(false);
    }
  };

  // 実行がない場合は何も表示しない
  if (runs.length === 0) {
    return null;
  }

  const latestRun = runs[0];
  const hasHistory = runs.length > 1;

  if (!latestRun) {
    return null;
  }

  return (
    <div className="ai-kick-status" data-testid="ai-kick-status">
      {/* 最新の実行 */}
      <div className="ai-kick-status__section">
        <h3 className="ai-kick-status__title">{t('aiKick.statusPanel.title')}</h3>

        <div className="ai-kick-status__run">
          {/* 状態バッジ */}
          <div className="ai-kick-status__badge-row">
            <span className={`ai-kick-status__status-badge ai-kick-status__status-badge--${latestRun.status}`}>
              {t(statusLabelKey(latestRun))}
            </span>
            <span className={priorityBadgeClass(latestRun.priorityClass)}>
              {t(`aiKick.priority.${latestRun.priorityClass}`)}
            </span>
          </div>

          {/* トークン数 */}
          {latestRun.weightedTokens > 0 && (
            <div className="ai-kick-status__info-row">
              <span className="ai-kick-status__info-label">Token:</span>
              <span className="ai-kick-status__info-value">{formatTokens(latestRun.weightedTokens)}</span>
            </div>
          )}

          {/* 停止理由 */}
          {latestRun.stopReason && (
            <div className="ai-kick-status__info-row">
              <span className="ai-kick-status__info-label">Stopped:</span>
              <span className="ai-kick-status__info-value">
                {t(stopReasonLabelKey(latestRun.stopReason))}
              </span>
            </div>
          )}

          {/* 結果 */}
          {latestRun.result && (
            <div className="ai-kick-status__info-row">
              <span className="ai-kick-status__info-label">Result:</span>
              <span className="ai-kick-status__info-value">
                {t(runResultLabelKey(latestRun))}
              </span>
            </div>
          )}

          {/* 要約（折りたたみ） */}
          {latestRun.summary && (
            <details className="ai-kick-status__summary-details">
              <summary className="ai-kick-status__summary-summary">
                {t('aiKick.statusPanel.summary')}
              </summary>
              <div className="ai-kick-status__summary-content">
                <p className="ai-kick-status__summary-text">{latestRun.summary}</p>
              </div>
            </details>
          )}

          {/* エラー詳細 */}
          {latestRun.errorDetail && (
            <details className="ai-kick-status__error-details">
              <summary className="ai-kick-status__error-summary">
                {t('aiKick.statusPanel.error')}
              </summary>
              <div className="ai-kick-status__error-content">
                <pre className="ai-kick-status__error-text">{latestRun.errorDetail}</pre>
              </div>
            </details>
          )}

          {/* 取り消しボタン（アクティブな実行の場合のみ） */}
          {isActive(latestRun) && (
            <button
              className="ai-kick-status__cancel-btn"
              onClick={handleCancel}
              disabled={isCancelling || isLoading}
              data-testid="ai-kick-cancel-btn"
            >
              {isCancelling ? '⏳' : t('aiKick.statusPanel.cancel')}
            </button>
          )}
        </div>

        {/* 履歴 */}
        {hasHistory && (
          <div className="ai-kick-status__history">
            <button
              className="ai-kick-status__history-toggle"
              onClick={() => setShowHistory(!showHistory)}
              aria-expanded={showHistory}
            >
              {showHistory ? '▼' : '▶'} {t('aiKick.statusPanel.history')} ({runs.length})
            </button>

            {showHistory && (
              <div className="ai-kick-status__history-list">
                {runs.slice(1).map((run) => (
                  <div key={run.runUuid} className="ai-kick-status__history-item">
                    <span className={`ai-kick-status__history-status ai-kick-status__history-status--${run.status}`}>
                      {t(statusLabelKey(run))}
                    </span>
                    <span className="ai-kick-status__history-time">
                      {new Date(run.queuedAt).toLocaleString()}
                    </span>
                    {run.weightedTokens > 0 && (
                      <span className="ai-kick-status__history-tokens">
                        {formatTokens(run.weightedTokens)}
                      </span>
                    )}
                  </div>
                ))}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
