import { useEffect, useState, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useEditor, useEditorState, EditorContent } from '@tiptap/react';
import StarterKit from '@tiptap/starter-kit';
import TiptapPlaceholder from '@tiptap/extension-placeholder';
import { apiClient } from '../../../shared/api/client';
import { useOptimisticMutation } from '../../../shared/hooks/useOptimisticMutation';
import { bumpTicketCounter } from '../../../shared/sync/ticketWrites';
import { fetchTicketUserOptions, ticketUserOptionsEnabled } from '../utils/ticketUserOptions';
import { createMentionExtension, renderCommentBodyWithMentions } from '../utils/createMentionExtension';
import type { Comment, TicketAttachment, TicketDetailView } from '../types/ticketDetailView';
import './TicketComments.css';

interface TicketCommentsProps {
  ticketId: string;
  comments: Comment[];
  attachments?: TicketAttachment[];
  /** @メンション候補の絞り込み用（プロジェクト優先、無ければチームメンバー） */
  projectId?: number | null;
  teamId?: number | null;
}

function formatRelativeTime(dateStr: string, isJa: boolean): string {
  const now = new Date();
  const date = new Date(dateStr);
  const diffMs = now.getTime() - date.getTime();
  const diffMin = Math.floor(diffMs / 60000);
  const diffHour = Math.floor(diffMs / 3600000);
  const diffDay = Math.floor(diffMs / 86400000);

  if (diffMin < 1) return isJa ? 'たった今' : 'just now';
  if (diffMin < 60) return isJa ? `${diffMin}分前` : `${diffMin}m ago`;
  if (diffHour < 24) return isJa ? `${diffHour}時間前` : `${diffHour}h ago`;
  if (diffDay < 7) return isJa ? `${diffDay}日前` : `${diffDay}d ago`;
  return date.toLocaleDateString(isJa ? 'ja-JP' : undefined, {
    month: 'short',
    day: 'numeric',
  });
}


function renderCommentAuthor(
  comment: Comment,
  t: (key: string, opts?: Record<string, unknown>) => string,
) {
  if (comment.actingUser) {
    return (
      <>
        {comment.actingUser.displayName || comment.actingUser.username}
        <span
          className="ticket-comment-item__ai-badge"
          title={t('ticketDetail.aiAgentActingTooltip', {
            author: comment.author?.displayName || comment.author?.username || 'AiAgent',
          })}
        >
          {t('ticketDetail.aiAgentBadge')}
        </span>
      </>
    );
  }
  return comment.author?.displayName || comment.author?.username || 'Unknown';
}

