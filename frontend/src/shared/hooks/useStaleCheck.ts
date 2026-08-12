/**
 * useStaleCheck — LWW上書き検出フック
 *
 * チケット詳細表示中に、他のユーザーがそのチケットを更新したかを
 * ポーリングで検出し、トースト通知を表示する。
 *
 * 仕組み:
 *   1. 5秒間隔で /tickets/{id}/ の updatedAt を取得
 *   2. 手元のキャッシュより新しければ「更新されました」トースト表示
 *   3. TanStack Query の invalidateQueries で自動リフェッチ
 */
import { useEffect, useRef } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useToastStore } from '../stores/toastStore';
import { apiClient } from '../api/client';

interface UseStaleCheckOptions {
  /** チケットID */
  ticketId: number | null;
  /** 現在のupdatedAt（ISO文字列） */
  updatedAt: string | null;
  /** ポーリング間隔（ミリ秒）。デフォルト 5000 */
  intervalMs?: number;
  /** 有効化フラグ */
  enabled?: boolean;
}

export function useStaleCheck({
  ticketId,
  updatedAt,
  intervalMs = 5000,
  enabled = true,
}: UseStaleCheckOptions) {
  const queryClient = useQueryClient();
  const addToast = useToastStore((s) => s.addToast);
  const lastKnownRef = useRef<string | null>(updatedAt);

  // updatedAt が変わったら追跡を更新
  useEffect(() => {
    lastKnownRef.current = updatedAt;
  }, [updatedAt]);

  useEffect(() => {
    if (!enabled || !ticketId || !updatedAt) return;

    const timer = setInterval(async () => {
      try {
        const res = await apiClient.head(`/tickets/${ticketId}/`);
        const serverUpdatedAt = res.headers?.['x-updated-at'] as string | undefined;

        if (
          serverUpdatedAt &&
          lastKnownRef.current &&
          new Date(serverUpdatedAt) > new Date(lastKnownRef.current)
        ) {
          // 他のユーザーが更新した
          addToast({
            type: 'info',
            message: 'このチケットが他のユーザーにより更新されました。最新データに更新します。',
          });

          // キャッシュを無効化してリフェッチ
          void queryClient.invalidateQueries({
            queryKey: ['ticket', ticketId],
          });

          // 次回の比較用に更新
          lastKnownRef.current = serverUpdatedAt;
        }
      } catch {
        // ネットワークエラーは無視（次回ポーリングで再試行）
      }
    }, intervalMs);

    return () => clearInterval(timer);
  }, [ticketId, enabled, intervalMs, queryClient, addToast, updatedAt]);
}
