/**
 * TicketDetail.tsx — チケット詳細画面
 *
 * Linearスタイルの2カラムレイアウト:
 *   左: タイトル + 説明 + コメントスレッド
 *   右: メタ情報パネル（ステータス, 優先度, 担当者, 期限, マイルストーン）
 */

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useParams, Link, useNavigate } from 'react-router-dom';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { useAuthStore } from '@/shared/stores/authStore';
import './TicketDetail.css';

interface TicketData {
  id: number;
  ticketKey: string;
  title: string;
  description: string;
  status: string;
  priority: string;
  ticketType: string;
  assignee: { id: number; username: string; displayName: string } | null;
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
  { value: 'open', label: 'Open', color: 'var(--color-status-open)' },
  { value: 'in_progress', label: 'In Progress', color: 'var(--color-status-in-progress)' },
  { value: 'resolved', label: 'Resolved', color: 'var(--color-status-resolved)' },
  { value: 'closed', label: 'Closed', color: 'var(--color-status-closed)' },
] as const;

const PRIORITY_OPTIONS = [
  { value: 'urgent', label: 'Urgent', icon: '⬆⬆' },
  { value: 'high', label: 'High', icon: '⬆' },
  { value: 'medium', label: 'Medium', icon: '—' },
  { value: 'low', label: 'Low', icon: '⬇' },
] as const;

