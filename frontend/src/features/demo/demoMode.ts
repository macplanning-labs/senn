/**
 * demoMode.ts — デモモード管理
 *
 * デモモードの有効化・無効化・フラグ確認を行う。
 * localStorage に保存。
 * TTL: 30分（超過時は自動 clear）
 */

const DEMO_MODE_KEY = 'senn_demo_mode';
const DEMO_ACCESS_TOKEN = 'senn-demo-token';
const DEMO_STARTED_AT_KEY = 'senn_demo_started_at';

// 30分 TTL
export const DEMO_TTL_MS = 30 * 60 * 1000;

export { DEMO_ACCESS_TOKEN };

export function isDemoMode(): boolean {
  const mode = localStorage.getItem(DEMO_MODE_KEY) === '1';
  if (!mode) return false;

  // TTL チェック
  const startedAtStr = localStorage.getItem(DEMO_STARTED_AT_KEY);
  if (!startedAtStr) return false;

  const startedAt = parseInt(startedAtStr, 10);
  if (isNaN(startedAt)) return false;

  const now = Date.now();
  if (now - startedAt > DEMO_TTL_MS) {
    // 期限切れ → clear
    clearDemoMode();
    return false;
  }

  return true;
}

export function enableDemoMode(): void {
  localStorage.setItem(DEMO_MODE_KEY, '1');
  localStorage.setItem(DEMO_STARTED_AT_KEY, Date.now().toString());
}

export function disableDemoMode(): void {
  localStorage.removeItem(DEMO_MODE_KEY);
  localStorage.removeItem(DEMO_STARTED_AT_KEY);
}

export function clearDemoMode(): void {
  disableDemoMode();
}
