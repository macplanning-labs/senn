/**
 * TicketForm.tsx — チケット作成・編集フォーム
 *
 * Zodバリデーション付き。作成時と編集時で共通化。
 * 開発標準書: Layer 1（フロントバリデーション）準拠。
 */

import { useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate, useParams } from 'react-router-dom';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { z } from 'zod';
import { apiClient } from '@/shared/api/client';
import './TicketForm.css';

// Zodバリデーションスキーマ
const ticketSchema = z.object({
  title: z.string().min(1, 'Title is required').max(500),
  description: z.string().default(''),
  status: z.enum(['open', 'in_progress', 'resolved', 'closed']).default('open'),
  priority: z.enum(['urgent', 'high', 'medium', 'low']).default('medium'),
  ticket_type: z.enum(['issue', 'feature', 'improvement', 'task']).default('issue'),
  due_date: z.string().nullable().default(null),
  start_date: z.string().nullable().default(null),
});

type TicketFormData = z.infer<typeof ticketSchema>;

interface TicketEditData {
  title: string;
  description: string;
  status: string;
  priority: string;
  ticket_type: string;
  start_date: string | null;
  due_date: string | null;
}

export function TicketForm() {
  const { t } = useTranslation();
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const isEditing = !!id && id !== 'new';

  // 編集時は既存データを取得
  const { data: existingTicket } = useQuery<TicketEditData>({
    queryKey: ['ticket-edit', id],
    queryFn: async () => {
      const res = await apiClient.get<TicketEditData>(`/tickets/${id}/`);
      return res.data;
    },
    enabled: isEditing,
  });

  const [formData, setFormData] = useState<TicketFormData>({
    title: '',
    description: '',
    status: 'open',
    priority: 'medium',
    ticket_type: 'issue',
    due_date: null,
    start_date: null,
  });
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [isInitialized, setIsInitialized] = useState(!isEditing);

  // 既存データが取得できたらフォームに反映
  if (isEditing && existingTicket && !isInitialized) {
    setFormData({
      title: existingTicket.title,
      description: existingTicket.description,
      status: existingTicket.status as TicketFormData['status'],
      priority: existingTicket.priority as TicketFormData['priority'],
      ticket_type: existingTicket.ticket_type as TicketFormData['ticket_type'],
      due_date: existingTicket.due_date,
      start_date: existingTicket.start_date,
    });
    setIsInitialized(true);
  }

  const mutation = useMutation({
    mutationFn: async (data: TicketFormData) => {
      if (isEditing) {
        await apiClient.patch(`/tickets/${id}/`, data);
      } else {
        await apiClient.post('/tickets/', data);
      }
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['tickets'] });
      if (isEditing) {
        void queryClient.invalidateQueries({ queryKey: ['ticket', id] });
      }
      navigate(isEditing ? `/tickets/${id}` : '/tickets');
    },
  });

  function handleSubmit(e: FormEvent) {
    e.preventDefault();
    setErrors({});

    const result = ticketSchema.safeParse(formData);
    if (!result.success) {
      const fieldErrors: Record<string, string> = {};
      for (const issue of result.error.issues) {
        const key = issue.path[0];
        if (key) fieldErrors[String(key)] = issue.message;
      }
      setErrors(fieldErrors);
      return;
    }

    mutation.mutate(result.data);
  }

  function updateField<K extends keyof TicketFormData>(
    field: K,
    value: TicketFormData[K],
  ) {
    setFormData((prev) => ({ ...prev, [field]: value }));
    // フィールド変更時にそのフィールドのエラーをクリア
    if (errors[field]) {
      setErrors((prev) => {
        const next = { ...prev };
        delete next[field];
        return next;
      });
    }
  }

  return (
    <div className="ticket-form" data-testid="ticket-form-page">
      <h1 className="ticket-form__title">
        {isEditing ? t('ticket.edit') : t('ticket.create')}
      </h1>

      <form className="ticket-form__form" onSubmit={(e) => { void handleSubmit(e); }}>
        {/* タイトル */}
        <div className="ticket-form__field">
          <label htmlFor="title" className="ticket-form__label">
            {t('ticket.title')} *
          </label>
          <input
            id="title"
            type="text"
            className={`ticket-form__input ${errors.title ? 'ticket-form__input--error' : ''}`}
            value={formData.title}
            onChange={(e) => updateField('title', e.target.value)}
            placeholder="What needs to be done?"
            autoFocus
            data-testid="ticket-title-input"
          />
          {errors.title && (
            <span className="ticket-form__error">{errors.title}</span>
          )}
        </div>

        {/* 説明 */}
        <div className="ticket-form__field">
          <label htmlFor="description" className="ticket-form__label">
            {t('ticket.description')}
          </label>
          <textarea
            id="description"
            className="ticket-form__textarea"
            value={formData.description}
            onChange={(e) => updateField('description', e.target.value)}
            placeholder="Add a description..."
            rows={6}
            data-testid="ticket-description-input"
          />
        </div>

        {/* 2カラム: ステータス + 優先度 */}
        <div className="ticket-form__row">
          <div className="ticket-form__field">
            <label htmlFor="status" className="ticket-form__label">
              {t('ticket.status.open').replace('Open', 'Status')}
            </label>
            <select
              id="status"
              className="ticket-form__select"
              value={formData.status}
              onChange={(e) => updateField('status', e.target.value as TicketFormData['status'])}
              data-testid="ticket-status-input"
            >
              <option value="open">Open</option>
              <option value="in_progress">In Progress</option>
              <option value="resolved">Resolved</option>
              <option value="closed">Closed</option>
            </select>
          </div>

          <div className="ticket-form__field">
            <label htmlFor="priority" className="ticket-form__label">
              {t('ticket.priority')}
            </label>
            <select
              id="priority"
              className="ticket-form__select"
              value={formData.priority}
              onChange={(e) => updateField('priority', e.target.value as TicketFormData['priority'])}
              data-testid="ticket-priority-input"
            >
              <option value="urgent">⬆⬆ Urgent</option>
              <option value="high">⬆ High</option>
              <option value="medium">— Medium</option>
              <option value="low">⬇ Low</option>
            </select>
          </div>
        </div>

        {/* 2カラム: 種別 + 期限 */}
        <div className="ticket-form__row">
          <div className="ticket-form__field">
            <label htmlFor="ticket_type" className="ticket-form__label">
              Type
            </label>
            <select
              id="ticket_type"
              className="ticket-form__select"
              value={formData.ticket_type}
              onChange={(e) => updateField('ticket_type', e.target.value as TicketFormData['ticket_type'])}
              data-testid="ticket-type-input"
            >
              <option value="issue">🐛 Issue</option>
              <option value="feature">✨ Feature</option>
              <option value="improvement">💡 Improvement</option>
              <option value="task">📋 Task</option>
            </select>
          </div>

          <div className="ticket-form__field">
            <label htmlFor="due_date" className="ticket-form__label">
              {t('ticket.dueDate')}
            </label>
            <input
              id="due_date"
              type="date"
              className="ticket-form__input"
              value={formData.due_date ?? ''}
              onChange={(e) => updateField('due_date', e.target.value || null)}
              data-testid="ticket-due-date-input"
            />
          </div>
        </div>

        {/* アクション */}
        <div className="ticket-form__actions">
          <button
            type="button"
            className="ticket-form__cancel"
            onClick={() => navigate(-1)}
            data-testid="ticket-form-cancel"
          >
            {t('common.cancel')}
          </button>
          <button
            type="submit"
            className="ticket-form__submit"
            disabled={mutation.isPending}
            data-testid="ticket-form-submit"
          >
            {mutation.isPending
              ? t('common.loading')
              : isEditing
                ? t('common.save')
                : t('ticket.create')}
          </button>
        </div>

        {mutation.isError && (
          <div className="ticket-form__server-error" role="alert">
            Failed to {isEditing ? 'update' : 'create'} ticket. Please try again.
          </div>
        )}
      </form>
    </div>
  );
}
