/**
 * projectWrites.test.ts — プロジェクトの書き込み（DEMO-000147 / 詳細設計 §3・§9）
 */

import 'fake-indexeddb/auto';
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { db, deleteCurrentUserDb, openUserDb, type LocalProject } from './db';

vi.mock('./pushRequester', () => ({ requestPush: vi.fn() }));

import { localUpdateProject, writeThroughProject } from './projectWrites';

function serverProject(id: number, name: string): Record<string, unknown> {
  return {
    id,
    name,
    prefix: 'PRJ',
    description: 'server',
    status: 'in_progress',
    priority: 'medium',
    ticketCount: 0,
    memberCount: 1,
    isMember: true,
    teams: [{ id: 7, name: 'T', slug: 't', icon: '', color: '' }],
    ownerId: 1,
    createdAt: '2026-09-29T00:00:00.000000Z',
    updatedAt: '2026-09-29T00:00:00.000000Z',
  };
}

function localProject(id: number, over: Partial<LocalProject> = {}): LocalProject {
  return {
    id,
    name: 'local',
    prefix: 'PRJ',
    description: 'before',
    status: 'in_progress',
    priority: 'medium',
    targetEndDate: null,
    ticketCount: 0,
    memberCount: 1,
    isMember: true,
    teams: [],
    ownerId: 1,
    createdAt: '2026-09-29T00:00:00.000Z',
    updatedAt: '2026-09-29T00:00:00.000Z',
    cycleAutoComplete: false,
    cycleAutoCreateNext: false,
    parentProjectId: null,
    childCount: 0,
    roadmapIds: [],
    aiPromptTemplate: null,
    _dirty: false,
    _syncedAt: null,
    ...over,
  };
}

describe('プロジェクトの書き込み（DEMO-000147）', () => {
  beforeEach(() => {
    openUserDb(9001);
  });

  afterEach(async () => {
    await deleteCurrentUserDb();
  });

  it('writeThroughProject: サーバー応答を _dirty=false の行として端末内に入れる', async () => {
    await writeThroughProject(serverProject(50, 'created'));
    const row = await db.projects.get(50);
    expect(row?.name).toBe('created');
    expect(row?._dirty).toBe(false);
    expect(row?._syncedAt).toBeTruthy();
    expect(row?._syncError).toBeNull();
  });

  it('writeThroughProject: 未送信の変更（_dirty=true）がある行は上書きしない', async () => {
    await db.projects.put(localProject(51, { name: 'edited offline', _dirty: true }));
    await writeThroughProject(serverProject(51, 'from server'));
    const row = await db.projects.get(51);
    expect(row?.name).toBe('edited offline');
    expect(row?._dirty).toBe(true);
  });

  it("localUpdateProject: method='put' で全項目の本文を1件だけキューに積み、行へ即時反映する", async () => {
    await db.projects.put(localProject(52));
    const body = { name: 'local', prefix: 'PRJ', description: 'after', priority: 'medium', teamIds: [7] };
    await localUpdateProject(52, body, { description: 'after' }, 'put');

    const row = await db.projects.get(52);
    expect(row?.description).toBe('after');
    expect(row?._dirty).toBe(true);

    const queue = await db.syncQueue.where('entityId').equals(52).toArray();
    expect(queue).toHaveLength(1);
    expect(queue[0].entity).toBe('project');
    expect(queue[0].operation).toBe('update');
    const payload = JSON.parse(queue[0].payload) as { key: number; patch: Record<string, unknown>; method: string };
    expect(payload.method).toBe('put');
    expect(payload.patch).toEqual(body);
  });

  it("localUpdateProject: 同じ行への続けての 'put' は1件にまとめる", async () => {
    await db.projects.put(localProject(53));
    const base = { name: 'local', prefix: 'PRJ', priority: 'medium', teamIds: [7] };
    await localUpdateProject(53, { ...base, description: 'one' }, { description: 'one' }, 'put');
    await localUpdateProject(53, { ...base, description: 'two' }, { description: 'two' }, 'put');

    const queue = await db.syncQueue.where('entityId').equals(53).toArray();
    expect(queue).toHaveLength(1);
    expect((JSON.parse(queue[0].payload) as { patch: { description: string } }).patch.description).toBe('two');
  });
});
