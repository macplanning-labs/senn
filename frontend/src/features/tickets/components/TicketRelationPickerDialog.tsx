import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { TicketRelationAddForm } from './TicketRelationAddForm';
import type { RelationCreateKind } from '../utils/classifyDependencies';
import './TicketRelations.css';

interface TicketRelationPickerDialogProps {
  ticketKey: string;
  ticketId: number;
  open: boolean;
  defaultKind: RelationCreateKind;
  kindLocked?: boolean;
  onOpenChange: (open: boolean) => void;
  onSubmit: (kind: RelationCreateKind, other: { id: number; ticketKey: string }) => Promise<void>;
  isSubmitting: boolean;
}

export function TicketRelationPickerDialog({
  ticketKey,
  ticketId,
  open,
  defaultKind,
  kindLocked = true,
  onOpenChange,
  onSubmit,
  isSubmitting,
}: TicketRelationPickerDialogProps) {
  const { t } = useTranslation();

  if (!open) return null;

  return createPortal(
    <div
      className="ticket-relation-picker"
      data-ticket-relation-picker-open
      data-testid="ticket-relation-picker"
      onClick={() => onOpenChange(false)}
      role="presentation"
    >
      <div
        className="ticket-relation-picker__panel"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-label={t('ticketDetail.addRelation')}
      >
        <TicketRelationAddForm
          ticketKey={ticketKey}
          ticketId={ticketId}
          open
          defaultKind={defaultKind}
          kindLocked={kindLocked}
          onOpenChange={onOpenChange}
          onSubmit={onSubmit}
          isSubmitting={isSubmitting}
        />
      </div>
    </div>,
    document.body,
  );
}
