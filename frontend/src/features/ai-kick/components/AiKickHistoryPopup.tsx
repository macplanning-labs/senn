/**
 * AiKickHistoryPopup.tsx - AI Kick 実行履歴のポップアップ
 *
 * チケットの AI 実行を一覧で表示する（1 実行 1 行）。
 * 実行中の行には取り消しボタンを出す。
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
import { isActive, statusLabelKey, formatTokens } from '../format';
import '../styles/AiKickHistoryPopup.css';
import '../i18n';

interface AiKickHistoryPopupProps {
  ticketKey: string;
  onClose: () => void;
}

export function AiKickHistoryPopup({ ticketKey, onClose }: AiKickHistoryPopupProps) {
  const { t } = useTranslation();
  const toast = useToast();
  const [runs, setRuns] = useState<AiKickRun[]>([]);
  const [cancellingUuid, setCancellingUuid] = useState<string | null>(null);

  // 実行一覧を取得（新しい順）
  const loadRuns = useCallback(async () => {
    try {
      const response = await getAiKickRuns(ticketKey);
      setRuns(response.runs || []);
    } catch (error) {
      console.error('Failed to load AI runs:', error);
    }
  }, [ticketKey]);

  // マウント時と ticketKey 変更時に取得
  useEffect(() => {
    void loadRuns();
  }, [loadRuns]);

  // 実行中がある間は 10 秒ごとに再取得
  const hasActive = runs.some((r) => isActive(r));
  useEffect(() => {
    if (!hasActive) return;

    const interval = setInterval(() => {
      void loadRuns();
    }, 10_000);

    return () => clearInterval(interval);
  }, [hasActive, loadRuns]);

  // Esc キーで閉じる
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose();
      }
    };
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [onClose]);

  // 実行を取り消す（AiKickStatus と同じ文言・同じ処理）
  const handleCancel = async (run: AiKickRun) => {
    if (!confirm(t('aiKick.statusPanel.cancelConfirm'))) {
      return;
    }

    setCancellingUuid(run.runUuid);
    try {
      await cancelAiKickRun(ticketKey, run.runUuid);
      toast.success(t('aiKick.statusPanel.cancelSuccess'));
      void loadRuns();
    } catch (error) {
      const parsed = parseAiKickError(error);
      toast.error(parsed.detail || t('aiKick.statusPanel.cancelError'));
    } finally {
      setCancellingUuid(null);
    }
  };

  return (
    <div
      className="ai-kick-history-popup__overlay"
      onClick={onClose}
    >
      <div
        className="ai-kick-history-popup"
        role="dialog"
        aria-modal="true"
        aria-label={t('aiKick.statusPanel.popupTitle')}
        data-testid="ai-kick-history-popup"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="ai-kick-history-popup__header">
          <h2 className="ai-kick-history-popup__title">{t('aiKick.statusPanel.popupTitle')}</h2>
          <button
            type="button"
            className="ai-kick-history-popup__close-x"
            onClick={onClose}
            aria-label={t('aiKick.statusPanel.popupClose')}
          >
            ×
          </button>
        </div>

        <div className="ai-kick-history-popup__body">
          {runs.length === 0 ? (
            <p className="ai-kick-history-popup__empty">{t('aiKick.statusPanel.popupEmpty')}</p>
          ) : (
            <table className="ai-kick-history-popup__table">
              <thead>
                <tr>
                  <th scope="col">{t('aiKick.statusPanel.colWhen')}</th>
                  <th scope="col">{t('aiKick.statusPanel.colWho')}</th>
                  <th scope="col">{t('aiKick.statusPanel.colStatus')}</th>
                  <th scope="col">{t('aiKick.statusPanel.colTokens')}</th>
                  <th scope="col" aria-label={t('aiKick.statusPanel.cancel')} />
                </tr>
              </thead>
              <tbody>
                {runs.map((run) => (
                  <tr key={run.runUuid} data-testid="ai-kick-history-row">
                    <td className="ai-kick-history-popup__time">
                      {new Date(run.queuedAt).toLocaleString()}
                    </td>
                    <td
                      className="ai-kick-history-popup__who"
                      title={run.triggeredBy?.name ?? undefined}
                      data-testid="ai-kick-history-who"
                    >
                      {run.triggeredBy?.name ?? '—'}
                    </td>
                    <td>
                      <span className={`ai-kick-history-popup__status ai-kick-history-popup__status--${run.status}`}>
                        {t(statusLabelKey(run))}
                      </span>
                    </td>
                    <td className="ai-kick-history-popup__tokens">
                      {run.weightedTokens > 0 ? formatTokens(run.weightedTokens) : ''}
                    </td>
                    <td className="ai-kick-history-popup__actions">
                      {isActive(run) && (
                        <button
                          type="button"
                          className="ai-kick-history-popup__cancel-btn"
                          onClick={() => void handleCancel(run)}
                          disabled={cancellingUuid !== null}
                          data-testid="ai-kick-cancel-btn"
                        >
                          {cancellingUuid === run.runUuid ? '⏳' : t('aiKick.statusPanel.cancel')}
                        </button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>

        <div className="ai-kick-history-popup__footer">
          <button
            type="button"
            className="ai-kick-history-popup__close-btn"
            onClick={onClose}
          >
            {t('aiKick.statusPanel.popupClose')}
          </button>
        </div>
      </div>
    </div>
  );
}
