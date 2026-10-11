/**
 * closeCheckApi.ts — チケットクローズ前の確認 API
 *
 * バックエンドの検証エンジンを呼び出し、クローズ可否を判定。
 * ブロッカーがある場合は警告を表示するが、ユーザーは続行可能。
 */

import { apiClient } from '@/shared/api/client';
import type { CloseCheckResponse } from '@/shared/api/types';

const CLOSE_CHECK_TIMEOUT_MS = 3000;

/**
 * チケットのクローズ可否を確認
 *
 * @param ticketKey - チケットキー (例: DEMO-000001)
 * @returns CloseCheckResponse | null (失敗時・タイムアウト時は null)
 */
export async function fetchCloseCheck(ticketKey: string): Promise<CloseCheckResponse | null> {
  try {
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), CLOSE_CHECK_TIMEOUT_MS);

    try {
      const response = await apiClient.get<CloseCheckResponse>(
        `/tickets/${ticketKey}/close-check/`,
        {
          signal: controller.signal,
        }
      );
      return response.data;
    } finally {
      clearTimeout(timeoutId);
    }
  } catch {
    // 失敗時・タイムアウト時は null を返す（呼び出し側が続行できるように）
    return null;
  }
}

/**
 * クローズ前に警告を表示すべきか判定
 *
 * @param res - fetchCloseCheck の結果
 * @returns true なら警告を表示すべき、false/null なら何もしない
 */
export function shouldWarnBeforeClose(res: CloseCheckResponse | null): boolean {
  // null（失敗・タイムアウト・ネットワークエラー）なら続行
  if (res === null) return false;

  // engine === "off" なら続行（検証エンジンが無効）
  if (res.engine === 'off') return false;

  // canClose === true でブロッカー空なら続行
  if (res.canClose === true && res.blockers.length === 0) return false;

  // ブロッカーがある かつ canClose === false なら警告
  if (res.canClose === false && res.blockers.length > 0) return true;

  // その他の場合（canClose === null など）は続行
  return false;
}

/**
 * 「完了」へ変更する直前の確認。警告が必要なときだけ結果を返し、それ以外は null。
 * null は「そのまま続行してよい」(エンジン無効・問題なし・失敗・タイムアウト・キー不明のすべて)。
 * この機能の失敗で完了操作を妨げないため、例外は投げない。
 */
export async function getCloseWarning(
  ticketKey: string | undefined,
): Promise<CloseCheckResponse | null> {
  if (!ticketKey) return null;
  const res = await fetchCloseCheck(ticketKey);
  return shouldWarnBeforeClose(res) ? res : null;
}
