/**
 * TicketDetailPanel.tsx — チケット詳細右ペイン
 *
 * チケット一覧の右側にスライドインするパネル。
 * TicketDetail.tsx のコンパクト版。ページ遷移なしで詳細を表示。
 */

import { useEffect, useRef, useState, useMemo } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useNavigate, useLocation } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import ReactMarkdown from 'react-markdown';
import { useEditor, useEditorState, EditorContent, ReactRenderer } from '@tiptap/react';
import StarterKit from '@tiptap/starter-kit';
import TiptapPlaceholder from '@tiptap/extension-placeholder';
import TiptapMention from '@tiptap/extension-mention';
import tippy from 'tippy.js';
import type { Instance as TippyInstance, GetReferenceClientRect } from 'tippy.js';
import { MentionList } from './MentionList';
import type { MentionListRef, MentionListItem } from './MentionList';
import { apiClient } from '@/shared/api/client';
import { useProject } from '@/shared/hooks/useProject';
import { useTeam } from '@/shared/hooks/useTeam';
import { useWorkflowStatuses } from '@/features/settings/hooks/useWorkflowStatuses';
import { useOptimisticMutation } from '@/shared/hooks/useOptimisticMutation';
import {
  TICKET_DASHBOARD_INVALIDATE_KEYS,
} from '@/shared/utils/ticketQueryInvalidation';
import { useAuthStore } from '@/shared/stores/authStore';
import { useUIStore } from '@/shared/stores/uiStore';
import { usePromptGenerationStore } from '@/shared/stores/promptGenerationStore';
import { useToast } from '@/shared/stores/toastStore';
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
  reviewers: { id: number; username: string; displayName: string }[];
  author: { id: number; username: string; displayName: string } | null;
  category: { id: number; name: string; color: string } | null;
  milestone: { id: number; name: string; dueDate: string | null } | null;
  project: number | null;
  projectPrefix?: string | null;
  team?: { id: number; slug: string; name: string } | null;
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
  isWatching: boolean;
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
  alias?: string | null;
}

