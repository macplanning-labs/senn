import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router-dom';
import { useTicketDependencies } from '../hooks/useTicketDependencies';
import { useTicketRelationActions } from '../hooks/useTicketRelationActions';
import {
  classifyDependencies,
  peerTicketKey,
  peerTicketTitle,
} from '../utils/classifyDependencies';
import { buildTicketDetailPath } from '../utils/ticketNavigation';
import './TicketRelationsSidebar.css';

type RelationGroup = 'blocking' | 'blockedBy' | 'related';

const GROUPS: RelationGroup[] = ['blocking', 'blockedBy', 'related'];

const GROUP_FLAGS: Record<RelationGroup, string | null> = {
  blockedBy: '🟠',
  blocking: '🔴',
  related: null,
};

interface TicketRelationsSidebarProps {
  ticketKey: string;
  ticketId: number;
  projectPrefix?: string | null;
  teamSlug?: string | null;
  projectId?: number | null;
  teamId?: number | undefined;
  /** main = メインカラム（アクティビティ直上）、sidebar = Properties（非推奨） */
  variant?: 'main' | 'sidebar';
}

export function TicketRelationsSidebar({
  ticketKey,
  ticketId,
  projectPrefix,
  teamSlug,
  projectId,
  teamId,
  variant = 'main',
}: TicketRelationsSidebarProps) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { data: dependencies = [] } = useTicketDependencies(ticketKey);
  const { handleDelete, isDeleting } = useTicketRelationActions(
    ticketKey,
    ticketId,
    projectId,
    teamId,
  );

  const classified = useMemo(
    () => classifyDependencies(dependencies, ticketId),
    [dependencies, ticketId],
  );

  const groupTitle = useMemo(
    () => ({
      blocking: t('ticketDetail.blocking'),
      blockedBy: t('ticketDetail.blockedBy'),
      related: t('ticketDetail.related'),
    }),
    [t],
  );

  const visibleGroups = GROUPS.filter((group) => classified[group].length > 0);
  if (visibleGroups.length === 0) return null;

  const rootClass =
    variant === 'sidebar' ? 'ticket-relations-sidebar' : 'ticket-relations-main';

  return (
    <section className={rootClass} data-testid="ticket-relations-main">
      {variant === 'main' && (
        <h5 className="ticket-relations-main__title">{t('ticketDetail.relations')}</h5>
      )}
      <div className="ticket-relations-main__groups">
      {visibleGroups.map((group) => (
        <div key={group} className="ticket-relations-sidebar__group">
          <span className="ticket-relations-sidebar__label">{groupTitle[group]}</span>
          <ul className="ticket-relations-sidebar__rows">
            {classified[group].map((dep) => {
              const peerKey = peerTicketKey(dep, ticketId);
              const peerTitle = peerTicketTitle(dep, ticketId);
              const flag = GROUP_FLAGS[group];
              return (
                <li key={dep.id} className="ticket-relations-sidebar__row">
                  <button
                    type="button"
                    className="ticket-relations-sidebar__row-main"
                    onClick={() =>
                      navigate(buildTicketDetailPath(projectPrefix, peerKey, undefined, teamSlug))
                    }
                    data-testid={`ticket-relation-sidebar-row-${dep.id}`}
                  >
                    {flag && (
                      <span className="ticket-relations-sidebar__flag" aria-hidden="true">
                        {flag}
                      </span>
                    )}
                    <span className="ticket-relations-sidebar__key">{peerKey}</span>
                    <span className="ticket-relations-sidebar__title">{peerTitle}</span>
                  </button>
                  <button
                    type="button"
                    className="ticket-relations-sidebar__remove"
                    onClick={() => void handleDelete(dep.id)}
                    disabled={isDeleting}
                    aria-label={t('ticketDetail.removeRelation')}
                    data-testid={`ticket-relation-sidebar-remove-${dep.id}`}
                  >
                    ×
                  </button>
                </li>
              );
            })}
          </ul>
        </div>
      ))}
      </div>
    </section>
  );
}
