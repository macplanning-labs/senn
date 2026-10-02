/**
 * mediaSession.ts — メディア認証セッション管理
 *
 * メディア専用Cookie（senn_media、HttpOnly、Path=/media、有効60分）の保持。
 * POST /api/v1/media-session/ を定期的に呼び出し、Cookie を更新・維持する。
 * 画像・添付ファイルのURLは変更しない。
 */

import { apiClient, getAccessToken } from './client';
import { DEMO_ACCESS_TOKEN } from '@/features/demo/demoMode';

let keepAliveIntervalId: number | null = null;
let lastEnsureTime: number = 0;

/**
 * メディアセッション確保
 *
 * - トークン無しまたはデモトークンの場合は何もしない
 * - 失敗しても例外を投げない（console.warn のみ）
 */
export async function ensureMediaSession(): Promise<void> {
  const token = getAccessToken();

  // トークンが無い、またはデモトークンの場合は何もしない
  if (!token || token === DEMO_ACCESS_TOKEN) {
    return;
  }

  try {
    await apiClient.post('/media-session/');
    lastEnsureTime = Date.now();
  } catch (error) {
    console.warn('[mediaSession] ensureMediaSession failed:', error);
    // 失敗しても画像が読めなくなるだけで、アプリは動く
  }
}

/**
 * 最大 timeoutMs だけ待って ensureMediaSession() を行う。
 * 起動時、最初の画面の画像が Cookie 発行より先に読み込まれて 401 になるのを避けるために使う。
 * 遅い/失敗しても、待ちきらずに先へ進む(画像が読めないだけで、アプリは動く)。
 */
export async function ensureMediaSessionWithin(timeoutMs: number): Promise<void> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    await Promise.race([
      ensureMediaSession(),
      new Promise<void>((resolve) => {
        timer = setTimeout(resolve, timeoutMs);
      }),
    ]);
  } finally {
    if (timer !== undefined) clearTimeout(timer);
  }
}

let visibilityChangeHandler: (() => void) | null = null;

/**
 * メディアセッション保持を開始
 *
 * - 30分ごとに ensureMediaSession() を呼ぶ
 * - 二重起動しない（すでに動いていれば同じ停止関数を返す）
 * - タブが非表示中は呼ばない
 * - 再表示時（visibilitychange）に前回から25分以上経っていれば即呼ぶ
 */
export function startMediaSessionKeepAlive(): () => void {
  // 既に動いていれば停止関数を返す（二重起動防止）
  if (keepAliveIntervalId !== null) {
    return stopMediaSessionKeepAlive;
  }

  const KEEP_ALIVE_INTERVAL = 30 * 60 * 1000; // 30分
  const VISIBILITY_THRESHOLD = 25 * 60 * 1000; // 25分

  /**
   * タブが表示中の場合のみ呼び出し
   */
  async function ensureIfVisible(): Promise<void> {
    if (document.visibilityState === 'visible') {
      await ensureMediaSession();
    }
  }

  /**
   * 再表示時のハンドラ
   * 前回から25分以上経っていれば即呼ぶ
   */
  function handleVisibilityChange(): void {
    if (document.visibilityState === 'visible') {
      const now = Date.now();
      if (now - lastEnsureTime >= VISIBILITY_THRESHOLD) {
        void ensureIfVisible();
      }
    }
  }

  // 30分ごとにチェック
  keepAliveIntervalId = window.setInterval(() => {
    void ensureIfVisible();
  }, KEEP_ALIVE_INTERVAL);

  // タブの表示・非表示イベントをリッスン
  visibilityChangeHandler = handleVisibilityChange;
  document.addEventListener('visibilitychange', handleVisibilityChange);

  // 停止関数を返す
  return stopMediaSessionKeepAlive;
}

/**
 * メディアセッション保持を停止
 */
export function stopMediaSessionKeepAlive(): void {
  if (keepAliveIntervalId !== null) {
    clearInterval(keepAliveIntervalId);
    keepAliveIntervalId = null;
  }
  if (visibilityChangeHandler !== null) {
    document.removeEventListener('visibilitychange', visibilityChangeHandler);
    visibilityChangeHandler = null;
  }
}
