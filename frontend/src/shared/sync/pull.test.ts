/**
 * pull.test.ts — 差分同期の取り込み（詳細設計 §3.4、T5）
 */
import 'fake-indexeddb/auto';
import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('../api/client', () => {
  const apiClient = { get: vi.fn(), post: vi.fn(), patch: vi.fn(), put: vi.fn(), delete: vi.fn() };
  return { apiClient, default: apiClient };
});

import { apiClient } from '../api/client';
import { db, openUserDb } from './db';
import { accessExpanded, isAccessible, pullEntity } from './pull';
import { httpError, nextUserId, page, ticketDto, ticketRow } from './__tests__/testUtils';

const get = apiClient.get as unknown as ReturnType<typeof vi.fn>;

describe('pullEntity（チケット）', () => {
  beforeEach(() => {
    openUserDb(nextUserId());
    get.mockReset();
  });

  it('フル同期: 複数ページをすべて取り込み、受け取らなかった行は消す（未送信の行は残す）', async () => {
    await db.tickets.bulkPut([
      ticketRow(90), // サーバーに無い → 消える
      ticketRow(91, {}, { _dirty: true }), // 未送信の変更 → 残る
      ticketRow(-5, {}, { _pendingCreate: true, _dirty: true }), // 作成中 → 残る
    ]);
    get
      .mockResolvedValueOnce(page([ticketDto(1), ticketDto(2)], { hasMore: true, cursor: 'p1' }))
      .mockResolvedValueOnce(page([ticketDto(3)], { cursor: 'p2' }));

    await pullEntity('tickets');

    expect(get).toHaveBeenNthCalledWith(1, '/sync/tickets/', { params: { limit: 500 } });
    expect(get).toHaveBeenNthCalledWith(2, '/sync/tickets/', { params: { limit: 500, cursor: 'p1' } });
    const ids = (await db.tickets.toCollection().primaryKeys()).sort((a, b) => a - b);
    expect(ids).toEqual([-5, 1, 2, 3, 91]);
    const meta = await db.syncMeta.get('tickets');
    expect(meta?.cursor).toBe('p2');
    expect(meta?.lastFullSyncAt).toBeTruthy();
  });

  it('差分: 変更を反映し、未送信の行は上書きしない。削除された行と、その送信待ちを消す', async () => {
    await db.syncMeta.put({ entity: 'tickets', cursor: 'c0', lastFullSyncAt: 'x', access: { all: true, teamIds: [], scopedProjects: [] } });
    await db.tickets.bulkPut([ticketRow(1), ticketRow(2, { title: '端末で編集中' }, { _dirty: true }), ticketRow(3)]);
    await db.syncQueue.bulkAdd([
      { entity: 'ticket', entityId: 3, operation: 'update', payload: '{}', createdAt: '', retryCount: 0 },
      { entity: 'reaction', entityId: 3, operation: 'create', payload: '{}', createdAt: '', retryCount: 0 },
    ]);
    get.mockResolvedValueOnce(
      page([ticketDto(1, { title: 'サーバーで変更' }), ticketDto(2, { title: 'サーバーの古い値' })], {
        deleted: [{ id: 3, key: 'ABC-000003' }],
      }),
    );

    await pullEntity('tickets');

    expect(get).toHaveBeenCalledWith('/sync/tickets/', { params: { limit: 500, cursor: 'c0' } });
    expect((await db.tickets.get(1))?.title).toBe('サーバーで変更');
    expect((await db.tickets.get(2))?.title).toBe('端末で編集中');
    expect(await db.tickets.get(3)).toBeUndefined();
    const queue = await db.syncQueue.toArray();
    expect(queue.map((q) => q.entity)).toEqual(['reaction']); // 同じ id でも別エンティティは消さない
  });

  it('T5: 見られる範囲から外れたチームの行を消す', async () => {
    await db.syncMeta.put({ entity: 'tickets', cursor: 'c0', lastFullSyncAt: 'x', access: { all: false, teamIds: [10, 20], scopedProjects: [] } });
    await db.tickets.bulkPut([
      ticketRow(1, { team: { id: 10, name: 'A', slug: 'a', icon: '', color: '' } }),
      ticketRow(2, { team: { id: 20, name: 'B', slug: 'b', icon: '', color: '' } }),
      ticketRow(3, { team: { id: 20, name: 'B', slug: 'b', icon: '', color: '' }, project: 7 }),
    ]);
    get.mockResolvedValueOnce(page([], { access: { all: false, teamIds: [10], scopedProjects: [{ teamId: 20, projectId: 7 }] } }));

    await pullEntity('tickets');

    expect((await db.tickets.toCollection().primaryKeys()).sort()).toEqual([1, 3]);
  });

  it('T5: 見られる範囲が増えたら、cursor を捨ててフル同期し直す', async () => {
    await db.syncMeta.put({ entity: 'tickets', cursor: 'c0', lastFullSyncAt: 'x', access: { all: false, teamIds: [10], scopedProjects: [] } });
    get
      .mockResolvedValueOnce(page([], { access: { all: false, teamIds: [10, 30], scopedProjects: [] }, cursor: 'c1' }))
      .mockResolvedValueOnce(page([ticketDto(5, { team: { id: 30, name: 'C', slug: 'c', icon: '', color: '' } })], { access: { all: false, teamIds: [10, 30], scopedProjects: [] }, cursor: 'f1' }));

    await pullEntity('tickets');

    expect(get).toHaveBeenLastCalledWith('/sync/tickets/', { params: { limit: 500 } });
    expect(await db.tickets.get(5)).toBeTruthy();
  });

  it('410（cursor が古すぎる）ならフル同期し直す', async () => {
    await db.syncMeta.put({ entity: 'tickets', cursor: 'old', lastFullSyncAt: 'x', access: null });
    await db.tickets.put(ticketRow(9));
    get.mockRejectedValueOnce(httpError(410)).mockResolvedValueOnce(page([ticketDto(1)]));

    await pullEntity('tickets');

    expect(get).toHaveBeenLastCalledWith('/sync/tickets/', { params: { limit: 500 } });
    expect((await db.tickets.toCollection().primaryKeys()).sort()).toEqual([1]);
  });

  it('巻き戻しで同じ行が再送されても、変わっていなければ書かず changed に数えない', async () => {
    await db.syncMeta.put({ entity: 'tickets', cursor: 'c0', lastFullSyncAt: 'x', access: { all: true, teamIds: [], scopedProjects: [] } });
    get.mockResolvedValueOnce(page([ticketDto(1)], { cursor: 'c1' }));
    expect((await pullEntity('tickets')).changed).toBe(1);
    const before = await db.tickets.get(1);

    get.mockResolvedValueOnce(page([ticketDto(1)], { cursor: 'c2' }));
    expect((await pullEntity('tickets')).changed).toBe(0);
    expect((await db.tickets.get(1))?._syncedAt).toBe(before?._syncedAt);
    expect((await db.syncMeta.get('tickets'))?.cursor).toBe('c2');

    get.mockResolvedValueOnce(page([ticketDto(1, { title: '変わった' })], { cursor: 'c3' }));
    expect((await pullEntity('tickets')).changed).toBe(1);
    expect((await db.tickets.get(1))?.title).toBe('変わった');
  });

  it('付随データ（コメント等）は行に入れない', async () => {
    get.mockResolvedValueOnce(page([{ ...ticketDto(1), comments: [{ id: 1 }], isWatching: true }]));
    await pullEntity('tickets');
    const row = (await db.tickets.get(1)) as unknown as Record<string, unknown>;
    expect(row.comments).toBeUndefined();
    expect(row.isWatching).toBeUndefined();
    expect(row.teamId).toBe(10);
  });
});

