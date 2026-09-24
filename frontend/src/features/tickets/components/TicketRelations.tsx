import { useTranslation } from 'react-i18next';
import { GitActivity } from './GitActivity';
import type { ReferenceLink } from '../types/ticketDetailView';
import './TicketRelations.css';

interface TicketRelationsProps {
  ticketKey: string;
  links: ReferenceLink[];
}

export function TicketRelations({ ticketKey, links }: TicketRelationsProps) {
  const { t } = useTranslation();
  const hasLinks = links.length > 0;

  return (
    <div className="ticket-relations" data-testid="ticket-relations">
      <h4 className="ticket-relations__title">{t('ticketDetail.relations')}</h4>

      <div className="ticket-relations__content">
        <div className="ticket-relations__section">
          <h5 className="ticket-relations__section-title">
            {t('ticketDetail.referenceLinks')}
          </h5>
          {hasLinks ? (
            <div className="ticket-relations__links">
              {links.map((link) => (
                <a
                  key={link.id}
                  href={link.url}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="ticket-relations__link-item"
                  title={link.title || link.url}
                >
                  <span className="ticket-relations__link-icon">🔗</span>
                  <span className="ticket-relations__link-title">
                    {link.title || link.url}
                  </span>
                </a>
              ))}
            </div>
          ) : (
            <div className="ticket-relations__empty">{t('ticketDetail.noLinks')}</div>
          )}
        </div>

        <div className="ticket-relations__section">
          <GitActivity ticketKey={ticketKey} />
        </div>
      </div>
    </div>
  );
}
