/**
 * useDependencyGraph.ts — 依存関係フロー TanStack Query hooks
 */
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import type { DependencyGraph, TaskDependency } from '@/shared/api/types';

/** プロジェクトまたはチームの依存関係グラフ(ノード+エッジ)一括取得 */
export function useDependencyGraph(
  projectId: number | undefined,
  teamId: number | undefined = undefined,
) {
  const endpoint = teamId ? `/teams/${teamId}/dependencies/` : `/projects/${projectId}/dependencies/`;
  const queryKey = ['dependency-graph', projectId, teamId];

  return useQuery<DependencyGraph>({
    queryKey,
    queryFn: async () => {
      const { data } = await apiClient.get(endpoint);
      return data as DependencyGraph;
    },
    enabled: !!(projectId || teamId),
  });
}

/** 依存関係を作成(ドラッグ接続時) */
export function useCreateDependency(
  projectId: number | undefined,
  teamId: number | undefined = undefined,
) {
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
      void qc.invalidateQueries({ queryKey: ['dependency-graph', projectId, teamId] });
    },
  });
}

/** 依存関係を削除(エッジクリック時) */
export function useDeleteDependency(
  projectId: number | undefined,
  teamId: number | undefined = undefined,
) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { ticketKey: string; dependencyId: number }) => {
      await apiClient.delete(`/tickets/${payload.ticketKey}/dependencies/${payload.dependencyId}/`);
    },
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ['dependency-graph', projectId, teamId] });
    },
  });
}

/** Cycle枠のドラッグ位置を保存(依存関係グラフ専用の軽量エンドポイント) */
export function useUpdateCycleGraphPosition() {
  return useMutation({
    mutationFn: async (payload: { cycleId: number; x: number; y: number }) => {
      await apiClient.patch(`/cycles/${payload.cycleId}/graph-position/`, {
        x: payload.x,
        y: payload.y,
      });
    },
  });
}
