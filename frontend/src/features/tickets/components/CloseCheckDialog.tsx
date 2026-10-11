/**
 * CloseCheckDialog.tsx — クローズ前の確認ダイアログ
 *
 * バックエンド検証エンジンから戻ったブロッカー一覧を表示し、
 * ユーザーが「それでも完了にする」を選んだら親の完了処理を実行する。
 */

import { useTranslation } from 'react-i18next';
import type { CloseCheckResponse } from '@/shared/api/types';

interface Props {
  isOpen: boolean;
  closeCheckResult: CloseCheckResponse;
  onConfirm: () => void;
  onCancel: () => void;
}

export function CloseCheckDialog({ isOpen, closeCheckResult, onConfirm, onCancel }: Props) {
  const { t } = useTranslation();

  if (!isOpen || !closeCheckResult) return null;

  const overlayStyle: React.CSSProperties = {
    position: 'fixed',
    inset: 0,
    background: 'rgba(0,0,0,0.5)',
    zIndex: 1000,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
  };

  const dialogStyle: React.CSSProperties = {
    background: 'var(--color-bg-elevated, #1f1f1f)',
    color: 'var(--color-text-primary)',
    border: '1px solid var(--color-border-default, #2a2a2a)',
    borderRadius: '12px',
    padding: '1.5rem',
    maxWidth: '560px',
    width: '90vw',
    maxHeight: '80vh',
    overflowY: 'auto',
    boxShadow: '0 20px 60px rgba(0,0,0,0.3)',
  };

  const blockerStyle: React.CSSProperties = {
    padding: '0.75rem',
    borderRadius: '8px',
    marginBottom: '0.75rem',
    background: 'rgba(229, 77, 77, 0.12)',
    border: '1px solid rgba(229, 77, 77, 0.4)',
    fontSize: '0.8125rem',
  };

  const getBlockerMessage = (code: string, arg: string, message: string): string => {
    const key = `closeCheck.blocker.${code}`;
    const translated = t(key, { defaultValue: message });
    // i18n キーが見つかった場合は arg を差し込む
    if (translated !== key) {
      return translated.replace('{{arg}}', arg);
    }
    // デフォルトメッセージを使う
    return message;
  };

  return (
    <div style={overlayStyle} onClick={onCancel}>
      <div
        style={dialogStyle}
        role="dialog"
        aria-modal="true"
        aria-label={t('closeCheck.title')}
        onClick={(e) => e.stopPropagation()}
      >
        <div
          style={{
            display: 'flex',
            justifyContent: 'space-between',
            alignItems: 'center',
            marginBottom: '1rem',
          }}
        >
          <h3 style={{ margin: 0, fontSize: '1rem' }}>⚠️ {t('closeCheck.title')}</h3>
          <button
            onClick={onCancel}
            style={{
              background: 'none',
              border: 'none',
              fontSize: '1.25rem',
              cursor: 'pointer',
              opacity: 0.6,
            }}
            aria-label={t('closeCheck.cancelButton')}
          >
            ✕
          </button>
        </div>

        {closeCheckResult.blockers.length > 0 && (
          <div style={{ marginBottom: '1.5rem' }}>
            <p style={{ margin: '0 0 0.75rem', fontSize: '0.875rem', opacity: 0.8 }}>
              {t('closeCheck.description')}
            </p>
            <div>
              {closeCheckResult.blockers.map((blocker, index) => (
                <div key={index} style={blockerStyle}>
                  {getBlockerMessage(blocker.code, blocker.arg, blocker.message)}
                </div>
              ))}
            </div>
          </div>
        )}

        <div style={{ display: 'flex', gap: '0.75rem', justifyContent: 'flex-end', marginTop: '1.5rem' }}>
          <button
            onClick={onCancel}
            autoFocus
            style={{
              padding: '0.5rem 1rem',
              borderRadius: '6px',
              border: '1px solid var(--color-border-default, #2a2a2a)',
              background: 'transparent',
              color: 'var(--color-text-primary)',
              cursor: 'pointer',
              fontSize: '0.8125rem',
            }}
          >
            {t('closeCheck.cancelButton')}
          </button>
          <button
            onClick={onConfirm}
            style={{
              padding: '0.5rem 1rem',
              borderRadius: '6px',
              border: 'none',
              background: 'var(--color-accent-primary, #5c6cff)',
              color: '#fff',
              cursor: 'pointer',
              fontSize: '0.8125rem',
            }}
          >
            {t('closeCheck.confirmButton')}
          </button>
        </div>
      </div>
    </div>
  );
}
