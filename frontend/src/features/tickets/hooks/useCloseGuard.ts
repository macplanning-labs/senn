/**
 * useCloseGuard.ts — 「完了」にする直前のクローズ可否確認
 *
 * 「完了」(closed)への変更だけを対象に、バックエンドのルール判定を呼ぶ。助言のみで、
 * 利用者は必ず続行できる。確認が使えない(無効・失敗・タイムアウト)ときは何も出さず続行する。
 */

import { useCallback, useRef, useState } from 'react';
import type { CloseCheckResponse } from '@/shared/api/types';
import { getCloseWarning } from '../api/closeCheckApi';

export interface PendingClose {
  status: string;
  result: CloseCheckResponse;
}

export function useCloseGuard(
  ticketKey: string | undefined,
  onProceed: (status: string) => void,
) {
  // 確認中の連打ガード。state だと再描画前の2回目を止められないので ref を使う
  const inFlight = useRef(false);
  const [pending, setPending] = useState<PendingClose | null>(null);

  /** ステータス変更の入口。完了以外は即実行、完了は確認してから実行する */
  const request = useCallback(
    async (status: string) => {
      if (status !== 'closed') {
        onProceed(status);
        return;
      }
      if (inFlight.current || pending) return;
      inFlight.current = true;
      try {
        const warning = await getCloseWarning(ticketKey);
        if (warning) {
          setPending({ status, result: warning });
        } else {
          onProceed(status);
        }
      } finally {
        inFlight.current = false;
      }
    },
    [ticketKey, onProceed, pending],
  );

  /** 「それでも完了にする」 */
  const confirm = useCallback(() => {
    if (!pending) return;
    const { status } = pending;
    setPending(null);
    onProceed(status);
  }, [pending, onProceed]);

  /** 「戻って修正する」(ダイアログを閉じる) */
  const dismiss = useCallback(() => setPending(null), []);

  return { request, pending, confirm, dismiss };
}
