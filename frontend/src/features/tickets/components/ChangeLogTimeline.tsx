/**
 * ChangeLogTimeline — チケット変更履歴タイムライン
 *
 * Jira/Linear 風の変更履歴表示。
 * フィールド名ごとにアイコンを変え、変更前後をインラインで表示する。
 * 作成イベントは API ログが空でも createdAt / createdByName があれば先頭に出す。
 */
import { useQuery } from '@tanstack/react-query';
import { apiClient } from '../../../shared/api/client';
import './ChangeLogTimeline.css';
import { useTranslation } from 'react-i18next';

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
  ticketId: string;
  createdAt?: string;
  createdByName?: string | null;
}

const FIELD_ICONS: Record<string, string> = {
  Created: '🎉',
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

function formatRelativeTime(dateStr: string, locale: string): string {
  const now = new Date();
  const date = new Date(dateStr);
  const diffMs = now.getTime() - date.getTime();
  const diffMin = Math.floor(diffMs / 60000);
  const diffHour = Math.floor(diffMs / 3600000);
  const diffDay = Math.floor(diffMs / 86400000);
  const isJa = locale.startsWith('ja');

  if (diffMin < 1) return isJa ? 'たった今' : 'just now';
  if (diffMin < 60) return isJa ? `${diffMin}分前` : `${diffMin}m ago`;
  if (diffHour < 24) return isJa ? `${diffHour}時間前` : `${diffHour}h ago`;
  if (diffDay < 7) return isJa ? `${diffDay}日前` : `${diffDay}d ago`;
  return date.toLocaleDateString(isJa ? 'ja-JP' : undefined, { month: 'short', day: 'numeric' });
}

function truncateValue(value: string, maxLen = 40, emptyLabel = '—'): string {
  if (!value || value === '') return emptyLabel;
  return value.length > maxLen ? value.slice(0, maxLen) + '…' : value;
}

export default function ChangeLogTimeline({ ticketId, createdAt, createdByName }: ChangeLogTimelineProps) {
  const { t, i18n } = useTranslation();
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

  const hasCreate = !!(createdAt && createdByName);
  const hasLogs = !!(logs && logs.length > 0);

  if (!hasCreate && !hasLogs) {
    return (
      <div className="changelog-timeline changelog-timeline--empty">
        <span className="changelog-timeline__empty-icon">📋</span>
        <span>{t('common.noChangeLog')}</span>
      </div>
    );
  }

  return (
    <div className="changelog-timeline">
      <h4 className="changelog-timeline__title">{t('changelog.title')}</h4>
      <div className="changelog-timeline__list">
        {hasCreate && (
          <div className="changelog-timeline__item" data-testid="changelog-created-event">
            <div className="changelog-timeline__icon">
              {FIELD_ICONS['Created'] || '🎉'}
            </div>
            <div className="changelog-timeline__content">
              <div className="changelog-timeline__header">
                <span className="changelog-timeline__user">{createdByName}</span>
                <span className="changelog-timeline__field">
                  {' '}
                  {t('changelog.createdIssue')}
                </span>
                <span className="changelog-timeline__time">
                  {formatRelativeTime(createdAt!, i18n.language)}
                </span>
              </div>
            </div>
          </div>
        )}
        {(logs ?? []).map((log) => (
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
                  {' '}
                  {t('changelog.changedField', { field: log.fieldName })}
                </span>
                <span className="changelog-timeline__time">
                  {formatRelativeTime(log.changedAt, i18n.language)}
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
