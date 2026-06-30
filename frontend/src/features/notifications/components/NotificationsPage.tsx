/**
 * NotificationsPage.tsx — 通知一覧ページ
 *
 * 全通知をフィルタ付きで表示。
 * Linear思想1: コンテキスト維持（ページ遷移なしでチケットに飛べる）。
 */

import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { apiClient } from '@/shared/api/client';
import { useOptimisticMutation } from '@/shared/hooks/useOptimisticMutation';
import './NotificationsPage.css';

interface Notification {
  id: number;
  category: string;
  title: string;
  message: string;
  ticketKey: string | null;
  wikiTitle: string | null;
  isRead: boolean;
  createdAt: string;
}

type FilterType = 'all' | 'unread' | 'read';

const CATEGORY_ICONS: Record<string, string> = {
  assigned: '👤',
  commented: '💬',
  status_changed: '🔄',
  due_soon: '⏰',
  overdue: '🔥',
  mentioned: '📢',
  wiki_updated: '📄',
};

const CATEGORY_LABELS: Record<string, string> = {
  assigned: 'Assigned',
  commented: 'Comment',
  status_changed: 'Status',
  due_soon: 'Due Soon',
  overdue: 'Overdue',
  mentioned: 'Mention',
  wiki_updated: 'Wiki',
};

function formatDate(dateStr: string): string {
  const date = new Date(dateStr);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffMins = Math.floor(diffMs / 60000);
  if (diffMins < 1) return 'just now';
  if (diffMins < 60) return `${diffMins}m ago`;
  const diffHours = Math.floor(diffMins / 60);
  if (diffHours < 24) return `${diffHours}h ago`;
  const diffDays = Math.floor(diffHours / 24);
  if (diffDays < 7) return `${diffDays}d ago`;
  return date.toLocaleDateString('ja-JP', {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

export function NotificationsPage() {
  const { t } = useTranslation();
  const [filter, setFilter] = useState<FilterType>('all');

  // 通知一覧
  const { data } = useQuery<{ results: Notification[] }>({
    queryKey: ['notifications'],
    queryFn: async () => {
      const res = await apiClient.get<{ results: Notification[] }>('/notifications/');
      return res.data;
    },
  });

  const notifications = data?.results ?? [];

  // フィルタ適用
  const filtered = notifications.filter((n) => {
    if (filter === 'unread') return !n.isRead;
    if (filter === 'read') return n.isRead;
    return true;
  });

  const unreadCount = notifications.filter((n) => !n.isRead).length;

  // 楽観的既読マーク
  const readMutation = useOptimisticMutation<void, number>({
    mutationFn: async (id) => {
      await apiClient.post(`/notifications/${id}/read/`);
    },
    queryKey: ['notifications'],
    updater: (currentData, id) => {
      const data = currentData as { results: Notification[] } | undefined;
      if (!data?.results) return currentData;
      return {
        ...data,
        results: data.results.map((n) =>
          n.id === id ? { ...n, isRead: true } : n,
        ),
      };
    },
    invalidateKeys: [['unread-count']],
  });

  // 楽観的一括既読
  const readAllMutation = useOptimisticMutation<void, void>({
    mutationFn: async () => {
      await apiClient.post('/notifications/read_all/');
    },
    queryKey: ['notifications'],
    updater: (currentData) => {
      const data = currentData as { results: Notification[] } | undefined;
      if (!data?.results) return currentData;
      return {
        ...data,
        results: data.results.map((n) => ({ ...n, isRead: true })),
      };
    },
    invalidateKeys: [['unread-count']],
    successMessage: t('notifications.markedAllRead', { defaultValue: 'All notifications marked as read' }),
  });

  return (
    <div className="notifications-page" data-testid="notifications-page">
      {/* ヘッダー */}
      <div className="notifications-page__header">
        <div className="notifications-page__title-row">
          <h1 className="notifications-page__title">
            {t('notifications.title', { defaultValue: 'Notifications' })}
          </h1>
          {unreadCount > 0 && (
            <span className="notifications-page__badge">{unreadCount}</span>
          )}
        </div>
        <div className="notifications-page__actions">
          {unreadCount > 0 && (
            <button
              className="notifications-page__mark-all"
              onClick={() => readAllMutation.mutate()}
              data-testid="mark-all-read"
            >
              ✓ Mark all read
            </button>
          )}
        </div>
      </div>

      {/* フィルタ */}
      <div className="notifications-page__filters">
        {(['all', 'unread', 'read'] as FilterType[]).map((f) => (
          <button
            key={f}
            className={`notifications-page__filter ${filter === f ? 'notifications-page__filter--active' : ''}`}
            onClick={() => setFilter(f)}
          >
            {f === 'all' ? 'All' : f === 'unread' ? `Unread (${unreadCount})` : 'Read'}
          </button>
        ))}
      </div>

      {/* 通知リスト */}
      <div className="notifications-page__list">
        {filtered.length === 0 ? (
          <div className="notifications-page__empty">
            <span className="notifications-page__empty-icon">🔔</span>
            <p>{filter === 'unread' ? 'No unread notifications' : 'No notifications yet'}</p>
          </div>
        ) : (
          filtered.map((n) => (
            <div
              key={n.id}
              className={`notifications-page__item ${!n.isRead ? 'notifications-page__item--unread' : ''}`}
              onClick={() => { if (!n.isRead) readMutation.mutate(n.id); }}
              data-testid={`notif-${n.id}`}
            >
              <div className="notifications-page__item-icon">
                {CATEGORY_ICONS[n.category] ?? '🔔'}
              </div>
              <div className="notifications-page__item-content">
                <div className="notifications-page__item-header">
                  <span className="notifications-page__item-category">
                    {CATEGORY_LABELS[n.category] ?? n.category}
                  </span>
                  <span className="notifications-page__item-time">
                    {formatDate(n.createdAt)}
                  </span>
                </div>
                <span className="notifications-page__item-title">{n.title}</span>
                {n.message && (
                  <span className="notifications-page__item-message">{n.message}</span>
                )}
                {n.ticketKey && (
                  <Link
                    to={`/tickets`}
                    className="notifications-page__item-link"
                    onClick={(e) => e.stopPropagation()}
                  >
                    {n.ticketKey}
                  </Link>
                )}
              </div>
              {!n.isRead && <span className="notifications-page__unread-dot" />}
            </div>
          ))
        )}
      </div>
    </div>
  );
}
