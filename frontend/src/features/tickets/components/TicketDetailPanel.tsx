/**
 * TicketDetailPanel.tsx — チケット詳細右ペイン
 *
 * チケット一覧の右側にスライドインするパネル。
 * TicketDetail.tsx のコンパクト版。ページ遷移なしで詳細を表示。
 */

import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { apiClient } from '@/shared/api/client';
import { useProject } from '@/shared/hooks/useProject';
import { useOptimisticMutation } from '@/shared/hooks/useOptimisticMutation';
import { TimeTracker } from './TimeTracker';
import { GitActivity } from './GitActivity';
import ChangeLogTimeline from './ChangeLogTimeline';
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
  commentCount: number;
  childCount: number;
  comments: CommentData[];
}

interface CommentData {
  id: number;
  body: string;
  author: { id: number; username: string; displayName: string };
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
  const navigate = useNavigate();
  const { projectKey } = useProject();
  const [commentText, setCommentText] = useState('');

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
    invalidateKeys: [['tickets']],
    errorMessage: 'ステータス変更に失敗しました。元に戻しました。',
  });

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
          },
        ],
      };
    },
    onSuccessCallback: () => {
      setCommentText('');
    },
    errorMessage: 'コメントの追加に失敗しました。',
  });

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
              if (projectKey) {
                navigate(`/p/${projectKey}/tickets/${ticket.id}/edit`);
              }
            }}
            aria-label="Edit ticket"
            title="編集"
          >
            ✏️
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
            onChange={(e) => statusMutation.mutate(e.target.value)}
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
          <span className="detail-panel__field-value">
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
          </span>
        </div>

        {/* 期限 */}
        <div className="detail-panel__field">
          <span className="detail-panel__field-label">Due date</span>
          <span className={`detail-panel__field-value ${ticket.dueDate && new Date(ticket.dueDate) < new Date() ? 'detail-panel__overdue' : ''}`}>
            {ticket.dueDate
              ? new Date(ticket.dueDate).toLocaleDateString()
              : '—'}
          </span>
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
            {ticket.description}
          </div>
        </div>
      )}

      {/* コメント */}
      <div className="detail-panel__comments">
        <h3 className="detail-panel__section-title">
          Comments ({ticket.comments?.length ?? 0})
        </h3>

        {ticket.comments?.map((comment) => (
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
              </span>
            </div>
            <div className="detail-panel__comment-body">{comment.body}</div>
          </div>
        ))}

        {/* コメント入力 */}
        <div className="detail-panel__comment-form">
          <textarea
            className="detail-panel__comment-input"
            placeholder="Add a comment..."
            value={commentText}
            onChange={(e) => setCommentText(e.target.value)}
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

      {/* タイムトラッカー */}
      <TimeTracker ticketId={ticket.id} />

      {/* Gitアクティビティ */}
      <GitActivity ticketId={ticket.id} />

      {/* 変更履歴タイムライン */}
      <ChangeLogTimeline ticketId={ticket.id} />
    </div>
  );
}
