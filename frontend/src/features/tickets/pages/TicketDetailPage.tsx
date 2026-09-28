import { useParams } from 'react-router-dom';
import { useState, useRef, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { useUIStore } from '@/shared/stores/uiStore';
import { useTicketDetail } from '../../../shared/sync/repos/ticketRepo';
import { syncStateOf } from '../../../shared/sync/ticketMapping';
import { useTicketDependencies } from '../hooks/useTicketDependencies';
import { useTicketBlockedStatus } from '../hooks/useTicketBlockedStatus';
import { useTicketRelationActions } from '../hooks/useTicketRelationActions';
import type { RelationCreateKind } from '../utils/classifyDependencies';
import type { TicketDetailView } from '../types/ticketDetailView';
import { TicketDetailTopBar, type RelationMenuAction } from '../components/TicketDetailTopBar';
import { TicketDetailSubBar } from '../components/TicketDetailSubBar';
import { TicketPropertiesSidebar } from '../components/TicketPropertiesSidebar';
import { TicketRelationPickerDialog } from '../components/TicketRelationPickerDialog';
import ChangeLogTimeline from '../components/ChangeLogTimeline';
import {
  TicketTitleDescriptionEditor,
  type TicketTitleDescriptionEditorHandle,
} from '../components/TicketTitleDescriptionEditor';
import { TicketSubTickets } from '../components/TicketSubTickets';
import { TicketRelations } from '../components/TicketRelations';
import { TicketRelationsSidebar } from '../components/TicketRelationsSidebar';
import { TicketComments } from '../components/TicketComments';
import './TicketDetailPage.css';

export function TicketDetailPage() {
  const { t } = useTranslation();
  const { ticketId } = useParams<{ ticketId: string }>();
  const { openTicketFormModal } = useUIStore();
  const editorRef = useRef<TicketTitleDescriptionEditorHandle>(null);
  const [relationPicker, setRelationPicker] = useState<{
    open: boolean;
    kind: RelationCreateKind;
    kindLocked: boolean;
  }>({ open: false, kind: 'related', kindLocked: true });

  const { data: ticket, isLoading, isError } = useTicketDetail(ticketId);
  const { data: dependencies = [] } = useTicketDependencies(ticket?.ticketKey);
  const { data: blockedStatus } = useTicketBlockedStatus(ticket?.id, dependencies);

  const relationActions = useTicketRelationActions(
    ticket?.ticketKey ?? '',
    ticket?.id ?? 0,
    ticket?.project,
    ticket?.team?.id,
  );

  const openRelationPicker = useCallback(
    (kind: RelationCreateKind, kindLocked = true) => {
      setRelationPicker({ open: true, kind, kindLocked });
    },
    [],
  );

  const handleRelationMenuAction = useCallback(
    (action: RelationMenuAction) => {
      switch (action) {
        case 'createExisting':
          openRelationPicker('related', false);
          return;
        case 'createNew':
          if (!ticket) return;
          openTicketFormModal(ticket.projectPrefix ?? null, ticket.team?.slug ?? null, {
            onCreated: async (created) => {
              await relationActions.handleCreate('related', created);
            },
          });
          return;
        case 'markBlocking':
          openRelationPicker('blocking', true);
          return;
        case 'markBlockedBy':
          openRelationPicker('blockedBy', true);
          return;
        case 'markRelated':
          openRelationPicker('related', true);
          return;
      }
    },
    [openRelationPicker, openTicketFormModal, relationActions, ticket],
  );

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
  const ticketView = ticket as unknown as TicketDetailView;

  return (
    <div className="ticket-detail-page" data-testid="ticket-detail-page" data-sync-state={syncStateOf(ticket)}>
      <TicketDetailTopBar
        ticket={ticketView}
        ticketId={ticketId!}
        onRelationMenuAction={handleRelationMenuAction}
      />
      <TicketDetailSubBar ticket={ticketView} />

      <div className="ticket-detail-main">
        <div className="ticket-detail-content">
          <TicketTitleDescriptionEditor
            ref={editorRef}
            ticket={ticketView}
            ticketId={ticketId!}
            isBlocked={blockedStatus?.isBlocked}
          />

          <TicketSubTickets parentTicket={ticketView} />

          <TicketRelations ticketKey={ticket.ticketKey} links={ticketView.links || []} />

          <TicketRelationsSidebar
            ticketKey={ticket.ticketKey}
            ticketId={ticket.id}
            projectPrefix={ticketView.projectPrefix}
            teamSlug={ticket.team?.slug}
            projectId={ticket.project}
            teamId={ticket.team?.id}
            variant="main"
          />

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
            onOpenRelationPicker={(kind) => openRelationPicker(kind, true)}
          />
        </aside>
      </div>

      <TicketRelationPickerDialog
        ticketKey={ticket.ticketKey}
        ticketId={ticket.id}
        open={relationPicker.open}
        defaultKind={relationPicker.kind}
        kindLocked={relationPicker.kindLocked}
        onOpenChange={(open) => setRelationPicker((prev) => ({ ...prev, open }))}
        onSubmit={relationActions.handleCreate}
        isSubmitting={relationActions.isCreating}
      />
    </div>
  );
}
