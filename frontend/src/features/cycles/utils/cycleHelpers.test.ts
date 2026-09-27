import { describe, expect, it } from 'vitest';
import type { Cycle } from '@/shared/api/types';
import {
  allTicketsClosed,
  applyCycleOptimisticPatch,
  canCompleteCycle,
  isValidCycleDateRange,
} from './cycleHelpers';

function makeCycle(overrides: Partial<Cycle> = {}): Cycle {
  return {
    id: 1,
    project: 1,
    name: 'Cycle 1',
    description: '',
    number: 1,
    status: 'planned',
    startDate: '2026-09-21',
    endDate: '2026-10-04',
    createdBy: null,
    createdAt: '2026-09-01T00:00:00Z',
    ticketCount: 6,
    completedCount: 6,
    totalPoints: 0,
    completedPoints: 0,
    team: null,
    ...overrides,
  };
}

describe('allTicketsClosed', () => {
  it('全チケット完了なら true', () => {
    expect(allTicketsClosed(makeCycle({ ticketCount: 6, completedCount: 6 }))).toBe(true);
  });

  it('未完了があれば false', () => {
    expect(allTicketsClosed(makeCycle({ ticketCount: 6, completedCount: 5 }))).toBe(false);
  });

  it('チケット 0 件は false', () => {
    expect(allTicketsClosed(makeCycle({ ticketCount: 0, completedCount: 0 }))).toBe(false);
  });
});

describe('canCompleteCycle', () => {
  it('active は未完了があっても完了可能', () => {
    expect(canCompleteCycle(makeCycle({ status: 'active', completedCount: 3 }))).toBe(true);
  });

  it('planned で全完了なら完了可能', () => {
    expect(canCompleteCycle(makeCycle({ status: 'planned', ticketCount: 6, completedCount: 6 }))).toBe(true);
  });

  it('planned で未完了があれば完了不可', () => {
    expect(canCompleteCycle(makeCycle({ status: 'planned', ticketCount: 6, completedCount: 5 }))).toBe(false);
  });

  it('completed は完了不可', () => {
    expect(canCompleteCycle(makeCycle({ status: 'completed' }))).toBe(false);
  });
});

describe('isValidCycleDateRange', () => {
  it('開始 < 終了なら true', () => {
    expect(isValidCycleDateRange('2026-09-21', '2026-10-09')).toBe(true);
  });

  it('同日または逆順なら false', () => {
    expect(isValidCycleDateRange('2026-10-09', '2026-09-21')).toBe(false);
    expect(isValidCycleDateRange('2026-10-09', '2026-10-09')).toBe(false);
  });
});

describe('applyCycleOptimisticPatch', () => {
  it('end_date を endDate に反映する', () => {
    const cycle = makeCycle({ endDate: '2026-10-04' });
    const next = applyCycleOptimisticPatch(cycle, {
      id: 1,
      project: 1,
      end_date: '2026-10-09',
    });
    expect(next.endDate).toBe('2026-10-09');
  });
});