function formatDate(dateStr: string | null): string {
  if (!dateStr) return '—';
  return new Date(dateStr).toLocaleDateString('en-US', {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  });
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

export function TicketDetail() {
  const { t } = useTranslation();
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const user = useAuthStore((s) => s.user);

  const [commentText, setCommentText] = useState('');

  // チケット詳細取得
  const { data: ticket, isLoading } = useQuery<TicketData>({
    queryKey: ['ticket', id],
    queryFn: async () => {
      const res = await apiClient.get<TicketData>(`/tickets/${id}/`);
      return res.data;
    },
    enabled: !!id,
  });

  // フィールド更新
  const updateMutation = useMutation({
    mutationFn: async (patch: Record<string, unknown>) => {
      await apiClient.patch(`/tickets/${id}/`, patch);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['ticket', id] });
      void queryClient.invalidateQueries({ queryKey: ['tickets'] });
    },
  });

  // コメント追加
  const commentMutation = useMutation({
    mutationFn: async (body: string) => {
      await apiClient.post(`/tickets/${id}/comments/`, { body });
    },
    onSuccess: () => {
      setCommentText('');
      void queryClient.invalidateQueries({ queryKey: ['ticket', id] });
    },
  });

  // 削除
  const deleteMutation = useMutation({
    mutationFn: async () => {
      await apiClient.delete(`/tickets/${id}/`);
    },
    onSuccess: () => {
      navigate('/tickets');
    },
  });

  if (isLoading) {
    return (
      <div className="ticket-detail__loading" data-testid="ticket-loading">
        <div className="ticket-detail__skeleton ticket-detail__skeleton--title" />
        <div className="ticket-detail__skeleton ticket-detail__skeleton--body" />
      </div>
    );
  }

  if (!ticket) {
    return (
      <div className="ticket-detail__empty">
        Ticket not found. <Link to="/tickets">Back to list</Link>
      </div>
    );
  }

  const currentStatus = STATUS_OPTIONS.find((s) => s.value === ticket.status);

  return (
    <div className="ticket-detail" data-testid="ticket-detail-page">
      {/* ── ヘッダー ── */}
      <div className="ticket-detail__header">
        <Link to="/tickets" className="ticket-detail__back" data-testid="back-to-list">
          ← {t('nav.tickets')}
        </Link>
        <div className="ticket-detail__header-right">
          <button
            className="ticket-detail__delete-btn"
            onClick={() => {
              if (window.confirm('Delete this ticket?')) {
                deleteMutation.mutate();
              }
            }}
            data-testid="delete-ticket-btn"
          >
            🗑 {t('common.delete')}
          </button>
        </div>
      </div>

      <div className="ticket-detail__layout">
        {/* ── 左カラム: メインコンテンツ ── */}
        <div className="ticket-detail__main">
          {/* チケットキー + タイトル */}
          <div className="ticket-detail__title-section">
            <span className="ticket-detail__key">{ticket.ticketKey}</span>
            <h1 className="ticket-detail__title" data-testid="ticket-title">
              {ticket.title}
            </h1>
          </div>

          {/* 説明 */}
          <div className="ticket-detail__description" data-testid="ticket-description">
            {ticket.description || (
              <span className="ticket-detail__no-description">
                No description provided.
              </span>
            )}
          </div>

          {/* コメントスレッド */}
          <div className="ticket-detail__comments" data-testid="comment-section">
            <h2 className="ticket-detail__section-title">
              Comments ({ticket.comments?.length ?? 0})
            </h2>

            {/* コメント一覧 */}
            <div className="ticket-detail__comment-list">
              {(ticket.comments ?? []).map((comment) => (
                <div
                  key={comment.id}
                  className="ticket-detail__comment"
                  data-testid={`comment-${comment.id}`}
                >
                  <div className="ticket-detail__comment-avatar">
                    {(comment.author.displayName || comment.author.username)[0]?.toUpperCase()}
                  </div>
                  <div className="ticket-detail__comment-content">
                    <div className="ticket-detail__comment-header">
                      <span className="ticket-detail__comment-author">
                        {comment.author.displayName || comment.author.username}
                      </span>
                      <span className="ticket-detail__comment-time">
                        {timeAgo(comment.createdAt)}
                      </span>
                    </div>
                    <div className="ticket-detail__comment-body">{comment.body}</div>
                  </div>
                </div>
              ))}
            </div>

            {/* コメント入力 */}
            <div className="ticket-detail__comment-form" data-testid="comment-form">
              <div className="ticket-detail__comment-avatar">
                {(user?.firstName ?? user?.username ?? '?')[0]?.toUpperCase()}
              </div>
              <div className="ticket-detail__comment-input-wrapper">
                <textarea
                  className="ticket-detail__comment-input"
                  placeholder="Add a comment..."
                  value={commentText}
                  onChange={(e) => setCommentText(e.target.value)}
                  rows={3}
                  data-testid="comment-input"
                />
                <button
                  className="ticket-detail__comment-submit"
                  disabled={!commentText.trim() || commentMutation.isPending}
                  onClick={() => {
                    if (commentText.trim()) {
                      commentMutation.mutate(commentText.trim());
                    }
                  }}
                  data-testid="comment-submit"
                >
                  {commentMutation.isPending ? '...' : 'Comment'}
                </button>
              </div>
            </div>
          </div>
        </div>

        {/* ── 右カラム: メタ情報パネル ── */}
        <aside className="ticket-detail__sidebar" data-testid="ticket-sidebar">
          {/* ステータス */}
          <div className="ticket-detail__field">
            <label className="ticket-detail__field-label">Status</label>
            <select
              className="ticket-detail__field-select"
              value={ticket.status}
              onChange={(e) => updateMutation.mutate({ status: e.target.value })}
              style={{
                color: currentStatus?.color,
                borderColor: currentStatus?.color,
              }}
              data-testid="detail-status-select"
            >
              {STATUS_OPTIONS.map((s) => (
                <option key={s.value} value={s.value}>{s.label}</option>
              ))}
            </select>
          </div>

          {/* 優先度 */}
          <div className="ticket-detail__field">
            <label className="ticket-detail__field-label">Priority</label>
            <select
              className="ticket-detail__field-select"
              value={ticket.priority}
              onChange={(e) => updateMutation.mutate({ priority: e.target.value })}
              data-testid="detail-priority-select"
            >
              {PRIORITY_OPTIONS.map((p) => (
                <option key={p.value} value={p.value}>{p.icon} {p.label}</option>
              ))}
            </select>
          </div>

          {/* 担当者 */}
          <div className="ticket-detail__field">
            <label className="ticket-detail__field-label">Assignee</label>
            <div className="ticket-detail__field-value">
              {ticket.assignee ? (
                <span className="ticket-detail__assignee">
                  <span className="ticket-detail__mini-avatar">
                    {(ticket.assignee.displayName || ticket.assignee.username)[0]?.toUpperCase()}
                  </span>
                  {ticket.assignee.displayName || ticket.assignee.username}
                </span>
              ) : (
                <span className="ticket-detail__unassigned">Unassigned</span>
              )}
            </div>
          </div>

          {/* 期限 */}
          <div className="ticket-detail__field">
            <label className="ticket-detail__field-label">Due Date</label>
            <div className="ticket-detail__field-value">
              <span
                className={
                  ticket.dueDate && new Date(ticket.dueDate) < new Date()
                    ? 'ticket-detail__overdue'
                    : ''
                }
              >
                {formatDate(ticket.dueDate)}
              </span>
            </div>
          </div>

          {/* カテゴリー */}
          {ticket.category && (
            <div className="ticket-detail__field">
              <label className="ticket-detail__field-label">Category</label>
              <div className="ticket-detail__field-value">
                <span
                  className="ticket-detail__category-badge"
                  style={{ '--cat-color': ticket.category.color } as React.CSSProperties}
                >
                  {ticket.category.name}
                </span>
              </div>
            </div>
          )}

          {/* マイルストーン */}
          {ticket.milestone && (
            <div className="ticket-detail__field">
              <label className="ticket-detail__field-label">Milestone</label>
              <div className="ticket-detail__field-value">
                {ticket.milestone.name}
                {ticket.milestone.dueDate && (
                  <span className="ticket-detail__milestone-due">
                    {' '}— {formatDate(ticket.milestone.dueDate)}
                  </span>
                )}
              </div>
            </div>
          )}

          {/* メタ情報 */}
          <div className="ticket-detail__meta">
            <div className="ticket-detail__meta-row">
              <span>Created</span>
              <span>{formatDate(ticket.createdAt)}</span>
            </div>
            <div className="ticket-detail__meta-row">
              <span>Updated</span>
              <span>{timeAgo(ticket.updatedAt)}</span>
            </div>
            {ticket.author && (
              <div className="ticket-detail__meta-row">
                <span>Author</span>
                <span>{ticket.author.displayName || ticket.author.username}</span>
              </div>
            )}
          </div>
        </aside>
      </div>
    </div>
  );
}
