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
import { useProject } from '@/shared/hooks/useProject';
import { useTeams } from '@/features/teams/hooks/useTeams';
import './TicketForm.css';

// Zodバリデーションスキーマ
const ticketSchema = z.object({
  title: z.string().min(1, 'Title is required').max(500),
  description: z.string().default(''),
  status: z.enum(['backlog', 'open', 'in_progress', 'resolved', 'closed', 'canceled']).default('open'),
  priority: z.enum(['urgent', 'high', 'medium', 'low']).default('medium'),
  ticket_type: z.enum(['issue', 'feature', 'improvement', 'task']).default('issue'),
  due_date: z.string().nullable().default(null),
  start_date: z.string().nullable().default(null),
  story_points: z.number().nullable().default(null),
});

type TicketFormData = z.infer<typeof ticketSchema>;

interface TicketEditData {
  title: string;
  description: string;
  status: string;
  priority: string;
  ticketType: string;
  assignees: { id: number; username: string; displayName: string }[];
  milestone: { id: number; name: string } | null;
  parent: number | null;
  category: { id: number; name: string } | null;
  cycle: number | null;
  labels: { id: number }[];
  assignedTeam: { id: number; name: string } | null;
  startDate: string | null;
  dueDate: string | null;
  storyPoints: number | null;
}

interface UserOption {
  id: number;
  username: string;
  displayName: string;
}

interface MilestoneOption {
  id: number;
  name: string;
}

interface LabelOption {
  id: number;
  name: string;
  color: string;
}

interface CategoryOption {
  id: number;
  name: string;
  level: number;
  parent: number | null;
}

interface ParentTicketOption {
  id: number;
  ticketKey: string;
  title: string;
}

