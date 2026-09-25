// @vitest-environment jsdom
/**
 * syncEngine.test.ts — 同期の起動・停止と、実行の直列化（詳細設計 §3.8）
 */
import 'fake-indexeddb/auto';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('../api/client', () => {
  const apiClient = { get: vi.fn(), post: vi.fn(), patch: vi.fn(), put: vi.fn(), delete: vi.fn() };
  return { apiClient, default: apiClient };
});

import { apiClient } from '../api/client';
import { runCycle, startSync, stopSync } from './syncEngine';
import { useSyncStatus } from './syncStatusStore';
import { nextUserId, page } from './__tests__/testUtils';

const get = apiClient.get as unknown as ReturnType<typeof vi.fn>;

describe('syncEngine', () => {
  beforeEach(() => {
    get.mockReset();
    get.mockResolvedValue(page([]));
  });
  afterEach(() => stopSync());

  it('stopSync でリスナーと interval を外す', () => {
    const add = vi.spyOn(window, 'addEventListener');
    const remove = vi.spyOn(window, 'removeEventListener');
    startSync(nextUserId());
    const added = add.mock.calls.map((c) => c[0]);
    expect(added).toEqual(expect.arrayContaining(['focus', 'online']));
    stopSync();
    const removed = remove.mock.calls.map((c) => c[0]);
    expect(removed).toEqual(expect.arrayContaining(['focus', 'online']));
  });

  it('プロジェクト → チケットの順に取り、終わると初回同期済みになる', async () => {
    startSync(nextUserId());
    await vi.waitFor(() => expect(useSyncStatus.getState().initialSyncDone).toBe(true));
    const urls = get.mock.calls.map((c) => c[0]);
    expect(urls.indexOf('/sync/projects/')).toBeLessThan(urls.indexOf('/sync/tickets/'));
  });

  it('実行中に呼ばれても並列にならず、終わってからもう1回だけ回る', async () => {
    startSync(nextUserId());
    await vi.waitFor(() => expect(useSyncStatus.getState().syncing).toBe(false));
    get.mockClear();

    let inFlight = 0;
    let maxInFlight = 0;
    get.mockImplementation(async () => {
      inFlight++;
      maxInFlight = Math.max(maxInFlight, inFlight);
      await new Promise((r) => setTimeout(r, 5));
      inFlight--;
      return page([]);
    });
    await Promise.all([runCycle(), runCycle(), runCycle()]);
    await vi.waitFor(() => expect(useSyncStatus.getState().syncing).toBe(false));
    await new Promise((r) => setTimeout(r, 50));

    expect(maxInFlight).toBe(1);
    // 1回目（projects + tickets）＋ もう1回（projects + tickets）
    expect(get).toHaveBeenCalledTimes(4);
  });
});