export function TicketComments({
  ticketId,
  comments: initialComments,
  attachments: initialAttachments,
  projectId,
  teamId,
}: TicketCommentsProps) {
  const { t, i18n } = useTranslation();
  const queryClient = useQueryClient();
  const [attachmentUploading, setAttachmentUploading] = useState(false);
  const [isDragOver, setIsDragOver] = useState(false);
  const [editingCommentId, setEditingCommentId] = useState<number | null>(null);
  const [editingText, setEditingText] = useState('');
  const [replyingToRootId, setReplyingToRootId] = useState<number | null>(null);
  const attachmentInputRef = useRef<HTMLInputElement>(null);
  const [comments, setComments] = useState(initialComments);

  useEffect(() => {
    setComments(initialComments);
  }, [initialComments]);

  const [attachments, setAttachments] = useState<TicketAttachment[]>(initialAttachments ?? []);

  useEffect(() => {
    setAttachments(initialAttachments ?? []);
  }, [initialAttachments]);

  // @メンション候補（プロジェクト優先、チームのみはメンバー）
  const { data: userOptionsData } = useQuery({
    queryKey: ['users', projectId ?? null, teamId ?? null],
    queryFn: () => fetchTicketUserOptions(apiClient, { projectId, teamId }),
    enabled: ticketUserOptionsEnabled({ projectId, teamId }),
  });
  const userOptions = userOptionsData ?? [];

  // userOptionsはReact Queryで非同期に更新されるが、TipTapのMention拡張の
  // suggestion.items()はエディタ生成時に一度だけクロージャとして固定されるため、
  // refで常に最新値を参照できるようにする。
  const userOptionsRef = useRef(userOptions);
  useEffect(() => {
    userOptionsRef.current = userOptions;
  }, [userOptions]);

  const commentMutation = useOptimisticMutation<void, { body: string; parentCommentId?: number | null }>({
    mutationFn: async ({ body, parentCommentId }) => {
      await apiClient.post(`/tickets/${ticketId}/comments/`, { body, parentCommentId });
    },
    queryKey: ['ticket', ticketId],
    updater: (currentData, { body, parentCommentId }) => {
      const data = currentData as TicketDetailView | undefined;
      if (!data) return currentData;
      return {
        ...data,
        comments: [
          ...(data.comments || []),
          {
            id: -Date.now(),
            body,
            author: { id: 0, username: 'you', displayName: 'You' },
            createdAt: new Date().toISOString(),
            updatedAt: null,
            isDeleted: false,
            parentCommentId: parentCommentId || null,
            replyCount: 0,
            canEdit: true,
            canDelete: true,
          },
        ],
      };
    },
    onSuccessCallback: () => {
      commentEditor?.commands.clearContent();
      setReplyingToRootId(null);
      void bumpTicketCounter(ticketId, 'commentCount', 1);
      void queryClient.invalidateQueries({ queryKey: ['ticket', ticketId] });
    },
    errorMessage: t('ticketDetail.errors.commentFailed'),
  });

  const editCommentMutation = useOptimisticMutation<void, { commentId: number; body: string }>({
    mutationFn: async ({ commentId, body }) => {
      await apiClient.patch(`/tickets/${ticketId}/comments/${commentId}/`, { body });
    },
    queryKey: ['ticket', ticketId],
    updater: (currentData, { commentId, body }) => {
      const data = currentData as TicketDetailView | undefined;
      if (!data) return currentData;
      return {
        ...data,
        comments: (data.comments || []).map((c) => (c.id === commentId ? { ...c, body, updatedAt: new Date().toISOString() } : c)),
      };
    },
    onSuccessCallback: () => {
      setEditingCommentId(null);
      setEditingText('');
    },
    errorMessage: t('ticketDetail.errors.commentEditFailed'),
  });

  const deleteCommentMutation = useOptimisticMutation<void, number>({
    mutationFn: async (commentId) => {
      await apiClient.delete(`/tickets/${ticketId}/comments/${commentId}/`);
    },
    queryKey: ['ticket', ticketId],
    updater: (currentData, commentId) => {
      const data = currentData as TicketDetailView | undefined;
      if (!data) return currentData;
      return {
        ...data,
        comments: (data.comments || []).map((c) => (c.id === commentId ? { ...c, isDeleted: true, body: '' } : c)),
      };
    },
    onSuccessCallback: () => {
      void bumpTicketCounter(ticketId, 'commentCount', -1);
    },
    errorMessage: t('ticketDetail.errors.commentDeleteFailed'),
  });

  const commentEditor = useEditor({
    extensions: [
      StarterKit.configure({
        heading: false,
        bulletList: false,
        orderedList: false,
        blockquote: false,
        codeBlock: false,
        horizontalRule: false,
        bold: false,
        italic: false,
        strike: false,
        code: false,
      }),
      TiptapPlaceholder.configure({
        placeholder: t('ticketDetail.addCommentPlaceholder'),
      }),
      createMentionExtension(userOptionsRef),
    ],
    editorProps: {
      attributes: { 'data-testid': 'ticket-comment-input' },
    },
    content: '',
  });

  const replyEditor = useEditor({
    extensions: [
      StarterKit.configure({
        heading: false,
        bulletList: false,
        orderedList: false,
        blockquote: false,
        codeBlock: false,
        horizontalRule: false,
        bold: false,
        italic: false,
        strike: false,
        code: false,
      }),
      TiptapPlaceholder.configure({
        placeholder: t('ticketDetail.addCommentPlaceholder'),
      }),
      createMentionExtension(userOptionsRef),
    ],
    editorProps: {
      attributes: { 'data-testid': 'ticket-reply-input' },
    },
    content: '',
  });

  const commentEditorIsEmpty = useEditorState({
    editor: commentEditor,
    selector: ({ editor }) => !editor || editor.isEmpty,
  });

  const replyEditorIsEmpty = useEditorState({
    editor: replyEditor,
    selector: ({ editor }) => !editor || editor.isEmpty,
  });

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
      setAttachments((prev) => prev.filter((a) => a.id !== attachmentId));
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

  const handleSubmitComment = () => {
    const body = commentEditor?.getText({ blockSeparator: '\n' }).trim();
    if (body) {
      commentMutation.mutate({ body, parentCommentId: null });
    }
  };

  const handleSubmitReply = (parentCommentId: number) => {
    const body = replyEditor?.getText({ blockSeparator: '\n' }).trim();
    if (body) {
      commentMutation.mutate({ body, parentCommentId });
    }
  };

  return (
    <div className="ticket-comments" data-testid="ticket-comments">
      <h4 className="ticket-comments__title">
        {t('ticketDetail.comments')}
        <span className="ticket-comments__count">{comments.length}</span>
      </h4>

      {attachments.length > 0 && (
        <ul className="ticket-comments__attachments">
          {attachments.map((att) => (
            <li key={att.id} className="ticket-comments__attachment">
              <span className="ticket-comments__attachment-icon">
                {att.isImage ? '🖼️' : '📄'}
              </span>
              <a
                href={att.fileUrl}
                download={att.filename}
                className="ticket-comments__attachment-link"
                title={att.filename}
              >
                {att.filename}
              </a>
              <span className="ticket-comments__attachment-size">
                ({att.sizeDisplay})
              </span>
              <button
                type="button"
                className="ticket-comments__attachment-delete"
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

      <div className="ticket-comments__list">
        {comments.length === 0 ? (
          <div className="ticket-comments__empty">{t('ticketDetail.noComments')}</div>
        ) : (
          comments
            .filter((c) => !c.parentCommentId)
            .map((rootComment) => (
              <div key={rootComment.id}>
                <div
                  id={`comment-${rootComment.id}`}
                  className="ticket-comment-item"
                >
                  <div className="ticket-comment-item__header">
                    <span className="ticket-comment-item__author">
                      {renderCommentAuthor(rootComment, t)}
                    </span>
                    <span className="ticket-comment-item__time">
                      {formatRelativeTime(rootComment.createdAt, i18n.language.startsWith('ja'))}
                    </span>
                  </div>
                  {rootComment.isDeleted ? (
                    <p className="ticket-comment-item__deleted">
                      {t('ticketDetail.commentDeleted')}
                    </p>
                  ) : editingCommentId === rootComment.id ? (
                    <div className="ticket-comment-item__edit-form">
                      <textarea
                        value={editingText}
                        onChange={(e) => setEditingText(e.target.value)}
                        className="ticket-comment-item__edit-textarea"
                        rows={4}
                      />
                      <div className="ticket-comment-item__edit-actions">
                        <button
                          type="button"
                          className="ticket-comment-item__edit-save"
                          onClick={() => editCommentMutation.mutate({ commentId: rootComment.id, body: editingText })}
                          disabled={editCommentMutation.isPending}
                        >
                          {t('ticketDetail.saveEdit')}
                        </button>
                        <button
                          type="button"
                          className="ticket-comment-item__edit-cancel"
                          onClick={() => {
                            setEditingCommentId(null);
                            setEditingText('');
                          }}
                          disabled={editCommentMutation.isPending}
                        >
                          {t('ticketDetail.cancelEdit')}
                        </button>
                      </div>
                    </div>
                  ) : (
                    <p className="ticket-comment-item__body">
                      {renderCommentBodyWithMentions(rootComment.body, userOptions)}
                    </p>
                  )}
                  {!rootComment.isDeleted && (rootComment.canEdit || rootComment.canDelete) && (
                    <div className="ticket-comment-item__actions">
                      {rootComment.canEdit && (
                        <button
                          type="button"
                          className="ticket-comment-item__action-btn"
                          onClick={() => {
                            setEditingCommentId(rootComment.id);
                            setEditingText(rootComment.body);
                          }}
                        >
                          {t('ticketDetail.editComment')}
                        </button>
                      )}
                      {rootComment.canDelete && (
                        <button
                          type="button"
                          className="ticket-comment-item__action-btn ticket-comment-item__action-btn--delete"
                          onClick={() => {
                            if (window.confirm(t('ticketDetail.confirmDeleteComment'))) {
                              deleteCommentMutation.mutate(rootComment.id);
                            }
                          }}
                          disabled={deleteCommentMutation.isPending}
                        >
                          {t('ticketDetail.deleteComment')}
                        </button>
                      )}
                      <button
                        type="button"
                        className="ticket-comment-item__action-btn"
                        onClick={() => setReplyingToRootId(replyingToRootId === rootComment.id ? null : rootComment.id)}
                      >
                        {t('ticketDetail.replyComment')}
                      </button>
                    </div>
                  )}
                </div>

                {comments
                  .filter((c) => c.parentCommentId === rootComment.id)
                  .map((reply) => (
                    <div
                      key={reply.id}
                      id={`comment-${reply.id}`}
                      className="ticket-comment-item ticket-comment-item--reply"
                    >
                      <div className="ticket-comment-item__header">
                        <span className="ticket-comment-item__author">
                          {renderCommentAuthor(reply, t)}
                        </span>
                        <span className="ticket-comment-item__time">
                          {formatRelativeTime(reply.createdAt, i18n.language.startsWith('ja'))}
                        </span>
                      </div>
                      {reply.isDeleted ? (
                        <p className="ticket-comment-item__deleted">
                          {t('ticketDetail.commentDeleted')}
                        </p>
                      ) : editingCommentId === reply.id ? (
                        <div className="ticket-comment-item__edit-form">
                          <textarea
                            value={editingText}
                            onChange={(e) => setEditingText(e.target.value)}
                            className="ticket-comment-item__edit-textarea"
                            rows={4}
                          />
                          <div className="ticket-comment-item__edit-actions">
                            <button
                              type="button"
                              className="ticket-comment-item__edit-save"
                              onClick={() => editCommentMutation.mutate({ commentId: reply.id, body: editingText })}
                              disabled={editCommentMutation.isPending}
                            >
                              {t('ticketDetail.saveEdit')}
                            </button>
                            <button
                              type="button"
                              className="ticket-comment-item__edit-cancel"
                              onClick={() => {
                                setEditingCommentId(null);
                                setEditingText('');
                              }}
                              disabled={editCommentMutation.isPending}
                            >
                              {t('ticketDetail.cancelEdit')}
                            </button>
                          </div>
                        </div>
                      ) : (
                        <p className="ticket-comment-item__body">
                          {renderCommentBodyWithMentions(reply.body, userOptions)}
                        </p>
                      )}
                      {!reply.isDeleted && (reply.canEdit || reply.canDelete) && (
                        <div className="ticket-comment-item__actions">
                          {reply.canEdit && (
                            <button
                              type="button"
                              className="ticket-comment-item__action-btn"
                              onClick={() => {
                                setEditingCommentId(reply.id);
                                setEditingText(reply.body);
                              }}
                            >
                              {t('ticketDetail.editComment')}
                            </button>
                          )}
                          {reply.canDelete && (
                            <button
                              type="button"
                              className="ticket-comment-item__action-btn ticket-comment-item__action-btn--delete"
                              onClick={() => {
                                if (window.confirm(t('ticketDetail.confirmDeleteComment'))) {
                                  deleteCommentMutation.mutate(reply.id);
                                }
                              }}
                              disabled={deleteCommentMutation.isPending}
                            >
                              {t('ticketDetail.deleteComment')}
                            </button>
                          )}
                        </div>
                      )}
                    </div>
                  ))}

                {replyingToRootId === rootComment.id && (
                  <div className="ticket-comment-item ticket-comment-item--reply-form">
                    <EditorContent editor={replyEditor} className="ticket-comments__input" />
                    <div className="ticket-comments__form-actions">
                      <button
                        type="button"
                        className="ticket-comments__submit"
                        onClick={() => handleSubmitReply(rootComment.id)}
                        disabled={replyEditorIsEmpty || commentMutation.isPending}
                      >
                        {commentMutation.isPending ? t('common.posting') : t('ticketDetail.postComment')}
                      </button>
                      <button
                        type="button"
                        className="ticket-comments__cancel"
                        onClick={() => setReplyingToRootId(null)}
                        disabled={commentMutation.isPending}
                      >
                        {t('ticketDetail.cancelEdit')}
                      </button>
                    </div>
                  </div>
                )}
              </div>
            ))
        )}
      </div>

      <div
        className={`ticket-comments__form${isDragOver ? ' ticket-comments__form--drag-over' : ''}`}
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
      >
        <EditorContent editor={commentEditor} className="ticket-comments__input" />
        <div className="ticket-comments__form-actions">
          <button
            type="button"
            className="ticket-comments__attach"
            title={t('ticketDetail.attach')}
            aria-label={t('ticketDetail.attach')}
            disabled={attachmentUploading}
            onClick={() => attachmentInputRef.current?.click()}
            data-testid="ticket-comment-attach"
          >
            📎
          </button>
          <input
            ref={attachmentInputRef}
            type="file"
            multiple
            onChange={handleAttachmentUpload}
            disabled={attachmentUploading}
            className="ticket-comments__file-input"
            data-testid="ticket-comment-attachment-input"
          />
          <button
            type="button"
            className="ticket-comments__submit"
            onClick={handleSubmitComment}
            disabled={commentEditorIsEmpty || commentMutation.isPending}
            data-testid="ticket-comment-submit"
          >
            {commentMutation.isPending
              ? t('common.posting')
              : t('ticketDetail.postComment')}
          </button>
        </div>
      </div>
    </div>
  );
}
