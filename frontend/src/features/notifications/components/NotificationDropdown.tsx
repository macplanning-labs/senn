/**
 * NotificationDropdown.tsx — 通知ドロップダウン（ヘッダー内ベル）
 *
 * 未読バッジ付きベルアイコン + ドロップダウン通知一覧。
 * ポーリングで未読数を自動更新。
 */

import { useState, useRef, useEffect } from 'react';
import { createPortal } from 'react-dom';
import { useQuery } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { apiClient } from '@/shared/api/client';
import { useProject } from '@/shared/hooks/useProject';
import { useOptimisticMutation } from '@/shared/hooks/useOptimisticMutation';
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
  projectKey?: string | null;
}

const categoryIcons: Record<string, string> = {
  assigned: '👤',
  commented: '💬',
  status_changed: '🔄',
  due_soon: '⏰',
  overdue: '🔥',
  mentioned: '📢',
  wiki_updated: '📄',
  cycle_auto_completed: '📅',
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
  const [isOpen, setIsOpen] = useState(false);
  const bellRef = useRef<HTMLButtonElement>(null);
  const dropdownRef = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState<{ top: number; right: number } | null>(null);
  const navigate = useNavigate();
  const { projectKey: routeProjectKey } = useProject();

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

  // 楽観的既読マーク — クリックの瞬間に未読ドットが消える（0ms）
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
    errorMessage: '既読マークに失敗しました。',
  });

  // 楽観的一括既読 — ボタン押下で全通知が即既読化 + バッジが0に
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
    errorMessage: '一括既読に失敗しました。',
  });

  // ドロップダウンを開いた時、ベルボタン基準で位置を計算する(document.bodyへポータルするため)
  useEffect(() => {
    if (!isOpen || !bellRef.current) return;
    const rect = bellRef.current.getBoundingClientRect();
    setPosition({
      top: rect.bottom + 8,
      right: window.innerWidth - rect.right,
    });
  }, [isOpen]);

  // 外側クリックで閉じる(ドロップダウンはportalでdocument.body直下にあるためdropdownRefも確認)
  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      const target = e.target as Node;
      if (
        bellRef.current && !bellRef.current.contains(target) &&
        (!dropdownRef.current || !dropdownRef.current.contains(target))
      ) {
        setIsOpen(false);
      }
    }
    if (isOpen) {
      document.addEventListener('mousedown', handleClickOutside);
      return () => document.removeEventListener('mousedown', handleClickOutside);
    }
  }, [isOpen]);

  // 通知クリック時のナビゲーション
  function handleNotificationClick(notification: Notification) {
    if (!notification.isRead) {
      readMutation.mutate(notification.id);
    }

    if (notification.ticketKey) {
      navigate(`/tickets`);
    } else if (notification.category === 'cycle_auto_completed') {
      const projectKey = notification.projectKey ?? routeProjectKey;
      if (projectKey) {
        navigate(`/p/${projectKey}/cycles`);
      }
    }
  }

  const unreadCount = unreadData?.count ?? 0;
  const notifications = notificationsData?.results ?? [];

  return (
    <div className="notif" data-testid="notification-dropdown">
      {/* ベルアイコン */}
      <button
        ref={bellRef}
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

      {/* ドロップダウン(document.body直下へportal。ヘッダーのスタッキングコンテキストに閉じ込められてZ-orderが壊れるのを防ぐ) */}
      {isOpen && position && createPortal(
        <div
          ref={dropdownRef}
          className="notif__dropdown"
          style={{ top: position.top, right: position.right }}
          data-testid="notif-panel"
        >
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
                  onClick={() => handleNotificationClick(n)}
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
        </div>,
        document.body,
      )}
    </div>
  );
}
