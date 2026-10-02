/**
 * applyDelta.test.ts — リアルタイムのパケット適用（seq の判定・版番号・削除・未送信の保護）
 */
import 'fake-indexeddb/auto';
import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('../../api/client', () => {
  const apiClient = { get: vi.fn(), post: vi.fn(), patch: vi.fn(), put: vi.fn(), delete: vi.fn(), defaults: { baseURL: '/api/v1' } };
  return { apiClient, default: apiClient };
});

import { db, openUserDb } from '../db';
import { nextUserId, ticketDto, ticketRow } from '../__tests__/testUtils';
import { applyAccess, applyDelta, applyResync, applyWelcome, buildResume } from './applyDelta';
import type { Change, DeltaPacket } from './packets';

const EPOCH = 'e1';
let seq = 0;

function delta(changes: Change[], over: Partial<DeltaPacket> = {}): DeltaPacket {
  seq += 1;
  return { type: 'delta', epoch: EPOCH, room: 't:10', seq, changes, ...over };
}
const upsert = (id: number, v: number, over: Record<string, unknown> = {}): Change => ({
  op: 'upsert', entity: 'ticket', id, v, data: ticketDto(id, { v, ...over }),
});

describe('applyDelta', () => {
  beforeEach(() => {
    openUserDb(nextUserId());
    seq = 0;
  });

  it('順に届いたパケットを書き、受信位置を進める。欠落は無い', async () => {
    const o1 = await applyDelta({ ...delta([upsert(1, 1)]), seq: 1 });
    // 初めて見る部屋は「欠落の可能性あり」として1回だけ差分同期で埋める（安全側）
    expect(o1.needsCatchUp).toBe(true);
    const o2 = await applyDelta({ ...delta([upsert(1, 2, { title: '更新' })]), seq: 2 });
    expect(o2).toEqual({ duplicate: false, needsCatchUp: false });
    expect((await db.tickets.get(1))?.title).toBe('更新');
    expect(await db.realtimeMeta.get('t:10')).toEqual({ room: 't:10', epoch: EPOCH, seq: 2 });
  });

  it('同じ seq の再送（重複）は捨てる。位置も動かさない', async () => {
    await applyDelta({ ...delta([upsert(1, 1)]), seq: 5 });
    const dup = await applyDelta({ ...delta([upsert(1, 9, { title: '来てはいけない' })]), seq: 5 });
    expect(dup.duplicate).toBe(true);
    expect((await db.tickets.get(1))?.title).not.toBe('来てはいけない');
    expect((await db.realtimeMeta.get('t:10'))?.seq).toBe(5);
  });

  it('seq が飛んだら、届いた分は適用しつつ、差分同期で埋めるよう求める', async () => {
    await applyDelta({ ...delta([upsert(1, 1)]), seq: 1 });
    const gap = await applyDelta({ ...delta([upsert(2, 1)]), seq: 4 });
    expect(gap.needsCatchUp).toBe(true);
    expect(await db.tickets.get(2)).toBeDefined();
    expect((await db.realtimeMeta.get('t:10'))?.seq).toBe(4);
  });

  it('epoch が変わったら（サーバー再起動）seq は比べず、差分同期を求める', async () => {
    await applyDelta({ ...delta([upsert(1, 1)]), seq: 50 });
    const o = await applyDelta({ type: 'delta', epoch: 'e2', room: 't:10', seq: 1, changes: [upsert(1, 2)] });
    expect(o.duplicate).toBe(false);
    expect(o.needsCatchUp).toBe(true);
    expect(await db.realtimeMeta.get('t:10')).toEqual({ room: 't:10', epoch: 'e2', seq: 1 });
  });

  it('先祖返りしない: 手元より古い（または同じ）版は捨てる', async () => {
    await applyDelta({ ...delta([upsert(1, 5, { title: '新しい' })]), seq: 1 });
    await applyDelta({ ...delta([upsert(1, 3, { title: '古い' })]), seq: 2 });
    await applyDelta({ ...delta([upsert(1, 5, { title: '同じ版' })]), seq: 3 });
    expect((await db.tickets.get(1))?.title).toBe('新しい');
    expect((await db.tickets.get(1))?.v).toBe(5);
  });

  it('未送信の変更（_dirty）がある行は、新しい版が来ても上書きしない', async () => {
    await db.tickets.put(ticketRow(1, { title: '端末で編集中' }, { _dirty: true, v: 1 }));
    await applyDelta({ ...delta([upsert(1, 9, { title: 'サーバー' })]), seq: 1 });
    expect((await db.tickets.get(1))?.title).toBe('端末で編集中');
  });

  it('削除: 行・子のリアクション・送信待ちを消す。削除の後に遅れて届いた古い upsert で蘇らない', async () => {
    await db.tickets.put(ticketRow(1, {}, { v: 3 }));
    await db.reactions.put({ id: 7, ticketId: 1, userId: 1, emoji: '👍', updatedAt: '', _dirty: false, _syncedAt: null } as never);
    await db.syncQueue.bulkAdd([
      { entity: 'ticket', entityId: 1, operation: 'update', payload: '{}', createdAt: '', retryCount: 0 },
      { entity: 'reaction', entityId: 7, operation: 'delete', payload: '{}', createdAt: '', retryCount: 0 },
      { entity: 'ticket', entityId: 2, operation: 'update', payload: '{}', createdAt: '', retryCount: 0 }, // 別の行は残る
    ]);

    await applyDelta({ ...delta([{ op: 'delete', entity: 'ticket', id: 1, v: 4 }]), seq: 1 });

    expect(await db.tickets.get(1)).toBeUndefined();
    expect(await db.reactions.get(7)).toBeUndefined();
    expect((await db.syncQueue.toArray()).map((q) => q.entityId)).toEqual([2]);
    // 遅れて届いた古い版
    await applyDelta({ ...delta([upsert(1, 3, { title: '亡霊' })]), seq: 2 });
    expect(await db.tickets.get(1)).toBeUndefined();
    // 削除より新しい版（再作成など）は入る
    await applyDelta({ ...delta([upsert(1, 5, { title: '再作成' })]), seq: 3 });
    expect((await db.tickets.get(1))?.title).toBe('再作成');
  });

  it('evict: 未送信の変更があっても消す（送り先が無い）。送信待ちも消す', async () => {
    await db.tickets.put(ticketRow(1, {}, { _dirty: true, v: 1 }));
    await db.syncQueue.add({ entity: 'ticket', entityId: 1, operation: 'update', payload: '{}', createdAt: '', retryCount: 0 });
    const o = await applyDelta({ ...delta([{ op: 'evict', entity: 'ticket', id: 1, v: 2 }]), seq: 1 });
    expect(await db.tickets.get(1)).toBeUndefined();
    expect(await db.syncQueue.count()).toBe(0);
    expect(o.duplicate).toBe(false);
    // evict は削除ではないので、記録は残さない。見えるようになったら（同じ版でも）また入る
    await applyDelta({ ...delta([upsert(1, 1)]), seq: 2 });
    expect(await db.tickets.get(1)).toBeDefined();
  });

  it('stale は書かずに、取り直し（差分同期）を求める', async () => {
    const o = await applyDelta({ ...delta([{ op: 'stale', entity: 'project', id: 3, v: 2 }]), seq: 1 });
    expect(o.needsCatchUp).toBe(true);
  });

  it('途中で失敗したら、行も受信位置も進まない（1つのトランザクション）', async () => {
    await applyDelta({ ...delta([upsert(1, 1)]), seq: 1 });
    // 行を書いたあと、受信位置の書き込みで失敗させる
    const spy = vi.spyOn(db.realtimeMeta, 'put').mockRejectedValueOnce(new Error('boom'));
    await expect(applyDelta({ ...delta([upsert(2, 1)]), seq: 2 })).rejects.toThrow('boom');
    spy.mockRestore();
    expect(await db.tickets.get(2)).toBeUndefined();
    expect((await db.realtimeMeta.get('t:10'))?.seq).toBe(1);
  });
});