interface CommentData {
  id: number;
  body: string;
  author: { id: number; username: string; displayName: string };
  createdAt: string;
  updatedAt: string | null;
  anchorStart?: number | null;
  anchorEnd?: number | null;
  anchorQuote?: string | null;
  parentCommentId: number | null;
  isDeleted: boolean;
  replyCount: number;
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

// @メンション拡張の共通ファクトリ。メインのコメント入力欄・返信入力欄の
// 両方で同じサジェスト挙動(候補一覧・キーボード操作)を使うために切り出した。
function createMentionExtension(userOptionsRef: React.MutableRefObject<UserOption[]>) {
  return TiptapMention.extend({
    // 注意: `@[label](id)` のような丸括弧付き形式はMarkdownのリンク記法と
    // 完全に一致してしまい、ReactMarkdownが実際にリンクとしてパースしてしまう
    // （このメンション検出ロジックが動く前に消費される）ため、コロン区切りの
    // 単一角括弧形式にする（`[text]`単体はCommonMarkのリンクにはならない）。
    renderText({ node }) {
      return `@[${node.attrs.label ?? node.attrs.id}:${node.attrs.id}]`;
    },
  }).configure({
    HTMLAttributes: { class: 'mention-node' },
    suggestion: {
      items: ({ query }: { query: string }): MentionListItem[] => {
        const q = query.toLowerCase();
        return userOptionsRef.current
          .filter((u) =>
            q.length === 0
            || u.username.toLowerCase().startsWith(q)
            || (u.displayName ?? '').toLowerCase().includes(q)
            || (u.alias ?? '').toLowerCase().includes(q)
          )
          .slice(0, 10);
      },
      render: () => {
        let component: ReactRenderer<MentionListRef, any>;
        let popup: TippyInstance[];
        return {
          onStart: (props) => {
            component = new ReactRenderer(MentionList, { props, editor: props.editor });
            if (!props.clientRect) return;
            popup = tippy('body', {
              getReferenceClientRect: props.clientRect as GetReferenceClientRect,
              appendTo: () => document.body,
              content: component.element,
              showOnCreate: true,
              interactive: true,
              trigger: 'manual',
              placement: 'bottom-start',
            });
          },
          onUpdate: (props) => {
            component.updateProps(props);
            if (!props.clientRect) return;
            popup[0]?.setProps({ getReferenceClientRect: props.clientRect as GetReferenceClientRect });
          },
          onKeyDown: (props) => {
            if (props.event.key === 'Escape') {
              popup[0]?.hide();
              return true;
            }
            return component.ref?.onKeyDown(props) ?? false;
          },
          onExit: () => {
            popup[0]?.destroy();
            component.destroy();
          },
        };
      },
    },
  });
}

export function TicketDetailPanel({ ticketId, onClose }: Props) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const location = useLocation();
  const toast = useToast();
  const { projectKey } = useProject();
  const { teamSlug } = useTeam();
  const queryClient = useQueryClient();
  const [aiLoading, setAiLoading] = useState(false);
  const [aiSuggestion, setAiSuggestion] = useState<{ suggested_points: number; confidence_score: number; reason: string } | null>(null);
  const [showCloseAnalysis, setShowCloseAnalysis] = useState(false);
  const [labelPickerOpen, setLabelPickerOpen] = useState(false);
  const [assigneePickerOpen, setAssigneePickerOpen] = useState(false);
  const [reviewerPickerOpen, setReviewerPickerOpen] = useState(false);
  const [linkCopied, setLinkCopied] = useState(false);
  const [attachmentUploading, setAttachmentUploading] = useState(false);
  const [linkAdding, setLinkAdding] = useState(false);
  const [newLinkUrl, setNewLinkUrl] = useState('');
  const [newLinkTitle, setNewLinkTitle] = useState('');
  const [editingCommentId, setEditingCommentId] = useState<number | null>(null);
  const [editingCommentText, setEditingCommentText] = useState('');
  const [openMenuCommentId, setOpenMenuCommentId] = useState<number | null>(null);
  const [replyingToRootId, setReplyingToRootId] = useState<number | null>(null);
  const [showInlineCommentForm, setShowInlineCommentForm] = useState(false);
  const [inlineCommentAnchor, setInlineCommentAnchor] = useState<{ start: number; end: number; quote: string } | null>(null);
  const [inlineCommentText, setInlineCommentText] = useState('');
  const [inlineCommentFloatingPos, setInlineCommentFloatingPos] = useState<{ x: number; y: number } | null>(null);
  const descriptionRef = useRef<HTMLDivElement>(null);
  const currentUser = useAuthStore((s) => s.user);
  const { openAndGenerate, phase } = usePromptGenerationStore();
  const { openTicketFormModal } = useUIStore();

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
      queryClient.invalidateQueries({ queryKey: ['my-issues'] });
    }
  }, [ticket?.id, ticket?.status, queryClient]);

  const { data: workflowStatuses = [] } = useWorkflowStatuses(
    ticket?.project ?? undefined,
    ticket?.project ? undefined : ticket?.team?.id,
  );

  // プロジェクト／Team で使えるラベル一覧(ピッカー表示用)
  const { data: labelOptionsData } = useQuery<{ results: LabelOption[] }>({
    queryKey: ['labels', ticket?.project ?? null, ticket?.team?.id ?? null],
    queryFn: async () => {
      const res = await apiClient.get<{ results: LabelOption[] }>('/labels/', {
        params: {
          ...(ticket?.project ? { project: ticket.project } : {}),
          ...(!ticket?.project && ticket?.team?.id ? { team: ticket.team.id } : {}),
        },
      });
      return res.data;
    },
    enabled: (!!ticket?.project || !!ticket?.team?.id) && labelPickerOpen,
  });
  const labelOptions = labelOptionsData?.results ?? [];

  // プロジェクトで使えるユーザー一覧(ピッカー表示用、コメント欄の@メンション候補・ハイライトにも使う)
  const { data: userOptionsData } = useQuery<{ results?: UserOption[] } & UserOption[]>({
    queryKey: ['users', ticket?.project],
    queryFn: async () => {
      const res = await apiClient.get<{ results?: UserOption[] } & UserOption[]>('/users/', {
        params: { project: ticket?.project },
      });
      return res.data;
    },
    enabled: !!ticket?.project,
  });
  const userOptions: UserOption[] = (userOptionsData as any)?.results ?? (Array.isArray(userOptionsData) ? userOptionsData : []);

  // userOptionsはReact Queryで非同期に更新されるが、TipTapのMention拡張の
  // suggestion.items()はエディタ生成時に一度だけクロージャとして固定されるため、
  // refで常に最新値を参照できるようにする。
  const userOptionsRef = useRef<UserOption[]>(userOptions);
  useEffect(() => {
    userOptionsRef.current = userOptions;
  }, [userOptions]);

  // コメントメニューの外側クリックで閉じる
  useEffect(() => {
    if (openMenuCommentId == null) return;
    const handler = (e: MouseEvent) => {
      if (!(e.target as HTMLElement).closest('.detail-panel__comment-menu-wrapper')) {
        setOpenMenuCommentId(null);
      }
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [openMenuCommentId]);

  // URLハッシュ(#comment-{id})で指定されたコメントへ自動スクロール＋一時ハイライト
  useEffect(() => {
    if (!ticket) return;
    const match = location.hash.match(/^#comment-(\d+)$/);
    if (!match) return;
    const el = document.getElementById(`comment-${match[1]}`);
    if (!el) return;
    el.scrollIntoView({ behavior: 'smooth', block: 'center' });
    el.classList.add('detail-panel__comment--highlighted');
    const timer = setTimeout(() => {
      el.classList.remove('detail-panel__comment--highlighted');
    }, 2000);
    return () => clearTimeout(timer);
  }, [ticket, location.hash]);

  // メインのコメント入力欄（TipTap）。@メンションのリアルタイム候補・色付けに対応する。
  // 保存形式は今まで通りプレーンなMarkdown文字列のまま。メンション部分だけ
  // editor.getText()で `@[表示名](ユーザーID)` 形式に変換して送信する
  // （renderTextでカスタマイズ。バックエンドのfind_mentioned_user_idsで
  // ID直接参照として検出、既存の素の@usernameも後方互換で検出し続ける）。
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
        placeholder: 'Add a comment... (@username でメンションできます)',
      }),
      createMentionExtension(userOptionsRef),
    ],
    editorProps: {
      attributes: { 'data-testid': 'comment-input' },
      handlePaste: (_view, event) => {
        const items = event.clipboardData?.items;
        if (!items) return false;
        const imageFiles: File[] = [];
        for (let i = 0; i < items.length; i++) {
          const item = items[i];
          if (item && item.type.startsWith('image/')) {
            const file = item.getAsFile();
            if (file) imageFiles.push(file);
          }
        }
        if (imageFiles.length === 0) return false;
        imageFiles.forEach((file) => { void uploadAttachmentFile(file); });
        return true;
      },
    },
    content: '',
  });

  // TipTap v3ではuseEditorがトランザクションごとに自動再描画しないため、
  // 「空かどうか」の最新状態をuseEditorStateで購読してボタンの活性/非活性に使う。
  const commentEditorIsEmpty = useEditorState({
    editor: commentEditor,
    selector: ({ editor }) => !editor || editor.isEmpty,
  });

  // 返信入力欄用エディタ(メインのコメント欄と同じ@メンション機能を提供する)。
  // 返信は同時に1スレッドしか開けない(replyingToRootId)ため、インスタンスは1つでよい。
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
        placeholder: '返信を入力... (@username でメンションできます)',
      }),
      createMentionExtension(userOptionsRef),
    ],
    editorProps: {
      attributes: { 'data-testid': 'comment-reply-input' },
    },
    content: '',
  });

  const replyEditorIsEmpty = useEditorState({
    editor: replyEditor,
    selector: ({ editor }) => !editor || editor.isEmpty,
  });

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

  // 楽観的開始日変更
  const startDateMutation = useOptimisticMutation<void, string | null>({
    mutationFn: async (startDate) => {
      await apiClient.patch(`/tickets/${ticketId}/`, { start_date: startDate });
    },
    queryKey: ticketQueryKey,
    updater: (currentData, startDate) => {
      const data = currentData as TicketData | undefined;
      if (!data) return currentData;
      return { ...data, startDate };
    },
    invalidateKeys: TICKET_DASHBOARD_INVALIDATE_KEYS,
    errorMessage: '開始日の変更に失敗しました。',
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

  // 楽観的レビュアー変更(全置換)
  const reviewersMutation = useOptimisticMutation<void, UserOption[]>({
    mutationFn: async (reviewers) => {
      await apiClient.patch(`/tickets/${ticketId}/`, { reviewers: reviewers.map((r) => r.id) });
    },
    queryKey: ticketQueryKey,
    updater: (currentData, reviewers) => {
      const data = currentData as TicketData | undefined;
      if (!data) return currentData;
      return { ...data, reviewers };
    },
    invalidateKeys: [['tickets']],
    errorMessage: 'レビュアーの変更に失敗しました。',
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

  const toggleReviewer = (user: UserOption) => {
    const current = ticket?.reviewers ?? [];
    const exists = current.some((r) => r.id === user.id);
    const next = exists ? current.filter((r) => r.id !== user.id) : [...current, user];
    reviewersMutation.mutate(next);
  };

  // 楽観的コメント追加 — 投稿ボタン押下で即スレッドに表示
  interface CommentPayload {
    body: string;
    anchor?: { start: number; end: number; quote: string };
    parentCommentId?: number;
  }
  const commentMutation = useOptimisticMutation<void, CommentPayload>({
    mutationFn: async (payload) => {
      await apiClient.post(`/tickets/${ticketId}/comments/`, payload);
    },
    queryKey: ticketQueryKey,
    updater: (currentData, payload) => {
      const data = currentData as TicketData | undefined;
      if (!data) return currentData;
      return {
        ...data,
        commentCount: data.commentCount + 1,
        comments: [
          ...data.comments,
          {
            id: -Date.now(), // 仮 ID（onSettled でサーバーの真値に置換）
            body: payload.body,
            author: { id: 0, username: 'you', displayName: 'You' },
            createdAt: new Date().toISOString(),
            updatedAt: null,
            anchorStart: payload.anchor?.start,
            anchorEnd: payload.anchor?.end,
            anchorQuote: payload.anchor?.quote,
            parentCommentId: payload.parentCommentId ?? null,
            isDeleted: false,
            replyCount: 0,
          },
        ],
      };
    },
    onSuccessCallback: (_data, variables) => {
      if (variables.parentCommentId) {
        setReplyingToRootId(null);
        replyEditor?.commands.clearContent();
      } else {
        commentEditor?.commands.clearContent();
        setShowInlineCommentForm(false);
        setInlineCommentText('');
        setInlineCommentAnchor(null);
      }
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

  // コメント削除(投稿者本人のみ)
  const deleteCommentMutation = useOptimisticMutation<void, number>({
    mutationFn: async (commentId) => {
      await apiClient.delete(`/tickets/${ticketId}/comments/${commentId}/`);
    },
    queryKey: ticketQueryKey,
    updater: (currentData, commentId) => {
      const data = currentData as TicketData | undefined;
      if (!data) return currentData;
      return {
        ...data,
        comments: data.comments.map((c) =>
          c.id === commentId ? { ...c, isDeleted: true, body: '' } : c
        ),
      };
    },
    onSuccessCallback: () => {
      setOpenMenuCommentId(null);
    },
    errorMessage: 'コメントの削除に失敗しました。',
  });

  // チケット全体のウォッチトグル
  const watchMutation = useOptimisticMutation<void, { watch: boolean }>({
    mutationFn: async ({ watch }) => {
      if (watch) {
        await apiClient.post(`/tickets/${ticketId}/watch/`);
      } else {
        await apiClient.delete(`/tickets/${ticketId}/watch/`);
      }
    },
    queryKey: ticketQueryKey,
    updater: (currentData, { watch }) => {
      const data = currentData as TicketData | undefined;
      if (!data) return currentData;
      return { ...data, isWatching: watch };
    },
    errorMessage: 'ウォッチ設定の変更に失敗しました。',
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

  // 説明文のテキスト選択を処理
  const handleDescriptionMouseUp = (_e: React.MouseEvent<HTMLDivElement>) => {
    const selection = window.getSelection();
    if (!selection || selection.toString().trim().length === 0) {
      setShowInlineCommentForm(false);
      setInlineCommentFloatingPos(null);
      return;
    }

    // 選択範囲が説明文コンテナ内にあるか確認
    if (!descriptionRef.current || !descriptionRef.current.contains(selection.anchorNode as Node)) {
      setShowInlineCommentForm(false);
      return;
    }

    // テキストオフセットを計算
    const range = selection.getRangeAt(0);
    const preCaretRange = range.cloneRange();
    preCaretRange.selectNodeContents(descriptionRef.current);
    preCaretRange.setEnd(range.endContainer, range.endOffset);
    const end = preCaretRange.toString().length;
    const start = end - selection.toString().length;

    // フローティングボタンの位置を計算（position: fixed で使うためビューポート基準のまま渡す）
    const rect = range.getBoundingClientRect();
    setInlineCommentFloatingPos({
      x: rect.left,
      y: rect.top - 50,
    });

    // アンカー情報を保存
    setInlineCommentAnchor({
      start,
      end,
      quote: selection.toString(),
    });
  };

  // Phase E: チケットキーと@プロジェクトprefixをMarkdownリンク形式に変換
  const convertMentionsToMarkdownLinks = (text: string): string => {
    // IME入力中に@が全角「＠」になることがあるため、判定前に半角へ正規化する
    let result = text.replace(/＠/g, '@');

    // 1. チケットキーパターン: @([A-Z]{2,10}-\d{6}) → [@$1](/p/PREFIX/tickets/$1)
    // チケットキーのプレフィックス部分をプロジェクトキーとして使用
    result = result.replace(/@([A-Z]{2,10})-(\d{6})/g, (_match, prefix, number) => {
      const ticketKey = `${prefix}-${number}`;
      return `[@${ticketKey}](/p/${prefix}/tickets/${ticketKey})`;
    });

    // 2. プロジェクトprefixパターン: @([A-Z]{2,10})(?!-\d) → [@$1](/p/$1)
    // ネガティブルックアヘッド (?!-\d) により、直後にハイフン+数字が続かないことを確認
    result = result.replace(/@([A-Z]{2,10})(?!-\d)/g, (_match, prefix) => {
      return `[@${prefix}](/p/${prefix})`;
    });

    return result;
  };

  // コメント表示時のメンション処理（テキストノード内での処理用）
  //
  // usernameは`taro.yamada@example.com`のようにメール形式（内部に@を含む）のことが
  // 多いため、文字クラスベースの正規表現（例: /@([A-Za-z0-9_.-]+)/）では内部の@で
  // 途切れて誤検出する。プロジェクトメンバーの実在するusername/表示名の一覧を先に
  // 用意し、本文中の@の直後にどのトークンが（最長一致で）続くかを走査する方式にする
  // ことで、username自体に@を含む場合や表示名（日本語含む）でも正しく検出できる。
  const renderMentionHighlight = (rawText: string) => {
    // IME入力中に@が全角「＠」になることがあるため、判定前に半角へ正規化する
    const text = rawText.replace(/＠/g, '@');
    // トークン(username/表示名/エイリアス)→そのユーザーの逆引きマップ。
    // 表示時は実際に打った文字に関わらず、そのユーザーの表示名で統一して見せる。
    const tokenToUser = new Map<string, UserOption>();
    const userById = new Map<number, UserOption>();
    userOptions.forEach((u) => {
      userById.set(u.id, u);
      [u.username, u.displayName, u.alias].forEach((t) => {
        if (t) tokenToUser.set(t, u);
      });
    });
    const tokens = Array.from(tokenToUser.keys()).sort((a, b) => b.length - a.length);

    // TipTapのMention拡張がeditor.getText()で出力する `@[表示名](ユーザーID)` 形式の
    // マッチ範囲を先に洗い出しておく。IDで現在のユーザー情報を引き直すことで、
    // 投稿後にユーザーが表示名を変更していても常に最新の表示名で表示できる。
    const bracketRegex = /@\[[^:\]]*:(\d+)\]/g;
    const bracketMatches: { start: number; end: number; userId: number }[] = [];
    let bm: RegExpExecArray | null;
    while ((bm = bracketRegex.exec(text)) !== null) {
      bracketMatches.push({ start: bm.index, end: bm.index + bm[0].length, userId: Number(bm[1]) });
    }

    const parts: React.ReactNode[] = [];
    let i = 0;
    let lastFlush = 0;

    while (i < text.length) {
      const bracketMatch = bracketMatches.find((m) => m.start === i);
      if (bracketMatch) {
        const matchedUser = userById.get(bracketMatch.userId);
        if (i > lastFlush) {
          parts.push(text.substring(lastFlush, i));
        }
        parts.push(
          <span key={`mention-${i}`} style={{ color: '#f97316', fontWeight: 500 }}>
            {`@${matchedUser?.displayName || matchedUser?.alias || matchedUser?.username || 'ユーザー'}`}
          </span>
        );
        i = bracketMatch.end;
        lastFlush = i;
        continue;
      }
      if (text[i] === '@') {
        const rest = text.slice(i + 1);
        const matchedToken = tokens.find((t) => rest.startsWith(t));
        if (matchedToken) {
          const matchedUser = tokenToUser.get(matchedToken);
          if (i > lastFlush) {
            parts.push(text.substring(lastFlush, i));
          }
          parts.push(
            <span key={`mention-${i}`} style={{ color: '#f97316', fontWeight: 500 }}>
              {`@${matchedUser?.displayName || matchedUser?.username || matchedToken}`}
            </span>
          );
          i += 1 + matchedToken.length;
          lastFlush = i;
          continue;
        }
      }
      i += 1;
    }

    if (lastFlush < text.length) {
      parts.push(text.substring(lastFlush));
    }

    return parts.length > 0 ? parts : [text];
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

  // コメント一覧のグルーピング(Hooksは早期returnより前で呼ぶ必要があるため、
  // isLoading/!ticketの分岐より前に置く)
  const { topLevelComments, repliesByParent } = useMemo(() => {
    const top: CommentData[] = [];
    const replies = new Map<number, CommentData[]>();
    for (const c of ticket?.comments ?? []) {
      if (c.parentCommentId == null) {
        top.push(c);
      } else {
        const list = replies.get(c.parentCommentId) ?? [];
        list.push(c);
        replies.set(c.parentCommentId, list);
      }
    }
    return { topLevelComments: top, repliesByParent: replies };
  }, [ticket?.comments]);

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

  const statusChoices = workflowStatuses.length > 0
    ? workflowStatuses.map((s) => ({ value: s.slug, label: s.name, color: s.color }))
    : STATUS_OPTIONS;
  const currentStatus = statusChoices.find((s) => s.value === ticket.status);

  // renderComment関数の定義
  const renderComment = (comment: CommentData, isReply: boolean) => {
    const isOwnComment = !!currentUser && currentUser.id === comment.author.id;
    const isEditing = editingCommentId === comment.id;
    return (
      <div
        key={comment.id}
        id={`comment-${comment.id}`}
        className={isReply ? 'detail-panel__comment detail-panel__comment--reply' : 'detail-panel__comment'}
      >
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
          {!isEditing && !comment.isDeleted && comment.id > 0 && (
            <div className="detail-panel__comment-menu-wrapper">
              <button
                type="button"
                className="detail-panel__comment-menu-trigger"
                onClick={() => setOpenMenuCommentId(openMenuCommentId === comment.id ? null : comment.id)}
                aria-label="コメントメニュー"
                title="メニュー"
              >
                ⋯
              </button>
              {openMenuCommentId === comment.id && (
                <div className="detail-panel__comment-menu">
                  {isOwnComment && (
                    <button
                      type="button"
                      className="detail-panel__comment-menu-item"
                      onClick={() => {
                        startEditingComment(comment);
                        setOpenMenuCommentId(null);
                      }}
                    >
                      編集
                    </button>
                  )}
                  {isOwnComment && (
                    <button
                      type="button"
                      className="detail-panel__comment-menu-item detail-panel__comment-menu-item--danger"
                      onClick={() => {
                        if (window.confirm('このコメントを削除しますか？')) {
                          deleteCommentMutation.mutate(comment.id);
                        }
                        setOpenMenuCommentId(null);
                      }}
                    >
                      削除
                    </button>
                  )}
                  <button
                    type="button"
                    className="detail-panel__comment-menu-item"
                    onClick={() => {
                      const url = `${window.location.origin}/p/${projectKey}/tickets/${ticket.ticketKey}#comment-${comment.id}`;
                      void navigator.clipboard.writeText(url);
                      toast.success('コメントへのリンクをコピーしました');
                      setOpenMenuCommentId(null);
                    }}
                  >
                    リンクをコピー
                  </button>
                  {!comment.isDeleted && (
                    <button
                      type="button"
                      className="detail-panel__comment-menu-item"
                      onClick={() => {
                        openTicketFormModal(projectKey ?? undefined, undefined, { initialDescription: comment.body });
                        setOpenMenuCommentId(null);
                      }}
                    >
                      このコメントから新規チケット作成
                    </button>
                  )}
                  {!comment.isDeleted && (
                    <button
                      type="button"
                      className="detail-panel__comment-menu-item"
                      onClick={() => {
                        openTicketFormModal(projectKey ?? undefined, undefined, {
                          initialDescription: comment.body,
                          initialParent: ticket.id,
                        });
                        setOpenMenuCommentId(null);
                      }}
                    >
                      このコメントからサブチケット作成
                    </button>
                  )}
                </div>
              )}
            </div>
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
        ) : comment.isDeleted ? (
          <div className="detail-panel__comment-body">
            <p className="detail-panel__comment-deleted">このコメントは削除されました</p>
          </div>
        ) : (
          <div className="detail-panel__comment-body">
            {/* インラインコメントの引用テキスト表示 */}
            {comment.anchorQuote && (
              <blockquote
                style={{
                  borderLeft: '3px solid #007bff',
                  paddingLeft: '12px',
                  marginLeft: 0,
                  marginBottom: '8px',
                  color: '#666',
                  fontStyle: 'italic',
                  fontSize: '0.95em',
                }}
              >
                {comment.anchorQuote}
              </blockquote>
            )}
            <ReactMarkdown
              components={{
                p: ({ children }) => (
                  <p>
                    {Array.isArray(children) ? children.map((child, idx) => {
                      if (typeof child === 'string') {
                        return <span key={idx}>{renderMentionHighlight(child)}</span>;
                      }
                      return child;
                    }) : renderMentionHighlight(String(children))}
                  </p>
                ),
                li: ({ children }) => (
                  <li>
                    {Array.isArray(children) ? children.map((child, idx) => {
                      if (typeof child === 'string') {
                        return <span key={idx}>{renderMentionHighlight(child)}</span>;
                      }
                      return child;
                    }) : renderMentionHighlight(String(children))}
                  </li>
                ),
              }}
            >
              {convertMentionsToMarkdownLinks(comment.body)}
            </ReactMarkdown>
          </div>
        )}
      </div>
    );
  };

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
            className="detail-panel__edit-btn"
            onClick={() => watchMutation.mutate({ watch: !ticket.isWatching })}
            disabled={watchMutation.isPending}
            aria-label={ticket.isWatching ? 'Unwatch ticket' : 'Watch ticket'}
            title={ticket.isWatching ? 'ウォッチ解除' : 'ウォッチする'}
            data-testid="watch-ticket-btn"
          >
            {ticket.isWatching ? '🔔' : '🔕'}
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
              const prefix = ticket.projectPrefix ?? projectKey;
              const slug = ticket.team?.slug ?? teamSlug;
              if (prefix) {
                navigate(`/project/${prefix}/tickets/${ticket.ticketKey}/edit`);
              } else if (slug) {
                navigate(`/team/${slug}/tickets/${ticket.ticketKey}/edit`);
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
            {statusChoices.map((s) => (
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

        {/* レビュアー */}
        <div className="detail-panel__field">
          <span className="detail-panel__field-label">Reviewers</span>
          <div style={{ position: 'relative' }}>
            <div
              className="detail-panel__field-value"
              style={{ display: 'flex', flexWrap: 'wrap', gap: '4px', alignItems: 'center', cursor: 'pointer', minHeight: '22px' }}
              onClick={() => setReviewerPickerOpen((v) => !v)}
              data-testid="reviewer-picker-toggle"
            >
              {ticket.reviewers?.length > 0 ? (
                <span className="detail-panel__reviewer">
                  {ticket.reviewers.map((r) => (
                    <span key={r.id} className="detail-panel__avatar" title={r.displayName || r.username}>
                      {(r.displayName || r.username)[0]?.toUpperCase()}
                    </span>
                  ))}
                  <span>
                    {ticket.reviewers.map((r) => r.displayName || r.username).join(', ')}
                  </span>
                </span>
              ) : (
                <span className="detail-panel__unassigned">No reviewers</span>
              )}
            </div>
            {reviewerPickerOpen && (
              <>
                <div
                  style={{ position: 'fixed', inset: 0, zIndex: 10 }}
                  onClick={() => setReviewerPickerOpen(false)}
                />
                <div
                  style={{
                    position: 'absolute', top: '100%', left: 0, marginTop: '4px', zIndex: 11,
                    background: 'var(--color-bg-secondary)', border: '1px solid var(--color-border-default)',
                    borderRadius: 'var(--radius-sm)', padding: '6px', minWidth: '200px',
                    maxHeight: '240px', overflowY: 'auto', boxShadow: 'var(--shadow-lg, 0 4px 12px rgba(0,0,0,0.3))',
                  }}
                  data-testid="reviewer-picker-menu"
                >
                  {userOptions.length === 0 ? (
                    <div style={{ fontSize: 'var(--font-size-sm)', opacity: 0.6, padding: '4px' }}>
                      メンバーがいません
                    </div>
                  ) : (
                    userOptions.map((opt) => {
                      const checked = ticket.reviewers?.some((r) => r.id === opt.id) ?? false;
                      return (
                        <label
                          key={opt.id}
                          style={{ display: 'flex', alignItems: 'center', gap: '6px', padding: '3px 4px', cursor: 'pointer', fontSize: 'var(--font-size-sm)' }}
                        >
                          <input
                            type="checkbox"
                            checked={checked}
                            onChange={() => toggleReviewer(opt)}
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

        {/* 開始日 */}
        <div className="detail-panel__field">
          <span className="detail-panel__field-label">Start date</span>
          <input
            type="date"
            className="detail-panel__field-select"
            value={ticket.startDate ? ticket.startDate.slice(0, 10) : ''}
            onChange={(e) => startDateMutation.mutate(e.target.value || null)}
            data-testid="start-date-input"
          />
        </div>

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
          <div
            ref={descriptionRef}
            className="detail-panel__description-text"
            onMouseUp={handleDescriptionMouseUp}
            style={{ position: 'relative' }}
          >
            <ReactMarkdown>{ticket.description}</ReactMarkdown>
          </div>
          {/* インラインコメント追加ボタン */}
          {inlineCommentFloatingPos && inlineCommentAnchor && !showInlineCommentForm && (
            <button
              onClick={() => setShowInlineCommentForm(true)}
              style={{
                position: 'fixed',
                left: inlineCommentFloatingPos.x,
                top: inlineCommentFloatingPos.y,
                padding: '6px 12px',
                backgroundColor: 'var(--color-primary, #007bff)',
                color: '#fff',
                border: 'none',
                borderRadius: '4px',
                cursor: 'pointer',
                zIndex: 1000,
                fontSize: '12px',
              }}
            >
              コメントを追加
            </button>
          )}
          {/* インラインコメントミニフォーム */}
          {showInlineCommentForm && inlineCommentAnchor && (
            <div
              style={{
                position: 'fixed',
                left: inlineCommentFloatingPos?.x || 0,
                top: (inlineCommentFloatingPos?.y || 0) + 40,
                backgroundColor: '#fff',
                border: '1px solid #ddd',
                borderRadius: '4px',
                padding: '12px',
                minWidth: '300px',
                zIndex: 1001,
                boxShadow: '0 2px 8px rgba(0,0,0,0.1)',
              }}
            >
              <textarea
                value={inlineCommentText}
                onChange={(e) => setInlineCommentText(e.target.value)}
                placeholder="コメントを入力..."
                style={{
                  width: '100%',
                  minHeight: '80px',
                  padding: '8px',
                  border: '1px solid #ddd',
                  borderRadius: '4px',
                  fontFamily: 'inherit',
                  fontSize: '14px',
                  marginBottom: '8px',
                }}
              />
              <div style={{ display: 'flex', gap: '8px', justifyContent: 'flex-end' }}>
                <button
                  onClick={() => {
                    setShowInlineCommentForm(false);
                    setInlineCommentText('');
                    setInlineCommentAnchor(null);
                  }}
                  style={{
                    padding: '6px 12px',
                    backgroundColor: '#f0f0f0',
                    border: '1px solid #ddd',
                    borderRadius: '4px',
                    cursor: 'pointer',
                    fontSize: '12px',
                  }}
                >
                  キャンセル
                </button>
                <button
                  onClick={() => {
                    if (inlineCommentText.trim() && inlineCommentAnchor) {
                      commentMutation.mutate({
                        body: inlineCommentText.trim(),
                        anchor: inlineCommentAnchor,
                      });
                    }
                  }}
                  disabled={!inlineCommentText.trim() || commentMutation.isPending}
                  style={{
                    padding: '6px 12px',
                    backgroundColor: 'var(--color-primary, #007bff)',
                    color: '#fff',
                    border: 'none',
                    borderRadius: '4px',
                    cursor: commentMutation.isPending ? 'not-allowed' : 'pointer',
                    fontSize: '12px',
                    opacity: commentMutation.isPending || !inlineCommentText.trim() ? 0.6 : 1,
                  }}
                >
                  {commentMutation.isPending ? '...' : 'コメント'}
                </button>
              </div>
            </div>
          )}
        </div>
      )}

      {/* 経過メモ */}
      <div className="detail-panel__comments">
        <h3 className="detail-panel__section-title">
          経過メモ ({ticket.comments?.length ?? 0})
        </h3>

        {topLevelComments.map((comment) => (
          <div key={comment.id}>
            {renderComment(comment, false)}
            {(repliesByParent.get(comment.id) ?? []).map((reply) => renderComment(reply, true))}
            {comment.id > 0 && (
            <div className="detail-panel__comment-reply-row">
              {replyingToRootId === comment.id ? (
                <div className="detail-panel__comment-edit-form">
                  <EditorContent editor={replyEditor} className="detail-panel__comment-editor" />
                  <div className="detail-panel__comment-edit-actions">
                    <button
                      type="button"
                      className="detail-panel__comment-cancel"
                      onClick={() => {
                        setReplyingToRootId(null);
                        replyEditor?.commands.clearContent();
                      }}
                      disabled={commentMutation.isPending}
                    >
                      キャンセル
                    </button>
                    <button
                      type="button"
                      className="detail-panel__comment-submit"
                      onClick={() => {
                        const body = replyEditor?.getText({ blockSeparator: '\n' }).trim();
                        if (!body) return;
                        commentMutation.mutate({ body, parentCommentId: comment.id });
                      }}
                      disabled={replyEditorIsEmpty || commentMutation.isPending}
                    >
                      {commentMutation.isPending ? '...' : '返信'}
                    </button>
                  </div>
                </div>
              ) : (
                <button
                  type="button"
                  className="detail-panel__comment-reply-link"
                  onClick={() => {
                    setReplyingToRootId(comment.id);
                    replyEditor?.commands.clearContent();
                  }}
                >
                  返信
                </button>
              )}
            </div>
            )}
          </div>
        ))}

        {/* コメント入力（TipTap。@メンションは投稿前からリアルタイムで色付け表示される） */}
        <div className="detail-panel__comment-form" style={{ position: 'relative' }}>
          <EditorContent editor={commentEditor} className="detail-panel__comment-editor" />
          <button
            className="detail-panel__comment-submit"
            disabled={commentEditorIsEmpty || commentMutation.isPending}
            onClick={() => {
              const body = commentEditor?.getText({ blockSeparator: '\n' }).trim();
              if (body) {
                commentMutation.mutate({ body });
              }
            }}
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
