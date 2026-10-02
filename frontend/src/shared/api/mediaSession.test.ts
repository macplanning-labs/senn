/**
 * mediaSession.test.ts — メディアセッション管理のテスト
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import * as mediaSession from './mediaSession';
import * as client from './client';
import { DEMO_ACCESS_TOKEN } from '@/features/demo/demoMode';

// モック設定
vi.mock('./client', () => ({
  apiClient: {
    post: vi.fn(),
  },
  getAccessToken: vi.fn(),
}));

vi.mock('@/features/demo/demoMode', () => ({
  DEMO_ACCESS_TOKEN: 'demo-token',
}));

describe('ensureMediaSession', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mediaSession.stopMediaSessionKeepAlive();
  });

  afterEach(() => {
    vi.clearAllMocks();
    mediaSession.stopMediaSessionKeepAlive();
  });

  it('does nothing when token is not present', async () => {
    vi.spyOn(client, 'getAccessToken').mockReturnValue(null);
    const mockPost = vi.spyOn(client.apiClient, 'post');

    await mediaSession.ensureMediaSession();

    expect(mockPost).not.toHaveBeenCalled();
  });

  it('does nothing when using demo token', async () => {
    vi.spyOn(client, 'getAccessToken').mockReturnValue(DEMO_ACCESS_TOKEN);
    const mockPost = vi.spyOn(client.apiClient, 'post');

    await mediaSession.ensureMediaSession();

    expect(mockPost).not.toHaveBeenCalled();
  });

  it('posts to /media-session/ once in normal case', async () => {
    vi.spyOn(client, 'getAccessToken').mockReturnValue('valid-token');
    const mockPost = vi.spyOn(client.apiClient, 'post').mockResolvedValue({ status: 204 });

    await mediaSession.ensureMediaSession();

    expect(mockPost).toHaveBeenCalledOnce();
    expect(mockPost).toHaveBeenCalledWith('/media-session/');
  });

  it('does not throw even if POST fails', async () => {
    vi.spyOn(client, 'getAccessToken').mockReturnValue('valid-token');
    const mockPost = vi.spyOn(client.apiClient, 'post').mockRejectedValue(new Error('Network error'));

    // Should not throw
    await expect(mediaSession.ensureMediaSession()).resolves.toBeUndefined();
    expect(mockPost).toHaveBeenCalledOnce();
  });
});

describe('startMediaSessionKeepAlive', () => {
  let eventListeners: Map<string, Function[]>;

  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
    mediaSession.stopMediaSessionKeepAlive();

    // Mock window and document for Node environment
    eventListeners = new Map();

    const mockSetInterval = vi.fn((cb: () => void, ms: number) => {
      return setInterval(cb, ms) as any;
    });

    const mockClearInterval = vi.fn((id: ReturnType<typeof setInterval>) => {
      clearInterval(id);
    });

    const mockAddEventListener = vi.fn((event: string, handler: Function) => {
      if (!eventListeners.has(event)) {
        eventListeners.set(event, []);
      }
      eventListeners.get(event)!.push(handler);
    });

    const mockRemoveEventListener = vi.fn((event: string, handler: Function) => {
      const handlers = eventListeners.get(event);
      if (handlers) {
        const index = handlers.indexOf(handler);
        if (index > -1) {
          handlers.splice(index, 1);
        }
      }
    });

    global.window = {
      setInterval: mockSetInterval,
      clearInterval: mockClearInterval,
    } as any;

    global.document = {
      visibilityState: 'visible',
      addEventListener: mockAddEventListener,
      removeEventListener: mockRemoveEventListener,
      dispatchEvent: (event: Event) => {
        const handlers = eventListeners.get(event.type) || [];
        handlers.forEach((h) => h());
      },
    } as any;
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
    mediaSession.stopMediaSessionKeepAlive();

    delete (global as any).window;
    delete (global as any).document;
  });

  it('does not start twice (prevents double start)', () => {
    vi.spyOn(client, 'getAccessToken').mockReturnValue('valid-token');
    const mockPost = vi.spyOn(client.apiClient, 'post').mockResolvedValue({ status: 204 });

    const stop1 = mediaSession.startMediaSessionKeepAlive();
    const stop2 = mediaSession.startMediaSessionKeepAlive();

    // Should return the same stop function
    expect(stop1).toBe(stop2);

    // advance timers by 30 minutes
    vi.advanceTimersByTime(30 * 60 * 1000);

    // Should have called POST only once (not twice)
    expect(mockPost).toHaveBeenCalledTimes(1);

    stop1();
  });

  it('calls ensureMediaSession on interval', () => {
    vi.spyOn(client, 'getAccessToken').mockReturnValue('valid-token');
    const mockPost = vi.spyOn(client.apiClient, 'post').mockResolvedValue({ status: 204 });

    const stop = mediaSession.startMediaSessionKeepAlive();

    // Advance 30 minutes
    vi.advanceTimersByTime(30 * 60 * 1000);

    expect(mockPost).toHaveBeenCalledWith('/media-session/');

    stop();
  });

  it('respects document.visibilityState', () => {
    vi.spyOn(client, 'getAccessToken').mockReturnValue('valid-token');
    const mockPost = vi.spyOn(client.apiClient, 'post').mockResolvedValue({ status: 204 });

    // Set document.visibilityState to hidden
    (global.document as any).visibilityState = 'hidden';

    const stop = mediaSession.startMediaSessionKeepAlive();

    // Advance 30 minutes while tab is hidden
    vi.advanceTimersByTime(30 * 60 * 1000);

    // Should NOT have called POST while hidden
    expect(mockPost).not.toHaveBeenCalled();

    // Make tab visible
    (global.document as any).visibilityState = 'visible';

    // Advance another 30 minutes
    vi.advanceTimersByTime(30 * 60 * 1000);

    // Now it should have called POST
    expect(mockPost).toHaveBeenCalledOnce();

    stop();
  });

  it('registers visibilitychange listener', () => {
    vi.spyOn(client, 'getAccessToken').mockReturnValue('valid-token');
    vi.spyOn(client.apiClient, 'post').mockResolvedValue({ status: 204 });

    const stop = mediaSession.startMediaSessionKeepAlive();

    // Check that addEventListener was called for visibilitychange
    const mockAddEventListener = global.document.addEventListener as any;
    const calls = mockAddEventListener.mock.calls;

    const visibilitychangeCalls = calls.filter((call: any[]) => call[0] === 'visibilitychange');
    expect(visibilitychangeCalls.length).toBeGreaterThan(0);

    stop();

    // After stopping, removeEventListener should have been called
    const mockRemoveEventListener = global.document.removeEventListener as any;
    const removeCalls = mockRemoveEventListener.mock.calls;
    const visibilitychangeRemoveCalls = removeCalls.filter((call: any[]) => call[0] === 'visibilitychange');
    expect(visibilitychangeRemoveCalls.length).toBeGreaterThan(0);
  });
});

describe('ensureMediaSessionWithin', () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  it('returns after the timeout even if the request never finishes', async () => {
    vi.useFakeTimers();
    vi.spyOn(client, 'getAccessToken').mockReturnValue('valid-token');
    vi.spyOn(client.apiClient, 'post').mockReturnValue(new Promise(() => {}) as never);

    let done = false;
    const p = mediaSession.ensureMediaSessionWithin(3000).then(() => {
      done = true;
    });
    await vi.advanceTimersByTimeAsync(2999);
    expect(done).toBe(false);
    await vi.advanceTimersByTimeAsync(2);
    await p;
    expect(done).toBe(true);
  });

  it('returns as soon as the request finishes', async () => {
    vi.spyOn(client, 'getAccessToken').mockReturnValue('valid-token');
    vi.spyOn(client.apiClient, 'post').mockResolvedValue({ status: 204 });
    await mediaSession.ensureMediaSessionWithin(3000);
    expect(client.apiClient.post).toHaveBeenCalledOnce();
  });
});
