/**
 * client.ts — axios HTTPクライアント
 *
 * JWT認証インターセプター付き。
 * アクセストークンの自動付与とリフレッシュ処理を一元管理。
 * デモモード対応：デモ時は fixture を返す。
 */

import axios, { AxiosError } from 'axios';
import type { InternalAxiosRequestConfig } from 'axios';
import { DEMO_ACCESS_TOKEN } from '@/features/demo/demoMode';

// API ベースURL（開発時はViteプロキシ経由、本番時は環境変数）
const API_BASE_URL = import.meta.env.VITE_API_BASE_URL ?? '';

/** axios インスタンス */
export const apiClient = axios.create({
  baseURL: `${API_BASE_URL}/api/v1`,
  timeout: 30000,
  headers: {
    'Content-Type': 'application/json',
  },
});

// ─── トークン管理 ───────────────────────────────
const TOKEN_KEY = 'wip_access_token';
const REFRESH_KEY = 'wip_refresh_token';

export function getAccessToken(): string | null {
  return localStorage.getItem(TOKEN_KEY);
}

export function setTokens(access: string, refresh: string): void {
  localStorage.setItem(TOKEN_KEY, access);
  localStorage.setItem(REFRESH_KEY, refresh);
}

export function clearTokens(): void {
  localStorage.removeItem(TOKEN_KEY);
  localStorage.removeItem(REFRESH_KEY);
}

export function getRefreshToken(): string | null {
  return localStorage.getItem(REFRESH_KEY);
}

/**
 * 「ログインそのもの」の API か。ここでの 401 は「期限切れ」ではなく「認証の失敗」
 * (資格情報が違う・SSO の情報が届かない)なので、リフレッシュ処理には入れず、そのままエラーとして返す。
 * 入れてしまうと、リフレッシュトークンが無い状態で「更新中」の印が立ったまま戻らず、
 * 以降の 401 が永久に待たされる(ログイン画面のスピナーが消えない。DEMO-000179)。
 */
const AUTH_FAILURE_PATHS = [
  '/auth/login/',
  '/auth/sso/login/',
  '/auth/login/verify/',
  '/auth/token/refresh/',
  '/auth/passkey/login/begin/',
  '/auth/passkey/login/complete/',
];

export function isAuthFailureRequest(url: string | undefined): boolean {
  if (!url) return false;
  const path = url.split('?')[0] ?? '';
  return AUTH_FAILURE_PATHS.some((p) => path.endsWith(p));
}

/** 認証ページにいるときはフルリロードしない（401 ループ防止） */
function redirectToLoginIfNeeded(): void {
  const path = window.location.pathname;
  if (
    path === '/login' ||
    path === '/demo' ||
    path === '/register' ||
    path === '/forgot-password' ||
    path === '/reset-password'
  ) {
    return;
  }
  window.location.href = '/login';
}

// ─── リクエストインターセプター（トークン自動付与 + デモ対応） ───
apiClient.interceptors.request.use(
  async (config: InternalAxiosRequestConfig) => {
    const token = getAccessToken();
    if (token && config.headers) {
      config.headers.Authorization = `Bearer ${token}`;
    }
    // FormData送信時はContent-Typeを外す（JSON化されてファイルが消えるのを防ぐ）
    if (config.data instanceof FormData) {
      config.headers.delete('Content-Type');
    }

    // デモモード時：本番 API へは一切出さない（アダプターのみ）
    if (token === DEMO_ACCESS_TOKEN) {
      const { handleDemoRequest } = await import('@/features/demo/demoApiAdapter');
      const demoResponse = handleDemoRequest({
        method: (config.method || 'get').toLowerCase() as
          | 'get'
          | 'post'
          | 'patch'
          | 'put'
          | 'delete',
        url: config.url || '',
        data: config.data,
      }) ?? { status: 200, data: { results: [] } };

      const errorData =
        demoResponse.data && typeof demoResponse.data === 'object'
          ? (demoResponse.data as Record<string, unknown>)
          : {};

      return {
        ...config,
        adapter: async () => {
          if (demoResponse.status >= 400) {
            throw new AxiosError(
              (typeof errorData.detail === 'string' && errorData.detail) || 'Demo mode error',
              String(demoResponse.status),
              config,
              undefined,
              {
                status: demoResponse.status,
                statusText: 'Error',
                headers: {},
                config,
                data: demoResponse.data,
              },
            );
          }
          return {
            data: demoResponse.data,
            status: demoResponse.status,
            statusText: 'OK',
            headers: {},
            config,
            request: {},
          };
        },
      } as InternalAxiosRequestConfig;
    }

    return config;
  },
  (error: AxiosError) => Promise.reject(error),
);

