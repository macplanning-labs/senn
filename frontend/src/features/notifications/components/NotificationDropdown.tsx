/**
 * NotificationDropdown.tsx — 通知ドロップダウン（ヘッダー内ベル）
 *
 * 未読バッジ付きベルアイコン + ドロップダウン通知一覧。
 * ポーリングで未読数を自動更新。
 */

import { useState, useRef, useEffect } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import './NotificationDropdown.css';

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

const categoryIcons: Record<string, string> = {
  assigned: '👤',
  commented: '💬',
  status_changed: '🔄',
  due_soon: '⏰',
  overdue: '🔥',
  mentioned: '📢',
  wiki_updated: '📄',
};

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

export function NotificationDropdown() {
  const queryClient = useQueryClient();
  const [isOpen, setIsOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);

  // 未読数（30秒ポーリング）
  const { data: unreadData } = useQuery<{ count: number }>({
    queryKey: ['unread-count'],
    queryFn: async () => {
      const res = await apiClient.get<{ count: number }>('/notifications/unread_count/');
      return res.data;
    },
    refetchInterval: 30000,
  });

  // 通知一覧（ドロップダウンを開いた時のみ）
  const { data: notificationsData } = useQuery<{ results: Notification[] }>({
    queryKey: ['notifications'],
    queryFn: async () => {
      const res = await apiClient.get<{ results: Notification[] }>('/notifications/');
      return res.data;
    },
    enabled: isOpen,
  });

  // 既読マーク
  const readMutation = useMutation({
    mutationFn: async (id: number) => {
      await apiClient.post(`/notifications/${id}/read/`);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['notifications'] });
      void queryClient.invalidateQueries({ queryKey: ['unread-count'] });
    },
  });

  // 一括既読
  const readAllMutation = useMutation({
    mutationFn: async () => {
      await apiClient.post('/notifications/read_all/');
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['notifications'] });
      void queryClient.invalidateQueries({ queryKey: ['unread-count'] });
    },
  });

  // 外側クリックで閉じる
  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      if (dropdownRef.current && !dropdownRef.current.contains(e.target as Node)) {
        setIsOpen(false);
      }
    }
    if (isOpen) {
      document.addEventListener('mousedown', handleClickOutside);
      return () => document.removeEventListener('mousedown', handleClickOutside);
    }
  }, [isOpen]);

  const unreadCount = unreadData?.count ?? 0;
  const notifications = notificationsData?.results ?? [];

  return (
    <div className="notif" ref={dropdownRef} data-testid="notification-dropdown">
      {/* ベルアイコン */}
      <button
        className="notif__bell"
        onClick={() => setIsOpen(!isOpen)}
        data-testid="notif-bell"
      >
        🔔
        {unreadCount > 0 && (
          <span className="notif__badge" data-testid="notif-badge">
            {unreadCount > 99 ? '99+' : unreadCount}
          </span>
        )}
      </button>

      {/* ドロップダウン */}
      {isOpen && (
        <div className="notif__dropdown" data-testid="notif-panel">
          <div className="notif__header">
            <span className="notif__header-title">Notifications</span>
            {unreadCount > 0 && (
              <button
                className="notif__mark-all"
                onClick={() => readAllMutation.mutate()}
                data-testid="notif-mark-all"
              >
                Mark all read
              </button>
            )}
          </div>

          <div className="notif__list">
            {!notifications.length ? (
              <div className="notif__empty">No notifications</div>
            ) : (
              notifications.slice(0, 20).map((n) => (
                <div
                  key={n.id}
                  className={`notif__item ${!n.isRead ? 'notif__item--unread' : ''}`}
                  onClick={() => { if (!n.isRead) readMutation.mutate(n.id); }}
                  data-testid={`notif-item-${n.id}`}
                >
                  <span className="notif__icon">
                    {categoryIcons[n.category] ?? '🔔'}
                  </span>
                  <div className="notif__content">
                    <span className="notif__title">{n.title}</span>
                    {n.message && (
                      <span className="notif__message">{n.message}</span>
                    )}
                    <span className="notif__time">{timeAgo(n.createdAt)}</span>
                  </div>
                  {!n.isRead && <span className="notif__unread-dot" />}
                </div>
              ))
            )}
          </div>
        </div>
      )}
    </div>
  );
}
