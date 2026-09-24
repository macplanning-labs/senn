/**
 * ProjectActivity.tsx — プロジェクトの変更履歴(Activity)タブ
 *
 * 新しい順に日付ごとにまとめて表示し、「もっと見る」で過去へさかのぼる。
 */

import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { useProject } from '@/shared/hooks/useProject';
import { useProjectActivity, type ProjectActivityEvent } from '../hooks/useProjectActivity';
import { activityCategory, dayKey, describeActivity } from '../utils/activityMessage';
import './ProjectActivity.css';

const ICONS: Record<ReturnType<typeof activityCategory>, string> = {
  project: '📁',
  team: '👥',
  milestone: '🚩',
  ticket: '🎫',
  update: '📣',
};

export function ProjectActivity() {
  const { t } = useTranslation();
  const { currentProject, isLoading: projectLoading } = useProject();
  const { data, isLoading, isError, hasNextPage, fetchNextPage, isFetchingNextPage } =
    useProjectActivity(currentProject?.id);

  const groups = useMemo(() => {
    const events = data?.pages.flatMap((p) => p.results) ?? [];
    const byDay = new Map<string, ProjectActivityEvent[]>();
    for (const e of events) {
      const k = dayKey(e.createdAt);
      byDay.set(k, [...(byDay.get(k) ?? []), e]);
    }
    return [...byDay.entries()];
  }, [data]);

  if (projectLoading || isLoading) {
    return <div className="project-activity__state">{t('projectActivity.loading')}</div>;
  }
  if (isError) {
    return <div className="project-activity__state project-activity__state--error">{t('projectActivity.loadFailed')}</div>;
  }
  if (groups.length === 0) {
    return (
      <div className="project-activity__state" data-testid="project-activity-empty">
        {t('projectActivity.empty')}
      </div>
    );
  }

  return (
    <div className="project-activity" data-testid="project-activity">
      {groups.map(([day, events]) => (
        <section key={day} className="project-activity__day">
          <h2 className="project-activity__day-title">{new Date(`${day}T00:00:00`).toLocaleDateString()}</h2>
          <ul className="project-activity__list">
            {events.map((e) => {
              const lines = describeActivity(e);
              return (
                <li key={e.id} className="project-activity__item" data-testid={`project-activity-${e.id}`}>
                  <span className="project-activity__icon" aria-hidden="true">
                    {ICONS[activityCategory(e.eventType)]}
                  </span>
                  <div className="project-activity__content">
                    {lines.map((l, i) => (
                      <div key={i} className="project-activity__message">
                        {t(`projectActivity.${l.key}`, {
                          ...l.params,
                          // 進捗投稿の健全性は、内部値ではなく表示ラベルにする
                          ...(l.key === 'updatePosted' && typeof l.params?.health === 'string'
                            ? { health: t(`projectUpdates.health.${l.params.health}`) }
                            : {}),
                        })}
                      </div>
                    ))}
                    <div className="project-activity__meta">
                      {e.actorName ?? t('projectActivity.system')} ·{' '}
                      {new Date(e.createdAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                    </div>
                  </div>
                </li>
              );
            })}
          </ul>
        </section>
      ))}

      {hasNextPage && (
        <button
          type="button"
          className="project-activity__more"
          onClick={() => void fetchNextPage()}
          disabled={isFetchingNextPage}
          data-testid="project-activity-more"
        >
          {isFetchingNextPage ? t('projectActivity.loading') : t('projectActivity.loadMore')}
        </button>
      )}
    </div>
  );
}
