import { useParams } from 'react-router-dom';
import { useRef, useCallback } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { apiClient } from '../../../shared/api/client';
import { TicketDetailTopBar } from '../components/TicketDetailTopBar';
import { TicketDetailSubBar } from '../components/TicketDetailSubBar';
import { TicketPropertiesSidebar } from '../components/TicketPropertiesSidebar';
import ChangeLogTimeline from '../components/ChangeLogTimeline';
import {
  TicketTitleDescriptionEditor,
  type TicketTitleDescriptionEditorHandle,
} from '../components/TicketTitleDescriptionEditor';
import { TicketSubTickets } from '../components/TicketSubTickets';
import { TicketRelations } from '../components/TicketRelations';
import { TicketComments } from '../components/TicketComments';
import type { TicketDetailView } from '../types/ticketDetailView';
import './TicketDetailPage.css';

export function TicketDetailPage() {
  const { t } = useTranslation();
  const { ticketId } = useParams<{ ticketId: string }>();
  const editorRef = useRef<TicketTitleDescriptionEditorHandle>(null);

  const { data: ticket, isLoading, isError } = useQuery<TicketDetailView>({
    queryKey: ['ticket', ticketId],
    queryFn: async () => {
      const res = await apiClient.get<TicketDetailView>(`/tickets/${ticketId}/`);
      return res.data;
    },
    enabled: !!ticketId,
  });

  const handleFocusTitle = useCallback(() => {
    editorRef.current?.startTitleEdit();
  }, []);

  const handleFocusDescription = useCallback(() => {
    editorRef.current?.startDescriptionEdit();
  }, []);

  if (isLoading) {
    return (
      <div className="ticket-detail-page ticket-detail-page--loading">
        <div className="ticket-detail-spinner" />
      </div>
    );
  }

  if (isError || !ticket) {
    return (
      <div className="ticket-detail-page ticket-detail-page--error">
        <p>{t('ticketDetail.notFound')}</p>
      </div>
    );
  }

  const authorName = ticket.author?.displayName || ticket.author?.username || null;

  return (
    <div className="ticket-detail-page" data-testid="ticket-detail-page">
      <TicketDetailTopBar ticket={ticket} ticketId={ticketId!} />
      <TicketDetailSubBar ticket={ticket} />

      <div className="ticket-detail-main">
        <div className="ticket-detail-content">
          <TicketTitleDescriptionEditor
            ref={editorRef}
            ticket={ticket}
            ticketId={ticketId!}
          />

          <TicketSubTickets
            parentTicketId={ticket.id}
            childCount={ticket.childCount ?? 0}
            projectPrefix={ticket.projectPrefix}
            teamSlug={ticket.team?.slug}
          />

          <TicketRelations ticketKey={ticket.ticketKey} links={ticket.links || []} />

          <ChangeLogTimeline
            ticketId={ticket.ticketKey}
            createdAt={ticket.createdAt}
            createdByName={authorName}
          />

          <TicketComments
            ticketId={ticketId!}
            comments={ticket.comments || []}
            attachments={ticket.attachments}
            projectId={ticket.project}
            teamId={ticket.team?.id}
          />
        </div>

        <aside className="ticket-detail-sidebar">
          <TicketPropertiesSidebar
            ticket={ticket}
            ticketId={ticketId!}
            onFocusTitle={handleFocusTitle}
            onFocusDescription={handleFocusDescription}
          />
        </aside>
      </div>
    </div>
  );
}
