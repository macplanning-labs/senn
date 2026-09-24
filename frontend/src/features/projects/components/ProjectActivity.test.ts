import { describe, it, expect, vi, beforeEach } from 'vitest';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';

vi.mock('react-i18next', () => ({
  // 文言そのものではなく「どのキーがどの値で呼ばれたか」を検証できるよう、キー＋パラメータを返す
  useTranslation: () => ({
    t: (key: string, params?: Record<string, unknown>) =>
      params ? `${key}${JSON.stringify(params)}` : key,
  }),
}));
vi.mock('./ProjectActivity.css', () => ({}));
vi.mock('@/shared/hooks/useProject', () => ({
  useProject: () => ({ currentProject: { id: 10 }, isLoading: false }),
}));

type Ev = {
  id: number; eventType: string; payload: Record<string, unknown>;
  actorId: number | null; actorName: string | null; createdAt: string;
};
let mockQuery: {
  data?: { pages: { results: Ev[]; next: number | null }[] };
  isLoading: boolean; isError: boolean; hasNextPage: boolean;
  fetchNextPage: () => void; isFetchingNextPage: boolean;
};
vi.mock('../hooks/useProjectActivity', () => ({
  useProjectActivity: () => mockQuery,
}));

import { ProjectActivity } from './ProjectActivity';

const base = { isLoading: false, isError: false, hasNextPage: false, fetchNextPage: vi.fn(), isFetchingNextPage: false };
const ev = (id: number, eventType: string, payload: Record<string, unknown>, actorName: string | null, at: string): Ev => ({
  id, eventType, payload, actorId: actorName ? 1 : null, actorName, createdAt: at,
});
const render = () => renderToStaticMarkup(createElement(ProjectActivity));

describe('ProjectActivity', () => {
  beforeEach(() => {
    mockQuery = { ...base, data: { pages: [{ results: [], next: null }] } };
  });

  it('履歴が無いときは空状態を表示する', () => {
    expect(render()).toContain('project-activity-empty');
  });

  it('イベントを文言キー付きで表示し、実行者が不明ならシステムと表示する', () => {
    mockQuery = {
      ...base,
      data: {
        pages: [{
          results: [
            ev(2, 'ticket_completed', { ticket_key: 'X-1', title: '完了した' }, null, '2026-09-19T05:00:00'),
            ev(1, 'project_updated', { changes: [{ field: 'status', from: 'planned', to: 'in_progress' }] }, '田中', '2026-09-19T04:00:00'),
          ],
          next: null,
        }],
      },
    };
    const html = render();
    expect(html).toContain('projectActivity.ticketCompleted');
    expect(html).toContain('X-1');
    expect(html).toContain('projectActivity.changeStatus');
    expect(html).toContain('田中');
    expect(html).toContain('projectActivity.system');
  });

  it('日付が違うイベントは別のグループ(見出し)になる', () => {
    mockQuery = {
      ...base,
      data: {
        pages: [{
          results: [
            ev(3, 'team_added', { team_name: 'A' }, '田中', '2026-09-20T09:00:00'),
            ev(2, 'team_added', { team_name: 'B' }, '田中', '2026-09-19T09:00:00'),
            ev(1, 'team_added', { team_name: 'C' }, '田中', '2026-09-19T08:00:00'),
          ],
          next: null,
        }],
      },
    };
    expect((render().match(/project-activity__day-title/g) ?? []).length).toBe(2);
  });

  it('進捗投稿は、健全性の内部値ではなく表示ラベルのキーで表示する', () => {
    mockQuery = {
      ...base,
      data: { pages: [{ results: [ev(1, 'update_posted', { update_id: 3, health: 'at_risk' }, '田中', '2026-09-19T04:00:00')], next: null }] },
    };
    const html = render();
    expect(html).toContain('projectActivity.updatePosted');
    expect(html).toContain('projectUpdates.health.at_risk');
  });

  it('次のページがあるときだけ「もっと見る」を表示する', () => {
    const results = [ev(1, 'team_added', { team_name: 'A' }, '田中', '2026-09-19T04:00:00')];
    mockQuery = { ...base, hasNextPage: true, data: { pages: [{ results, next: 1 }] } };
    expect(render()).toContain('project-activity-more');
    mockQuery = { ...base, hasNextPage: false, data: { pages: [{ results, next: null }] } };
    expect(render()).not.toContain('project-activity-more');
  });

  it('読み込み失敗時はエラーを表示する', () => {
    mockQuery = { ...base, isError: true, data: undefined };
    expect(render()).toContain('projectActivity.loadFailed');
  });
});
