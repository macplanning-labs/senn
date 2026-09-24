import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router-dom';
import { apiClient } from '../../../shared/api/client';
import type { TicketListItem } from '../../../shared/api/types';
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

  const { data: childTickets = [], isLoading } = useQuery<TicketListItem[]>({
    queryKey: ['ticket-children', parentTicketId],
    queryFn: async () => {
      const res = await apiClient.get(`/tickets/`, {
        params: { parent: parentTicketId },
      });
      return res.data?.results || [];
    },
    enabled: !!parentTicketId,
    staleTime: 30_000,
  });

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
                <span className="ticket-sub-ticket-item__key">{child.ticketKey}</span>
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
