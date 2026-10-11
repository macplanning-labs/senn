/**
 * AiKickDialog.tsx - AI Kick 実行ダイアログ
 *
 * プレビュー取得、警告表示、確認、実行
 */

import { useState, useEffect, useRef, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { useToast } from '@/shared/stores/toastStore';
import {
  getAiKickPreview,
  kickAi,
  parseAiKickError,
  type AiKickPreview,
  type AiKickRun,
  type AiKickWarningCode,
} from '../api';
import { formatTokens, priorityBadgeClass } from '../format';
import '../styles/AiKickDialog.css';
import '../i18n';

interface AiKickDialogProps {
  ticketKey: string;
  onClose: () => void;
  onRunCreated?: (run: AiKickRun) => void;
}

type DialogPhase = 'loading' | 'preview' | 'error' | 'executing';

export function AiKickDialog({
  ticketKey,
  onClose,
  onRunCreated,
}: AiKickDialogProps) {
  const { t } = useTranslation();
  const toast = useToast();
  const [phase, setPhase] = useState<DialogPhase>('loading');
  const [preview, setPreview] = useState<AiKickPreview | null>(null);
  const [errorMessage, setErrorMessage] = useState<string>('');
  const [isConfirmed, setIsConfirmed] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const modalRef = useRef<HTMLDivElement>(null);
  const confirmCheckboxRef = useRef<HTMLInputElement>(null);

  // プレビューを取得
  const loadPreview = useCallback(async () => {
    setPhase('loading');
    setErrorMessage('');
    setIsConfirmed(false);
    try {
      const data = await getAiKickPreview(ticketKey);
      setPreview(data);
      setPhase('preview');
    } catch (error) {
      const parsed = parseAiKickError(error);
      setErrorMessage(parsed.detail || t('aiKick.dialog.error.generic'));
      setPhase('error');
    }
  }, [ticketKey, t]);

  // 初回マウント時にプレビューを取得
  useEffect(() => {
    loadPreview();
  }, [loadPreview]);

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

  // フォーカス閉じ込め
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key !== 'Tab') return;
      if (!modalRef.current) return;

      const focusableElements = modalRef.current.querySelectorAll(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
      );
      const firstElement = focusableElements[0] as HTMLElement;
      const lastElement = focusableElements[focusableElements.length - 1] as HTMLElement;

      if (e.shiftKey) {
        if (document.activeElement === firstElement) {
          e.preventDefault();
          lastElement?.focus();
        }
      } else {
        if (document.activeElement === lastElement) {
          e.preventDefault();
          firstElement?.focus();
        }
      }
    };

    const modal = modalRef.current;
    if (modal) {
      modal.addEventListener('keydown', handleKeyDown);
      return () => modal.removeEventListener('keydown', handleKeyDown);
    }
  }, []);

  // 背景スクロール防止
  useEffect(() => {
    const originalOverflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    return () => {
      document.body.style.overflow = originalOverflow;
    };
  }, []);

  const handleExecute = async () => {
    if (!preview || isSubmitting) return;

    setIsSubmitting(true);
    try {
      const response = await kickAi(ticketKey, {
        promptHash: preview.promptHash,
        confirmed: preview.verdict === 'needs_confirm' && isConfirmed,
      });

      toast.success(t('aiKick.dialog.success'));

      if (onRunCreated) {
        onRunCreated({
          runUuid: response.runUuid,
          status: response.status,
          triggerType: 'manual',
          priorityClass: response.priorityClass,
          queuedAt: new Date().toISOString(),
          startedAt: null,
          endedAt: null,
          result: null,
          summary: null,
          weightedTokens: 0,
          stopReason: null,
          errorDetail: null,
        });
      }

      onClose();
    } catch (error) {
      const parsed = parseAiKickError(error);

      if (parsed.code === 'prompt_changed') {
        // プレビューを取り直す
        setErrorMessage(t('aiKick.dialog.error.promptChanged'));
        setIsConfirmed(false);
        if (confirmCheckboxRef.current) {
          confirmCheckboxRef.current.checked = false;
        }
        setPhase('preview');
        // 自動で再取得
        setTimeout(() => loadPreview(), 2000);
        return;
      }

      if (parsed.code === 'needs_confirm') {
        // 警告を更新
        setErrorMessage(t('aiKick.dialog.error.needsConfirm'));
        if (parsed.warnings && preview) {
          setPreview({ ...preview, warnings: parsed.warnings });
        }
        return;
      }

      // その他のエラー
      setErrorMessage(parsed.detail || t('aiKick.dialog.error.generic'));
      setPhase('error');
    } finally {
      setIsSubmitting(false);
    }
  };

  const needsConfirmation = preview?.verdict === 'needs_confirm';
  const canExecute =
    !isSubmitting && preview && preview.verdict !== 'deny' && (!needsConfirmation || isConfirmed);

  const getWarningClass = (code: AiKickWarningCode): string => {
    if (code === 'budget_over' || code === 'emergency') {
      return 'ai-kick-dialog__warning ai-kick-dialog__warning--strong';
    }
    return 'ai-kick-dialog__warning ai-kick-dialog__warning--notice';
  };

  return (
    <div className="ai-kick-dialog__overlay" onClick={onClose}>
      <div
        className="ai-kick-dialog"
        onClick={(e) => e.stopPropagation()}
        ref={modalRef}
        role="dialog"
        aria-modal="true"
        data-testid="ai-kick-dialog"
      >
        {/* ヘッダー */}
        <div className="ai-kick-dialog__header">
          <h2 className="ai-kick-dialog__title">{t('aiKick.dialog.title')}</h2>
          <p className="ai-kick-dialog__subtitle">{ticketKey}</p>
          <button
            className="ai-kick-dialog__close"
            onClick={onClose}
            aria-label={t('aiKick.dialog.close')}
          >
            ×
          </button>
        </div>

        {/* ボディ */}
        <div className="ai-kick-dialog__body">
          {phase === 'loading' && (
            <div className="ai-kick-dialog__loading">
              <div className="ai-kick-dialog__spinner" />
              <p className="ai-kick-dialog__loading-text">{t('aiKick.dialog.preparing')}</p>
            </div>
          )}

          {phase === 'error' && (
            <div className="ai-kick-dialog__error-section">
              <p className="ai-kick-dialog__error-message">{errorMessage}</p>
              <button
                className="ai-kick-dialog__retry-btn"
                onClick={loadPreview}
                disabled={isSubmitting}
              >
                {t('aiKick.dialog.retry')}
              </button>
            </div>
          )}

          {phase === 'preview' && preview && (
            <>
              {/* 優先度情報 */}
              <div className="ai-kick-dialog__section">
                <div className="ai-kick-dialog__priority-info">
                  <span className={priorityBadgeClass(preview.priorityClass)}>
                    {t(`aiKick.priority.${preview.priorityClass}`)}
                  </span>
                  {preview.blocksOpen > 0 && (
                    <p className="ai-kick-dialog__blocks-open">
                      {t('aiKick.dialog.warning.blocksOpen', { count: preview.blocksOpen })}
                    </p>
                  )}
                </div>
              </div>

              {/* リポジトリ・ブランチ情報 */}
              <div className="ai-kick-dialog__section">
                <div className="ai-kick-dialog__info-row">
                  <label>{t('aiKick.dialog.repository')}</label>
                  <code>{preview.repo}</code>
                </div>
                <div className="ai-kick-dialog__info-row">
                  <label>{t('aiKick.dialog.branch')}</label>
                  <code>{preview.baseBranch}</code>
                </div>
              </div>

              {/* 上限情報 */}
              {preview.caps && (
                <div className="ai-kick-dialog__section">
                  <p className="ai-kick-dialog__limits-title">{t('aiKick.dialog.limits.title')}</p>
                  <ul className="ai-kick-dialog__limits-list">
                    <li>
                      {t('aiKick.dialog.limits.max')} {preview.caps.maxTurns} {t('aiKick.dialog.limits.turns')}
                    </li>
                    <li>
                      {t('aiKick.dialog.limits.max')} {preview.caps.maxMinutes} {t('aiKick.dialog.limits.minutes')}
                    </li>
                    <li>
                      {t('aiKick.dialog.limits.max')}{' '}
                      {formatTokens(preview.caps.maxWeightedTokens)}{' '}
                      {t('aiKick.dialog.limits.tokens')}
                    </li>
                  </ul>
                </div>
              )}

              {/* 注意書き */}
              <div className="ai-kick-dialog__section ai-kick-dialog__section--warning">
                <p className="ai-kick-dialog__note">{t('aiKick.dialog.note')}</p>
                {preview.repo && (
                  <p className="ai-kick-dialog__note" data-testid="ai-kick-data-sharing">
                    {t('aiKick.dialog.dataSharing', { repo: preview.repo })}
                  </p>
                )}
              </div>

              {/* 拒否の場合 */}
              {preview.verdict === 'deny' && preview.denied && (
                <div className="ai-kick-dialog__section ai-kick-dialog__section--error">
                  <p className="ai-kick-dialog__denied-title">{t('aiKick.dialog.denied')}</p>
                  <p className="ai-kick-dialog__denied-message">{preview.denied.message}</p>
                </div>
              )}

              {/* 警告 */}
              {preview.warnings.length > 0 && (
                <div className="ai-kick-dialog__section">
                  {preview.warnings.map((warning) => (
                    <div key={warning.code} className={getWarningClass(warning.code)}>
                      <p className="ai-kick-dialog__warning-message">{warning.message}</p>
                    </div>
                  ))}
                </div>
              )}

              {/* 外部コンテンツ警告 */}
              {preview.externalContent && (
                <div className="ai-kick-dialog__section">
                  <div className="ai-kick-dialog__warning ai-kick-dialog__warning--notice">
                    <p className="ai-kick-dialog__warning-message">
                      {t('aiKick.dialog.warning.externalContent')}
                    </p>
                  </div>
                </div>
              )}

              {/* プロンプトプレビュー */}
              {preview.verdict !== 'deny' && (
                <div className="ai-kick-dialog__section">
                  <details className="ai-kick-dialog__preview-details">
                    <summary className="ai-kick-dialog__preview-summary">
                      {t('aiKick.dialog.preview.title')}
                    </summary>
                    <div className="ai-kick-dialog__preview-content">
                      <pre className="ai-kick-dialog__preview-text">{preview.prompt}</pre>
                    </div>
                  </details>
                </div>
              )}

              {/* エラー表示 */}
              {errorMessage && (
                <div className="ai-kick-dialog__section ai-kick-dialog__section--error">
                  <p className="ai-kick-dialog__error-message">{errorMessage}</p>
                </div>
              )}

              {/* 確認チェックボックス（needs_confirm のみ） */}
              {preview.verdict === 'needs_confirm' && (
                <div className="ai-kick-dialog__section">
                  <label className="ai-kick-dialog__confirmation">
                    <input
                      ref={confirmCheckboxRef}
                      type="checkbox"
                      checked={isConfirmed}
                      onChange={(e) => setIsConfirmed(e.target.checked)}
                      disabled={isSubmitting}
                      className="ai-kick-dialog__confirmation-input"
                    />
                    <span className="ai-kick-dialog__confirmation-text">
                      {preview.priorityClass === 'emergency'
                        ? t('aiKick.dialog.confirmation.emergency')
                        : t('aiKick.dialog.confirmation.default')}
                    </span>
                  </label>
                </div>
              )}
            </>
          )}
        </div>

        {/* フッター */}
        {phase === 'preview' && preview && preview.verdict !== 'deny' && (
          <div className="ai-kick-dialog__footer">
            <button className="ai-kick-dialog__btn ai-kick-dialog__btn--secondary" onClick={onClose}>
              {t('aiKick.dialog.cancel')}
            </button>
            <button
              className="ai-kick-dialog__btn ai-kick-dialog__btn--primary"
              onClick={handleExecute}
              disabled={!canExecute}
              data-testid="ai-kick-execute-btn"
            >
              {isSubmitting ? '⏳' : t('aiKick.dialog.execute')}
            </button>
          </div>
        )}

        {phase === 'error' && (
          <div className="ai-kick-dialog__footer">
            <button className="ai-kick-dialog__btn ai-kick-dialog__btn--secondary" onClick={onClose}>
              {t('aiKick.dialog.close')}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
