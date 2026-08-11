/**
 * useDependencyGraph.ts — 依存関係フロー TanStack Query hooks
 */
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import type { DependencyGraph, TaskDependency } from '@/shared/api/types';

/** プロジェクトの依存関係グラフ(ノード+エッジ)一括取得 */
export function useDependencyGraph(projectId: number | undefined) {
  return useQuery<DependencyGraph>({
    queryKey: ['dependency-graph', projectId],
    queryFn: async () => {
      const { data } = await apiClient.get(`/projects/${projectId}/dependencies/`);
      return data as DependencyGraph;
    },
    enabled: !!projectId,
  });
}

/** 依存関係を作成(ドラッグ接続時) */
export function useCreateDependency(projectId: number | undefined) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: {
      fromTicketKey: string;
      toTaskId: number;
      dependencyType?: string;
    }) => {
      const { data } = await apiClient.post<TaskDependency>(
        `/tickets/${payload.fromTicketKey}/dependencies/`,
        {
          to_task: payload.toTaskId,
          dependency_type: payload.dependencyType ?? 'blocks',
        },
      );
      return data;
    },
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ['dependency-graph', projectId] });
    },
  });
}

/** 依存関係を削除(エッジクリック時) */
export function useDeleteDependency(projectId: number | undefined) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { ticketKey: string; dependencyId: number }) => {
      await apiClient.delete(`/tickets/${payload.ticketKey}/dependencies/${payload.dependencyId}/`);
    },
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ['dependency-graph', projectId] });
    },
  });
}
