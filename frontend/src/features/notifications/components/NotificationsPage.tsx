/**
 * NotificationsPage.tsx — 通知一覧ページ
 *
 * 全通知をフィルタ付きで表示。
 * Linear思想1: コンテキスト維持（ページ遷移なしでチケットに飛べる）。
 */

import { useState, useCallback, useEffect } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { apiClient } from '@/shared/api/client';
import { useProject } from '@/shared/hooks/useProject';
import { useOptimisticMutation } from '@/shared/hooks/useOptimisticMutation';
import { buildTicketDetailPath } from '@/features/tickets/utils/ticketNavigation';
import { hasCommandModifier } from '@/shared/hooks/keyboardGuards';
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
  projectKey?: string | null;
  teamSlug?: string | null;
}

type FilterType = 'all' | 'unread' | 'read';

const CATEGORY_ICONS: Record<string, string> = {
  assigned: '👤',
  commented: '💬',
  status_changed: '🔄',
  due_soon: '⏰',
  overdue: '🔥',
  mentioned: '📢',
  review_requested: '👁️',
  wiki_updated: '📄',
  cycle_auto_completed: '📅',
  updated: '📝',
};

function categoryLabel(category: string, t: (key: string, options?: { defaultValue?: string }) => string): string {
  if (category === 'cycle_auto_completed') {
    return t('notifications.cycleAutoCompleted', { defaultValue: 'Cycle auto-completed' });
  }
  const labels: Record<string, string> = {
    assigned: 'Assigned',
    commented: 'Comment',
    status_changed: 'Status',
    due_soon: 'Due Soon',
    overdue: 'Overdue',
    mentioned: 'Mention',
    review_requested: 'Review Requested',
    wiki_updated: 'Wiki',
    updated: 'Updated',
  };
  return labels[category] ?? category;
}

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

function isInputFocused(): boolean {
  const active = document.activeElement;
  if (!active) return false;
  const tag = active.tagName.toLowerCase();
  return (
    tag === 'input' ||
    tag === 'textarea' ||
    tag === 'select' ||
    (active as HTMLElement).isContentEditable
  );
}

export function NotificationsPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { projectKey: routeProjectKey } = useProject();
  const [filter, setFilter] = useState<FilterType>('all');
  const [selectedIndex, setSelectedIndex] = useState(-1);

  // 通知一覧
  const { data } = useQuery<{ results: Notification[] }>({
    queryKey: ['notifications'],
    queryFn: async () => {
      const res = await apiClient.get<Notification[] | { results: Notification[] }>('/notifications/');
      const body = res.data;
      return { results: Array.isArray(body) ? body : (body.results ?? []) };
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

  // キーボードナビゲーション
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (hasCommandModifier(e) || isInputFocused()) return;

      switch (e.key) {
        case 'j':
        case 'ArrowDown': {
          e.preventDefault();
          const next = Math.min(selectedIndex + 1, filtered.length - 1);
          setSelectedIndex(next);
          break;
        }
        case 'k':
        case 'ArrowUp': {
          e.preventDefault();
          const prev = Math.max(selectedIndex - 1, 0);
          setSelectedIndex(prev);
          break;
        }
        case 'Enter': {
          if (selectedIndex >= 0 && filtered[selectedIndex]) {
            e.preventDefault();
            handleNotificationClick(filtered[selectedIndex]);
          }
          break;
        }
        case 'Escape': {
          e.preventDefault();
          setSelectedIndex(-1);
          break;
        }
      }
    },
    [selectedIndex, filtered],
  );

  useEffect(() => {
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [handleKeyDown]);

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

  // 通知クリック時のナビゲーション
  function handleNotificationClick(notification: Notification) {
    if (!notification.isRead) {
      readMutation.mutate(notification.id);
    }

    if (notification.ticketKey) {
      // teamSlugがある通知はチーム側の詳細に留める(プロジェクト側への意図しない遷移を防ぐ)
      const projectKey = notification.teamSlug ? null : notification.projectKey;
      navigate(buildTicketDetailPath(projectKey, notification.ticketKey, undefined, notification.teamSlug));
    } else if (notification.category === 'cycle_auto_completed') {
      const projectKey = notification.projectKey ?? routeProjectKey;
      if (projectKey) {
        navigate(`/project/${projectKey}/cycles`);
      }
    }
  }

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
          filtered.map((n, index) => {
            const isHighPriority = n.category === 'review_requested' || n.category === 'mentioned';
            return (
            <div
              key={n.id}
              className={`notifications-page__item ${!n.isRead ? 'notifications-page__item--unread' : ''} ${index === selectedIndex ? 'notifications-page__item--selected' : ''} ${isHighPriority ? 'notifications-page__item--high-priority' : ''}`}
              onClick={() => handleNotificationClick(n)}
              data-testid={`notif-${n.id}`}
            >
              <div className="notifications-page__item-icon">
                {CATEGORY_ICONS[n.category] ?? '🔔'}
              </div>
              <div className="notifications-page__item-content">
                <div className="notifications-page__item-header">
                  <span className="notifications-page__item-category">
                    {categoryLabel(n.category, t)}
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
                  <span className="notifications-page__item-link">
                    {n.ticketKey}
                  </span>
                )}
              </div>
              {!n.isRead && <span className="notifications-page__unread-dot" />}
            </div>
          );
          })
        )}
      </div>
    </div>
  );
}
