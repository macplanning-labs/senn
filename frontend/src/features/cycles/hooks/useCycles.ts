/**
 * useCycles.ts — サイクル（スプリント管理）TanStack Query hooks
 */
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { useOptimisticMutation } from '@/shared/hooks/useOptimisticMutation';
import type { Cycle, VelocityData, CycleProgress, BurndownPoint } from '@/shared/api/types';

/** サイクル一覧を取得（project/team両対応） */
export function useCycles(projectId: number | undefined, teamId: number | undefined = undefined) {
  return useQuery<Cycle[]>({
    queryKey: ['cycles', projectId, teamId],
    queryFn: async () => {
      const params: Record<string, number> = {};
      if (projectId) params.project = projectId;
      if (teamId) params.team = teamId;
      const { data } = await apiClient.get('/cycles/', { params });
      return data;
    },
    enabled: !!(projectId || teamId),
  });
}

/** サイクル詳細を取得 */
export function useCycle(cycleId: number | undefined) {
  return useQuery<Cycle>({
    queryKey: ['cycle', cycleId],
    queryFn: async () => {
      const { data } = await apiClient.get(`/cycles/${cycleId}/`);
      return data;
    },
    enabled: !!cycleId,
  });
}

/** サイクル進捗を取得 */
export function useCycleProgress(cycleId: number | undefined) {
  return useQuery<CycleProgress>({
    queryKey: ['cycle-progress', cycleId],
    queryFn: async () => {
      const { data } = await apiClient.get(`/cycles/${cycleId}/progress/`);
      return data;
    },
    enabled: !!cycleId,
  });
}

/** ベロシティデータを取得 */
export function useVelocity(projectId: number | undefined) {
  return useQuery<VelocityData[]>({
    queryKey: ['velocity', projectId],
    queryFn: async () => {
      const { data } = await apiClient.get('/cycles/velocity/', {
        params: { project: projectId },
      });
      return data;
    },
    enabled: !!projectId,
  });
}

/** バーンダウンデータを取得 */
export function useBurndown(cycleId: number | undefined) {
  return useQuery<BurndownPoint[]>({
    queryKey: ['burndown', cycleId],
    queryFn: async () => {
      const { data } = await apiClient.get(`/cycles/${cycleId}/burndown/`);
      return data;
    },
    enabled: !!cycleId,
  });
}

/** サイクル作成（project/team両対応） */
export function useCreateCycle() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: {
      project?: number;
      team?: number;
      name: string;
      description?: string;
      start_date: string;
      end_date: string;
      status?: string;
      teamId?: number;
    }) => {
      const { data } = await apiClient.post('/cycles/', payload);
      return data as Cycle;
    },
    onSuccess: (_data, variables) => {
      if (variables.project) {
        void qc.invalidateQueries({ queryKey: ['cycles', variables.project] });
      }
      if (variables.team) {
        void qc.invalidateQueries({ queryKey: ['cycles', undefined, variables.team] });
      }
    },
  });
}

/**
 * サイクル更新（楽観的UI）
 *
 * projectId はフック生成時に固定（呼び出し元コンポーネントが把握している値）。
 * 一覧キャッシュ(['cycles', projectId])を主対象に即時反映しつつ、
 * additionalOptimisticUpdates で詳細キャッシュ(['cycle', id])も同時に更新する。
 * CycleList・CycleDetail のどちらから操作しても、もう一方の画面が
 * 古い表示のまま残らないようにするための設計（詳細設計書 §1.2 参照）。
 */
export function useUpdateCycle(projectId: number | undefined) {
  return useOptimisticMutation<Cycle, {
    id: number;
    project: number;
    name?: string;
    description?: string;
    start_date?: string;
    end_date?: string;
    status?: string;
    teamId?: number;
  }>({
    mutationFn: async ({ id, ...payload }) => {
      const { data } = await apiClient.patch(`/cycles/${id}/`, payload);
      return data as Cycle;
    },
    queryKey: ['cycles', projectId],
    updater: (currentData, variables) => {
      const cycles = currentData as Cycle[] | undefined;
      if (!cycles) return currentData;
      return cycles.map((c) => (c.id === variables.id ? { ...c, ...variables } : c));
    },
    additionalOptimisticUpdates: (variables) => [
      {
        queryKey: ['cycle', variables.id],
        updater: (currentData) => {
          const cycle = currentData as Cycle | undefined;
          if (!cycle) return currentData;
          return { ...cycle, ...variables };
        },
      },
    ],
    errorMessage: 'サイクルの更新に失敗しました。元に戻しました。',
  });
}

/**
 * サイクル完了（楽観的UI）
 *
 * projectId/teamId はフック生成時に固定。mutate() 時には cycleId のみ渡す。
 * 一覧・詳細の両キャッシュを同時に 'completed' へ即時反映する。
 * velocity は完了操作の結果に依存するため楽観更新はせず invalidate のみ行う。
 */
export function useCompleteCycle(projectId: number | undefined, teamId: number | undefined = undefined) {
  return useOptimisticMutation<unknown, {
    cycleId: number;
    carryOverTo?: number;
  }>({
    mutationFn: async ({ cycleId, carryOverTo }) => {
      const { data } = await apiClient.post(`/cycles/${cycleId}/complete/`, {
        carry_over_to: carryOverTo,
      });
      return data;
    },
    queryKey: ['cycles', projectId, teamId],
    updater: (currentData, variables) => {
      const cycles = currentData as Cycle[] | undefined;
      if (!cycles) return currentData;
      return cycles.map((c) =>
        c.id === variables.cycleId ? { ...c, status: 'completed' as const } : c,
      );
    },
    additionalOptimisticUpdates: (variables) => [
      {
        queryKey: ['cycle', variables.cycleId],
        updater: (currentData) => {
          const cycle = currentData as Cycle | undefined;
          if (!cycle) return currentData;
          return { ...cycle, status: 'completed' as const };
        },
      },
    ],
    invalidateKeys: projectId ? [['velocity', projectId]] : [],
    errorMessage: 'サイクルの完了に失敗しました。元に戻しました。',
  });
}

/** サイクル削除（project/team両対応） */
export function useDeleteCycle() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ id }: { id: number; projectId?: number; teamId?: number }) => {
      await apiClient.delete(`/cycles/${id}/`);
    },
    onSuccess: (_data, variables) => {
      if (variables.projectId) {
        void qc.invalidateQueries({ queryKey: ['cycles', variables.projectId] });
      }
      if (variables.teamId) {
        void qc.invalidateQueries({ queryKey: ['cycles', undefined, variables.teamId] });
      }
    },
  });
}
