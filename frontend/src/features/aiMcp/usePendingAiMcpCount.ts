/** 承認待ちの「AI Agent を利用した MCP 操作の申請」の件数(システム管理者のバッジ用) */
import { useQuery } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { AI_MCP_PENDING_COUNT_KEY, type AiMcpAdminList } from './types';

export function usePendingAiMcpCount(enabled: boolean): number {
  const { data } = useQuery<number>({
    queryKey: AI_MCP_PENDING_COUNT_KEY,
    enabled,
    staleTime: 60_000,
    refetchInterval: 120_000,
    queryFn: async () => {
      const res = await apiClient.get<AiMcpAdminList>('/system-admin/ai-mcp-requests/', {
        params: { status: 'pending' },
      });
      return res.data.pendingCount;
    },
  });
  return enabled ? (data ?? 0) : 0;
}
