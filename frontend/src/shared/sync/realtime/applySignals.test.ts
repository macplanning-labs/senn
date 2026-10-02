/**
 * applySignals.test.ts — 表の無い対象（添付・サイクル・通知・Wiki・リアクション）の合図
 */
import 'fake-indexeddb/auto';
import { QueryClient } from '@tanstack/react-query';
import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('../../api/client', () => {
  const apiClient = { get: vi.fn(), post: vi.fn(), patch: vi.fn(), put: vi.fn(), delete: vi.fn(), defaults: { baseURL: '/api/v1' } };
  return { apiClient, default: apiClient };
});

import { apiClient } from '../../api/client';
import { db, openUserDb } from '../db';
import { registerSyncQueryClient, registerSyncTrigger } from '../pull';
import { nextUserId, ticketRow } from '../__tests__/testUtils';
import { applyDelta } from './applyDelta';
import { applyAllSignals } from './applySignals';
import type { Change, DeltaPacket } from './packets';

const get = apiClient.get as unknown as ReturnType<typeof vi.fn>;
let invalidated: unknown[][] = [];
let syncTriggered = 0;
let seq = 0;

const stale = (entity: string, id: number): Change => ({ op: 'stale', entity: entity as never, id, v: 0 });
async function send(changes: Change[]) {
  const p: DeltaPacket = { type: 'delta', epoch: 'e', room: 'u:1', seq: ++seq, changes };
  return applyDelta(p);
}
const flush = () => new Promise((r) => setTimeout(r, 20));
/** 橋渡し（invalidate を合図に同期を起こす）は 200ms 遅れて発火するので、それより長く待つ */
const waitForBridge = () => new Promise((r) => setTimeout(r, 300));

beforeEach(async () => {
  openUserDb(nextUserId());
  get.mockReset();
  get.mockResolvedValue({ data: [] });
  invalidated = [];
  syncTriggered = 0;
  seq = 0;
  const qc = new QueryClient();
  qc.invalidateQueries = ((f: { queryKey: unknown[] }) => {
    invalidated.push(f.queryKey);
    return Promise.resolve();
  }) as never;
  registerSyncQueryClient(qc);
  registerSyncTrigger(() => void (syncTriggered += 1));
  // 部屋の位置を作っておく（初めて見る部屋は「欠落の可能性」として扱われるため）
  await db.realtimeMeta.put({ room: 'u:1', epoch: 'e', seq: 0 });
});

describe('合図（表の無い対象）', () => {
  it('添付の合図は詳細の付随データを取り直す。同期は起こし直さない（合図で同期が回り続けない）', async () => {
    const o = await send([stale('attachment', 7)]);
    expect(invalidated).toEqual([['ticket']]);
    await waitForBridge();
    expect(syncTriggered).toBe(0);
    expect(o.needsCatchUp).toBe(false); // 差分同期は不要
  });

  it('通知・サイクル・Wiki は、それぞれのキャッシュを取り直す', async () => {
    await send([stale('notification', 0), stale('cycle', 3), stale('wiki', 9)]);
    expect(invalidated.map((k) => k[0]).sort()).toEqual(['cycle', 'cycle-progress', 'cycles', 'notifications', 'unread-count', 'wiki', 'wiki-page', 'wiki-pages']);
  });

  it('同じ種別の合図が重なっても、取り直しは1回', async () => {
    await send([stale('notification', 0), stale('notification', 0), stale('attachment', 1), stale('attachment', 2)]);
    expect(invalidated.filter((k) => k[0] === 'notifications')).toHaveLength(1);
    expect(invalidated.filter((k) => k[0] === 'ticket')).toHaveLength(1);
  });

  it('端末内 DB には何も書かない（実データではない）', async () => {
    await send([stale('attachment', 7), stale('notification', 0)]);
    expect(await db.tickets.count()).toBe(0);
    expect(await db.comments.count()).toBe(0);
  });

  it('テーブルのある種別の stale（プロジェクトなど）は、従来どおり差分同期を求める', async () => {
    const o = await send([stale('project', 3)]);
    expect(o.needsCatchUp).toBe(true);
    expect(invalidated).toEqual([]);
  });
});

describe('リアクションの合図（id はチケット ID）', () => {
  it('すでにリアクションを持つチケットは、サーバーから取り直す', async () => {
    await db.tickets.put(ticketRow(5));
    await db.reactions.put({ id: 1, ticketId: 5, userId: 2, emojiKind: 'unicode', emojiValue: '👍', updatedAt: '', _dirty: false, _syncedAt: null });
    await send([stale('reaction', 5)]);
    await vi.waitFor(() => expect(get).toHaveBeenCalledWith('/tickets/ABC-000005/reactions/'));
  });

  it('リアクションを一度も表示していない・詳細を開いていないチケットは、取りに行かない（開いたときに取る）', async () => {
    await db.tickets.put(ticketRow(6));
    await send([stale('reaction', 6)]);
    await flush();
    expect(get).not.toHaveBeenCalled();
  });

  it('端末に無いチケットは無視する', async () => {
    await send([stale('reaction', 999)]);
    await flush();
    expect(get).not.toHaveBeenCalled();
  });

  it('同じチケットの合図が重なっても、取り直しは1回', async () => {
    await db.tickets.put(ticketRow(5));
    await db.reactions.put({ id: 1, ticketId: 5, userId: 2, emojiKind: 'unicode', emojiValue: '👍', updatedAt: '', _dirty: false, _syncedAt: null });
    await send([stale('reaction', 5), stale('reaction', 5), stale('reaction', 5)]);
    await flush();
    expect(get).toHaveBeenCalledTimes(1);
  });
});

describe('取りこぼしたとき', () => {
  it('欠落・再同期では、表の無い対象のキャッシュをすべて取り直す', () => {
    applyAllSignals();
    const heads = invalidated.map((k) => k[0]);
    expect(heads).toEqual(expect.arrayContaining(['ticket', 'cycles', 'notifications', 'unread-count', 'wiki-pages']));
    expect(syncTriggered).toBe(0);
  });
});
