/** HTTP通信とリトライ機能 */

import {
  cloudflareAccessBlockMessage,
  isCloudflareAccessBlock,
} from "../cloudflare-access.js";

export interface FetchOptions {
  baseUrl: string;
  headers: Record<string, string>;
  timeout?: number;
  maxRetries?: number;
}

export class HttpError extends Error {
  statusCode: number;
  details: unknown;
  isRetryable: boolean;

  constructor(
    statusCode: number,
    message: string,
    details: unknown,
    isRetryable: boolean
  ) {
    super(message);
    this.name = "HttpError";
    this.statusCode = statusCode;
    this.details = details;
    this.isRetryable = isRetryable;
  }
}

/**
 * リトライ可能なエラーか判定
 * - 5xx（サーバーエラー）
 * - 429（レート制限）
 * - ネットワークエラー・タイムアウト
 */
function isRetryable(err: unknown, statusCode?: number): boolean {
  if (statusCode !== undefined) {
    return statusCode >= 500 || statusCode === 429;
  }
  // ネットワークエラー・タイムアウト
  if (err instanceof Error) {
    // Node の fetch は接続失敗を TypeError("fetch failed") で投げる(原因は err.cause)
    if (err.name === "AbortError" || err.name === "TimeoutError") return true;
    if (err instanceof TypeError && err.message.toLowerCase().includes("fetch failed")) {
      return true;
    }
    const message = err.message.toLowerCase();
    return (
      message.includes("network") ||
      message.includes("timeout") ||
      message.includes("aborted") ||
      message.includes("econnrefused") ||
      message.includes("econnreset")
    );
  }
  return false;
}

/**
 * 待機時間を計算（指数バックオフ: 1秒→2秒→4秒）
 */
function getBackoffMs(attemptNumber: number): number {
  return Math.pow(2, attemptNumber - 1) * 1000;
}

/**
 * JSON POST リクエストを実行（リトライ付き）
 */
export async function fetchJson<T>(
  path: string,
  body: unknown,
  options: FetchOptions
): Promise<T> {
  const url = new URL(path, options.baseUrl).href;
  const maxRetries = options.maxRetries ?? 3;
  const timeout = options.timeout ?? 30000;

  let lastError: HttpError | Error | null = null;

  for (let attempt = 1; attempt <= maxRetries; attempt++) {
    try {
      const controller = new AbortController();
      const timeoutId = setTimeout(() => controller.abort(), timeout);

      let response: Response;
      try {
        response = await fetch(url, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            ...options.headers,
          },
          body: JSON.stringify(body),
          signal: controller.signal,
        });
      } finally {
        clearTimeout(timeoutId);
      }

      if (!response.ok) {
        const text = await response.text().catch(() => "");
        let details: unknown;
        let isJson = true;
        try {
          details = JSON.parse(text);
        } catch {
          isJson = false;
          details = { message: response.statusText };
        }
        const retryable = isRetryable(null, response.status);
        // JSON でない応答が Cloudflare Access のものなら、原因が分かる文にする（DEMO-000116）
        const blockedByCloudflare =
          !isJson && isCloudflareAccessBlock(text, response.headers);
        const error = new HttpError(
          response.status,
          blockedByCloudflare
            ? cloudflareAccessBlockMessage(response.status)
            : `HTTP ${response.status}`,
          details,
          retryable
        );

        if (!error.isRetryable) {
          throw error;
        }

        lastError = error;
        if (attempt < maxRetries) {
          const waitMs = getBackoffMs(attempt);
          await new Promise((resolve) => setTimeout(resolve, waitMs));
          continue;
        }
        throw error;
      }

      const result = (await response.json()) as T;
      return result;
    } catch (err) {
      if (err instanceof HttpError) {
        throw err;
      }

      if (!isRetryable(err)) {
        throw err;
      }

      lastError = err instanceof Error ? err : new Error(String(err));
      if (attempt < maxRetries) {
        const waitMs = getBackoffMs(attempt);
        await new Promise((resolve) => setTimeout(resolve, waitMs));
        continue;
      }
      throw lastError;
    }
  }

  throw lastError || new Error("Unknown error");
}
