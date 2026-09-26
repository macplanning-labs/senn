import {
  useState,
  useRef,
  useEffect,
  useImperativeHandle,
  forwardRef,
} from 'react';
import { useTranslation } from 'react-i18next';
import { useQueryClient } from '@tanstack/react-query';
import { apiClient } from '../../../shared/api/client';
import { localUpdateTicket } from '../../../shared/sync/ticketWrites';
import type { TicketAttachment, TicketDetailView } from '../types/ticketDetailView';
import { ReactionBar } from './ReactionBar';
import './TicketTitleDescriptionEditor.css';

export interface TicketTitleDescriptionEditorHandle {
  startTitleEdit: () => void;
  startDescriptionEdit: () => void;
}

interface TicketTitleDescriptionEditorProps {
  ticket: TicketDetailView;
  ticketId: string;
}

export const TicketTitleDescriptionEditor = forwardRef<
  TicketTitleDescriptionEditorHandle,
  TicketTitleDescriptionEditorProps
>(function TicketTitleDescriptionEditor({ ticket, ticketId }, ref) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [editingTitle, setEditingTitle] = useState(false);
  const [editingDescription, setEditingDescription] = useState(false);
  const [titleValue, setTitleValue] = useState(ticket.title);
  const [descriptionValue, setDescriptionValue] = useState(ticket.description);
  const [attachmentUploading, setAttachmentUploading] = useState(false);
  const [isDragOver, setIsDragOver] = useState(false);

  const titleEditRef = useRef<HTMLInputElement>(null);
  const descriptionEditRef = useRef<HTMLTextAreaElement>(null);
  const attachmentInputRef = useRef<HTMLInputElement>(null);
  const descriptionEditStartUpdatedAtRef = useRef<string | null>(null);

  useEffect(() => {
    setTitleValue(ticket.title);
    setDescriptionValue(ticket.description);
  }, [ticket.title, ticket.description]);

  useImperativeHandle(ref, () => ({
    startTitleEdit: () => setEditingTitle(true),
    startDescriptionEdit: () => setEditingDescription(true),
  }));


  useEffect(() => {
    if (editingTitle && titleEditRef.current) {
      titleEditRef.current.focus();
      titleEditRef.current.select();
    }
  }, [editingTitle]);

  useEffect(() => {
    if (editingDescription && descriptionEditRef.current) {
      descriptionEditRef.current.focus();
    }
  }, [editingDescription]);

  // 同時編集の警告（決定事項1）用: 編集を始めた時点の更新日時を覚える（編集中に届いた他人の変更で上書きしない）
  useEffect(() => {
    if (editingDescription) {
      descriptionEditStartUpdatedAtRef.current = ticket.updatedAt;
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editingDescription]);

  const handleTitleSave = async () => {
    if (titleValue.trim() !== ticket.title.trim()) {
      await localUpdateTicket(ticketId, { title: titleValue }, { title: titleValue });
      setEditingTitle(false);
    } else {
      setEditingTitle(false);
    }
  };

  const handleDescriptionSave = async () => {
    if (descriptionValue !== ticket.description) {
      // Check for conflict warning (決定1: description only)
      if (navigator.onLine) {
        try {
          const res = await apiClient.get<TicketDetailView>(`/tickets/${ticketId}/`);
          const serverUpdatedAt = res.data?.updatedAt;
          const startUpdatedAt = descriptionEditStartUpdatedAtRef.current;
          if (startUpdatedAt && serverUpdatedAt && startUpdatedAt !== serverUpdatedAt) {
            const confirmed = window.confirm(t('sync.descriptionConflict'));
            if (!confirmed) return;
          }
        } catch {
          // If fetch fails, continue with save
        }
      }

      await localUpdateTicket(ticketId, { description: descriptionValue }, { description: descriptionValue });
      setEditingDescription(false);
    } else {
      setEditingDescription(false);
    }
  };

  const uploadAttachmentFile = async (file: File) => {
    const formData = new FormData();
    formData.append('file', file);

    setAttachmentUploading(true);
    try {
      const res = await apiClient.post<TicketAttachment>(
        `/tickets/${ticketId}/attachments/`,
        formData,
      );
      queryClient.setQueryData<TicketDetailView | undefined>(['ticket', ticketId], (old) => {
        if (!old) return old;
        return {
          ...old,
          attachments: [...(old.attachments ?? []), res.data],
        };
      });
    } catch (error) {
      console.error('Attachment upload failed:', error);
      alert(t('ticketDetail.errors.attachFailed', { defaultValue: 'Failed to upload attachment.' }));
    } finally {
      setAttachmentUploading(false);
    }
  };

  const handleAttachmentUpload = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = e.currentTarget.files;
    if (!files || files.length === 0) return;

    for (const file of Array.from(files)) {
      await uploadAttachmentFile(file);
    }
    e.currentTarget.value = '';
  };

  const handleDeleteAttachment = async (attachmentId: number) => {
    if (
      !window.confirm(
        t('ticketDetail.confirmDeleteAttachment', { defaultValue: 'Delete this attachment?' }),
      )
    )
      return;

    try {
      await apiClient.delete(`/tickets/${ticketId}/attachments/${attachmentId}/`);
      queryClient.setQueryData<TicketDetailView | undefined>(['ticket', ticketId], (old) => {
        if (!old) return old;
        return {
          ...old,
          attachments: (old.attachments ?? []).filter((a) => a.id !== attachmentId),
        };
      });
    } catch (error) {
      console.error('Attachment delete failed:', error);
      alert(
        t('ticketDetail.errors.attachDeleteFailed', {
          defaultValue: 'Failed to delete attachment.',
        }),
      );
    }
  };

  const attachments = (ticket.attachments ?? []).filter((a) => !a.commentId);

  return (
    <div className="ticket-title-description-editor">
      <div className="ticket-title-section">
        {editingTitle ? (
          <input
            ref={titleEditRef}
            type="text"
            className="ticket-title-editor__input"
            value={titleValue}
            onChange={(e) => setTitleValue(e.target.value)}
            onBlur={handleTitleSave}
            onKeyDown={(e) => {
              if (e.key === 'Enter') handleTitleSave();
              if (e.key === 'Escape') {
                setTitleValue(ticket.title);
                setEditingTitle(false);
              }
            }}
            placeholder={t('ticketDetail.titlePlaceholder')}
            data-testid="ticket-title-editor"
          />
        ) : (
          <h2
            className="ticket-title-editor__display"
            tabIndex={0}
            onClick={() => setEditingTitle(true)}
            data-testid="ticket-title-display"
          >
            {titleValue}
          </h2>
        )}
      </div>

      <div className="ticket-description-section">
        {editingDescription ? (
          <textarea
            ref={descriptionEditRef}
            className="ticket-description-editor__input"
            value={descriptionValue}
            onChange={(e) => setDescriptionValue(e.target.value)}
            onBlur={handleDescriptionSave}
            onDragOver={(e) => {
              e.preventDefault();
              setIsDragOver(true);
            }}
            onDragLeave={() => setIsDragOver(false)}
            onDrop={async (e) => {
              e.preventDefault();
              setIsDragOver(false);
              const files = Array.from(e.dataTransfer.files);
              for (const file of files) {
                await uploadAttachmentFile(file);
              }
            }}
            onKeyDown={(e) => {
              if (e.key === 'Escape') {
                setDescriptionValue(ticket.description);
                setEditingDescription(false);
              }
            }}
            placeholder={t('ticketDetail.descriptionPlaceholder')}
            rows={8}
            data-testid="ticket-description-editor"
          />
        ) : (
          <div
            className={`ticket-description-editor__display${isDragOver ? ' ticket-description-editor__display--drag-over' : ''}`}
            tabIndex={0}
            onClick={() => setEditingDescription(true)}
            onDragOver={(e) => {
              e.preventDefault();
              setIsDragOver(true);
            }}
            onDragLeave={() => setIsDragOver(false)}
            onDrop={async (e) => {
              e.preventDefault();
              setIsDragOver(false);
              const files = Array.from(e.dataTransfer.files);
              for (const file of files) {
                await uploadAttachmentFile(file);
              }
            }}
            data-testid="ticket-description-display"
          >
            {descriptionValue ? (
              <p className="ticket-description-editor__text">{descriptionValue}</p>
            ) : (
              <p className="ticket-description-editor__empty">
                {t('ticketDetail.descriptionEmpty')}
              </p>
            )}
          </div>
        )}
        <div className="ticket-description-editor__toolbar">
          <ReactionBar
            className="ticket-description-editor__reactions"
            ticketKey={ticket.ticketKey}
            ticketId={ticket.id}
            projectPrefix={ticket.projectPrefix}
            projectId={ticket.project}
            addButtonContent="😊"
          />
          <button
            type="button"
            className="ticket-description-editor__tool ticket-description-editor__tool--active"
            title={t('ticketDetail.attach')}
            aria-label={t('ticketDetail.attach')}
            disabled={attachmentUploading}
            onClick={() => attachmentInputRef.current?.click()}
            data-testid="ticket-description-attach"
          >
            📎
          </button>
          <input
            ref={attachmentInputRef}
            type="file"
            multiple
            onChange={handleAttachmentUpload}
            disabled={attachmentUploading}
            className="ticket-description-editor__file-input"
            data-testid="ticket-description-attachment-input"
          />
        </div>
        {attachments.length > 0 && (
          <ul className="ticket-description-editor__attachments">
            {attachments.map((att) => (
              <li key={att.id} className="ticket-description-editor__attachment">
                <span className="ticket-description-editor__attachment-icon">
                  {att.isImage ? '🖼️' : '📄'}
                </span>
                <a
                  href={att.fileUrl}
                  download={att.filename}
                  className="ticket-description-editor__attachment-link"
                  title={att.filename}
                >
                  {att.filename}
                </a>
                <span className="ticket-description-editor__attachment-size">
                  ({att.sizeDisplay})
                </span>
                <button
                  type="button"
                  className="ticket-description-editor__attachment-delete"
                  onClick={() => handleDeleteAttachment(att.id)}
                  title={t('ticketDetail.deleteAttachment', { defaultValue: 'Delete attachment' })}
                  aria-label={t('ticketDetail.deleteAttachment', {
                    defaultValue: 'Delete attachment',
                  })}
                >
                  ✕
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
});
