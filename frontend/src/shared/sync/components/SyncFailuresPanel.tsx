/**
 * SyncFailuresPanel.tsx — 同期失敗した変更のモーダルパネル
 *
 * role="dialog" でアクセシビリティ対応。Esc キー・背景クリックで閉じる。
 */

import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import {
  listFailedChanges,
  retryFailedChange,
  retryAllFailedChanges,
  discardFailedChange,
} from '../failedChanges';
import { useLiveRows } from '../repos/useLiveRows';
import './SyncFailuresPanel.css';

interface Props {
  isOpen: boolean;
  onClose: () => void;
}

export function SyncFailuresPanel({ isOpen, onClose }: Props) {
  const { t } = useTranslation();
  const modalRef = useRef<HTMLDivElement>(null);
  const [isLoading, setIsLoading] = useState(false);

  // リスト取得
  const { data: changes, loaded } = useLiveRows(listFailedChanges, [], []);

  // Esc キーハンドリング
  useEffect(() => {
    if (!isOpen) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose();
      }
    };

    document.addEventListener('keydown', handleKeyDown);
    return () => {
      document.removeEventListener('keydown', handleKeyDown);
    };
  }, [isOpen, onClose]);

  // root に inert を設定してアクセシビリティ対応
  useEffect(() => {
    if (!isOpen) return;

    const rootElement = document.getElementById('root');
    if (rootElement) {
      rootElement.setAttribute('inert', '');
    }

    return () => {
      if (rootElement) {
        rootElement.removeAttribute('inert');
      }
    };
  }, [isOpen]);

  // 背景クリックで閉じる
  const handleBackdropClick = (e: React.MouseEvent<HTMLDivElement>) => {
    if (e.target === e.currentTarget) {
      onClose();
    }
  };

  // 再試行ハンドラ
  const handleRetry = async (id: number) => {
    setIsLoading(true);
    try {
      await retryFailedChange(id);
    } finally {
      setIsLoading(false);
    }
  };

  // すべて再試行ハンドラ
  const handleRetryAll = async () => {
    setIsLoading(true);
    try {
      await retryAllFailedChanges();
    } finally {
      setIsLoading(false);
    }
  };

  // 破棄ハンドラ
  const handleDiscard = async (id: number) => {
    if (!window.confirm(t('sync.discardConfirm'))) {
      return;
    }
    setIsLoading(true);
    try {
      await discardFailedChange(id);
    } finally {
      setIsLoading(false);
    }
  };

  if (!isOpen) return null;

  const isEmpty = loaded && changes.length === 0;

  return createPortal(
    <div className="sync-failures-backdrop" onClick={handleBackdropClick}>
      <div
        ref={modalRef}
        className="sync-failures-panel"
        role="dialog"
        aria-labelledby="sync-failures-title"
        aria-modal="true"
        data-testid="sync-failures-panel"
      >
        <div className="sync-failures-header">
          <h2 id="sync-failures-title" className="sync-failures-title">
            {t('sync.failedTitle')}
          </h2>
          <button
            type="button"
            className="sync-failures-close"
            onClick={onClose}
            aria-label="Close"
          >
            ✕
          </button>
        </div>

        <div className="sync-failures-content">
          {!loaded ? (
            <div className="sync-failures-loading">{t('common.loading')}</div>
          ) : isEmpty ? (
            <div className="sync-failures-empty">{t('sync.failedEmpty')}</div>
          ) : (
            <>
              <div className="sync-failures-actions">
                <button
                  type="button"
                  className="sync-failures-retry-all"
                  onClick={handleRetryAll}
                  disabled={isLoading}
                >
                  {t('sync.retryAll')}
                </button>
              </div>

              <div className="sync-failures-list">
                {changes.map((change) => (
                  <div
                    key={change.id}
                    className="sync-failure-row"
                    data-testid="sync-failure-row"
                  >
                    <div className="sync-failure-info">
                      <div className="sync-failure-operation">
                        {getOperationLabel(change.operation, t)}
                      </div>
                      <div className="sync-failure-label">{change.label}</div>
                      {change.lastError && (
                        <div className="sync-failure-error">{change.lastError}</div>
                      )}
                      <div className="sync-failure-time">
                        {new Date(change.createdAt).toLocaleString()}
                      </div>
                    </div>

                    <div className="sync-failure-actions-item">
                      <button
                        type="button"
                        className="sync-failure-retry"
                        onClick={() => handleRetry(change.id)}
                        disabled={isLoading}
                        data-testid="sync-failure-retry"
                      >
                        {t('sync.retry')}
                      </button>
                      <button
                        type="button"
                        className="sync-failure-discard"
                        onClick={() => handleDiscard(change.id)}
                        disabled={isLoading}
                        data-testid="sync-failure-discard"
                      >
                        {t('sync.discard')}
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            </>
          )}
        </div>
      </div>
    </div>,
    document.body
  );
}

function getOperationLabel(operation: string, t: (key: string) => string): string {
  switch (operation) {
    case 'create':
      return t('sync.opCreate');
    case 'update':
      return t('sync.opUpdate');
    case 'delete':
      return t('sync.opDelete');
    default:
      return operation;
  }
}
