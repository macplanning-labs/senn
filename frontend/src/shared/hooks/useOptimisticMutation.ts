/**
 * useOptimisticMutation.ts — 楽観的UI共通パターン
 *
 * TanStack Query の useMutation をラップし、
 * 0ms の体感速度を実現する楽観的更新パターンを提供。
 *
 * フロー:
 *   ① onMutate: QueryClient のキャッシュを即時書き換え（0ms）
 *   ② mutationFn: バックグラウンドで API 呼び出し
 *   ③-a onSuccess: キャッシュを invalidate（サーバーの真値で上書き）
 *   ③-b onError: キャッシュをロールバック + トースト通知
 */

import { useMutation, useQueryClient } from '@tanstack/react-query';
import type { QueryKey } from '@tanstack/react-query';
import { useToastStore } from '@/shared/stores/toastStore';

// ── 型定義 ──

interface OptimisticMutationOptions<TData, TVariables> {
  /** API呼び出し関数 */
  mutationFn: (variables: TVariables) => Promise<TData>;

  /** 楽観更新対象の QueryKey */
  queryKey: QueryKey;

  /**
   * キャッシュを楽観的に更新する関数。
   * 現在のキャッシュデータと mutation 変数を受け取り、新しいキャッシュデータを返す。
   * undefined を返すとキャッシュ更新をスキップ。
   */
  updater: (
    currentData: unknown,
    variables: TVariables,
  ) => unknown;

  /** 成功時のコールバック */
  onSuccessCallback?: (data: TData, variables: TVariables) => void;

  /** 成功時のトーストメッセージ（省略可） */
  successMessage?: string;

  /** エラー時のトーストメッセージ（省略可、デフォルト: '操作に失敗しました'） */
  errorMessage?: string;

  /** invalidate する追加の QueryKey 群 */
  invalidateKeys?: QueryKey[];
}

/**
 * 楽観的更新付き useMutation ラッパー
 *
 * @example
 * ```ts
 * const statusMutation = useOptimisticMutation({
 *   mutationFn: ({ id, status }) => apiClient.patch(`/tickets/${id}/`, { status }),
 *   queryKey: ['tickets', 'kanban', projectId],
 *   updater: (data, { id, status }) => ({
 *     ...data,
 *     results: data.results.map(t => t.id === id ? { ...t, status } : t),
 *   }),
 *   successMessage: 'ステータスを更新しました',
 * });
 * ```
 */
export function useOptimisticMutation<TData = unknown, TVariables = unknown>(
  options: OptimisticMutationOptions<TData, TVariables>,
) {
  const queryClient = useQueryClient();
  const { addToast } = useToastStore();

  return useMutation({
    mutationFn: options.mutationFn,

    // ① キャッシュを即時書き換え（0ms）
    onMutate: async (variables: TVariables) => {
      // 進行中のリフェッチをキャンセル（楽観更新が上書きされるのを防止）
      await queryClient.cancelQueries({ queryKey: options.queryKey });

      // 現在のキャッシュをスナップショット（ロールバック用）
      const previousData = queryClient.getQueryData(options.queryKey);

      // キャッシュを楽観的に更新
      if (previousData !== undefined) {
        const newData = options.updater(previousData, variables);
        if (newData !== undefined) {
          queryClient.setQueryData(options.queryKey, newData);
        }
      }

      // context としてスナップショットを返す（onError で使用）
      return { previousData };
    },

    // ③-b エラー時: ロールバック + トースト通知
    onError: (_error, _variables, context) => {
      // キャッシュをロールバック
      if (context?.previousData !== undefined) {
        queryClient.setQueryData(options.queryKey, context.previousData);
      }

      addToast({
        type: 'error',
        message: options.errorMessage ?? '操作に失敗しました。変更を元に戻しました。',
      });
    },

    // ③-a 成功時: サーバーの真値でキャッシュを上書き
    onSuccess: (data, variables) => {
      if (options.successMessage) {
        addToast({ type: 'success', message: options.successMessage });
      }
      options.onSuccessCallback?.(data, variables);
    },

    // 成功・失敗に関わらず、キャッシュを最新化
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: options.queryKey });

      // 追加の invalidate
      if (options.invalidateKeys) {
        for (const key of options.invalidateKeys) {
          void queryClient.invalidateQueries({ queryKey: key });
        }
      }
    },
  });
}