describe('resume / welcome / access / resync', () => {
  beforeEach(() => openUserDb(nextUserId()));

  it('受信位置が無ければ epoch なしの resume', async () => {
    expect(await buildResume()).toEqual({ type: 'resume', epoch: null, rooms: [] });
  });

  it('resume は最も多い epoch の部屋だけを送る', async () => {
    await db.realtimeMeta.bulkPut([
      { room: 't:1', epoch: 'a', seq: 4 },
      { room: 't:2', epoch: 'a', seq: 9 },
      { room: 't:3', epoch: 'b', seq: 1 },
    ]);
    const r = await buildResume();
    expect(r.epoch).toBe('a');
    expect(r.rooms.sort((x, y) => x.room.localeCompare(y.room))).toEqual([{ room: 't:1', seq: 4 }, { room: 't:2', seq: 9 }]);
  });

  it('welcome: 購読していない部屋の受信位置を捨てる', async () => {
    await db.realtimeMeta.bulkPut([{ room: 't:1', epoch: 'a', seq: 1 }, { room: 't:2', epoch: 'a', seq: 1 }]);
    await applyWelcome({ type: 'welcome', epoch: 'a', connectionId: '1', rooms: [{ room: 't:1', seq: 1 }], access: { all: false, teamIds: [1], scopedProjects: [] }, serverTime: '' });
    expect((await db.realtimeMeta.toArray()).map((m) => m.room)).toEqual(['t:1']);
  });

  it('resync: 示された seq から続け、差分同期を求める', async () => {
    const o = await applyResync({ type: 'resync', epoch: 'a', room: 't:1', seq: 42, reason: 'buffer_exceeded', entities: ['ticket'] });
    expect(o.needsCatchUp).toBe(true);
    expect(await db.realtimeMeta.get('t:1')).toEqual({ room: 't:1', epoch: 'a', seq: 42 });
  });

  it('access: 外れた部屋の位置を消し、増えた部屋の位置を作る', async () => {
    await db.realtimeMeta.bulkPut([{ room: 't:1', epoch: 'a', seq: 3 }]);
    const o = await applyAccess({ type: 'access', access: { all: false, teamIds: [2], scopedProjects: [] }, rooms: [{ room: 't:2', seq: 7 }] }, 'a');
    expect(o.needsCatchUp).toBe(true);
    expect(await db.realtimeMeta.toArray()).toEqual([{ room: 't:2', epoch: 'a', seq: 7 }]);
  });
});
