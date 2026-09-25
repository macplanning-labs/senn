/**
 * push.test.ts — 書き込み（ticketWrites）と送信（push）の組み合わせ（詳細設計 §3.5〜§3.7、T7）
 */
import 'fake-indexeddb/auto';
import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('../api/client', () => {
  const apiClient = { get: vi.fn(), post: vi.fn(), patch: vi.fn(), put: vi.fn(), delete: vi.fn() };
  return { apiClient, default: apiClient };
});

import { apiClient } from '../api/client';
import { db, openUserDb } from './db';
import { onKeyRemap, pushEntityItem, MAX_RETRY } from './push';
import { registerPushRequester } from './pushRequester';
import { localCreateTicket, localDeleteTickets, localUpdateTicket, localUpdateTickets } from './ticketWrites';
import { httpError, nextUserId, ticketDto, ticketRow } from './__tests__/testUtils';
import { useToastStore } from '../stores/toastStore';

const mocked = apiClient as unknown as Record<'get' | 'post' | 'patch' | 'put' | 'delete', ReturnType<typeof vi.fn>>;

/** テストでは自動送信しない（送るタイミングをテストが決める） */
registerPushRequester(() => undefined);

/** キューを全部1回ずつ送る（syncEngine の pushQueueOnce と同じ順序） */
async function flush(): Promise<void> {
  const order = { custom_emoji: 0, reaction: 1, project: 2, ticket: 3, wiki: 4 } as const;
  const items = (await db.syncQueue.where('retryCount').below(MAX_RETRY).toArray()).sort(
    (a, b) => order[a.entity] - order[b.entity] || (a.id ?? 0) - (b.id ?? 0),
  );
  for (const snap of items) {
    const item = await db.syncQueue.get(snap.id!);
    if (item && (item.entity === 'ticket' || item.entity === 'project')) await pushEntityItem(item);
  }
}

describe('チケットの更新', () => {
  beforeEach(async () => {
    openUserDb(nextUserId());
    Object.values(mocked).forEach((m) => m.mockReset());
    await db.tickets.put(ticketRow(1));
  });

  it('連続した更新は1件にまとまり、行は即時に変わる', async () => {
    await localUpdateTicket('ABC-000001', { status: 'in_progress' }, { status: 'in_progress' });
    await localUpdateTicket('ABC-000001', { priority: 'high' }, { priority: 'high' });

    const row = await db.tickets.get(1);
    expect(row?.status).toBe('in_progress');
    expect(row?.priority).toBe('high');
    expect(row?._dirty).toBe(true);
    const queue = await db.syncQueue.toArray();
    expect(queue).toHaveLength(1);
    expect(JSON.parse(queue[0].payload)).toEqual({ key: 'ABC-000001', patch: { status: 'in_progress', priority: 'high' } });
  });

  it('送信が通ると、応答の値で行を置き換えて送信待ちが消える', async () => {
    await localUpdateTicket('ABC-000001', { status: 'closed' }, { status: 'closed' });
    mocked.patch.mockResolvedValueOnce({ data: { ...ticketDto(1, { status: 'closed', updatedAt: '2026-09-25T00:00:00Z' }), comments: [] } });
    await flush();

    expect(mocked.patch).toHaveBeenCalledWith('/tickets/ABC-000001/', { status: 'closed' });
    const row = await db.tickets.get(1);
    expect(row?._dirty).toBe(false);
    expect(row?.updatedAt).toBe('2026-09-25T00:00:00Z');
    expect(await db.syncQueue.count()).toBe(0);
  });

  it('送信中に追記された変更は消えず、次回もう一度送る', async () => {
    await localUpdateTicket('ABC-000001', { status: 'closed' }, { status: 'closed' });
    mocked.patch.mockImplementationOnce(async () => {
      await localUpdateTicket('ABC-000001', { title: '送信中の編集' }, { title: '送信中の編集' });
      return { data: ticketDto(1, { status: 'closed' }) };
    });
    await flush();

    const row = await db.tickets.get(1);
    expect(row?.title).toBe('送信中の編集');
    expect(row?._dirty).toBe(true);
    const queue = await db.syncQueue.toArray();
    expect(queue).toHaveLength(1);
    expect(JSON.parse(queue[0].payload).patch).toEqual({ status: 'closed', title: '送信中の編集' });
  });

  it('4xx は再送せず、サーバーの値へ戻してトーストを出す', async () => {
    const addToast = vi.spyOn(useToastStore.getState(), 'addToast');
    await localUpdateTicket('ABC-000001', { status: 'bogus' }, { status: 'bogus' });
    mocked.patch.mockRejectedValueOnce(httpError(400, 'このチームに存在しないステータスです'));
    mocked.get.mockResolvedValueOnce({ data: ticketDto(1, { status: 'open' }) });
    await flush();

    expect(mocked.get).toHaveBeenCalledWith('/tickets/ABC-000001/');
    const row = await db.tickets.get(1);
    expect(row?.status).toBe('open');
    expect(row?._syncError).toBe('このチームに存在しないステータスです');
    expect(await db.syncQueue.count()).toBe(0);
    expect(addToast).toHaveBeenCalledWith({ type: 'error', message: 'このチームに存在しないステータスです' });
  });

  it('5xx・通信エラーは再送回数を増やし、5回で自動送信を止める', async () => {
    await localUpdateTicket('ABC-000001', { status: 'closed' }, { status: 'closed' });
    mocked.patch.mockRejectedValue(httpError(503));
    for (let i = 0; i < 7; i++) await flush();

    expect(mocked.patch).toHaveBeenCalledTimes(MAX_RETRY);
    const [item] = await db.syncQueue.toArray();
    expect(item.retryCount).toBe(MAX_RETRY);
    expect(item.lastError).toBe('HTTP 503');
    expect((await db.tickets.get(1))?.status).toBe('closed'); // 端末の変更は保ったまま
  });

  it('一括更新は1トランザクションで各行に当たる', async () => {
    await db.tickets.put(ticketRow(2));
    await localUpdateTickets(['ABC-000001', 'ABC-000002'], { status: 'closed' }, { status: 'closed' });
    expect((await db.tickets.bulkGet([1, 2])).map((r) => r?.status)).toEqual(['closed', 'closed']);
    expect(await db.syncQueue.count()).toBe(2);
  });
});

