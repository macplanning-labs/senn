import { describe, expect, it } from 'vitest';
import type { TaskDependency } from '@/shared/api/types';
import {
  buildCreateDependencyRequest,
  classifyDependencies,
  peerTicketKey,
} from './classifyDependencies';

function dep(partial: Partial<TaskDependency> & Pick<TaskDependency, 'id'>): TaskDependency {
  return {
    fromTask: 1,
    fromTaskKey: 'A-1',
    fromTaskTitle: 'A',
    toTask: 2,
    toTaskKey: 'B-1',
    toTaskTitle: 'B',
    dependencyType: 'blocks',
    createdAt: '2026-01-01T00:00:00Z',
    ...partial,
  };
}

describe('classifyDependencies', () => {
  it('blocks outgoing as blocking and incoming as blockedBy', () => {
    const deps = [
      dep({ id: 1, fromTask: 10, toTask: 20, dependencyType: 'blocks' }),
      dep({ id: 2, fromTask: 30, toTask: 10, dependencyType: 'blocks' }),
    ];
    const result = classifyDependencies(deps, 10);
    expect(result.blocking.map((d) => d.id)).toEqual([1]);
    expect(result.blockedBy.map((d) => d.id)).toEqual([2]);
    expect(result.related).toEqual([]);
  });

  it('relates_to on either side goes to related', () => {
    const deps = [
      dep({ id: 3, fromTask: 10, toTask: 40, dependencyType: 'relates_to' }),
      dep({ id: 4, fromTask: 50, toTask: 10, dependencyType: 'relates_to' }),
      dep({ id: 5, fromTask: 99, toTask: 88, dependencyType: 'relates_to' }),
    ];
    const result = classifyDependencies(deps, 10);
    expect(result.related.map((d) => d.id)).toEqual([3, 4]);
    expect(result.blocking).toEqual([]);
    expect(result.blockedBy).toEqual([]);
  });

  it('peerTicketKey picks the other ticket', () => {
    const edge = dep({ id: 6, fromTask: 10, toTask: 20 });
    expect(peerTicketKey(edge, 10)).toBe('B-1');
    expect(peerTicketKey(edge, 20)).toBe('A-1');
  });
});

describe('buildCreateDependencyRequest', () => {
  const current = { key: 'CUR-1', id: 10 };
  const other = { id: 20, ticketKey: 'OTH-1' };

  it('blocking posts from current ticket', () => {
    expect(buildCreateDependencyRequest('blocking', current.key, current.id, other)).toEqual({
      postTicketKey: 'CUR-1',
      body: { to_task: 20, dependency_type: 'blocks' },
    });
  });

  it('blockedBy posts from other ticket toward current', () => {
    expect(buildCreateDependencyRequest('blockedBy', current.key, current.id, other)).toEqual({
      postTicketKey: 'OTH-1',
      body: { to_task: 10, dependency_type: 'blocks' },
    });
  });

  it('related posts relates_to from current ticket', () => {
    expect(buildCreateDependencyRequest('related', current.key, current.id, other)).toEqual({
      postTicketKey: 'CUR-1',
      body: { to_task: 20, dependency_type: 'relates_to' },
    });
  });
});
