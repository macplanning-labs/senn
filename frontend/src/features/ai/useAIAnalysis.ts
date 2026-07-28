/**
 * useAIAnalysis.ts — AI分析機能の React Query hooks
 *
 * - useContextAnalysis: チケットコンテキスト分析（TeamRule違反検出 + 実装アドバイス）
 * - useCloseAnalysis: チケットクローズ分析（技術的負債抽出 + バックログ候補生成）
 */
import { useMutation } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import type { ContextAnalysisResult, CloseAnalysisResult } from '@/shared/api/types';

export function useContextAnalysis() {
  return useMutation<ContextAnalysisResult, Error, number>({
    mutationFn: async (ticketId: number) => {
      const res = await apiClient.post<ContextAnalysisResult>(
        '/ai/context-analysis/',
        { ticket_id: ticketId },
      );
      return res.data;
    },
  });
}

export function useCloseAnalysis() {
  return useMutation<CloseAnalysisResult, Error, number>({
    mutationFn: async (ticketId: number) => {
      const res = await apiClient.post<CloseAnalysisResult>(
        '/ai/close-analysis/',
        { ticket_id: ticketId },
      );
      return res.data;
    },
  });
}