describe('T7: オフライン作成と仮 id / 仮キーの付け替え', () => {
  beforeEach(() => {
    openUserDb(nextUserId());
    Object.values(mocked).forEach((m) => m.mockReset());
  });

  it('作成 → 同じ仮行の編集 → 子チケット → リアクション → 送信で、すべて本物に付け替わる', async () => {
    const remaps: Array<[string, string]> = [];
    const off = onKeyRemap((a, b) => remaps.push([a, b]));

    const parent = await localCreateTicket({ title: '親', ticket_type: 'task', team_id: 10 }, { team: { id: 10, name: 'T', slug: 't', icon: '', color: '' } });
    expect(parent.tempId).toBeLessThan(0);
    expect(parent.tempKey.startsWith('local-')).toBe(true);
    const tempRow = await db.tickets.get(parent.tempId);
    expect(tempRow?._pendingCreate).toBe(true);
    expect(tempRow?.teamId).toBe(10);

    // 作成前の編集は作成の本文に合成され、更新は作られない
    await localUpdateTicket(parent.tempKey, { title: '親（編集後）' }, { title: '親（編集後）' });
    const child = await localCreateTicket({ title: '子', ticket_type: 'task', team_id: 10, parent: parent.tempId }, { parent: parent.tempId });
    await db.reactions.put({ id: -1, ticketId: parent.tempId, userId: 1, emojiKind: 'unicode', emojiValue: '👍', updatedAt: '', _dirty: true, _syncedAt: null });
    await db.syncQueue.add({ entity: 'reaction', entityId: -1, operation: 'create', payload: JSON.stringify({ ticketKey: parent.tempKey, emojiKind: 'unicode', emojiValue: '👍' }), createdAt: '', retryCount: 0 });

    const createItems = (await db.syncQueue.toArray()).filter((q) => q.entity === 'ticket');
    expect(createItems.map((q) => q.operation)).toEqual(['create', 'create']);
    expect(JSON.parse(createItems[0].payload).body.title).toBe('親（編集後）');

    mocked.post
      .mockResolvedValueOnce({ data: ticketDto(500, { title: '親（編集後）' }) })
      .mockResolvedValueOnce({ data: ticketDto(501, { title: '子', parent: 500 }) });
    await flush();

    // 送信時に冪等キーを付け、子の親は本 id で送る
    expect(mocked.post.mock.calls[0][2]).toEqual({ headers: { 'Idempotency-Key': createItems[0].idempotencyKey } });
    expect(mocked.post.mock.calls[1][1]).toMatchObject({ parent: 500 });

    expect(await db.tickets.get(parent.tempId)).toBeUndefined();
    expect(await db.tickets.get(child.tempId)).toBeUndefined();
    expect((await db.tickets.get(500))?.ticketKey).toBe('ABC-000500');
    expect((await db.tickets.get(501))?.parentId).toBe(500);
    expect((await db.reactions.get(-1))?.ticketId).toBe(500);
    const reactionItem = (await db.syncQueue.toArray()).find((q) => q.entity === 'reaction');
    expect(JSON.parse(reactionItem!.payload).ticketKey).toBe('ABC-000500');
    expect(remaps).toContainEqual([parent.tempKey, 'ABC-000500']);
    off();
  });

  it('作成の送信中に編集されたら、差分を更新として送る', async () => {
    const created = await localCreateTicket({ title: 'A', ticket_type: 'task', team_id: 10 });
    mocked.post.mockImplementationOnce(async () => {
      await localUpdateTicket(created.tempKey, { title: 'B' }, { title: 'B' });
      return { data: ticketDto(600, { title: 'A' }) };
    });
    await flush();

    const row = await db.tickets.get(600);
    expect(row?.title).toBe('B');
    expect(row?._dirty).toBe(true);
    const [upd] = await db.syncQueue.toArray();
    expect(upd.operation).toBe('update');
    expect(JSON.parse(upd.payload)).toEqual({ key: 'ABC-000600', patch: { title: 'B' } });
  });

  it('作成が 4xx なら仮の行を消す', async () => {
    const created = await localCreateTicket({ title: 'A', ticket_type: 'task' });
    mocked.post.mockRejectedValueOnce(httpError(400, 'teamId is required'));
    await flush();
    expect(await db.tickets.get(created.tempId)).toBeUndefined();
    expect(await db.syncQueue.count()).toBe(0);
  });
});

