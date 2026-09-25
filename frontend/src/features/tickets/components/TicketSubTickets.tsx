import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router-dom';
import { useSubTickets } from '../../../shared/sync/repos/ticketRepo';
import { isTempTicketKey } from '../../../shared/sync/ticketWrites';
import { syncStateOf } from '../../../shared/sync/ticketMapping';
import { buildTicketDetailPath } from '../utils/ticketNavigation';
import './TicketSubTickets.css';

interface TicketSubTicketsProps {
  parentTicketId: number;
  childCount: number;
  projectPrefix?: string | null;
  teamSlug?: string | null;
}

export function TicketSubTickets({
  parentTicketId,
  childCount,
  projectPrefix,
  teamSlug,
}: TicketSubTicketsProps) {
  const { t } = useTranslation();
  const navigate = useNavigate();

  const { data: childTickets = [], isLoading } = useSubTickets(parentTicketId);

  return (
    <div className="ticket-sub-tickets" data-testid="ticket-sub-tickets">
      <h4 className="ticket-sub-tickets__title">
        {t('ticketDetail.subTickets')}
        {(childCount > 0 || childTickets.length > 0) && (
          <span className="ticket-sub-tickets__count">
            {childTickets.length || childCount}
          </span>
        )}
      </h4>

      {isLoading ? (
        <div className="ticket-sub-tickets__loading">{t('common.loading')}</div>
      ) : childTickets.length === 0 ? (
        <div className="ticket-sub-tickets__empty">{t('ticketDetail.subTicketsEmpty')}</div>
      ) : (
        <div className="ticket-sub-tickets__list">
          {childTickets.map((child) => (
            <button
              key={child.id}
              type="button"
              className="ticket-sub-ticket-item"
              data-sync-state={syncStateOf(child)}
              onClick={() =>
                navigate(
                  buildTicketDetailPath(
                    projectPrefix,
                    child.ticketKey,
                    undefined,
                    teamSlug,
                  ),
                )
              }
            >
              <div className="ticket-sub-ticket-item__header">
                <span className="ticket-sub-ticket-item__key">
                  {isTempTicketKey(child.ticketKey) ? (
                    <span className="sync-badge sync-badge--creating">{t('sync.creating')}</span>
                  ) : (
                    child.ticketKey
                  )}
                </span>
                <span
                  className={`ticket-sub-ticket-item__status ticket-sub-ticket-item__status--${child.status}`}
                >
                  {child.status}
                </span>
              </div>
              <p className="ticket-sub-ticket-item__title">{child.title}</p>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
