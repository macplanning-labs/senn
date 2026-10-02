/**
 * commentSync.test.ts — コメントの端末内 DB への取り込み（リアルタイム・差分同期・親の削除）と楽観書き込み
 */
import 'fake-indexeddb/auto';
import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('../api/client', () => {
  const apiClient = { get: vi.fn(), post: vi.fn(), patch: vi.fn(), put: vi.fn(), delete: vi.fn(), defaults: { baseURL: '/api/v1' } };
  return { apiClient, default: apiClient };
});

import { apiClient } from '../api/client';
import { db, openUserDb } from './db';
import { pullEntity } from './pull';
import { applyDelta } from './realtime/applyDelta';
import type { Change, DeltaPacket } from './realtime/packets';
import { commitAddedComment, optimisticAddComment, optimisticDeleteComment, optimisticEditComment } from './commentWrites';
import { commentDto, nextUserId, page, ticketDto, ticketRow } from './__tests__/testUtils';

const get = apiClient.get as unknown as ReturnType<typeof vi.fn>;
let seq = 0;
const delta = (changes: Change[]): DeltaPacket => ({ type: 'delta', epoch: 'e', room: 't:10', seq: ++seq, changes });
const upsertComment = (id: number, ticketId: number, v: number, over: Record<string, unknown> = {}): Change => ({
  op: 'upsert', entity: 'comment', id, v, data: commentDto(id, ticketId, { v, ...over }),
});

beforeEach(() => {
  openUserDb(nextUserId());
  get.mockReset();
  seq = 0;
});

describe('リアルタイム（コメント）', () => {
  it('コメントが届き、手元より古い版は捨てる', async () => {
    await applyDelta(delta([upsertComment(1, 5, 3, { body: '新しい' })]));
    await applyDelta(delta([upsertComment(1, 5, 2, { body: '古い' })]));
    expect((await db.comments.get(1))?.body).toBe('新しい');
    await applyDelta(delta([upsertComment(1, 5, 4, { body: '編集後' })]));
    expect((await db.comments.get(1))?.body).toBe('編集後');
    expect((await db.comments.get(1))?.v).toBe(4);
  });

  it('論理削除（isDeleted の更新）は通常の更新として反映され、本文は空になる', async () => {
    await applyDelta(delta([upsertComment(1, 5, 1)]));
    await applyDelta(delta([upsertComment(1, 5, 2, { isDeleted: true, body: '' })]));
    expect(await db.comments.get(1)).toMatchObject({ isDeleted: true, body: '' });
  });

  it('物理削除の後に遅れて届いた古い版で蘇らない', async () => {
    await applyDelta(delta([upsertComment(1, 5, 2)]));
    await applyDelta(delta([{ op: 'delete', entity: 'comment', id: 1, v: 3 }]));
    expect(await db.comments.get(1)).toBeUndefined();
    await applyDelta(delta([upsertComment(1, 5, 2)]));
    expect(await db.comments.get(1)).toBeUndefined();
  });

  it('親チケットの削除・見えなくなったとき、そのコメントも一緒に消える（他のチケットのコメントは残る）', async () => {
    await db.tickets.bulkPut([ticketRow(5), ticketRow(6)]);
    await db.comments.bulkPut([
      ...[1, 2].map((id) => ({ ...commentDtoRow(id, 5) })),
      commentDtoRow(3, 6),
    ]);
    await applyDelta(delta([{ op: 'evict', entity: 'ticket', id: 5, v: 9 }]));
    expect((await db.comments.toArray()).map((x) => x.id)).toEqual([3]);

    await applyDelta(delta([{ op: 'delete', entity: 'ticket', id: 6, v: 9 }]));
    expect(await db.comments.count()).toBe(0);
  });

  it('コメントが変わったことは changedEntities に出る（詳細の付随データを取り直す判断に使う）', async () => {
    const { applyServerChanges, syncTables } = await import('./writeGateway');
    const r = await db.transaction('rw', syncTables(), () =>
      applyServerChanges([{ op: 'upsert', entity: 'comment', id: 1, v: 1, data: commentDto(1, 5) }]),
    );
    expect([...r.changedEntities]).toEqual(['comment']);
  });
});

function commentDtoRow(id: number, ticketId: number) {
  // toLocalComment を通した行（テストデータ作成用）
  return { ...commentDto(id, ticketId), _syncedAt: null } as never;
}

