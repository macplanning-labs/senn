/**
 * GitActivity.tsx — チケットのGitアクティビティ表示
 *
 * チケット詳細パネルに表示するGitイベント一覧。
 * コミットSHA短縮表示 + GitHubリンク、PRステータスバッジ等。
 */

import { useQuery } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import type { GitEvent } from '@/shared/api/types';
import './GitActivity.css';

interface GitActivityProps {
  ticketKey: string;
}

function formatTimeAgo(dateStr: string): string {
  const now = new Date();
  const d = new Date(dateStr);
  const diff = Math.floor((now.getTime() - d.getTime()) / 1000);
  if (diff < 60) return `${diff}s ago`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}

const EVENT_ICONS: Record<string, string> = {
  commit: '📝',
  pull_request: '🔀',
  branch: '🌿',
};

const PR_STATE_STYLES: Record<string, { bg: string; color: string; label: string }> = {
  open: { bg: 'rgba(56, 139, 253, 0.15)', color: '#58a6ff', label: 'Open' },
  merged: { bg: 'rgba(163, 113, 247, 0.15)', color: '#a371f7', label: 'Merged' },
  closed: { bg: 'rgba(248, 81, 73, 0.15)', color: '#f85149', label: 'Closed' },
};

export function GitActivity({ ticketKey }: GitActivityProps) {
  const { data: events = [], isLoading } = useQuery<GitEvent[]>({
    queryKey: ['git-events', ticketKey],
    queryFn: async () => {
      const res = await apiClient.get(`/tickets/${ticketKey}/git-events/`);
      return res.data;
    },
    staleTime: 30_000,
  });

  if (isLoading) {
    return (
      <div className="git-activity__loading">
        Loading git activity...
      </div>
    );
  }

  if (events.length === 0) {
    return null; // Gitイベントがなければ何も表示しない
  }

  return (
    <div className="git-activity">
      <h4 className="git-activity__title">
        <span className="git-activity__icon">⚡</span>
        Git Activity
        <span className="git-activity__count">{events.length}</span>
      </h4>
      <div className="git-activity__list">
        {events.map((event) => (
          <div key={event.id} className="git-event">
            <span className="git-event__type-icon">
              {EVENT_ICONS[event.eventType] || '📎'}
            </span>

            <div className="git-event__content">
              <div className="git-event__header">
                {event.eventType === 'commit' && (
                  <a
                    href={event.url}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="git-event__sha"
                    title={event.sha}
                  >
                    {event.shaShort}
                  </a>
                )}
                {event.eventType === 'pull_request' && event.prNumber && (
                  <>
                    <a
                      href={event.url}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="git-event__pr-link"
                    >
                      #{event.prNumber}
                    </a>
                    {event.prState && PR_STATE_STYLES[event.prState] && (
                      <span
                        className="git-event__pr-badge"
                        style={{
                          background: PR_STATE_STYLES[event.prState]!.bg,
                          color: PR_STATE_STYLES[event.prState]!.color,
                        }}
                      >
                        {PR_STATE_STYLES[event.prState]!.label}
                      </span>
                    )}
                  </>
                )}
                <span className="git-event__time">
                  {formatTimeAgo(event.createdAt)}
                </span>
              </div>

              <div className="git-event__title" title={event.title}>
                {event.url ? (
                  <a
                    href={event.url}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="git-event__title-link"
                  >
                    {event.title}
                  </a>
                ) : (
                  event.title
                )}
              </div>

              <div className="git-event__meta">
                {event.authorName && (
                  <span className="git-event__author">
                    {event.authorAvatarUrl && (
                      <img
                        src={event.authorAvatarUrl}
                        alt={event.authorName}
                        className="git-event__avatar"
                      />
                    )}
                    {event.authorName}
                  </span>
                )}
                {event.branch && (
                  <span className="git-event__branch">
                    🌿 {event.branch}
                  </span>
                )}
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
