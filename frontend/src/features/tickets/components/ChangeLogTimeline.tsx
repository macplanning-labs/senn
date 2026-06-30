/**
 * ChangeLogTimeline — チケット変更履歴タイムライン
 *
 * Jira/Linear 風の変更履歴表示。
 * フィールド名ごとにアイコンを変え、変更前後をインラインで表示する。
 */
import { useQuery } from '@tanstack/react-query';
import { apiClient } from '../../../shared/api/client';
import './ChangeLogTimeline.css';

interface ChangeLog {
  id: number;
  fieldName: string;
  oldValue: string;
  newValue: string;
  changedBy: {
    id: number;
    username: string;
    displayName: string;
  } | null;
  changedAt: string;
}

interface ChangeLogTimelineProps {
  ticketId: number;
}

const FIELD_ICONS: Record<string, string> = {
  Status: '🔄',
  Priority: '🔺',
  Title: '✏️',
  Assignees: '👤',
  Description: '📝',
  'Ticket Type': '🏷️',
  Category: '📂',
  Milestone: '🎯',
  'Start Date': '📅',
  'Due Date': '⏰',
};

function formatRelativeTime(dateStr: string): string {
  const now = new Date();
  const date = new Date(dateStr);
  const diffMs = now.getTime() - date.getTime();
  const diffMin = Math.floor(diffMs / 60000);
  const diffHour = Math.floor(diffMs / 3600000);
  const diffDay = Math.floor(diffMs / 86400000);

  if (diffMin < 1) return 'たった今';
  if (diffMin < 60) return `${diffMin}分前`;
  if (diffHour < 24) return `${diffHour}時間前`;
  if (diffDay < 7) return `${diffDay}日前`;
  return date.toLocaleDateString('ja-JP', { month: 'short', day: 'numeric' });
}

function truncateValue(value: string, maxLen = 40): string {
  if (!value || value === '') return '(なし)';
  return value.length > maxLen ? value.slice(0, maxLen) + '…' : value;
}

export default function ChangeLogTimeline({ ticketId }: ChangeLogTimelineProps) {
  const { data: logs, isLoading } = useQuery<ChangeLog[]>({
    queryKey: ['tickets', ticketId, 'change-logs'],
    queryFn: async () => {
      const res = await apiClient.get(`/tickets/${ticketId}/change-logs/`);
      return res.data;
    },
    enabled: !!ticketId,
    staleTime: 30_000,
  });

  if (isLoading) {
    return (
      <div className="changelog-timeline changelog-timeline--loading">
        <div className="changelog-timeline__spinner" />
      </div>
    );
  }

  if (!logs || logs.length === 0) {
    return (
      <div className="changelog-timeline changelog-timeline--empty">
        <span className="changelog-timeline__empty-icon">📋</span>
        <span>変更履歴はありません</span>
      </div>
    );
  }

  return (
    <div className="changelog-timeline">
      <h4 className="changelog-timeline__title">変更履歴</h4>
      <div className="changelog-timeline__list">
        {logs.map((log) => (
          <div key={log.id} className="changelog-timeline__item">
            <div className="changelog-timeline__icon">
              {FIELD_ICONS[log.fieldName] || '📋'}
            </div>
            <div className="changelog-timeline__content">
              <div className="changelog-timeline__header">
                <span className="changelog-timeline__user">
                  {log.changedBy?.displayName || log.changedBy?.username || 'System'}
                </span>
                <span className="changelog-timeline__field">
                  が <strong>{log.fieldName}</strong> を変更
                </span>
                <span className="changelog-timeline__time">
                  {formatRelativeTime(log.changedAt)}
                </span>
              </div>
              <div className="changelog-timeline__diff">
                <span className="changelog-timeline__old">
                  {truncateValue(log.oldValue)}
                </span>
                <span className="changelog-timeline__arrow">→</span>
                <span className="changelog-timeline__new">
                  {truncateValue(log.newValue)}
                </span>
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
