/**
 * TicketDetailPanel.tsx — チケット詳細右ペイン
 *
 * チケット一覧の右側にスライドインするパネル。
 * TicketDetail.tsx のコンパクト版。ページ遷移なしで詳細を表示。
 */

import { useEffect, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import ReactMarkdown from 'react-markdown';
import { apiClient } from '@/shared/api/client';
import { useProject } from '@/shared/hooks/useProject';
import { useOptimisticMutation } from '@/shared/hooks/useOptimisticMutation';
import {
  TICKET_DASHBOARD_INVALIDATE_KEYS,
} from '@/shared/utils/ticketQueryInvalidation';
import { useAuthStore } from '@/shared/stores/authStore';
import { usePromptGenerationStore } from '@/shared/stores/promptGenerationStore';
import { TimeTracker } from './TimeTracker';
import { GitActivity } from './GitActivity';
import ChangeLogTimeline from './ChangeLogTimeline';
import { AIAnalysisPanel } from '@/features/ai/components/AIAnalysisPanel';
import { CloseAnalysisDialog } from '@/features/ai/components/CloseAnalysisDialog';
import './TicketDetailPanel.css';

interface TicketData {
  id: number;
  ticketKey: string;
  title: string;
  description: string;
  status: string;
  priority: string;
  ticketType: string;
  assignees: { id: number; username: string; displayName: string }[];
  author: { id: number; username: string; displayName: string } | null;
  category: { id: number; name: string; color: string } | null;
  milestone: { id: number; name: string; dueDate: string | null } | null;
  project: number | null;
  parent: number | null;
  startDate: string | null;
  dueDate: string | null;
  closedAt: string | null;
  createdAt: string;
  updatedAt: string;
  storyPoints: number | null;
  commentCount: number;
  childCount: number;
  comments: CommentData[];
  attachments: AttachmentData[];
  links: ReferenceLinkData[];
  linkedWikiPages?: { id: number; title: string; slug: string; category: string }[];
  labels: { id: number; name: string; color: string }[];
}

interface LabelOption {
  id: number;
  name: string;
  color: string;
}

interface UserOption {
  id: number;
  username: string;
  displayName: string;
}

interface CommentData {
  id: number;
  body: string;
  author: { id: number; username: string; displayName: string };
  createdAt: string;
  updatedAt: string | null;
}

interface AttachmentData {
  id: number;
  filename: string;
  fileSize: number;
  sizeDisplay: string;
  isImage: boolean;
  createdAt: string;
  uploader: { id: number; username: string; displayName: string };
  fileUrl: string;
}

interface ReferenceLinkData {
  id: number;
  url: string;
  title: string | null;
  createdBy: { id: number; displayName: string };
  createdAt: string;
}

const STATUS_OPTIONS = [
  { value: 'backlog', label: 'Backlog', color: 'var(--color-status-backlog, #6b7280)' },
  { value: 'open', label: 'Open', color: 'var(--color-status-open)' },
  { value: 'in_progress', label: 'In Progress', color: 'var(--color-status-in-progress)' },
  { value: 'resolved', label: 'Resolved', color: 'var(--color-status-resolved)' },
  { value: 'closed', label: 'Closed', color: 'var(--color-status-closed)' },
  { value: 'canceled', label: 'Canceled', color: 'var(--color-status-canceled, #9ca3af)' },
] as const;

const PRIORITY_OPTIONS = [
  { value: 'urgent', label: 'Urgent', icon: '⚠' },
  { value: 'high', label: 'High', icon: '▮▮▮' },
  { value: 'medium', label: 'Medium', icon: '▮▮' },
  { value: 'low', label: 'Low', icon: '▮' },
] as const;

/** ラベルの背景色からテキスト色を計算(LabelSettings.tsxと同じロジック) */
function getLabelTextColor(bgColor: string): string {
  const hex = bgColor.replace('#', '');
  const r = parseInt(hex.substring(0, 2), 16);
  const g = parseInt(hex.substring(2, 4), 16);
  const b = parseInt(hex.substring(4, 6), 16);
  const luminance = (0.299 * r + 0.587 * g + 0.114 * b) / 255;
  return luminance > 0.5 ? '#1a1a2e' : '#ffffff';
}

function timeAgo(dateStr: string): string {
  const diff = Date.now() - new Date(dateStr).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

interface Props {
  ticketId: string;
  onClose: () => void;
}

export function TicketDetailPanel({ ticketId, onClose }: Props) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { projectKey } = useProject();
  const queryClient = useQueryClient();
  const [commentText, setCommentText] = useState('');
  const [aiLoading, setAiLoading] = useState(false);
  const [aiSuggestion, setAiSuggestion] = useState<{ suggested_points: number; confidence_score: number; reason: string } | null>(null);
  const [showCloseAnalysis, setShowCloseAnalysis] = useState(false);
  const [labelPickerOpen, setLabelPickerOpen] = useState(false);
  const [assigneePickerOpen, setAssigneePickerOpen] = useState(false);
  const [linkCopied, setLinkCopied] = useState(false);
  const [attachmentUploading, setAttachmentUploading] = useState(false);
  const [linkAdding, setLinkAdding] = useState(false);
  const [newLinkUrl, setNewLinkUrl] = useState('');
  const [newLinkTitle, setNewLinkTitle] = useState('');
  const [editingCommentId, setEditingCommentId] = useState<number | null>(null);
  const [editingCommentText, setEditingCommentText] = useState('');
  const currentUser = useAuthStore((s) => s.user);
  const { openAndGenerate, phase } = usePromptGenerationStore();

  const ticketQueryKey = ['ticket', ticketId];

  // チケット詳細取得
  const { data: ticket, isLoading } = useQuery<TicketData>({
    queryKey: ticketQueryKey,
    queryFn: async () => {
      const res = await apiClient.get<TicketData>(`/tickets/${ticketId}/`);
      return res.data;
    },
    enabled: !!ticketId,
  });

  // 一覧を経由しないステータス変更(他タブ・他セッション・APIの直接更新等)を
  // 一覧側の表示に反映させるため、詳細取得時に一覧キャッシュを無効化する
  useEffect(() => {
    if (ticket) {
      queryClient.invalidateQueries({ queryKey: ['tickets'] });
    }
  }, [ticket?.id, ticket?.status, queryClient]);

  // プロジェクトで使えるラベル一覧(ピッカー表示用)
  const { data: labelOptionsData } = useQuery<{ results: LabelOption[] }>({
    queryKey: ['labels', ticket?.project],
    queryFn: async () => {
      const res = await apiClient.get<{ results: LabelOption[] }>('/labels/', {
        params: { project: ticket?.project },
      });
      return res.data;
    },
    enabled: !!ticket?.project && labelPickerOpen,
  });
  const labelOptions = labelOptionsData?.results ?? [];

  // プロジェクトで使えるユーザー一覧(ピッカー表示用)
  const { data: userOptionsData } = useQuery<{ results?: UserOption[] } & UserOption[]>({
    queryKey: ['users', ticket?.project],
    queryFn: async () => {
      const res = await apiClient.get<{ results?: UserOption[] } & UserOption[]>('/users/', {
        params: { project: ticket?.project },
      });
      return res.data;
    },
    enabled: !!ticket?.project && assigneePickerOpen,
  });
  const userOptions: UserOption[] = (userOptionsData as any)?.results ?? (Array.isArray(userOptionsData) ? userOptionsData : []);

  // AI 設定取得（タイムアウトとモデル）
  const { data: aiSettings } = useQuery<{ ollamaTimeoutSecs: number; ollamaModel: string }>({
    queryKey: ['settings-ai'],
    queryFn: async () => {
      const res = await apiClient.get<{ ollamaTimeoutSecs: number; ollamaModel: string }>('/settings/ai/');
      return res.data;
    },
  });

  // 楽観的ステータス変更 — ドロップダウン変更の瞬間にパネルが即更新（0ms）
  const statusMutation = useOptimisticMutation<void, string>({
    mutationFn: async (status) => {
      await apiClient.patch(`/tickets/${ticketId}/`, { status });
    },
    queryKey: ticketQueryKey,
    updater: (currentData, status) => {
      const data = currentData as TicketData | undefined;
      if (!data) return currentData;
      return { ...data, status };
    },
    invalidateKeys: TICKET_DASHBOARD_INVALIDATE_KEYS,
    errorMessage: 'ステータス変更に失敗しました。元に戻しました。',
  });

  // ステータス変更ハンドラ — closed/resolved 時にクローズ分析を起動
  const handleStatusChange = (newStatus: string) => {
    statusMutation.mutate(newStatus);
    if (newStatus === 'closed' || newStatus === 'resolved') {
      setShowCloseAnalysis(true);
    }
  };

  // 楽観的優先度変更
  const priorityMutation = useOptimisticMutation<void, string>({
    mutationFn: async (priority) => {
      await apiClient.patch(`/tickets/${ticketId}/`, { priority });
    },
    queryKey: ticketQueryKey,
    updater: (currentData, priority) => {
      const data = currentData as TicketData | undefined;
      if (!data) return currentData;
      return { ...data, priority };
    },
    invalidateKeys: [['tickets']],
    errorMessage: '優先度変更に失敗しました。元に戻しました。',
  });

  // 楽観的ストーリーポイント変更
  const storyPointsMutation = useOptimisticMutation<void, number | null>({
    mutationFn: async (points) => {
      await apiClient.patch(`/tickets/${ticketId}/`, { story_points: points });
    },
    queryKey: ticketQueryKey,
    updater: (currentData, points) => {
      const data = currentData as TicketData | undefined;
      if (!data) return currentData;
      return { ...data, storyPoints: points };
    },
    invalidateKeys: [['tickets']],
    errorMessage: 'ストーリーポイント変更に失敗しました。',
  });

  // 楽観的期限変更
  const dueDateMutation = useOptimisticMutation<void, string | null>({
    mutationFn: async (dueDate) => {
      await apiClient.patch(`/tickets/${ticketId}/`, { due_date: dueDate });
    },
    queryKey: ticketQueryKey,
    updater: (currentData, dueDate) => {
      const data = currentData as TicketData | undefined;
      if (!data) return currentData;
      return { ...data, dueDate };
    },
    invalidateKeys: TICKET_DASHBOARD_INVALIDATE_KEYS,
    errorMessage: '期限の変更に失敗しました。',
  });

  // 楽観的ラベル変更(全置換)
  const labelsMutation = useOptimisticMutation<void, LabelOption[]>({
    mutationFn: async (labels) => {
      await apiClient.patch(`/tickets/${ticketId}/`, { labels: labels.map((l) => l.id) });
    },
    queryKey: ticketQueryKey,
    updater: (currentData, labels) => {
      const data = currentData as TicketData | undefined;
      if (!data) return currentData;
      return { ...data, labels };
    },
    invalidateKeys: [['tickets']],
    errorMessage: 'ラベルの変更に失敗しました。',
  });

  // 楽観的担当者変更(全置換)
  const assigneesMutation = useOptimisticMutation<void, UserOption[]>({
    mutationFn: async (assignees) => {
      await apiClient.patch(`/tickets/${ticketId}/`, { assignees: assignees.map((a) => a.id) });
    },
    queryKey: ticketQueryKey,
    updater: (currentData, assignees) => {
      const data = currentData as TicketData | undefined;
      if (!data) return currentData;
      return { ...data, assignees };
    },
    invalidateKeys: [['tickets']],
    errorMessage: '担当者の変更に失敗しました。',
  });

  const toggleLabel = (label: LabelOption) => {
    const current = ticket?.labels ?? [];
    const exists = current.some((l) => l.id === label.id);
    const next = exists ? current.filter((l) => l.id !== label.id) : [...current, label];
    labelsMutation.mutate(next);
  };

  const toggleAssignee = (user: UserOption) => {
    const current = ticket?.assignees ?? [];
    const exists = current.some((a) => a.id === user.id);
    const next = exists ? current.filter((a) => a.id !== user.id) : [...current, user];
    assigneesMutation.mutate(next);
  };

  // 楽観的コメント追加 — 投稿ボタン押下で即スレッドに表示
  const commentMutation = useOptimisticMutation<void, string>({
    mutationFn: async (body) => {
      await apiClient.post(`/tickets/${ticketId}/comments/`, { body });
    },
    queryKey: ticketQueryKey,
    updater: (currentData, body) => {
      const data = currentData as TicketData | undefined;
      if (!data) return currentData;
      return {
        ...data,
        commentCount: data.commentCount + 1,
        comments: [
          ...data.comments,
          {
            id: -Date.now(), // 仮 ID（onSettled でサーバーの真値に置換）
            body,
            author: { id: 0, username: 'you', displayName: 'You' },
            createdAt: new Date().toISOString(),
            updatedAt: null,
          },
        ],
      };
    },
    onSuccessCallback: () => {
      setCommentText('');
    },
    errorMessage: 'コメントの追加に失敗しました。',
  });

  // 経過メモ編集(投稿者本人のみ)
  const editCommentMutation = useOptimisticMutation<void, { commentId: number; body: string }>({
    mutationFn: async ({ commentId, body }) => {
      await apiClient.patch(`/tickets/${ticketId}/comments/${commentId}/`, { body });
    },
    queryKey: ticketQueryKey,
    updater: (currentData, { commentId, body }) => {
      const data = currentData as TicketData | undefined;
      if (!data) return currentData;
      return {
        ...data,
        comments: data.comments.map((c) =>
          c.id === commentId ? { ...c, body, updatedAt: new Date().toISOString() } : c
        ),
      };
    },
    onSuccessCallback: () => {
      setEditingCommentId(null);
      setEditingCommentText('');
    },
    errorMessage: '経過メモの編集に失敗しました。',
  });

  const startEditingComment = (comment: CommentData) => {
    setEditingCommentId(comment.id);
    setEditingCommentText(comment.body);
  };

  const cancelEditingComment = () => {
    setEditingCommentId(null);
    setEditingCommentText('');
  };

  const saveEditingComment = (commentId: number) => {
    const trimmed = editingCommentText.trim();
    if (!trimmed) return;
    editCommentMutation.mutate({ commentId, body: trimmed });
  };

  // 添付ファイル実際のアップロードとキャッシュ更新
  const uploadAttachmentFile = async (file: File) => {
    const formData = new FormData();
    formData.append('file', file);

    setAttachmentUploading(true);
    try {
      const res = await apiClient.post<AttachmentData>(`/tickets/${ticketId}/attachments/`, formData);
      // キャッシュを更新（attachmentsが undefined の場合に備える）
      queryClient.setQueryData<TicketData | undefined>(ticketQueryKey, (old) => {
        if (!old) return old;
        return {
          ...old,
          attachments: [...(old.attachments ?? []), res.data],
        };
      });
    } catch (error) {
      console.error('ファイルアップロード失敗:', error);
      alert('ファイルのアップロードに失敗しました。');
    } finally {
      setAttachmentUploading(false);
    }
  };

  // 添付ファイルアップロード（入力要素の onChange ハンドラ）
  const handleAttachmentUpload = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = e.currentTarget.files;
    if (!files || files.length === 0) return;

    for (const file of Array.from(files)) {
      await uploadAttachmentFile(file);
    }
    e.currentTarget.value = ''; // フォームをリセット
  };

  // コメント入力エリアへの画像ペースト処理
  const handleCommentPaste = async (e: React.ClipboardEvent<HTMLTextAreaElement>) => {
    const items = e.clipboardData?.items;
    if (!items) return;
    const imageFiles: File[] = [];
    for (let i = 0; i < items.length; i++) {
      const item = items[i];
      if (item && item.type.startsWith('image/')) {
        const file = item.getAsFile();
        if (file) {
          imageFiles.push(file);
        }
      }
    }
    if (imageFiles.length === 0) return;
    e.preventDefault();
    for (const file of imageFiles) {
      await uploadAttachmentFile(file);
    }
  };

  // 添付ファイル削除
  const handleDeleteAttachment = async (attachmentId: number) => {
    if (!window.confirm('このファイルを削除しますか？')) return;

    try {
      // DELETE /api/v1/tickets/{ticket_id}/attachments/{attachment_id}/
      await apiClient.delete(`/tickets/${ticket?.id}/attachments/${attachmentId}/`);
      if (ticket) {
        const updatedTicket = {
          ...ticket,
          attachments: ticket.attachments.filter((a) => a.id !== attachmentId),
        };
        queryClient.setQueryData(ticketQueryKey, updatedTicket);
      }
    } catch (error) {
      console.error('ファイル削除失敗:', error);
      alert('ファイルの削除に失敗しました。');
    }
  };

  // プロンプト生成 — モーダルを開く
  const handleGeneratePrompt = () => {
    if (!ticket) return;
    const timeoutSecs = aiSettings?.ollamaTimeoutSecs ?? 60;
    const model = aiSettings?.ollamaModel ?? 'unknown';
    openAndGenerate(
      { id: ticket.id, ticketKey: ticket.ticketKey, title: ticket.title },
      timeoutSecs,
      model
    );
  };

  // 参照リンク追加
  const handleAddLink = async (url: string, title: string) => {
    const trimmedUrl = url.trim();
    if (!trimmedUrl) return;
    setLinkAdding(true);
    try {
      const res = await apiClient.post<ReferenceLinkData>(`/tickets/${ticketId}/links/`, {
        url: trimmedUrl,
        title: title.trim() || undefined,
      });
      queryClient.setQueryData<TicketData | undefined>(ticketQueryKey, (old) => {
        if (!old) return old;
        return {
          ...old,
          links: [...(old.links ?? []), res.data],
        };
      });
      setNewLinkUrl('');
      setNewLinkTitle('');
    } catch (error) {
      console.error('参照リンクの追加に失敗:', error);
      alert('参照リンクの追加に失敗しました。');
    } finally {
      setLinkAdding(false);
    }
  };

  // 参照リンク削除
  const handleDeleteLink = async (linkId: number) => {
    if (!window.confirm('この参照リンクを削除しますか？')) return;

    try {
      await apiClient.delete(`/tickets/${ticket?.id}/links/${linkId}/`);
      if (ticket) {
        const updatedTicket = {
          ...ticket,
          links: ticket.links.filter((l) => l.id !== linkId),
        };
        queryClient.setQueryData(ticketQueryKey, updatedTicket);
      }
    } catch (error) {
      console.error('参照リンク削除失敗:', error);
      alert('参照リンクの削除に失敗しました。');
    }
  };

  // チケット削除(子チケットも再帰的に削除される)
  const deleteTicketMutation = useMutation({
    mutationFn: async () => {
      await apiClient.delete(`/tickets/${ticketId}/`);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['tickets'] });
      onClose();
    },
    onError: (error) => {
      console.error('チケット削除失敗:', error);
      alert('チケットの削除に失敗しました。');
    },
  });

  const handleDeleteTicket = () => {
    if (!ticket) return;
    const warning = ticket.childCount > 0
      ? `このチケットには子チケットが${ticket.childCount}件あります。削除すると子チケットもすべて削除されます。本当に削除しますか？`
      : 'このチケットを削除しますか？この操作は取り消せません。';
    if (!window.confirm(warning)) return;
    deleteTicketMutation.mutate();
  };

  if (isLoading) {
    return (
      <div className="detail-panel detail-panel--loading" data-testid="detail-panel">
        <div className="detail-panel__header">
          <div className="detail-panel__skeleton-title" />
          <button className="detail-panel__close" onClick={onClose}>✕</button>
        </div>
      </div>
    );
  }

  if (!ticket) return null;

  const currentStatus = STATUS_OPTIONS.find((s) => s.value === ticket.status);

  return (
    <div className="detail-panel" data-testid="detail-panel">
      {/* ヘッダー */}
      <div className="detail-panel__header">
        <span className="detail-panel__key">{ticket.ticketKey}</span>
        <div className="detail-panel__header-actions">
          <button
            className="detail-panel__edit-btn"
            onClick={() => {
              const url = `${window.location.origin}/p/${projectKey}/tickets/${ticket.ticketKey}`;
              void navigator.clipboard.writeText(url);
              setLinkCopied(true);
              setTimeout(() => setLinkCopied(false), 1500);
            }}
            aria-label="Copy ticket link"
            title={linkCopied ? 'コピーしました' : 'リンクをコピー'}
            data-testid="copy-link-btn"
          >
            {linkCopied ? '✅' : '🔗'}
          </button>
          <button
            className="detail-panel__edit-btn detail-panel__prompt-btn"
            onClick={handleGeneratePrompt}
            disabled={phase === 'generating'}
            aria-label={t('ai.generatePrompt')}
            title={t('ai.generatePrompt')}
            data-testid="generate-prompt-btn"
          >
            📋
          </button>
          <button
            className="detail-panel__edit-btn"
            onClick={() => {
              if (projectKey) {
                navigate(`/p/${projectKey}/tickets/${ticket.ticketKey}/edit`);
              }
            }}
            aria-label="Edit ticket"
            title="編集"
          >
            ✏️
          </button>
          <button
            className="detail-panel__edit-btn detail-panel__edit-btn--danger"
            onClick={handleDeleteTicket}
            disabled={deleteTicketMutation.isPending}
            aria-label="Delete ticket"
            title="削除"
            data-testid="delete-ticket-btn"
          >
            🗑️
          </button>
          <button
            className="detail-panel__close"
            onClick={onClose}
            aria-label="Close panel"
            data-testid="panel-close"
          >
            ✕
          </button>
        </div>
      </div>

      {/* タイトル */}
      <h2 className="detail-panel__title">{ticket.title}</h2>

      {/* メタ情報 */}
      <div className="detail-panel__meta">
        {/* ステータス */}
        <div className="detail-panel__field">
          <span className="detail-panel__field-label">Status</span>
          <select
            className="detail-panel__field-select"
            value={ticket.status}
            onChange={(e) => handleStatusChange(e.target.value)}
            style={{ color: currentStatus?.color }}
          >
            {STATUS_OPTIONS.map((s) => (
              <option key={s.value} value={s.value}>{s.label}</option>
            ))}
          </select>
        </div>

        {/* 優先度 */}
        <div className="detail-panel__field">
          <span className="detail-panel__field-label">Priority</span>
          <select
            className="detail-panel__field-select"
            value={ticket.priority}
            onChange={(e) => priorityMutation.mutate(e.target.value)}
          >
            {PRIORITY_OPTIONS.map((p) => (
              <option key={p.value} value={p.value}>{p.icon} {p.label}</option>
            ))}
          </select>
        </div>

        {/* 担当者 */}
        <div className="detail-panel__field">
          <span className="detail-panel__field-label">Assignees</span>
          <div style={{ position: 'relative' }}>
            <div
              className="detail-panel__field-value"
              style={{ display: 'flex', flexWrap: 'wrap', gap: '4px', alignItems: 'center', cursor: 'pointer', minHeight: '22px' }}
              onClick={() => setAssigneePickerOpen((v) => !v)}
              data-testid="assignee-picker-toggle"
            >
              {ticket.assignees?.length > 0 ? (
                <span className="detail-panel__assignee">
                  {ticket.assignees.map((a) => (
                    <span key={a.id} className="detail-panel__avatar" title={a.displayName || a.username}>
                      {(a.displayName || a.username)[0]?.toUpperCase()}
                    </span>
                  ))}
                  <span>
                    {ticket.assignees.map((a) => a.displayName || a.username).join(', ')}
                  </span>
                </span>
              ) : (
                <span className="detail-panel__unassigned">Unassigned</span>
              )}
            </div>
            {assigneePickerOpen && (
              <>
                <div
                  style={{ position: 'fixed', inset: 0, zIndex: 10 }}
                  onClick={() => setAssigneePickerOpen(false)}
                />
                <div
                  style={{
                    position: 'absolute', top: '100%', left: 0, marginTop: '4px', zIndex: 11,
                    background: 'var(--color-bg-secondary)', border: '1px solid var(--color-border-default)',
                    borderRadius: 'var(--radius-sm)', padding: '6px', minWidth: '200px',
                    maxHeight: '240px', overflowY: 'auto', boxShadow: 'var(--shadow-lg, 0 4px 12px rgba(0,0,0,0.3))',
                  }}
                  data-testid="assignee-picker-menu"
                >
                  {userOptions.length === 0 ? (
                    <div style={{ fontSize: 'var(--font-size-sm)', opacity: 0.6, padding: '4px' }}>
                      メンバーがいません
                    </div>
                  ) : (
                    userOptions.map((opt) => {
                      const checked = ticket.assignees?.some((a) => a.id === opt.id) ?? false;
                      return (
                        <label
                          key={opt.id}
                          style={{ display: 'flex', alignItems: 'center', gap: '6px', padding: '3px 4px', cursor: 'pointer', fontSize: 'var(--font-size-sm)' }}
                        >
                          <input
                            type="checkbox"
                            checked={checked}
                            onChange={() => toggleAssignee(opt)}
                          />
                          <span>{opt.displayName || opt.username}</span>
                        </label>
                      );
                    })
                  )}
                </div>
              </>
            )}
          </div>
        </div>

        {/* 起票者 */}
        {ticket.author && (
          <div className="detail-panel__field">
            <span className="detail-panel__field-label">Author</span>
            <span className="detail-panel__field-value">
              {ticket.author.displayName || ticket.author.username}
            </span>
          </div>
        )}

        {/* 期限 */}
        <div className="detail-panel__field">
          <span className="detail-panel__field-label">Due date</span>
          <input
            type="date"
            className={`detail-panel__field-select ${ticket.dueDate && new Date(ticket.dueDate) < new Date() ? 'detail-panel__overdue' : ''}`}
            value={ticket.dueDate ? ticket.dueDate.slice(0, 10) : ''}
            onChange={(e) => dueDateMutation.mutate(e.target.value || null)}
            data-testid="due-date-input"
          />
        </div>

        {/* ラベル */}
        <div className="detail-panel__field">
          <span className="detail-panel__field-label">Labels</span>
          <div style={{ position: 'relative' }}>
            <div
              className="detail-panel__field-value"
              style={{ display: 'flex', flexWrap: 'wrap', gap: '4px', alignItems: 'center', cursor: 'pointer', minHeight: '22px' }}
              onClick={() => setLabelPickerOpen((v) => !v)}
              data-testid="label-picker-toggle"
            >
              {ticket.labels?.length > 0 ? (
                ticket.labels.map((l) => (
                  <span
                    key={l.id}
                    className="settings-label-badge"
                    style={{ background: l.color, color: getLabelTextColor(l.color) }}
                  >
                    {l.name}
                  </span>
                ))
              ) : (
                <span className="detail-panel__unassigned">+ ラベルを追加</span>
              )}
            </div>
            {labelPickerOpen && (
              <>
                <div
                  style={{ position: 'fixed', inset: 0, zIndex: 10 }}
                  onClick={() => setLabelPickerOpen(false)}
                />
                <div
                  style={{
                    position: 'absolute', top: '100%', left: 0, marginTop: '4px', zIndex: 11,
                    background: 'var(--color-bg-secondary)', border: '1px solid var(--color-border-default)',
                    borderRadius: 'var(--radius-sm)', padding: '6px', minWidth: '200px',
                    maxHeight: '240px', overflowY: 'auto', boxShadow: 'var(--shadow-lg, 0 4px 12px rgba(0,0,0,0.3))',
                  }}
                  data-testid="label-picker-menu"
                >
                  {labelOptions.length === 0 ? (
                    <div style={{ fontSize: 'var(--font-size-sm)', opacity: 0.6, padding: '4px' }}>
                      ラベルがありません
                    </div>
                  ) : (
                    labelOptions.map((opt) => {
                      const checked = ticket.labels?.some((l) => l.id === opt.id) ?? false;
                      return (
                        <label
                          key={opt.id}
                          style={{ display: 'flex', alignItems: 'center', gap: '6px', padding: '3px 4px', cursor: 'pointer', fontSize: 'var(--font-size-sm)' }}
                        >
                          <input
                            type="checkbox"
                            checked={checked}
                            onChange={() => toggleLabel(opt)}
                          />
                          <span
                            className="settings-label-badge"
                            style={{ background: opt.color, color: getLabelTextColor(opt.color) }}
                          >
                            {opt.name}
                          </span>
                        </label>
                      );
                    })
                  )}
                </div>
              </>
            )}
          </div>
        </div>

        {/* ストーリーポイント */}
        <div className="detail-panel__field">
          <span className="detail-panel__field-label">Story Points</span>
          <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
            <select
              value={ticket.storyPoints ?? ''}
              onChange={(e) => {
                const val = e.target.value === '' ? null : Number(e.target.value);
                storyPointsMutation.mutate(val);
              }}
              style={{
                padding: '2px 6px',
                background: 'var(--color-bg-tertiary)',
                border: '1px solid var(--color-border-default)',
                borderRadius: 'var(--radius-sm)',
                color: 'var(--color-text-primary)',
                fontSize: 'var(--font-size-sm)',
                cursor: 'pointer',
              }}
              data-testid="story-points-input"
            >
              <option value="">—</option>
              <option value="1">1 — 瞬殺 / No-brainer</option>
              <option value="2">2 — 普通 / Straightforward</option>
              <option value="3">3 — ちょい重 / Moderate</option>
              <option value="5">5 — 時の運 / Risky</option>
              <option value="8">8 — 泥沼 / Here be dragons 🐉</option>
            </select>
            <button
              type="button"
              className="detail-panel__ai-btn"
              title={aiSuggestion?.reason || t('ai.suggestPoints')}
              disabled={aiLoading}
              onClick={async () => {
                setAiLoading(true);
                setAiSuggestion(null);
                try {
                  const res = await apiClient.post('/ai/suggest-points/', {
                    title: ticket.title,
                    description: ticket.description || '',
                  });
                  const data = res.data as { suggested_points: number; confidence_score: number; reason: string };
                  setAiSuggestion(data);
                  if (data.confidence_score > 0) {
                    storyPointsMutation.mutate(data.suggested_points);
                  }
                } catch {
                  setAiSuggestion({ suggested_points: 2, confidence_score: 0, reason: 'AI unavailable' });
                } finally {
                  setAiLoading(false);
                }
              }}
              data-testid="ai-suggest-btn"
            >
              {aiLoading ? '⏳' : '🤖'}
            </button>
          </div>
          {aiSuggestion && aiSuggestion.confidence_score > 0 && (
            <div className="detail-panel__ai-reason">
              <span className="detail-panel__ai-confidence">
                {Math.round(aiSuggestion.confidence_score * 100)}%
              </span>
              {aiSuggestion.reason}
            </div>
          )}
        </div>

        {/* カテゴリ */}
        {ticket.category && (
          <div className="detail-panel__field">
            <span className="detail-panel__field-label">Category</span>
            <span
              className="detail-panel__category-badge"
              style={{ borderColor: ticket.category.color }}
            >
              {ticket.category.name}
            </span>
          </div>
        )}

        {/* マイルストーン */}
        {ticket.milestone && (
          <div className="detail-panel__field">
            <span className="detail-panel__field-label">Milestone</span>
            <span className="detail-panel__field-value">
              {ticket.milestone.name}
            </span>
          </div>
        )}
      </div>

      {/* 説明 */}
      {ticket.description && (
        <div className="detail-panel__description">
          <h3 className="detail-panel__section-title">Description</h3>
          <div className="detail-panel__description-text">
            <ReactMarkdown>{ticket.description}</ReactMarkdown>
          </div>
        </div>
      )}

      {/* 経過メモ */}
      <div className="detail-panel__comments">
        <h3 className="detail-panel__section-title">
          経過メモ ({ticket.comments?.length ?? 0})
        </h3>

        {ticket.comments?.map((comment) => {
          const isOwnComment = !!currentUser && currentUser.id === comment.author.id;
          const isEditing = editingCommentId === comment.id;
          return (
            <div key={comment.id} className="detail-panel__comment">
              <div className="detail-panel__comment-header">
                <span className="detail-panel__comment-avatar">
                  {(comment.author.displayName || comment.author.username)[0]?.toUpperCase()}
                </span>
                <span className="detail-panel__comment-author">
                  {comment.author.displayName || comment.author.username}
                </span>
                <span className="detail-panel__comment-time">
                  {timeAgo(comment.createdAt)}
                  {comment.updatedAt && ' (編集済み)'}
                </span>
                {isOwnComment && !isEditing && (
                  <button
                    type="button"
                    className="detail-panel__comment-edit-btn"
                    onClick={() => startEditingComment(comment)}
                    aria-label="経過メモを編集"
                    title="編集"
                  >
                    ✏️
                  </button>
                )}
              </div>
              {isEditing ? (
                <div className="detail-panel__comment-edit-form">
                  <textarea
                    className="detail-panel__comment-input"
                    value={editingCommentText}
                    onChange={(e) => setEditingCommentText(e.target.value)}
                    rows={6}
                    autoFocus
                    data-testid="comment-edit-input"
                  />
                  <div className="detail-panel__comment-edit-actions">
                    <button
                      type="button"
                      className="detail-panel__comment-cancel"
                      onClick={cancelEditingComment}
                      disabled={editCommentMutation.isPending}
                    >
                      キャンセル
                    </button>
                    <button
                      type="button"
                      className="detail-panel__comment-submit"
                      onClick={() => saveEditingComment(comment.id)}
                      disabled={!editingCommentText.trim() || editCommentMutation.isPending}
                      data-testid="comment-edit-save"
                    >
                      {editCommentMutation.isPending ? '...' : '保存'}
                    </button>
                  </div>
                </div>
              ) : (
                <div className="detail-panel__comment-body">
                  <ReactMarkdown>{comment.body}</ReactMarkdown>
                </div>
              )}
            </div>
          );
        })}

        {/* コメント入力 */}
        <div className="detail-panel__comment-form">
          <textarea
            className="detail-panel__comment-input"
            placeholder="Add a comment..."
            value={commentText}
            onChange={(e) => setCommentText(e.target.value)}
            onPaste={(e) => { void handleCommentPaste(e); }}
            rows={10}
            data-testid="comment-input"
          />
          <button
            className="detail-panel__comment-submit"
            disabled={!commentText.trim() || commentMutation.isPending}
            onClick={() => commentText.trim() && commentMutation.mutate(commentText.trim())}
            data-testid="comment-submit"
          >
            {commentMutation.isPending ? '...' : 'Comment'}
          </button>
        </div>
      </div>

      {/* 添付ファイル */}
      <div style={{ marginTop: '1.5rem' }}>
        <h3 className="detail-panel__section-title">
          📎 Attachments ({ticket.attachments?.length ?? 0})
        </h3>

        {ticket.attachments && ticket.attachments.length > 0 && (
          <div style={{ marginBottom: '1rem' }}>
            {ticket.attachments.map((att) => (
              <div
                key={att.id}
                style={{
                  display: 'flex',
                  justifyContent: 'space-between',
                  alignItems: 'center',
                  padding: '0.5rem 0.75rem',
                  background: 'var(--color-bg-secondary, #f5f5f5)',
                  borderRadius: '4px',
                  marginBottom: '0.5rem',
                  fontSize: '0.8125rem',
                }}
              >
                <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', flex: 1, minWidth: 0 }}>
                  <span>{att.isImage ? '🖼️' : '📄'}</span>
                  <a
                    href={att.fileUrl}
                    download={att.filename}
                    style={{
                      color: 'var(--color-text-link)',
                      textDecoration: 'none',
                      overflow: 'hidden',
                      textOverflow: 'ellipsis',
                      whiteSpace: 'nowrap',
                    }}
                    title={att.filename}
                  >
                    {att.filename}
                  </a>
                  <span style={{ opacity: 0.6, whiteSpace: 'nowrap' }}>({att.sizeDisplay})</span>
                </div>
                <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', marginLeft: '0.5rem' }}>
                  <span style={{ opacity: 0.5, fontSize: '0.75rem' }}>
                    {timeAgo(att.createdAt)}
                  </span>
                  <button
                    onClick={() => handleDeleteAttachment(att.id)}
                    style={{
                      background: 'transparent',
                      border: 'none',
                      cursor: 'pointer',
                      padding: '2px 4px',
                      color: '#ef4444',
                      fontSize: '0.75rem',
                    }}
                    title="Delete attachment"
                  >
                    ✕
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}

        {/* アップロード入力 */}
        <div style={{ display: 'flex', gap: '0.5rem' }}>
          <label
            style={{
              flex: 1,
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              padding: '0.5rem',
              border: '1px dashed var(--color-border-default, #ccc)',
              borderRadius: '4px',
              cursor: attachmentUploading ? 'wait' : 'pointer',
              background: 'var(--color-bg-tertiary, #fafafa)',
              opacity: attachmentUploading ? 0.6 : 1,
              fontSize: '0.8125rem',
            }}
          >
            {attachmentUploading ? '📤 Uploading...' : '📤 Click to upload file'}
            <input
              type="file"
              multiple
              onChange={handleAttachmentUpload}
              disabled={attachmentUploading}
              style={{ display: 'none' }}
              data-testid="attachment-input"
            />
          </label>
        </div>
      </div>

      {/* 参照リンク */}
      <div style={{ marginTop: '1.5rem' }}>
        <h3 className="detail-panel__section-title">
          🔗 Reference Links ({ticket.links?.length ?? 0})
        </h3>

        {ticket.links && ticket.links.length > 0 && (
          <div style={{ marginBottom: '1rem' }}>
            {ticket.links.map((link) => (
              <div
                key={link.id}
                style={{
                  display: 'flex',
                  justifyContent: 'space-between',
                  alignItems: 'center',
                  padding: '0.5rem 0.75rem',
                  background: 'var(--color-bg-secondary, #f5f5f5)',
                  borderRadius: '4px',
                  marginBottom: '0.5rem',
                  fontSize: '0.8125rem',
                }}
              >
                <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', flex: 1, minWidth: 0 }}>
                  <span>🔗</span>
                  <a
                    href={link.url}
                    target="_blank"
                    rel="noopener noreferrer"
                    style={{
                      color: 'var(--color-text-link)',
                      textDecoration: 'none',
                      overflow: 'hidden',
                      textOverflow: 'ellipsis',
                      whiteSpace: 'nowrap',
                    }}
                    title={link.url}
                  >
                    {link.title || link.url}
                  </a>
                </div>
                <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', marginLeft: '0.5rem' }}>
                  <span style={{ opacity: 0.5, fontSize: '0.75rem' }}>
                    {timeAgo(link.createdAt)}
                  </span>
                  <button
                    onClick={() => handleDeleteLink(link.id)}
                    style={{
                      background: 'transparent',
                      border: 'none',
                      cursor: 'pointer',
                      padding: '2px 4px',
                      color: '#ef4444',
                      fontSize: '0.75rem',
                    }}
                    title="Delete link"
                  >
                    ✕
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}

        {/* リンク追加フォーム */}
        <div style={{ display: 'flex', gap: '0.5rem' }}>
          <input
            type="text"
            placeholder="URL"
            value={newLinkUrl}
            onChange={(e) => setNewLinkUrl(e.target.value)}
            disabled={linkAdding}
            style={{
              flex: 2,
              padding: '0.5rem',
              border: '1px solid var(--color-border-default, #ccc)',
              borderRadius: '4px',
              fontSize: '0.8125rem',
            }}
          />
          <input
            type="text"
            placeholder="タイトル(任意)"
            value={newLinkTitle}
            onChange={(e) => setNewLinkTitle(e.target.value)}
            disabled={linkAdding}
            style={{
              flex: 1,
              padding: '0.5rem',
              border: '1px solid var(--color-border-default, #ccc)',
              borderRadius: '4px',
              fontSize: '0.8125rem',
            }}
          />
          <button
            onClick={() => void handleAddLink(newLinkUrl, newLinkTitle)}
            disabled={linkAdding || !newLinkUrl.trim()}
            style={{
              padding: '0.5rem 0.75rem',
              border: '1px dashed var(--color-border-default, #ccc)',
              borderRadius: '4px',
              cursor: linkAdding || !newLinkUrl.trim() ? 'not-allowed' : 'pointer',
              background: 'var(--color-bg-tertiary, #fafafa)',
              opacity: linkAdding || !newLinkUrl.trim() ? 0.6 : 1,
              fontSize: '0.8125rem',
              whiteSpace: 'nowrap',
            }}
          >
            {linkAdding ? '追加中...' : '追加'}
          </button>
        </div>
      </div>

      {/* タイムトラッカー */}
      <TimeTracker ticketId={ticket.id} />

      {/* Gitアクティビティ */}
      <GitActivity ticketKey={ticket.ticketKey} />

      {/* 変更履歴タイムライン */}
      <ChangeLogTimeline ticketId={ticket.ticketKey} />

      {/* 紐付きWikiページ */}
      {ticket.linkedWikiPages && ticket.linkedWikiPages.length > 0 && (
        <div style={{ marginTop: '1rem' }}>
          <h4 style={{ fontSize: '0.8125rem', fontWeight: 600, margin: '0 0 0.5rem', opacity: 0.7 }}>
            📖 紐付きWiki ({ticket.linkedWikiPages.length})
          </h4>
          <div style={{ display: 'flex', flexDirection: 'column', gap: '0.25rem' }}>
            {ticket.linkedWikiPages.map((wp: { id: number; title: string; slug: string; category: string }) => (
              <a
                key={wp.id}
                href={`/wiki?page=${wp.slug}`}
                style={{
                  padding: '0.375rem 0.5rem', borderRadius: '4px',
                  background: 'var(--color-bg-tertiary)',
                  color: 'var(--color-text-primary)',
                  fontSize: '0.8125rem', textDecoration: 'none',
                  display: 'block',
                }}
              >
                📄 {wp.title}
              </a>
            ))}
          </div>
        </div>
      )}

      {/* AIコンテキスト分析 */}
      <AIAnalysisPanel ticketId={ticket.id} />

      {/* クローズ分析ダイアログ */}
      <CloseAnalysisDialog
        ticketId={ticket.id}
        projectId={ticket.project ?? 0}
        isOpen={showCloseAnalysis}
        onClose={() => setShowCloseAnalysis(false)}
      />
    </div>
  );
}
