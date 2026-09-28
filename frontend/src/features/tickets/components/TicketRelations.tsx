import { useCallback, useEffect, useRef, useState, type FormEvent, type KeyboardEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { GitActivity } from './GitActivity';
import { useReferenceLinkActions } from '../hooks/useReferenceLinkActions';
import type { ReferenceLink } from '../types/ticketDetailView';
import './TicketRelations.css';

interface TicketRelationsProps {
  ticketKey: string;
  links: ReferenceLink[];
}

export function TicketRelations({ ticketKey, links }: TicketRelationsProps) {
  const { t } = useTranslation();
  const urlRef = useRef<HTMLInputElement>(null);
  const [formOpen, setFormOpen] = useState(false);
  const [url, setUrl] = useState('');
  const [title, setTitle] = useState('');
  const { handleAdd, handleDelete, isAdding, isDeleting } = useReferenceLinkActions(ticketKey);

  const openForm = useCallback(() => setFormOpen(true), []);
  const closeForm = useCallback(() => {
    setFormOpen(false);
    setUrl('');
    setTitle('');
  }, []);

  useEffect(() => {
    if (!formOpen) return;
    const timer = window.setTimeout(() => urlRef.current?.focus(), 0);
    return () => window.clearTimeout(timer);
  }, [formOpen]);

  const onSubmit = useCallback(
    async (e: FormEvent) => {
      e.preventDefault();
      if (!url.trim() || isAdding) return;
      try {
        await handleAdd(url, title);
        closeForm();
      } catch {
        // toast handled in hook
      }
    },
    [closeForm, handleAdd, isAdding, title, url],
  );

  const onInputKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      closeForm();
    }
  };

  return (
    <>
      <div className="ticket-reference-links" data-testid="ticket-reference-links">
        {links.length > 0 && (
          <>
            <h5 className="ticket-reference-links__title">{t('ticketDetail.referenceLinks')}</h5>
            <ul className="ticket-reference-links__rows">
              {links.map((link) => (
                <li key={link.id} className="ticket-reference-links__row">
                  <a
                    href={link.url}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="ticket-reference-links__row-main"
                    title={link.url}
                  >
                    <span className="ticket-reference-links__icon" aria-hidden="true">
                      🔗
                    </span>
                    <span className="ticket-reference-links__label">{link.title || link.url}</span>
                  </a>
                  <button
                    type="button"
                    className="ticket-reference-links__remove"
                    onClick={() => void handleDelete(link.id)}
                    disabled={isDeleting}
                    aria-label={t('ticketDetail.removeLink')}
                    data-testid={`ticket-reference-link-remove-${link.id}`}
                  >
                    ×
                  </button>
                </li>
              ))}
            </ul>
          </>
        )}

        {formOpen && (
          <form className="ticket-reference-links__form" onSubmit={onSubmit} data-testid="ticket-reference-links-form">
            <input
              ref={urlRef}
              type="url"
              className="ticket-reference-links__input"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              onKeyDown={onInputKeyDown}
              placeholder={t('ticketDetail.linkUrlPlaceholder')}
              disabled={isAdding}
              required
              data-testid="ticket-reference-link-url"
            />
            <input
              type="text"
              className="ticket-reference-links__input"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              onKeyDown={onInputKeyDown}
              placeholder={t('ticketDetail.linkTitlePlaceholder')}
              disabled={isAdding}
              data-testid="ticket-reference-link-title"
            />
            <div className="ticket-reference-links__form-actions">
              <button type="button" className="ticket-reference-links__cancel-btn" onClick={closeForm} disabled={isAdding}>
                {t('common.cancel')}
              </button>
              <button
                type="submit"
                className="ticket-reference-links__submit-btn"
                disabled={!url.trim() || isAdding}
                data-testid="ticket-reference-link-submit"
              >
                {t('ticketDetail.linkCreate')}
              </button>
            </div>
          </form>
        )}

        {!formOpen && (
          <button
            type="button"
            className="ticket-reference-links__add-btn"
            onClick={openForm}
            data-testid="ticket-reference-links-add-btn"
          >
            {t('ticketDetail.linksAdd')}
          </button>
        )}
      </div>

      <GitActivity ticketKey={ticketKey} />
    </>
  );
}
