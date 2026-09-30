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
    // startSync 直後は syncing=false のままなので、それでは「開始サイクルの終了」を待てない。
    // 開始サイクル（非同期に始まる）が終わると lastSyncAt が入る。これを待ってから測り始める
    await vi.waitFor(() => expect(useSyncStatus.getState().lastSyncAt).not.toBeNull(), { timeout: 5000 });
    await vi.waitFor(() => expect(useSyncStatus.getState().syncing).toBe(false), { timeout: 5000 });
    get.mockClear();

    // 1回目の取得を止めておき、「1回目が実行中」の状態を確実に作る（時間待ちに頼らない）
    let release!: () => void;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    let inFlight = 0;
    let maxInFlight = 0;
    get.mockImplementation(async () => {
      inFlight++;
      maxInFlight = Math.max(maxInFlight, inFlight);
      await gate; // 解放後は、待たずに通る
      inFlight--;
      return page([]);
    });

    const first = runCycle();
    await vi.waitFor(() => expect(get).toHaveBeenCalledTimes(1), { timeout: 5000 });
    // 実行中に、さらに2回呼ぶ。並列には走らず、「もう1回」の予約になるだけ
    await Promise.all([runCycle(), runCycle()]);
    expect(get).toHaveBeenCalledTimes(1);

    release();
    await first; // 1回目が終わった時点で、予約された「もう1回」が始まっている

    // 「もう1回」が終わるのを、取得の回数で待つ（1回目 projects + tickets、もう1回 projects + tickets）
    await vi.waitFor(() => expect(get).toHaveBeenCalledTimes(4), { timeout: 5000 });
    await vi.waitFor(() => expect(useSyncStatus.getState().syncing).toBe(false), { timeout: 5000 });
    // 3回目が走らないこと（予約は1回に畳まれる）。多少待って、回数が増えていないことを確かめる
    await new Promise((r) => setTimeout(r, 50));

    expect(maxInFlight).toBe(1);
    expect(get).toHaveBeenCalledTimes(4);
  }, 15000);
});