export function TicketForm() {
  const { t } = useTranslation();
  const { ticketId } = useParams<{ ticketId: string }>();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { projectKey, currentProject } = useProject();
  const isEditing = !!ticketId && ticketId !== 'new';

  // 編集時は既存データを取得
  const { data: existingTicket } = useQuery<TicketEditData>({
    queryKey: ['ticket-edit', ticketId],
    queryFn: async () => {
      const res = await apiClient.get<TicketEditData>(`/tickets/${ticketId}/`);
      return res.data;
    },
    enabled: isEditing,
  });

  // ユーザー一覧取得（アサイン用）
  const { data: users } = useQuery<UserOption[]>({
    queryKey: ['users'],
    queryFn: async () => {
      const res = await apiClient.get<{ results?: UserOption[]; } & UserOption[]>('/users/');
      return res.data.results ?? res.data;
    },
  });

  // マイルストーン一覧取得
  const { data: milestones } = useQuery<MilestoneOption[]>({
    queryKey: ['milestones'],
    queryFn: async () => {
      const res = await apiClient.get<{ results?: MilestoneOption[] } & MilestoneOption[]>('/milestones/');
      return res.data.results ?? res.data;
    },
  });

  // サイクル一覧取得
  const { data: cycles } = useQuery<{ id: number; name: string; number: number; status: string }[]>({
    queryKey: ['cycles', currentProject?.id],
    queryFn: async () => {
      const res = await apiClient.get('/cycles/', {
        params: { project: currentProject?.id },
      });
      return (res.data.results ?? res.data) as { id: number; name: string; number: number; status: string }[];
    },
    enabled: !!currentProject?.id,
  });

  // ラベル一覧取得
  const { data: labels } = useQuery<LabelOption[]>({
    queryKey: ['labels'],
    queryFn: async () => {
      const res = await apiClient.get<{ results?: LabelOption[] } & LabelOption[]>('/labels/');
      return res.data.results ?? res.data;
    },
  });

  // カテゴリー一覧取得
  const { data: categories } = useQuery<CategoryOption[]>({
    queryKey: ['categories'],
    queryFn: async () => {
      const res = await apiClient.get<CategoryOption[]>('/categories/');
      return res.data;
    },
  });

  // 親チケット候補取得（同一プロジェクトの親チケットのみ）
  const { data: parentTickets } = useQuery<{ results: ParentTicketOption[] }>({
    queryKey: ['parent-tickets', currentProject?.id],
    queryFn: async () => {
      const params: Record<string, string> = { parent__isnull: 'true' };
      if (currentProject?.id) params.project = String(currentProject.id);
      const res = await apiClient.get<{ results: ParentTicketOption[] }>('/tickets/', { params });
      return res.data;
    },
  });

  const [formData, setFormData] = useState<TicketFormData>({
    title: '',
    description: '',
    status: 'open',
    priority: 'medium',
    ticket_type: 'issue',
    due_date: null,
    start_date: null,
    story_points: null,
  });
  const [assigneeIds, setAssigneeIds] = useState<number[]>([]);
  const [milestoneId, setMilestoneId] = useState<string>('');
  const [cycleId, setCycleId] = useState<string>('');
  const [selectedLabels, setSelectedLabels] = useState<number[]>([]);
  const [parentId, setParentId] = useState<string>('');
  const [categoryId, setCategoryId] = useState<string>('');
  const [teamId, setTeamId] = useState<string>('');
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [isInitialized, setIsInitialized] = useState(!isEditing);

  // 既存データが取得できたらフォームに反映
  if (isEditing && existingTicket && !isInitialized) {
    setFormData({
      title: existingTicket.title,
      description: existingTicket.description,
      status: existingTicket.status as TicketFormData['status'],
      priority: existingTicket.priority as TicketFormData['priority'],
      ticket_type: existingTicket.ticketType as TicketFormData['ticket_type'],
      due_date: existingTicket.dueDate,
      start_date: existingTicket.startDate,
      story_points: existingTicket.storyPoints ?? null,
    });
    setAssigneeIds(existingTicket.assignees?.map((a) => a.id) ?? []);
    setMilestoneId(existingTicket.milestone ? String(existingTicket.milestone.id) : '');
    setCycleId(existingTicket.cycle != null ? String(existingTicket.cycle) : '');
    setSelectedLabels(existingTicket.labels?.map((l) => l.id) ?? []);
    setParentId(existingTicket.parent != null ? String(existingTicket.parent) : '');
    setCategoryId(existingTicket.category ? String(existingTicket.category.id) : '');
    setTeamId(existingTicket.assignedTeam ? String(existingTicket.assignedTeam.id) : '');
    setIsInitialized(true);
  }

  const mutation = useMutation({
    mutationFn: async (data: TicketFormData) => {
      const payload = {
        ...data,
        assignees: assigneeIds,
        milestone: milestoneId ? Number(milestoneId) : null,
        cycle: cycleId ? Number(cycleId) : null,
        labels: selectedLabels,
        parent: parentId ? Number(parentId) : null,
        category: categoryId ? Number(categoryId) : null,
        assigned_team: teamId ? Number(teamId) : null,
        project: currentProject?.id,
      };
      if (isEditing) {
        await apiClient.patch(`/tickets/${ticketId}/`, payload);
      } else {
        await apiClient.post('/tickets/', payload);
      }
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['tickets'] });
      if (isEditing) {
        void queryClient.invalidateQueries({ queryKey: ['ticket', ticketId] });
      }
      if (projectKey) {
        navigate(`/p/${projectKey}/tickets`);
      } else {
        navigate(-1);
      }
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

        {/* 2カラム: 種別 + カテゴリー */}
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
            <label htmlFor="category" className="ticket-form__label">
              {t('ticket.category', 'Category')}
            </label>
            <select
              id="category"
              className="ticket-form__select"
              value={categoryId}
              onChange={(e) => setCategoryId(e.target.value)}
              data-testid="ticket-category-input"
            >
              <option value="">— None</option>
              {(categories ?? []).map((c) => (
                <option key={c.id} value={c.id}>
                  {c.level === 2 ? `  ${c.name}` : c.name}
                </option>
              ))}
            </select>
          </div>
        </div>

        {/* 日程: 開始日 + 期限 */}
        <div className="ticket-form__row">
          <div className="ticket-form__field">
            <label htmlFor="start_date" className="ticket-form__label">
              {t('ticket.startDate', 'Start Date')}
            </label>
            <input
              id="start_date"
              type="date"
              className="ticket-form__input"
              value={formData.start_date ?? ''}
              onChange={(e) => updateField('start_date', e.target.value || null)}
              data-testid="ticket-start-date-input"
            />
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

        {/* 親チケット */}
        <div className="ticket-form__field">
          <label htmlFor="parent" className="ticket-form__label">
            {t('ticket.parent', 'Parent Ticket')}
          </label>
          <select
            id="parent"
            className="ticket-form__select"
            value={parentId}
            onChange={(e) => setParentId(e.target.value)}
            data-testid="ticket-parent-input"
          >
            <option value="">— None (Top-level)</option>
            {(parentTickets?.results ?? []).filter((t) => String(t.id) !== ticketId).map((t) => (
              <option key={t.id} value={t.id}>
                {t.ticketKey} {t.title}
              </option>
            ))}
          </select>
        </div>

        {/* 3行目: Assignees + Milestone */}
        <div className="ticket-form__row">
          <div className="ticket-form__field">
            <label className="ticket-form__label">
              {t('ticket.assignees', 'Assignees')}
            </label>
            <div className="ticket-form__multi-select" data-testid="ticket-assignee-input">
              {(users ?? []).map((u) => (
                <label key={u.id} className="ticket-form__checkbox-label">
                  <input
                    type="checkbox"
                    checked={assigneeIds.includes(u.id)}
                    onChange={(e) => {
                      if (e.target.checked) {
                        setAssigneeIds([...assigneeIds, u.id]);
                      } else {
                        setAssigneeIds(assigneeIds.filter((id) => id !== u.id));
                      }
                    }}
                  />
                  <span className="ticket-form__checkbox-name">
                    {u.displayName || u.username}
                  </span>
                </label>
              ))}
            </div>
          </div>

          <div className="ticket-form__field">
            <label htmlFor="milestone" className="ticket-form__label">
              {t('ticket.milestone', 'Milestone')}
            </label>
            <select
              id="milestone"
              className="ticket-form__select"
              value={milestoneId}
              onChange={(e) => setMilestoneId(e.target.value)}
              data-testid="ticket-milestone-input"
            >
              <option value="">— None</option>
              {(milestones ?? []).map((ms) => (
                <option key={ms.id} value={ms.id}>
                  {ms.name}
                </option>
              ))}
            </select>
          </div>

          <div className="ticket-form__field">
            <label htmlFor="cycle" className="ticket-form__label">
              Cycle
            </label>
            <select
              id="cycle"
              className="ticket-form__select"
              value={cycleId}
              onChange={(e) => setCycleId(e.target.value)}
              data-testid="ticket-cycle-input"
            >
              <option value="">— None</option>
              {(cycles ?? []).map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name}
                </option>
              ))}
            </select>
          </div>
        </div>

        {/* 担当チーム */}
        <TeamSelect teamId={teamId} onTeamChange={setTeamId} />

        {/* ラベル */}
        <div className="ticket-form__field">
          <label className="ticket-form__label">
            {t('ticket.labels', 'Labels')}
          </label>
          <div className="ticket-form__labels-grid">
            {(labels ?? []).map((label) => (
              <label key={label.id} className="ticket-form__label-chip">
                <input
                  type="checkbox"
                  checked={selectedLabels.includes(label.id)}
                  onChange={(e) => {
                    if (e.target.checked) {
                      setSelectedLabels((prev) => [...prev, label.id]);
                    } else {
                      setSelectedLabels((prev) => prev.filter((id) => id !== label.id));
                    }
                  }}
                />
                <span
                  className="ticket-form__label-color"
                  style={{ backgroundColor: label.color || '#6366f1' }}
                />
                {label.name}
              </label>
            ))}
            {(labels ?? []).length === 0 && (
              <span className="ticket-form__label-empty">
                No labels defined
              </span>
            )}
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

/** 担当チームセレクト（TicketForm内サブコンポーネント） */
function TeamSelect({ teamId, onTeamChange }: { teamId: string; onTeamChange: (v: string) => void }) {
  const { data: teams } = useTeams();
  return (
    <div className="ticket-form__field">
      <label htmlFor="assigned_team" className="ticket-form__label">
        Team
      </label>
      <select
        id="assigned_team"
        className="ticket-form__select"
        value={teamId}
        onChange={(e) => onTeamChange(e.target.value)}
        data-testid="ticket-team-input"
      >
        <option value="">— None</option>
        {(teams ?? []).map((team) => (
          <option key={team.id} value={team.id}>
            {team.icon} {team.name}
          </option>
        ))}
      </select>
    </div>
  );
}
