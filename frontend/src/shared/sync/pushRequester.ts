/**
 * pushRequester.ts — 書き込み直後に「送信して」と頼むための小さな窓口
 *
 * ticketWrites / projectWrites → syncEngine の直接 import は循環するため、
 * syncEngine が起動時に実体を登録し、書き込み側はここを呼ぶ。
 */

let requester: (() => void) | null = null;

export function registerPushRequester(fn: () => void): void {
  requester = fn;
}

export function requestPush(): void {
  requester?.();
}