describe('削除', () => {
  beforeEach(async () => {
    openUserDb(nextUserId());
    Object.values(mocked).forEach((m) => m.mockReset());
    await db.tickets.bulkPut([ticketRow(1), ticketRow(2)]);
  });

  it('削除は即時に印が付き、送信後に行が消える（未送信の更新は捨てる）', async () => {
    await localUpdateTicket('ABC-000001', { title: 'x' }, { title: 'x' });
    await localDeleteTickets(['ABC-000001']);
    expect((await db.tickets.get(1))?._deleted).toBe(true);
    const ops = (await db.syncQueue.toArray()).map((q) => q.operation);
    expect(ops).toEqual(['delete']);

    mocked.delete.mockResolvedValueOnce({ data: null });
    await flush();
    expect(mocked.patch).not.toHaveBeenCalled();
    expect(await db.tickets.get(1)).toBeUndefined();
    expect(await db.syncQueue.count()).toBe(0);
  });

  it('既に無い（404）は成功扱い', async () => {
    await localDeleteTickets(['ABC-000002']);
    mocked.delete.mockRejectedValueOnce(httpError(404));
    await flush();
    expect(await db.tickets.get(2)).toBeUndefined();
  });

  it('送信前の作成中チケットを消すと、作成ごと取り消す（サーバーに送らない）', async () => {
    const created = await localCreateTicket({ title: 'A', ticket_type: 'task' });
    await localDeleteTickets([created.tempKey]);
    await flush();
    expect(mocked.post).not.toHaveBeenCalled();
    expect(await db.tickets.get(created.tempId)).toBeUndefined();
    expect(await db.syncQueue.count()).toBe(0);
  });
});
