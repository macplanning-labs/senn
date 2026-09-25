/**
 * 同期まわりのテスト共通部品（fake-indexeddb ＋ apiClient のモック前提）
 */
import type { LocalTicket } from '../db';
import { toLocalTicket } from '../ticketMapping';

let seq = 1000;
export function nextUserId(): number {
  seq += 1;
  return seq;
}

export function ticketDto(id: number, over: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    id,
    ticketKey: `ABC-${String(id).padStart(6, '0')}`,
    title: `チケット${id}`,
    description: '',
    status: 'open',
    priority: 'medium',
    ticketType: 'task',
    assignees: [],
    reviewers: [],
    author: { id: 1, username: 'a', displayName: 'A' },
    category: null,
    milestone: null,
    project: null,
    projectPrefix: null,
    projectName: null,
    parent: null,
    labels: [],
    startDate: null,
    dueDate: null,
    storyPoints: null,
    cycle: null,
    cycleName: null,
    team: { id: 10, name: 'T', slug: 't', icon: '', color: '#000' },
    commentCount: 0,
    childCount: 0,
    totalTimeSpent: 0,
    gantt_order: 0,
    createdAt: '2026-09-01T00:00:00Z',
    updatedAt: '2026-09-01T00:00:00Z',
    closedAt: null,
    ...over,
  };
}

export function ticketRow(id: number, over: Record<string, unknown> = {}, meta: Partial<LocalTicket> = {}): LocalTicket {
  return { ...toLocalTicket(ticketDto(id, over)), ...meta };
}

export function page(changes: Record<string, unknown>[], opts: Partial<{ deleted: Array<{ id: number; key?: string | null }>; hasMore: boolean; cursor: string; access: unknown }> = {}) {
  return {
    data: {
      changes,
      deleted: opts.deleted ?? [],
      access: opts.access ?? { all: true, teamIds: [], scopedProjects: [] },
      cursor: opts.cursor ?? 'c1',
      hasMore: opts.hasMore ?? false,
      serverTime: new Date().toISOString(),
    },
  };
}

export function httpError(status: number, detail?: string): Error & { response: { status: number; data: { detail?: string } } } {
  const e = new Error(`HTTP ${status}`) as Error & { response: { status: number; data: { detail?: string } } };
  e.response = { status, data: detail ? { detail } : {} };
  return e;
}