// ─── レスポンスインターセプター（401時にリフレッシュ） ───
let isRefreshing = false;
let failedQueue: Array<{
  resolve: (token: string) => void;
  reject: (error: unknown) => void;
}> = [];

function processQueue(error: unknown, token: string | null): void {
  failedQueue.forEach((prom) => {
    if (token) {
      prom.resolve(token);
    } else {
      prom.reject(error);
    }
  });
  failedQueue = [];
}

apiClient.interceptors.response.use(
  (response) => response,
  async (error: AxiosError) => {
    const originalRequest = error.config;
    const token = getAccessToken();

    // デモトークンの場合、リフレッシュロジックをスキップ
    if (token === DEMO_ACCESS_TOKEN) {
      return Promise.reject(error);
    }

    // ログインそのものの 401 は、認証の失敗。リフレッシュ処理には入れず、そのまま返す
    if (error.response?.status === 401 && isAuthFailureRequest(originalRequest?.url)) {
      return Promise.reject(error);
    }

    // 401かつリフレッシュ未実行の場合
    if (
      error.response?.status === 401 &&
      originalRequest &&
      !('_retry' in originalRequest)
    ) {
      if (isRefreshing) {
        // 既にリフレッシュ中なら待機キューに追加
        return new Promise((resolve, reject) => {
          failedQueue.push({
            resolve: (token: string) => {
              if (originalRequest.headers) {
                originalRequest.headers.Authorization = `Bearer ${token}`;
              }
              resolve(apiClient(originalRequest));
            },
            reject,
          });
        });
      }

      (originalRequest as unknown as Record<string, unknown>)._retry = true;
      isRefreshing = true;

      const refreshToken = getRefreshToken();
      if (!refreshToken) {
        // 「更新中」の印を戻し、すでに待っている要求も、エラーで終わらせる(戻さないと、以降の 401 が永久に待たされる)
        isRefreshing = false;
        processQueue(error, null);
        clearTokens();
        redirectToLoginIfNeeded();
        return Promise.reject(error);
      }

      try {
        // バックエンドはリフレッシュのたびにリフレッシュトークンをローテーションし
        // (古いものはブラックリスト登録)、新しいペアを返す。ここで新しいrefreshを
        // 保存し損ねると、次回以降のリフレッシュが「無効化済みトークン」で401になり続ける
        // (DEMO-000047)。
        const { data } = await axios.post<{ access: string; refresh: string }>(
          `${API_BASE_URL}/api/v1/auth/token/refresh/`,
          { refresh: refreshToken },
        );
        setTokens(data.access, data.refresh);
        processQueue(null, data.access);

        if (originalRequest.headers) {
          originalRequest.headers.Authorization = `Bearer ${data.access}`;
        }
        return apiClient(originalRequest);
      } catch (refreshError) {
        processQueue(refreshError, null);
        // 429(更新の回数制限。DEMO-000153)は、トークンが無効になったわけではない。
        // ここでトークンを消すとログアウトしてしまうので、消さずにエラーだけ返す(次の 401 で、もう一度更新する)
        if (axios.isAxiosError(refreshError) && refreshError.response?.status === 429) {
          return Promise.reject(refreshError);
        }
        clearTokens();
        redirectToLoginIfNeeded();
        return Promise.reject(refreshError);
      } finally {
        isRefreshing = false;
      }
    }

    return Promise.reject(error);
  },
);

export default apiClient;
