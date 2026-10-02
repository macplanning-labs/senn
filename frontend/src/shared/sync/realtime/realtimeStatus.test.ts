/**
 * realtimeStatus.test.ts — リアルタイム接続状態のテスト
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import * as realtimeStatusModule from './realtimeStatus';

// Fake BroadcastChannel implementation for testing
class FakeBroadcastChannel {
  private static instances: Map<string, FakeBroadcastChannel[]> = new Map();
  name: string;
  private listeners: Array<(event: MessageEvent<unknown>) => void> = [];

  constructor(name: string) {
    this.name = name;
    if (!FakeBroadcastChannel.instances.has(name)) {
      FakeBroadcastChannel.instances.set(name, []);
    }
    FakeBroadcastChannel.instances.get(name)!.push(this);
  }

  postMessage(message: unknown): void {
    const instances = FakeBroadcastChannel.instances.get(this.name) || [];
    // Post to all OTHER instances (not self) asynchronously
    queueMicrotask(() => {
      instances.forEach((instance) => {
        if (instance !== this) {
          instance.listeners.forEach((listener) => {
            listener(new MessageEvent('message', { data: message }));
          });
        }
      });
    });
  }

  addEventListener(type: string, listener: (event: MessageEvent<unknown>) => void): void {
    if (type === 'message') {
      this.listeners.push(listener);
    }
  }

  removeEventListener(type: string, listener: (event: MessageEvent<unknown>) => void): void {
    if (type === 'message') {
      this.listeners = this.listeners.filter((l) => l !== listener);
    }
  }

  close(): void {
    const instances = FakeBroadcastChannel.instances.get(this.name) || [];
    const index = instances.indexOf(this);
    if (index >= 0) {
      instances.splice(index, 1);
    }
  }

  static reset(): void {
    FakeBroadcastChannel.instances.clear();
  }
}

// Helper to simulate multiple tabs by reloading the module with a fresh instance
async function getModuleInstance() {
  // Clear module cache and reload
  vi.resetModules();
  // Set up fake BroadcastChannel before importing
  if (typeof globalThis !== 'undefined') {
    (globalThis as unknown as { BroadcastChannel?: typeof FakeBroadcastChannel }).BroadcastChannel = FakeBroadcastChannel;
  }
  return import('./realtimeStatus');
}

describe('realtimeStatus', () => {
  beforeEach(() => {
    FakeBroadcastChannel.reset();
    if (typeof globalThis !== 'undefined') {
      (globalThis as unknown as { BroadcastChannel?: typeof FakeBroadcastChannel }).BroadcastChannel = FakeBroadcastChannel;
    }
  });

  afterEach(() => {
    FakeBroadcastChannel.reset();
    vi.resetModules();
    realtimeStatusModule.resetForTests();
  });

  it('should have connected state when local is connected', () => {
    realtimeStatusModule.setLocalConnected(true);
    expect(realtimeStatusModule.getEffectiveState()).toBe('connected');
  });

  it('should have disconnected state when local is disconnected', () => {
    realtimeStatusModule.setLocalConnected(false);
    expect(realtimeStatusModule.getEffectiveState()).toBe('disconnected');
  });

  it('(b) should sync to non-leader tab when leader connects', async () => {
    // Get two module instances (simulating two tabs)
    const tab1Module = await getModuleInstance();
    const tab2Module = await getModuleInstance();

    // Tab 1 is the leader, connects
    tab1Module.setLocalConnected(true);
    const stop1 = tab1Module.startStatusSharing(123);

    // Tab 2 is non-leader, joins later
    const stop2 = tab2Module.startStatusSharing(123);

    // Wait for async message delivery
    await new Promise((resolve) => setTimeout(resolve, 10));

    // Tab 2 should now know tab 1 is connected
    expect(tab2Module.getEffectiveState()).toBe('connected');

    stop1();
    stop2();
  });

  it('(c) should reply with status when hello is received', async () => {
    const tab1Module = await getModuleInstance();
    const tab2Module = await getModuleInstance();

    // Tab 1 connects first
    tab1Module.setLocalConnected(true);
    const stop1 = tab1Module.startStatusSharing(456);

    // Tab 2 joins after tab 1 is already connected
    const stop2 = tab2Module.startStatusSharing(456);

    // Wait for hello/reply exchange
    await new Promise((resolve) => setTimeout(resolve, 10));

    // Tab 2 should see connected
    expect(tab2Module.getEffectiveState()).toBe('connected');

    stop1();
    stop2();
  });

  it('(d) should disconnect when leader disconnects', async () => {
    const tab1Module = await getModuleInstance();
    const tab2Module = await getModuleInstance();

    // Both start, tab 1 connects
    tab1Module.setLocalConnected(true);
    const stop1 = tab1Module.startStatusSharing(789);
    const stop2 = tab2Module.startStatusSharing(789);

    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(tab2Module.getEffectiveState()).toBe('connected');

    // Tab 1 disconnects
    tab1Module.setLocalConnected(false);
    await new Promise((resolve) => setTimeout(resolve, 10));

    // Tab 2 should now be disconnected
    expect(tab2Module.getEffectiveState()).toBe('disconnected');

    stop1();
    stop2();
  });

  it('(e) should work without BroadcastChannel', () => {
    // Simulate BroadcastChannel being undefined
    const originalBC = (globalThis as unknown as { BroadcastChannel?: unknown }).BroadcastChannel;
    try {
      (globalThis as unknown as { BroadcastChannel?: unknown }).BroadcastChannel = undefined;

      realtimeStatusModule.setLocalConnected(true);
      expect(realtimeStatusModule.getEffectiveState()).toBe('connected');

      realtimeStatusModule.setLocalConnected(false);
      expect(realtimeStatusModule.getEffectiveState()).toBe('disconnected');

      // startStatusSharing should return a no-op function
      const stop = realtimeStatusModule.startStatusSharing(999);
      expect(() => stop()).not.toThrow();
    } finally {
      (globalThis as unknown as { BroadcastChannel?: unknown }).BroadcastChannel = originalBC;
    }
  });

  it('(f) should notify only on effective state change', async () => {
    const states: string[] = [];
    const unsubscribe = realtimeStatusModule.subscribeRealtimeState((state) => {
      states.push(state);
    });

    // Initial state is disconnected
    expect(states).toEqual([]);

    // Set local connected
    realtimeStatusModule.setLocalConnected(true);
    expect(states).toEqual(['connected']);

    // Set local connected again (should not notify)
    realtimeStatusModule.setLocalConnected(true);
    expect(states).toEqual(['connected']);

    // Set local disconnected
    realtimeStatusModule.setLocalConnected(false);
    expect(states).toEqual(['connected', 'disconnected']);

    // Set local disconnected again (should not notify)
    realtimeStatusModule.setLocalConnected(false);
    expect(states).toEqual(['connected', 'disconnected']);

    unsubscribe();
  });

  it('(g) should clear remote state when stopSharing is called', async () => {
    const tab1Module = await getModuleInstance();
    const tab2Module = await getModuleInstance();

    // Tab 1 connects
    tab1Module.setLocalConnected(true);
    const stop1 = tab1Module.startStatusSharing(999);
    const stop2 = tab2Module.startStatusSharing(999);

    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(tab2Module.getEffectiveState()).toBe('connected');

    // Tab 2 stops sharing
    stop2();

    // Tab 2 should now be disconnected
    expect(tab2Module.getEffectiveState()).toBe('disconnected');

    stop1();
  });
});

describe('代表タブが突然いなくなったとき（強制終了・クラッシュ）', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    FakeBroadcastChannel.reset();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('代表タブは接続中、定期的に状態を知らせる。途絶えたら、他のタブは一定時間後に「切れた」とみなす', async () => {
    const leader = await getModuleInstance();
    const stopLeader = leader.startStatusSharing(1);
    leader.setLocalConnected(true);
    const follower = await getModuleInstance();
    follower.startStatusSharing(1);
    await vi.advanceTimersByTimeAsync(50);
    expect(follower.getEffectiveState()).toBe('connected');

    // 代表タブが生きている間は、定期の知らせで「接続中」が保たれる（30秒を何度越えても）
    await vi.advanceTimersByTimeAsync(120_000);
    expect(follower.getEffectiveState()).toBe('connected');

    // 代表タブが、切断の連絡もなく消える（タイマーも止まる）
    stopLeader();
    // 通常の停止と違い、知らせが途絶えただけの状態を作る: 代表のタイマーを止め、切断の連絡は送らせない
    leader.resetForTests();
    await vi.advanceTimersByTimeAsync(40_000);
    expect(follower.getEffectiveState()).toBe('disconnected');
  });

  it('切断の連絡が届けば、待たずにすぐ「切れた」になる', async () => {
    const leader = await getModuleInstance();
    leader.startStatusSharing(1);
    leader.setLocalConnected(true);
    const follower = await getModuleInstance();
    follower.startStatusSharing(1);
    await vi.advanceTimersByTimeAsync(50);
    expect(follower.getEffectiveState()).toBe('connected');
    leader.setLocalConnected(false);
    await vi.advanceTimersByTimeAsync(50);
    expect(follower.getEffectiveState()).toBe('disconnected');
  });

  it('購読者は、期限切れによる変化でも通知される', async () => {
    const leader = await getModuleInstance();
    leader.startStatusSharing(1);
    leader.setLocalConnected(true);
    const follower = await getModuleInstance();
    follower.startStatusSharing(1);
    const seen: string[] = [];
    follower.subscribeRealtimeState((s) => seen.push(s));
    await vi.advanceTimersByTimeAsync(50);
    leader.resetForTests();
    await vi.advanceTimersByTimeAsync(40_000);
    expect(seen).toEqual(['connected', 'disconnected']);
  });
});

