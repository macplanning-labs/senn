/**
 * client.test.ts — 401 の処理(リフレッシュ)のテスト
 *
 * 不具合(DEMO-000179): リフレッシュトークンが無い状態で最初の 401 を受けると、「更新中」の印が戻らず、
 * 以降の 401 が永久に待たされた(ログイン画面のスピナーが消えない)。
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { AxiosError } from 'axios';

const store = new Map<string, string>();
vi.stubGlobal('localStorage', {
  getItem: (k: string) => store.get(k) ?? null,
  setItem: (k: string, v: string) => void store.set(k, v),
  removeItem: (k: string) => void store.delete(k),
});
vi.stubGlobal('window', { location: { pathname: '/login', href: '' } });

async function load() {
  return await import('./client');
}

/** 常に 401 を返す擬似のサーバー */
function always401(apiClient: { defaults: { adapter?: unknown } }) {
  apiClient.defaults.adapter = async (config: never) => {
    throw new AxiosError('Unauthorized', 'ERR_BAD_REQUEST', config, undefined, {
      status: 401,
      statusText: 'Unauthorized',
      headers: {},
      config,
      data: { detail: '認証情報が正しくありません' },
    });
  };
}

/** 待たされ続けて(解決しないで)いないかを見る */
async function settle(p: Promise<unknown>): Promise<'rejected' | 'resolved' | 'hung'> {
  return await Promise.race([
    p.then(
      () => 'resolved' as const,
      () => 'rejected' as const,
    ),
    new Promise<'hung'>((r) => setTimeout(() => r('hung'), 400)),
  ]);
}

beforeEach(() => {
  store.clear();
});

describe('isAuthFailureRequest', () => {
  it('ログインそのものの API を判定する', async () => {
    const { isAuthFailureRequest } = await load();
    for (const u of ['/auth/login/', '/auth/sso/login/', '/auth/login/verify/', '/auth/token/refresh/', '/auth/passkey/login/begin/', '/auth/passkey/login/complete/']) {
      expect(isAuthFailureRequest(u), u).toBe(true);
    }
    expect(isAuthFailureRequest('/auth/sso/login/?x=1')).toBe(true);
    expect(isAuthFailureRequest('/tickets/')).toBe(false);
    expect(isAuthFailureRequest('/auth/me/')).toBe(false);
    expect(isAuthFailureRequest(undefined)).toBe(false);
  });
});

describe('401 の処理', () => {
  it('SSO ログインが 401 を 2 回返しても、2 回とも待たされずにエラーで終わる', async () => {
    const { apiClient } = await load();
    always401(apiClient);
    expect(await settle(apiClient.post('/auth/sso/login/', {}))).toBe('rejected');
    // 1 回目のあとで、「更新中」のまま固まらない
    expect(await settle(apiClient.post('/auth/sso/login/', {}))).toBe('rejected');
  });

  it('パスワードの誤り(/auth/login/ の 401)も、何度でもエラーで終わる', async () => {
    const { apiClient } = await load();
    always401(apiClient);
    for (let i = 0; i < 3; i++) {
      expect(await settle(apiClient.post('/auth/login/', { username: 'a', password: 'b' }))).toBe('rejected');
    }
  });

  it('パスキーのログインが 401(別の ID のパスキーなど)でも、何度でもエラーで終わる', async () => {
    const { apiClient } = await load();
    always401(apiClient);
    for (let i = 0; i < 3; i++) {
      expect(await settle(apiClient.post('/auth/passkey/login/complete/', {}))).toBe('rejected');
    }
  });

  it('リフレッシュトークンが無いまま、ふつうの API が 401 を続けても、固まらない', async () => {
    const { apiClient } = await load();
    always401(apiClient);
    expect(await settle(apiClient.get('/tickets/'))).toBe('rejected');
    expect(await settle(apiClient.get('/tickets/'))).toBe('rejected');
    expect(await settle(apiClient.get('/projects/'))).toBe('rejected');
  });
});
