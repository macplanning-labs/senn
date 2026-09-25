import { useParams } from 'react-router-dom';
import { useRef, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { useTicketDetail } from '../../../shared/sync/repos/ticketRepo';
import { syncStateOf } from '../../../shared/sync/ticketMapping';
import type { TicketDetailView } from '../types/ticketDetailView';
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
import './TicketDetailPage.css';

export function TicketDetailPage() {
  const { t } = useTranslation();
  const { ticketId } = useParams<{ ticketId: string }>();
  const editorRef = useRef<TicketTitleDescriptionEditorHandle>(null);

  const { data: ticket, isLoading, isError } = useTicketDetail(ticketId);

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
  // 端末内 DB の行＋付随データ（TicketDetailData）は、画面の型 TicketDetailView と項目名が同じ
  const ticketView = ticket as unknown as TicketDetailView;

  return (
    <div className="ticket-detail-page" data-testid="ticket-detail-page" data-sync-state={syncStateOf(ticket)}>
      <TicketDetailTopBar ticket={ticketView} ticketId={ticketId!} />
      <TicketDetailSubBar ticket={ticketView} />

      <div className="ticket-detail-main">
        <div className="ticket-detail-content">
          <TicketTitleDescriptionEditor
            ref={editorRef}
            ticket={ticketView}
            ticketId={ticketId!}
          />

          <TicketSubTickets
            parentTicketId={ticket.id}
            childCount={ticket.childCount ?? 0}
            projectPrefix={ticket.projectPrefix}
            teamSlug={ticket.team?.slug}
          />

          <TicketRelations ticketKey={ticket.ticketKey} links={ticketView.links || []} />

          <ChangeLogTimeline
            ticketId={ticket.ticketKey}
            createdAt={ticket.createdAt}
            createdByName={authorName}
          />

          <TicketComments
            ticketId={ticketId!}
            comments={ticketView.comments || []}
            attachments={ticketView.attachments}
            projectId={ticket.project}
            teamId={ticket.team?.id}
          />
        </div>

        <aside className="ticket-detail-sidebar">
          <TicketPropertiesSidebar
            ticket={ticketView}
            ticketId={ticketId!}
            onFocusTitle={handleFocusTitle}
            onFocusDescription={handleFocusDescription}
          />
        </aside>
      </div>
    </div>
  );
}
