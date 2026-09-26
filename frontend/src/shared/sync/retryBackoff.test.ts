// @vitest-environment jsdom
/**
 * retryBackoff.test.ts — 送れなかった変更の再送（時間ベースのバックオフ）と、古い変更で新しい値を上書きしないこと
 *
 * 背景: 以前は送信の「きっかけ」（30秒ごと・フォーカス・タブ復帰・編集のたび）ごとに再送回数を数えていたため、
 * スリープ復帰直後やタブ切り替え中に数回編集するだけで、通信エラーの変更が「同期できなかった変更」になっていた。
 */
import 'fake-indexeddb/auto';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('../api/client', () => {
  const apiClient = { get: vi.fn(), post: vi.fn(), patch: vi.fn(), put: vi.fn(), delete: vi.fn() };
  return { apiClient, default: apiClient };
});

import { apiClient } from '../api/client';
import { db, MAX_RETRY, openUserDb } from './db';
import { backoffMs, classifyFailure, pushEntityItem } from './push';
import { registerPushRequester } from './pushRequester';
import { pushQueueOnce, stopSync } from './syncEngine';
import { discardFailedChange, reviveTransientFailures, wakeQueue } from './failedChanges';
import { localUpdateTicket } from './ticketWrites';
import { httpError, nextUserId, ticketDto, ticketRow } from './__tests__/testUtils';

const mocked = apiClient as unknown as Record<'get' | 'post' | 'patch' | 'put' | 'delete', ReturnType<typeof vi.fn>>;
const networkError = () => Object.assign(new Error('Network Error'), { code: 'ERR_NETWORK' });

registerPushRequester(() => undefined);

const T0 = new Date('2026-09-26T07:00:00Z').getTime();

