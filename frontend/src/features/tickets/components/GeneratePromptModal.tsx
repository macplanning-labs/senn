import { useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { usePromptGenerationStore } from '@/shared/stores/promptGenerationStore';
import './GeneratePromptModal.css';

let toastTimeout: ReturnType<typeof setTimeout> | null = null;

export function GeneratePromptModal() {
  const { t } = useTranslation();
  const modalRef = useRef<HTMLDivElement>(null);
  const previewRef = useRef<HTMLTextAreaElement>(null);

  const {
    isOpen,
    phase,
    ticketKey,
    ticketTitle,
    promptText,
    errorMessage,
    model,
    close,
    copyToClipboard,
  } = usePromptGenerationStore();

  useEffect(() => {
    if (previewRef.current && phase === 'success') {
      previewRef.current.scrollTop = 0;
    }
  }, [phase]);

  const handleCopy = async () => {
    try {
      await copyToClipboard();
      showToast(t('ai.promptCopied', { ticketKey }));
    } catch (error) {
      console.error('Copy failed:', error);
    }
  };

  const showToast = (message: string) => {
    if (toastTimeout) clearTimeout(toastTimeout);
    const toast = document.createElement('div');
    toast.className = 'prompt-modal__toast';
    toast.textContent = message;
    document.body.appendChild(toast);
    toastTimeout = setTimeout(() => {
      toast.remove();
    }, 3000);
  };

  if (!isOpen) return null;

  return (
    <div className="prompt-modal__overlay" onClick={close}>
      <div
        className="prompt-modal"
        onClick={(e) => e.stopPropagation()}
        ref={modalRef}
        data-testid="generate-prompt-modal"
      >
        <div className="prompt-modal__header">
          <h2 className="prompt-modal__title">{t('ai.promptModalTitle')}</h2>
          <p className="prompt-modal__subtitle">
            {ticketKey} — {ticketTitle}
          </p>
          <button
            className="prompt-modal__close"
            onClick={close}
            aria-label="Close"
          >
            ×
          </button>
        </div>

        <div className="prompt-modal__body">
          {phase === 'generating' && (
            <div className="prompt-modal__generating">
              <div className="prompt-modal__spinner" />
              <p className="prompt-modal__generating-text">
                ⏳ {t('ai.promptGenerating')} ({model})
              </p>
            </div>
          )}

          {phase === 'success' && (
            <div className="prompt-modal__preview-section">
              <label className="prompt-modal__preview-label">
                {t('ai.promptPreview')}
              </label>
              <textarea
                ref={previewRef}
                className="prompt-modal__preview"
                value={promptText}
                readOnly
              />
            </div>
          )}

          {phase === 'error' && (
            <div className="prompt-modal__error">
              <p className="prompt-modal__error-text">{errorMessage}</p>
            </div>
          )}
        </div>

        <div className="prompt-modal__footer">
          <button
            className="prompt-modal__button prompt-modal__button--close"
            onClick={close}
          >
            {t('common.close', { defaultValue: '閉じる' })}
          </button>
          {phase === 'success' && (
            <button
              className="prompt-modal__button prompt-modal__button--copy"
              onClick={handleCopy}
            >
              {t('ai.promptCopy')}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
