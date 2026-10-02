import { describe, it, expect, vi, beforeEach } from 'vitest';

// DEMO-000166: アカウントが切り替わるとき(ログアウト・新しいログイン)に、画面のキャッシュを消すこと。
// 消さないと、前の利用者のチーム名などが次の利用者に見える
const clearCachedQueries = vi.fn();
vi.mock('@/shared/sync/pull', () => ({ clearCachedQueries }));
vi.mock('@/shared/sync/syncEngine', () => ({
  prepareLogout: vi.fn(async () => ({ pending: 0 })),
  stopSync: vi.fn(),
}));
vi.mock('@/shared/sync/db', () => ({ deleteCurrentUserDb: vi.fn(async () => {}) }));
vi.mock('@/features/demo/demoMode', () => ({
  enableDemoMode: vi.fn(),
  clearDemoMode: vi.fn(),
  DEMO_ACCESS_TOKEN: 'demo',
}));
vi.mock('@/shared/api/mediaSession', () => ({
  ensureMediaSession: vi.fn(async () => {}),
  ensureMediaSessionWithin: vi.fn(async () => {}),
  startMediaSessionKeepAlive: vi.fn(),
  stopMediaSessionKeepAlive: vi.fn(),
}));
vi.mock('@/shared/api/client', () => ({
  apiClient: {
    post: vi.fn(async () => ({ data: {} })),
    get: vi.fn(async () => ({ data: { id: 2, username: 'next' } })),
  },
  setTokens: vi.fn(),
  clearTokens: vi.fn(),
  getAccessToken: vi.fn(() => null),
  getRefreshToken: vi.fn(() => 'r'),
}));

import { useAuthStore } from './authStore';

describe('authStore: アカウントの切り替えで画面のキャッシュを消す', () => {
  beforeEach(() => {
    clearCachedQueries.mockClear();
  });

  it('ログアウトで消す', async () => {
    await useAuthStore.getState().logout({ force: true });
    expect(clearCachedQueries).toHaveBeenCalled();
  });

  it('発行済みのトークンでのログイン(新規登録・招待の受諾・パスキー)で消す', async () => {
    await useAuthStore.getState().loginWithPasskey('a', 'r');
    expect(clearCachedQueries).toHaveBeenCalled();
  });
});
