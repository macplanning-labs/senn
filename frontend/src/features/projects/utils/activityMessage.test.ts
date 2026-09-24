import { describe, it, expect } from 'vitest';
import { activityCategory, dayKey, describeActivity } from './activityMessage';
import type { ProjectActivityEvent } from '../hooks/useProjectActivity';

const ev = (eventType: string, payload: Record<string, unknown> = {}): ProjectActivityEvent => ({
  id: 1,
  eventType,
  payload,
  actorId: 1,
  actorName: 'A',
  createdAt: '2026-09-19T06:00:00Z',
});

describe('describeActivity', () => {
  it('プロジェクト更新は、変更項目ごとに1行になる', () => {
    const lines = describeActivity(
      ev('project_updated', {
        changes: [
          { field: 'status', from: 'planned', to: 'in_progress' },
          { field: 'description' },
          { field: 'priority', from: 'medium', to: 'high' },
        ],
      }),
    );
    expect(lines.map((l) => l.key)).toEqual(['changeStatus', 'changeDescription', 'changePriority']);
    expect(lines[0]?.params).toEqual({ from: 'planned', to: 'in_progress' });
  });

  it('ターゲット期日は、設定・解除・変更で別の文言になる', () => {
    const k = (from: unknown, to: unknown) =>
      describeActivity(ev('project_updated', { changes: [{ field: 'targetEndDate', from, to }] }))[0]?.key;
    expect(k(null, '2026-10-31')).toBe('changeTargetDateSet');
    expect(k('2026-10-31', null)).toBe('changeTargetDateCleared');
    expect(k('2026-10-31', '2026-11-30')).toBe('changeTargetDate');
  });

  it('チーム・マイルストーン・チケットのイベントを変換できる', () => {
    expect(describeActivity(ev('team_added', { team_name: 'T' }))[0]).toEqual({ key: 'teamAdded', params: { name: 'T' } });
    expect(describeActivity(ev('milestone_created', { name: 'M' }))[0]?.key).toBe('milestoneCreated');
    expect(describeActivity(ev('ticket_completed', { ticket_key: 'X-1', title: 't' }))[0]).toEqual({
      key: 'ticketCompleted',
      params: { ticket: 'X-1', title: 't' },
    });
  });

  it('マイルストーンの期日変更と名前変更を区別する', () => {
    const lines = describeActivity(
      ev('milestone_updated', {
        name: 'M',
        changes: [
          { field: 'name', from: 'M0', to: 'M' },
          { field: 'dueDate', from: '2026-10-01', to: '2026-11-01' },
        ],
      }),
    );
    expect(lines.map((l) => l.key)).toEqual(['milestoneRenamed', 'milestoneDueChanged']);
  });

  it('未知のイベント種別や壊れた payload でも例外にならない', () => {
    expect(describeActivity(ev('something_new'))[0]?.key).toBe('unknown');
    expect(describeActivity(ev('project_updated', { changes: 'bad' }))[0]?.key).toBe('changeOther');
    expect(describeActivity(ev('project_updated', {}))[0]?.key).toBe('changeOther');
  });

  it('親が変更されたイベント', () => {
    const lines = describeActivity(
      ev('parent_changed', {
        from: { id: 1, prefix: 'P1', name: 'Parent 1' },
        to: { id: 2, prefix: 'P2', name: 'Parent 2' },
      }),
    );
    expect(lines[0]?.key).toBe('parentChanged');
    expect(lines[0]?.params).toEqual({ from: 'P1 Parent 1', to: 'P2 Parent 2' });
  });

  it('親が削除されたイベント', () => {
    const lines = describeActivity(
      ev('parent_changed', {
        from: { id: 1, prefix: 'P1', name: 'Parent 1' },
        to: null,
      }),
    );
    expect(lines[0]?.key).toBe('parentChanged');
    expect(lines[0]?.params).toEqual({ from: 'P1 Parent 1', to: '' });
  });

  it('子が追加・削除されたイベント', () => {
    expect(describeActivity(ev('child_added', { project: { prefix: 'C1', name: 'Child 1' } }))[0]).toEqual({
      key: 'childAdded',
      params: { project: 'C1 Child 1' },
    });
    expect(describeActivity(ev('child_removed', { project: { prefix: 'C1', name: 'Child 1' } }))[0]).toEqual({
      key: 'childRemoved',
      params: { project: 'C1 Child 1' },
    });
  });

  it('関連が追加・削除されたイベント', () => {
    expect(describeActivity(ev('relation_added', { project: { prefix: 'R1', name: 'Related 1' } }))[0]).toEqual({
      key: 'relationAdded',
      params: { project: 'R1 Related 1' },
    });
    expect(describeActivity(ev('relation_removed', { project: { prefix: 'R1', name: 'Related 1' } }))[0]).toEqual({
      key: 'relationRemoved',
      params: { project: 'R1 Related 1' },
    });
  });

  it('ロードマップが追加・削除されたイベント', () => {
    expect(describeActivity(ev('roadmap_added', { roadmap: { name: 'Roadmap 2026' } }))[0]).toEqual({
      key: 'roadmapAdded',
      params: { name: 'Roadmap 2026' },
    });
    expect(describeActivity(ev('roadmap_removed', { roadmap: { name: 'Roadmap 2026' } }))[0]).toEqual({
      key: 'roadmapRemoved',
      params: { name: 'Roadmap 2026' },
    });
  });
});

describe('activityCategory / dayKey', () => {
  it('種別からカテゴリを決める', () => {
    expect(activityCategory('team_removed')).toBe('team');
    expect(activityCategory('milestone_deleted')).toBe('milestone');
    expect(activityCategory('ticket_added')).toBe('ticket');
    expect(activityCategory('update_posted')).toBe('update');
    expect(activityCategory('project_created')).toBe('project');
  });

  it('同じ日付のイベントは同じキーになる', () => {
    expect(dayKey('2026-09-19T01:00:00')).toBe(dayKey('2026-09-19T22:00:00'));
    expect(dayKey('2026-09-19T01:00:00')).not.toBe(dayKey('2026-09-20T01:00:00'));
  });
});