describe('差分同期（コメント）', () => {
  it('フル同期: 取り込み、受け取らなかった行は消す。ただし楽観書き込み中の行（負の id）は残す', async () => {
    await db.comments.bulkPut([commentDtoRow(90, 5), commentDtoRow(-123, 5)]);
    get.mockResolvedValueOnce(page([commentDto(1, 5, { v: 1 }), commentDto(2, 5, { v: 1 })], { cursor: 'k1' }));
    await pullEntity('comments');

    expect(get).toHaveBeenCalledWith('/sync/comments/', { params: { limit: 500 } });
    expect((await db.comments.toCollection().primaryKeys()).sort((a, b) => a - b)).toEqual([-123, 1, 2]);
    expect((await db.syncMeta.get('comments'))?.lastFullSyncAt).toBeTruthy();
  });

  it('差分: deleted は key があれば削除（版を記録）、key がなければ見えなくなっただけ', async () => {
    await db.syncMeta.put({ entity: 'comments', cursor: 'k0', lastFullSyncAt: 'x', access: { all: true, teamIds: [], scopedProjects: [] } });
    await db.comments.bulkPut([{ ...commentDtoRow(1, 5), v: 1 } as never, { ...commentDtoRow(2, 5), v: 1 } as never]);
    get.mockResolvedValueOnce(page([], { deleted: [{ id: 1, key: '1', v: 2 } as never, { id: 2, key: null }], cursor: 'k1' }));
    await pullEntity('comments');

    expect(await db.comments.count()).toBe(0);
    expect(await db.tombstones.get(['comment', 1])).toMatchObject({ v: 2 });
    expect(await db.tombstones.get(['comment', 2])).toBeUndefined();
  });

  it('チケットの見られる範囲が広がったら、コメントもフル同期し直す（cursor を捨てる）', async () => {
    await db.syncMeta.put({ entity: 'tickets', cursor: 'c0', lastFullSyncAt: 'x', access: { all: false, teamIds: [1], scopedProjects: [] } });
    await db.syncMeta.put({ entity: 'comments', cursor: 'k0', lastFullSyncAt: 'x', access: null });
    get.mockImplementation(async (url: string) => {
      if (url === '/sync/tickets/') {
        return page([ticketDto(1)], { access: { all: false, teamIds: [1, 2], scopedProjects: [] } });
      }
      return page([]);
    });
    await pullEntity('tickets');
    expect((await db.syncMeta.get('comments'))?.cursor).toBeNull();
  });
});

describe('楽観書き込み（commentWrites）', () => {
  const viewer = { id: 7, username: 'me', displayName: 'Me' };

  beforeEach(async () => {
    await db.tickets.put(ticketRow(5));
  });

  it('追加: 仮のコメントが入り、サーバーの id に置き換わる（消えない）。失敗すれば取り消せる', async () => {
    const added = await optimisticAddComment({ ticketKey: 'ABC-000005', body: 'こんにちは', author: viewer });
    expect(added).toBeDefined();
    expect(added!.tempId).toBeLessThan(0);
    expect(await db.comments.get(added!.tempId)).toMatchObject({ ticketId: 5, body: 'こんにちは', author: viewer, parentCommentId: null });

    await commitAddedComment(added!.tempId, 42);
    expect(await db.comments.get(added!.tempId)).toBeUndefined();
    expect(await db.comments.get(42)).toMatchObject({ body: 'こんにちは', ticketId: 5 });

    const failed = await optimisticAddComment({ ticketKey: 'ABC-000005', body: '失敗する', author: viewer });
    await failed!.rollback();
    expect(await db.comments.get(failed!.tempId)).toBeUndefined();
  });

  it('リアルタイムが先に本物の行を届けていたら、確定時にそれを残す（版つきの値を上書きしない）', async () => {
    const added = await optimisticAddComment({ ticketKey: 'ABC-000005', body: '下書き', author: viewer });
    await applyDelta(delta([upsertComment(42, 5, 1, { body: 'サーバーの値' })]));
    await commitAddedComment(added!.tempId, 42);
    expect(await db.comments.get(42)).toMatchObject({ body: 'サーバーの値', v: 1 });
    expect(await db.comments.get(added!.tempId)).toBeUndefined();
  });

  it('端末にないチケットには仮のコメントを作らない', async () => {
    expect(await optimisticAddComment({ ticketKey: 'NOPE-1', body: 'x', author: viewer })).toBeUndefined();
    expect(await db.comments.count()).toBe(0);
  });

  it('編集・削除: 先に反映し、失敗したら元に戻せる', async () => {
    await db.comments.put({ ...commentDtoRow(1, 5), body: '元の本文', updatedAt: null } as never);
    const undoEdit = await optimisticEditComment(1, '編集中');
    expect(await db.comments.get(1)).toMatchObject({ body: '編集中' });
    expect((await db.comments.get(1))?.updatedAt).not.toBeNull();
    await undoEdit!();
    expect(await db.comments.get(1)).toMatchObject({ body: '元の本文', updatedAt: null });

    const undoDelete = await optimisticDeleteComment(1);
    expect(await db.comments.get(1)).toMatchObject({ isDeleted: true, body: '' });
    await undoDelete!();
    expect(await db.comments.get(1)).toMatchObject({ isDeleted: false, body: '元の本文' });

    expect(await optimisticEditComment(999, 'x')).toBeUndefined();
  });
});
