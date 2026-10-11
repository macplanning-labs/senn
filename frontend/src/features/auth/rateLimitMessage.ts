/**
 * rateLimitMessage.ts — ログイン・登録の「回数制限(429)」の文言(DEMO-000153)
 *
 * サーバーは 2 種類の 429 を返す。
 * - 試行ロック: `{"error":"account_locked","retry_after_seconds":N}`(同じアカウントで失敗が続いた)
 * - IP の率制限: `{"detail":"リクエストが多すぎます。…"}`(同じ IP から短時間に多すぎる)
 * どちらも「パスワードが違う」とは別の文言にし、待てばよいことを伝える。
 * サーバーの文言は日本語だけなので使わず、表示中の言語の文言を出す。
 */
import { isAxiosError } from 'axios';

type Translate = (key: string, options?: Record<string, unknown>) => string;

/** ロック解除までの分数(端数は切り上げ、最低 1 分)。秒数が分からなければ null */
export function lockMinutes(retryAfterSeconds: unknown): number | null {
  const n = typeof retryAfterSeconds === 'string' ? Number(retryAfterSeconds) : retryAfterSeconds;
  if (typeof n !== 'number' || !Number.isFinite(n) || n <= 0) return null;
  return Math.max(1, Math.ceil(n / 60));
}

/** 429 なら、表示する文言を返す。429 でなければ null(呼び出し側の既定の文言を使う) */
export function rateLimitMessage(err: unknown, t: Translate): string | null {
  if (!isAxiosError(err) || err.response?.status !== 429) return null;
  const data = err.response.data as { error?: string; retry_after_seconds?: unknown } | undefined;
  if (data?.error === 'account_locked') {
    const minutes =
      lockMinutes(data.retry_after_seconds) ?? lockMinutes(err.response.headers?.['retry-after']) ?? 15;
    return t('auth.accountLocked', { minutes });
  }
  return t('auth.tooManyRequests');
}
