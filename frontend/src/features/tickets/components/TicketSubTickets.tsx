import { useCallback, useEffect, useRef, useState, type FormEvent, type KeyboardEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router-dom';
import { useSubTickets } from '../../../shared/sync/repos/ticketRepo';
import { bumpTicketCounter, isTempTicketKey, localCreateTicket } from '../../../shared/sync/ticketWrites';
import { syncStateOf } from '../../../shared/sync/ticketMapping';
import { buildTicketDetailPath } from '../utils/ticketNavigation';
import { useStatusOptions } from '../hooks/useStatusOptions';
import type { TicketDetailView } from '../types/ticketDetailView';
import './TicketSubTickets.css';

const DONE_STATUSES = new Set(['resolved', 'closed']);

interface TicketSubTicketsProps {
  parentTicket: TicketDetailView;
}

function userInitials(displayName: string, username: string): string {
  const source = displayName.trim() || username.trim();
  if (!source) return '?';
  const parts = source.split(/\s+/).filter(Boolean);
  if (parts.length >= 2) {
    return `${parts[0]![0] ?? ''}${parts[1]![0] ?? ''}`.toUpperCase();
  }
  return source.slice(0, 2).toUpperCase();
}

export function TicketSubTickets({ parentTicket }: TicketSubTicketsProps) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const inputRef = useRef<HTMLInputElement>(null);

  const [formOpen, setFormOpen] = useState(false);
  const [title, setTitle] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);

  const { data: childTickets = [], isLoading } = useSubTickets(parentTicket.id);
  const { options: statusOptions, labelOf: statusLabelOf } = useStatusOptions(
    parentTicket.project,
    parentTicket.team?.id,
  );

  const totalCount = childTickets.length;
  const completedCount = childTickets.filter((child) => DONE_STATUSES.has(child.status)).length;

  const openForm = useCallback(() => {
    setFormOpen(true);
  }, []);

  const closeForm = useCallback(() => {
    setFormOpen(false);
    setTitle('');
  }, []);

  useEffect(() => {
    if (!formOpen) return;
    const timer = window.setTimeout(() => inputRef.current?.focus(), 0);
    return () => window.clearTimeout(timer);
  }, [formOpen]);

  useEffect(() => {
    const handleKeyDown = (e: globalThis.KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey) || !e.shiftKey || e.key.toLowerCase() !== 'o') return;
      if (e.altKey) return;
      e.preventDefault();
      openForm();
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [openForm]);

  const handleCreate = useCallback(async () => {
    const trimmed = title.trim();
    if (!trimmed || isSubmitting) return;

    setIsSubmitting(true);
    try {
      const labelIds = parentTicket.labels.map((label) => label.id);
      await localCreateTicket(
        {
          title: trimmed,
          ticket_type: parentTicket.ticketType ?? 'task',
          status: 'open',
          priority: parentTicket.priority,
          project: parentTicket.project,
          cycle: parentTicket.cycle,
          labels: labelIds.length > 0 ? labelIds : undefined,
          parent: parentTicket.id,
          team_id: parentTicket.team?.id ?? null,
        },
        {
          projectPrefix: parentTicket.projectPrefix ?? null,
          projectName: parentTicket.projectName ?? null,
          cycleName: parentTicket.cycleName,
          labels: parentTicket.labels,
          team: parentTicket.team
            ? {
                id: parentTicket.team.id,
                name: parentTicket.team.name,
                slug: parentTicket.team.slug,
                icon: '',
                color: '',
              }
            : null,
          parent: parentTicket.id,
          assignees: [],
        },
      );
      await bumpTicketCounter(parentTicket.ticketKey, 'childCount', 1);
      closeForm();
    } finally {
      setIsSubmitting(false);
    }
  }, [closeForm, isSubmitting, parentTicket, title]);

  const handleFormSubmit = (e: FormEvent) => {
    e.preventDefault();
    void handleCreate();
  };

  const handleInputKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      closeForm();
    }
  };

  const statusColor = (status: string): string =>
    statusOptions.find((option) => option.value === status)?.color ?? 'var(--color-text-secondary, #a0a0a0)';

  return (
    <div className="ticket-sub-tickets" data-testid="ticket-sub-tickets">
      {totalCount > 0 && (
        <div className="ticket-sub-tickets__header">
          <h4 className="ticket-sub-tickets__title">
            {t('ticketDetail.subTickets')}
            <span className="ticket-sub-tickets__progress" data-testid="ticket-sub-tickets-progress">
              {completedCount} / {totalCount}
            </span>
          </h4>
        </div>
      )}

      {formOpen && (
        <form className="ticket-sub-tickets__form" onSubmit={handleFormSubmit} data-testid="ticket-sub-tickets-form">
          <input
            ref={inputRef}
            type="text"
            className="ticket-sub-tickets__input"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            onKeyDown={handleInputKeyDown}
            placeholder={t('ticketDetail.subTicketsTitlePlaceholder')}
            disabled={isSubmitting}
            maxLength={500}
            data-testid="ticket-sub-tickets-title-input"
          />
          <div className="ticket-sub-tickets__form-actions">
            <button
              type="button"
              className="ticket-sub-tickets__cancel-btn"
              onClick={closeForm}
              disabled={isSubmitting}
            >
              {t('common.cancel')}
            </button>
            <button
              type="submit"
              className="ticket-sub-tickets__submit-btn"
              disabled={!title.trim() || isSubmitting}
              data-testid="ticket-sub-tickets-submit-btn"
            >
              {t('ticketDetail.subTicketsCreate')}
            </button>
          </div>
        </form>
      )}

      {isLoading ? (
        <div className="ticket-sub-tickets__loading">{t('common.loading')}</div>
      ) : totalCount > 0 ? (
        <div className="ticket-sub-tickets__list">
          {childTickets.map((child) => {
            const primaryAssignee = child.assignees[0];
            return (
              <button
                key={child.id}
                type="button"
                className="ticket-sub-ticket-item"
                data-sync-state={syncStateOf(child)}
                onClick={() =>
                  navigate(
                    buildTicketDetailPath(
                      parentTicket.projectPrefix,
                      child.ticketKey,
                      undefined,
                      parentTicket.team?.slug,
                    ),
                  )
                }
              >
                <span
                  className="ticket-sub-ticket-item__status-icon"
                  style={{ backgroundColor: statusColor(child.status) }}
                  title={statusLabelOf(child.status)}
                  aria-hidden="true"
                />
                <span className="ticket-sub-ticket-item__key">
                  {isTempTicketKey(child.ticketKey) ? (
                    <span className="sync-badge sync-badge--creating">{t('sync.creating')}</span>
                  ) : (
                    child.ticketKey
                  )}
                </span>
                <span className="ticket-sub-ticket-item__title">{child.title}</span>
                <span
                  className={`ticket-sub-ticket-item__assignee${primaryAssignee ? '' : ' ticket-sub-ticket-item__assignee--empty'}`}
                  title={
                    primaryAssignee
                      ? primaryAssignee.displayName || primaryAssignee.username
                      : t('ticketDetail.unassigned')
                  }
                >
                  {primaryAssignee
                    ? userInitials(primaryAssignee.displayName, primaryAssignee.username)
                    : '—'}
                </span>
              </button>
            );
          })}
        </div>
      ) : null}

      {!formOpen && (
        <button
          type="button"
          className="ticket-sub-tickets__add-link"
          onClick={openForm}
          data-testid="ticket-sub-tickets-add-btn"
        >
          {t('ticketDetail.subTicketsAdd')}
        </button>
      )}
    </div>
  );
}