describe('isAccessible / accessExpanded', () => {
  it('チーム・スコープ付きプロジェクト・全体の判定', () => {
    const access = { all: false, teamIds: [1], scopedProjects: [{ teamId: 2, projectId: 5 }] };
    expect(isAccessible({ teamId: 1, projectId: null }, access)).toBe(true);
    expect(isAccessible({ teamId: 2, projectId: 5 }, access)).toBe(true);
    expect(isAccessible({ teamId: 2, projectId: 6 }, access)).toBe(false);
    expect(isAccessible({ teamId: null, projectId: 5 }, access)).toBe(false);
    expect(isAccessible({ teamId: null, projectId: null }, { all: true, teamIds: [], scopedProjects: [] })).toBe(true);
  });

  it('増えたときだけ true', () => {
    const a = { all: false, teamIds: [1], scopedProjects: [] };
    expect(accessExpanded(a, { ...a, teamIds: [1, 2] })).toBe(true);
    expect(accessExpanded(a, { ...a, teamIds: [] })).toBe(false);
    expect(accessExpanded(a, { ...a, scopedProjects: [{ teamId: 3, projectId: 4 }] })).toBe(true);
    expect(accessExpanded(a, { all: true, teamIds: [], scopedProjects: [] })).toBe(true);
    expect(accessExpanded(null, a)).toBe(false);
  });
});
