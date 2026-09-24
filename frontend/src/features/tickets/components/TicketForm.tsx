/**
 * TicketForm.tsx — チケット作成・編集フォーム
 *
 * Zodバリデーション付き。作成時と編集時で共通化。
 * 開発標準書: Layer 1（フロントバリデーション）準拠。
 */

import { useState, useEffect, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate, useParams } from 'react-router-dom';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { z } from 'zod';
import { apiClient } from '@/shared/api/client';
import { TICKET_DASHBOARD_INVALIDATE_KEYS } from '@/shared/utils/ticketQueryInvalidation';
import { useProject } from '@/shared/hooks/useProject';
import { useTeam } from '@/shared/hooks/useTeam';
import { useTeams } from '@/features/teams/hooks/useTeams';
import { activeTeams as filterActiveTeams } from '@/features/teams/utils/archivedTeams';
import { useCycle } from '@/features/cycles/hooks/useCycles';
import { buildTicketShareUrl } from '../utils/ticketNavigation';
import { projectsForTeam } from '../utils/projectChoice';
import { fetchTicketUserOptions, ticketUserOptionsEnabled } from '../utils/ticketUserOptions';
import './TicketForm.css';

// Zodバリデーションスキーマ
const ticketSchema = z.object({
  title: z.string().min(1, 'Title is required').max(500),
  description: z.string().default(''),
  status: z.enum(['backlog', 'open', 'in_progress', 'resolved', 'closed', 'canceled']).default('open'),
  priority: z.enum(['urgent', 'high', 'medium', 'low']).default('medium'),
  ticket_type: z.enum(['bug', 'issue', 'task', 'qa']).default('issue'),
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
  team: { id: number; name: string } | null;
  project: number | null;
  startDate: string | null;
  dueDate: string | null;
  storyPoints: number | null;
}

interface UserOption {
  id: number;
  username: string;
  displayName: string;
  alias?: string | null;
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

interface TicketFormProps {
  /** モーダルから開く場合、URLの:projectKeyが取れないのでこちらを優先する */
  projectKeyOverride?: string;
  /** モーダルから開く場合、URLの:teamSlugが取れないのでこちらを優先する */
  teamSlugOverride?: string;
  /** 指定するとモーダルモードになり、成功/キャンセル時にnavigateの代わりにこれを呼ぶ */
  onClose?: () => void;
  /** 新規作成時、descriptionの初期値（コメントから新規チケット化する場合など） */
  initialDescription?: string;
  /** 新規作成時、parentの初期値（コメントからサブチケット作成する場合など） */
  initialParent?: number | null;
  /** 新規作成時、cycle の初期値（Cycle 詳細からの起票） */
  initialCycleId?: number | null;
}

export function TicketForm({
  projectKeyOverride,
  teamSlugOverride,
  onClose,
  initialDescription,
  initialParent,
  initialCycleId,
}: TicketFormProps = {}) {
  const { t } = useTranslation();
  const { ticketId } = useParams<{ ticketId: string }>();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { projectKey: urlProjectKey, currentProject: urlCurrentProject, projectList } = useProject();
  const { currentTeam, teamList, teamSlug: urlTeamSlug } = useTeam();
  const projectKey = projectKeyOverride ?? urlProjectKey;
  const currentProject = projectKeyOverride
    ? projectList.find((p) => p.prefix.toLowerCase() === projectKeyOverride.toLowerCase()) ?? null
    : urlCurrentProject;
  const isEditing = !!ticketId && ticketId !== 'new';
  // 新規作成時にユーザーが選んだプロジェクト。null は「画面の文脈(URL)に従う」、'' は「プロジェクトなし(チームのみ)」
  const [projectSel, setProjectSel] = useState<string | null>(null);
  const activeProject =
    projectSel === null
      ? currentProject
      : projectList.find((p) => String(p.id) === projectSel) ?? null;
  const { data: initialCycle } = useCycle(
    !isEditing && initialCycleId ? initialCycleId : undefined,
  );

  // 編集時は既存データを取得
  // 詳細パネル(TicketDetailPanel)と同じキー['ticket', ticketId]を使う。
  // 別キーだと詳細パネル側のインライン編集やここでの保存がこのクエリを
  // 無効化し忘れ、5分間のstaleTime内は編集フォームに古いデータが
  // 表示され続ける(F5でリロードするまで直らない)バグになるため。
  const { data: existingTicket } = useQuery<TicketEditData>({
    queryKey: ['ticket', ticketId],
    queryFn: async () => {
      const res = await apiClient.get<TicketEditData>(`/tickets/${ticketId}/`);
      return res.data;
    },
    enabled: isEditing,
  });

  // サイクル一覧取得（Project / Team / Cycle起票の所属チーム）
  const cycleTeamId = currentTeam?.id
    ?? (initialCycle?.team ? initialCycle.team.id : undefined);

  const { data: users } = useQuery<UserOption[]>({
    queryKey: ['users', activeProject?.id ?? null, cycleTeamId ?? null],
    queryFn: () =>
      fetchTicketUserOptions(apiClient, {
        projectId: activeProject?.id ?? null,
        teamId: cycleTeamId ?? null,
      }),
    enabled: ticketUserOptionsEnabled({
      projectId: activeProject?.id ?? null,
      teamId: cycleTeamId ?? null,
    }),
  });

  // マイルストーン一覧取得
  const { data: milestones } = useQuery<MilestoneOption[]>({
    queryKey: ['milestones'],
    queryFn: async () => {
      const res = await apiClient.get<{ results?: MilestoneOption[] } & MilestoneOption[]>('/milestones/');
      return res.data.results ?? res.data;
    },
  });

  const { data: cycles } = useQuery<{ id: number; name: string; number: number; status: string }[]>({
    queryKey: ['cycles', activeProject?.id, cycleTeamId],
    queryFn: async () => {
      const params: Record<string, number> = {};
      if (activeProject?.id) params.project = activeProject.id;
      if (cycleTeamId) params.team = cycleTeamId;
      const res = await apiClient.get('/cycles/', { params });
      return (res.data.results ?? res.data) as { id: number; name: string; number: number; status: string }[];
    },
    enabled: !!activeProject?.id || !!cycleTeamId,
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
    queryKey: ['parent-tickets', activeProject?.id],
    queryFn: async () => {
      const params: Record<string, string> = { parent__isnull: 'true' };
      if (activeProject?.id) params.project = String(activeProject.id);
      const res = await apiClient.get<{ results: ParentTicketOption[] }>('/tickets/', { params });
      return res.data;
    },
  });

  const [formData, setFormData] = useState<TicketFormData>(() => ({
    title: '',
    description: initialDescription ?? '',
    status: 'open',
    priority: 'medium',
    ticket_type: 'issue',
    due_date: null,
    start_date: new Date().toISOString().slice(0, 10),
    story_points: null,
  }));
  const [linkCopied, setLinkCopied] = useState(false);
  const [assigneeIds, setAssigneeIds] = useState<number[]>([]);
  const [milestoneId, setMilestoneId] = useState<string>('');
  const [cycleId, setCycleId] = useState<string>(() =>
    initialCycleId != null ? String(initialCycleId) : '',
  );
  const [selectedLabels, setSelectedLabels] = useState<number[]>([]);
  const [parentId, setParentId] = useState<string>(
    () => (initialParent != null ? String(initialParent) : ''),
  );
  const [categoryId, setCategoryId] = useState<string>('');
  const [teamId, setTeamId] = useState<string>('');
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [isInitialized, setIsInitialized] = useState(false);
  const [pendingImages, setPendingImages] = useState<{ file: File; previewUrl: string }[]>([]);

  // 新規作成時の所属チーム初期値。
  // 優先: 明示 slug → Cycle の所属チーム → URL の team → 参加チーム（Cycle/現在チーム優先）。
  useEffect(() => {
    if (isInitialized || isEditing) return;

    const resolveBySlug = (slug: string | null | undefined) => {
      if (!slug) return null;
      if (currentTeam?.slug.toLowerCase() === slug.toLowerCase()) return currentTeam;
      return teamList.find((t) => t.slug.toLowerCase() === slug.toLowerCase()) ?? null;
    };

    const slug = teamSlugOverride ?? initialCycle?.team?.slug ?? urlTeamSlug;
    if (slug) {
      const match = resolveBySlug(slug);
      if (match) {
        setTeamId(String(match.id));
        setIsInitialized(true);
        return;
      }
      if (initialCycle?.team?.id) {
        setTeamId(String(initialCycle.team.id));
        setIsInitialized(true);
        return;
      }
      if (teamList.length === 0 && !currentTeam) {
        return;
      }
      setIsInitialized(true);
      return;
    }

    if (initialCycleId && !initialCycle) {
      return;
    }

    const projectTeams = currentProject?.teams ?? [];
    if (projectTeams.length > 0) {
      const selectedTeam =
        (initialCycle?.team
          ? projectTeams.find((t) => t.id === initialCycle.team!.id)
          : undefined)
        ?? projectTeams.find((t) => t.id === currentTeam?.id)
        ?? projectTeams[0];
      if (selectedTeam) {
        setTeamId(String(selectedTeam.id));
      }
      setIsInitialized(true);
      return;
    }

    setIsInitialized(true);
  }, [
    currentProject?.teams,
    currentTeam,
    initialCycle,
    initialCycleId,
    isEditing,
    isInitialized,
    teamList,
    teamSlugOverride,
    urlTeamSlug,
  ]);

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
    setTeamId(existingTicket.team ? String(existingTicket.team.id) : '');
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
        teamId: teamId ? Number(teamId) : null,
        project: isEditing
          ? (existingTicket?.project ?? null)
          : (activeProject?.id ?? null),
      };

      let targetTicketKey: string;
      if (isEditing) {
        await apiClient.patch(`/tickets/${ticketId}/`, payload);
        targetTicketKey = ticketId as string;
      } else {
        const res = await apiClient.post<{ id: number; ticketKey: string }>('/tickets/', payload);
        targetTicketKey = res.data.ticketKey;
      }

      if (pendingImages.length > 0) {
        const uploaded: { filename: string; fileUrl: string }[] = [];
        for (const img of pendingImages) {
          const fileFormData = new FormData();
          fileFormData.append('file', img.file);
          const uploadRes = await apiClient.post<{ filename: string; fileUrl: string }>(
            `/tickets/${targetTicketKey}/attachments/`,
            fileFormData,
          );
          uploaded.push({ filename: uploadRes.data.filename, fileUrl: uploadRes.data.fileUrl });
        }
        const imagesMarkdown = uploaded.map((u) => `![${u.filename}](${u.fileUrl})`).join('\n');
        await apiClient.patch(`/tickets/${targetTicketKey}/`, {
          ...payload,
          description: `${payload.description}\n\n${imagesMarkdown}`,
        });
      }
    },
    onSuccess: () => {
      pendingImages.forEach((img) => URL.revokeObjectURL(img.previewUrl));
      setPendingImages([]);
      for (const key of TICKET_DASHBOARD_INVALIDATE_KEYS) {
        void queryClient.invalidateQueries({ queryKey: key });
      }
      if (isEditing) {
        void queryClient.invalidateQueries({ queryKey: ['ticket', ticketId] });
      }
      if (onClose) {
        onClose();
        return;
      }
      if (teamSlugOverride) {
        navigate(`/team/${teamSlugOverride}/tickets`);
      } else if (projectKey) {
        navigate(`/project/${projectKey}/tickets`);
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

    if (!isEditing && !teamId) {
      setErrors({ teamId: 'Team is required' });
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

  function fieldError(field: keyof TicketFormData) {
    return errors[field] ? (
      <span className="ticket-form__error">{errors[field]}</span>
    ) : null;
  }

  function addPendingImages(files: File[]): boolean {
    const images = files.filter((f) => f.type.startsWith('image/'));
    if (images.length === 0) return false;
    setPendingImages((prev) => [
      ...prev,
      ...images.map((file) => ({ file, previewUrl: URL.createObjectURL(file) })),
    ]);
    return true;
  }

  function removePendingImage(index: number) {
    setPendingImages((prev) => {
      const target = prev[index];
      if (target) URL.revokeObjectURL(target.previewUrl);
      return prev.filter((_, i) => i !== index);
    });
  }

  return (
    <div className="ticket-form" data-testid="ticket-form-page">
      <h1 className="ticket-form__title" style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
        {isEditing ? t('ticket.edit') : t('ticket.create')}
        {isEditing && (
          <button
            type="button"
            className="detail-panel__edit-btn"
            onClick={() => {
              const url = buildTicketShareUrl(window.location.origin, ticketId, {
                projectKey,
                teamSlug: teamSlugOverride ?? urlTeamSlug,
              });
              void navigator.clipboard.writeText(url);
              setLinkCopied(true);
              setTimeout(() => setLinkCopied(false), 1500);
            }}
            aria-label="Copy ticket link"
            title={linkCopied ? 'コピーしました' : 'リンクをコピー'}
            data-testid="copy-link-btn-edit"
          >
            {linkCopied ? '✅' : '🔗'}
          </button>
        )}
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
            onPaste={(e) => {
              const files = Array.from(e.clipboardData?.files ?? []);
              if (addPendingImages(files)) e.preventDefault();
            }}
            onDragOver={(e) => e.preventDefault()}
            onDrop={(e) => {
              e.preventDefault();
              addPendingImages(Array.from(e.dataTransfer.files));
            }}
            placeholder="Add a description..."
            rows={6}
            data-testid="ticket-description-input"
          />
          {fieldError('description')}
          {pendingImages.length > 0 && (
            <div className="ticket-form__pending-images" data-testid="pending-images">
              {pendingImages.map((img, idx) => (
                <div key={idx} className="ticket-form__pending-image">
                  <img src={img.previewUrl} alt={img.file.name} />
                  <button
                    type="button"
                    className="ticket-form__pending-image-remove"
                    onClick={() => removePendingImage(idx)}
                    aria-label="Remove image"
                    title="Remove image"
                  >
                    ✕
                  </button>
                </div>
              ))}
            </div>
          )}
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
            {fieldError('status')}
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
            {fieldError('priority')}
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
              <option value="bug">🐛 Bug</option>
              <option value="issue">📋 Issue</option>
              <option value="task">📝 Task</option>
              <option value="qa">🔍 QA</option>
            </select>
            {fieldError('ticket_type')}
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
            <div style={{ fontSize: 'var(--font-size-sm)', color: 'var(--color-text-tertiary)', marginTop: 'var(--space-1)' }}>
              {t('ticket.categoryHelp')}
            </div>
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
            {fieldError('start_date')}
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
            {fieldError('due_date')}
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
            {(parentTickets?.results ?? []).filter((t) => t.ticketKey !== ticketId).map((t) => (
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
              {users && users.length === 0 && (
                <span className="ticket-form__label-empty">
                  プロジェクトにメンバーがいません。設定画面でメンバーを追加してください。
                </span>
              )}
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
            <div style={{ fontSize: 'var(--font-size-sm)', color: 'var(--color-text-tertiary)', marginTop: 'var(--space-1)' }}>
              {t('ticket.milestoneHelp')}
            </div>
          </div>

          <div className="ticket-form__field">
            <label htmlFor="cycle" className="ticket-form__label">
              {t('ticket.cycle')}
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
            <div style={{ fontSize: 'var(--font-size-sm)', color: 'var(--color-text-tertiary)', marginTop: 'var(--space-1)' }}>
              {t('ticket.cycleHelp')}
            </div>
          </div>
        </div>

        {/* 所属チーム (F3-1: team_id) — 必須 */}
        <TeamSelect
          teamId={teamId}
          onTeamChange={setTeamId}
          label="Team *"
          required
          projectTeams={activeProject?.teams?.length ? activeProject.teams : null}
        />
        {errors.teamId ? <span className="ticket-form__error">{errors.teamId}</span> : null}

        {/* 所属プロジェクト(任意)— 新規作成時のみ。選んだチームが参加しているプロジェクトだけ選べる */}
        {!isEditing && (
          <div className="ticket-form__field">
            <label htmlFor="project" className="ticket-form__label">
              {t('ticket.project', 'Project')}
            </label>
            <select
              id="project"
              className="ticket-form__select"
              value={activeProject ? String(activeProject.id) : ''}
              onChange={(e) => {
                setProjectSel(e.target.value);
                // メンバー・サイクル・親チケットはプロジェクトごとに違うため選び直す
                setAssigneeIds([]);
                setCycleId('');
                setParentId('');
              }}
              data-testid="ticket-project-input"
            >
              <option value="">{t('ticket.projectNone', '— なし(チームのみ)')}</option>
              {projectsForTeam(projectList, teamId ? Number(teamId) : null).map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
            <div style={{ fontSize: 'var(--font-size-sm)', color: 'var(--color-text-tertiary)', marginTop: 'var(--space-1)' }}>
              {t('ticket.projectHelp')}
            </div>
          </div>
        )}

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
            onClick={() => (onClose ? onClose() : navigate(-1))}
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

/** チームセレクト（TicketForm内サブコンポーネント） */
function TeamSelect({
  teamId,
  onTeamChange,
  label = 'Team',
  required = false,
  /** プロジェクト付き起票時は参加チームのみ。未指定なら所属チーム一覧 */
  projectTeams = null,
}: {
  teamId: string;
  onTeamChange: (v: string) => void;
  label?: string;
  required?: boolean;
  projectTeams?: { id: number; name: string; icon?: string | null }[] | null;
}) {
  const { data: allTeams } = useTeams();
  // アーカイブ済みチームを除外
  const teams = projectTeams ?? filterActiveTeams(allTeams) ?? [];
  return (
    <div className="ticket-form__field">
      <label htmlFor="team" className="ticket-form__label">
        {label}
      </label>
      <select
        id="team"
        className="ticket-form__select"
        value={teamId}
        onChange={(e) => onTeamChange(e.target.value)}
        data-testid="ticket-team-input"
        required={required}
      >
        <option value="">— None</option>
        {teams.map((team) => (
          <option key={team.id} value={team.id}>
            {team.icon} {team.name}
          </option>
        ))}
      </select>
    </div>
  );
}