describe('再送のバックオフ', () => {
  beforeEach(async () => {
    stopSync();
    registerPushRequester(() => undefined);
    vi.useFakeTimers({ toFake: ['Date'] });
    vi.setSystemTime(T0);
    vi.spyOn(Math, 'random').mockReturnValue(0.5); // ゆらぎ無し
    openUserDb(nextUserId());
    Object.values(mocked).forEach((m) => m.mockReset());
    await db.tickets.bulkPut([ticketRow(1), ticketRow(2), ticketRow(3)]);
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('待ち時間は2秒から倍々で増え、5分で頭打ち', () => {
    expect(backoffMs(1)).toBe(2_000);
    expect(backoffMs(2)).toBe(4_000);
    expect(backoffMs(5)).toBe(32_000);
    expect(backoffMs(20)).toBe(300_000);
  });

  it('失敗の分類: 応答なし=network、5xx/401/408/429=server、その他4xx=reject', () => {
    expect(classifyFailure(networkError())).toBe('network');
    expect(classifyFailure(Object.assign(new Error('timeout of 30000ms exceeded'), { code: 'ECONNABORTED' }))).toBe('network');
    expect(classifyFailure(httpError(502))).toBe('server');
    expect(classifyFailure(httpError(401))).toBe('server');
    expect(classifyFailure(httpError(408))).toBe('server');
    expect(classifyFailure(httpError(429))).toBe('server');
    expect(classifyFailure(httpError(400))).toBe('reject');
    expect(classifyFailure(httpError(403))).toBe('reject');
  });

  it('再現: 通信エラー中にきっかけが何度来ても（フォーカス・タブ復帰・編集）、待ち時間内は送らず失敗扱いにもならない', async () => {
    await localUpdateTicket('ABC-000001', { status: 'closed' }, { status: 'closed' });
    mocked.patch.mockRejectedValue(networkError());

    // 以前はこれだけで retryCount が 5 に達して「同期できなかった変更」になっていた
    for (const trigger of ['edit', 'focus', 'visible', 'rerun', 'interval', 'focus', 'visible']) {
      await pushQueueOnce(trigger);
    }
    expect(mocked.patch).toHaveBeenCalledTimes(1);
    const [item] = await db.syncQueue.toArray();
    expect(item.retryCount).toBe(0);
    expect(item.lastError).toContain('trigger=edit');
    expect(item.lastError).toContain('online=true');
    expect(Date.parse(item.nextAttemptAt!)).toBe(T0 + 2_000);

    // 待ち時間が明けたら送る。通信が戻っていれば成功
    vi.setSystemTime(T0 + 2_000);
    mocked.patch.mockResolvedValueOnce({ data: ticketDto(1, { status: 'closed' }) });
    await pushQueueOnce('interval');
    expect(mocked.patch).toHaveBeenCalledTimes(2);
    expect(await db.syncQueue.count()).toBe(0);
    expect((await db.tickets.get(1))?._dirty).toBe(false);
  });

  it('通信エラーが出たら、その回の残りの項目は送らない（失敗を積み増さない）', async () => {
    await localUpdateTicket('ABC-000001', { status: 'closed' }, { status: 'closed' });
    await localUpdateTicket('ABC-000002', { status: 'closed' }, { status: 'closed' });
    await localUpdateTicket('ABC-000003', { status: 'closed' }, { status: 'closed' });
    mocked.patch.mockRejectedValue(networkError());

    await pushQueueOnce('interval');
    expect(mocked.patch).toHaveBeenCalledTimes(1);
    const items = await db.syncQueue.toArray();
    expect(items.map((q) => q.attempts ?? 0)).toEqual([1, 0, 0]);
  });

  it('5xx は他の項目の送信を止めない', async () => {
    await localUpdateTicket('ABC-000001', { status: 'closed' }, { status: 'closed' });
    await localUpdateTicket('ABC-000002', { status: 'closed' }, { status: 'closed' });
    mocked.patch.mockRejectedValueOnce(httpError(500)).mockResolvedValueOnce({ data: ticketDto(2, { status: 'closed' }) });

    await pushQueueOnce('interval');
    expect(mocked.patch).toHaveBeenCalledTimes(2);
    const items = await db.syncQueue.toArray();
    expect(items).toHaveLength(1);
    expect(items[0].retryCount).toBe(1);
  });

  it('作成が待ち時間中は後ろの作成を送らないが、既存チケットの更新は送る', async () => {
    const { localCreateTicket } = await import('./ticketWrites');
    await localCreateTicket({ title: '親', ticket_type: 'task' });
    await localCreateTicket({ title: '子', ticket_type: 'task' });
    await localUpdateTicket('ABC-000001', { status: 'closed' }, { status: 'closed' });
    mocked.post.mockRejectedValueOnce(httpError(503));
    mocked.patch.mockResolvedValueOnce({ data: ticketDto(1, { status: 'closed' }) });

    await pushQueueOnce('interval');
    expect(mocked.post).toHaveBeenCalledTimes(1);
    expect(mocked.patch).toHaveBeenCalledTimes(1);
  });

  it('オフラインのときは送らない', async () => {
    await localUpdateTicket('ABC-000001', { status: 'closed' }, { status: 'closed' });
    const spy = vi.spyOn(navigator, 'onLine', 'get').mockReturnValue(false);
    await pushQueueOnce('interval');
    expect(mocked.patch).not.toHaveBeenCalled();
    spy.mockRestore();
  });

  it('編集すると、その行の待ち時間は解除されてすぐ送る', async () => {
    await localUpdateTicket('ABC-000001', { status: 'closed' }, { status: 'closed' });
    mocked.patch.mockRejectedValueOnce(networkError());
    await pushQueueOnce('edit');
    expect((await db.syncQueue.toArray())[0].nextAttemptAt).toBeDefined();

    await localUpdateTicket('ABC-000001', { priority: 'high' }, { priority: 'high' });
    const [item] = await db.syncQueue.toArray();
    expect(item.nextAttemptAt).toBeUndefined();
    mocked.patch.mockResolvedValueOnce({ data: ticketDto(1, { status: 'closed', priority: 'high' }) });
    await pushQueueOnce('edit');
    expect(mocked.patch).toHaveBeenLastCalledWith('/tickets/ABC-000001/', { status: 'closed', priority: 'high' });
    expect(await db.syncQueue.count()).toBe(0);
  });

  it('オンライン復帰（wakeQueue）で全項目の待ち時間を解除する', async () => {
    await localUpdateTicket('ABC-000001', { status: 'closed' }, { status: 'closed' });
    mocked.patch.mockRejectedValueOnce(networkError());
    await pushQueueOnce('interval');
    await wakeQueue();
    const [item] = await db.syncQueue.toArray();
    expect(item.nextAttemptAt).toBeUndefined();
    expect(item.attempts).toBe(1); // 回数はそのまま
  });
});

describe('古い変更で新しい値を上書きしない', () => {
  beforeEach(async () => {
    stopSync();
    registerPushRequester(() => undefined);
    openUserDb(nextUserId());
    Object.values(mocked).forEach((m) => m.mockReset());
    await db.tickets.put(ticketRow(1));
  });

  it('失敗扱いの更新があるチケットを編集すると、同じ項目にまとめて送り直す', async () => {
    await localUpdateTicket('ABC-000001', { status: 'in_progress' }, { status: 'in_progress' });
    const [failed] = await db.syncQueue.toArray();
    await db.syncQueue.update(failed.id!, { retryCount: MAX_RETRY, lastError: 'HTTP 500' });

    await localUpdateTicket('ABC-000001', { status: 'closed', priority: 'high' }, { status: 'closed', priority: 'high' });
    const items = await db.syncQueue.toArray();
    expect(items).toHaveLength(1);
    expect(items[0].retryCount).toBe(0);
    expect(JSON.parse(items[0].payload).patch).toEqual({ status: 'closed', priority: 'high' });
  });

  it('新しい更新が通ったら、残っている古い更新から同じ項目を取り除く（以前のバージョンで分かれて積まれた項目）', async () => {
    const oldId = await db.syncQueue.add({
      entity: 'ticket', entityId: 1, operation: 'update',
      payload: JSON.stringify({ key: 'ABC-000001', patch: { status: 'in_progress', title: '古いタイトル' } }),
      createdAt: '', retryCount: MAX_RETRY, lastError: 'HTTP 500',
    });
    const other = await db.syncQueue.add({
      entity: 'ticket', entityId: 1, operation: 'update',
      payload: JSON.stringify({ key: 'ABC-000001', patch: { status: 'closed' } }),
      createdAt: '', retryCount: 0,
    });
    mocked.patch.mockResolvedValueOnce({ data: ticketDto(1, { status: 'closed' }) });
    await pushEntityItem((await db.syncQueue.get(other))!);

    const old = await db.syncQueue.get(oldId);
    // status は新しい値で送信済みなので、古い status=in_progress を後から送り直さない
    expect(JSON.parse(old!.payload).patch).toEqual({ title: '古いタイトル' });

    // 古い項目が status だけなら項目ごと消える
    await db.syncQueue.update(oldId, { payload: JSON.stringify({ key: 'ABC-000001', patch: { status: 'in_progress' } }) });
    const again = await db.syncQueue.add({
      entity: 'ticket', entityId: 1, operation: 'update',
      payload: JSON.stringify({ key: 'ABC-000001', patch: { status: 'closed' } }),
      createdAt: '', retryCount: 0,
    });
    mocked.patch.mockResolvedValueOnce({ data: ticketDto(1, { status: 'closed' }) });
    await pushEntityItem((await db.syncQueue.get(again))!);
    expect(await db.syncQueue.get(oldId)).toBeUndefined();
  });

  it('破棄: 同じ行に他の送信待ちがあれば、その項目だけ捨てて行はそのまま', async () => {
    await db.tickets.put(ticketRow(1, { status: 'closed', priority: 'high' }, { _dirty: true }));
    const failedId = await db.syncQueue.add({
      entity: 'ticket', entityId: 1, operation: 'update',
      payload: JSON.stringify({ key: 'ABC-000001', patch: { status: 'closed' } }),
      createdAt: '', retryCount: MAX_RETRY,
    });
    await db.syncQueue.add({
      entity: 'ticket', entityId: 1, operation: 'update',
      payload: JSON.stringify({ key: 'ABC-000001', patch: { priority: 'high' } }),
      createdAt: '', retryCount: 0,
    });

    await discardFailedChange(failedId);
    expect(mocked.get).not.toHaveBeenCalled();
    expect(await db.syncQueue.count()).toBe(1);
    const row = await db.tickets.get(1);
    expect(row?._dirty).toBe(true);
    expect(row?.priority).toBe('high');
  });
});

describe('以前のバージョンで失敗扱いになった通信エラーの復帰', () => {
  beforeEach(() => {
    openUserDb(nextUserId());
  });

  it('Network Error・タイムアウトで失敗扱いの項目は送信待ちに戻す。HTTP エラーはそのまま', async () => {
    const base = { entity: 'ticket' as const, operation: 'update' as const, createdAt: '', retryCount: MAX_RETRY };
    const a = await db.syncQueue.add({ ...base, entityId: 1, payload: '{}', lastError: 'Network Error' });
    const b = await db.syncQueue.add({ ...base, entityId: 2, payload: '{}', lastError: 'timeout of 30000ms exceeded' });
    const c = await db.syncQueue.add({ ...base, entityId: 3, payload: '{}', lastError: 'HTTP 500' });

    expect(await reviveTransientFailures()).toBe(2);
    expect((await db.syncQueue.get(a))?.retryCount).toBe(0);
    expect((await db.syncQueue.get(b))?.retryCount).toBe(0);
    expect((await db.syncQueue.get(c))?.retryCount).toBe(MAX_RETRY);
  });
});
