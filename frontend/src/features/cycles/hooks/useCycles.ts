/**
 * useCycles.ts — サイクル（スプリント管理）TanStack Query hooks
 */
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import type { Cycle, VelocityData } from '@/shared/api/types';

/** サイクル一覧を取得 */
export function useCycles(projectId: number | undefined) {
  return useQuery<Cycle[]>({
    queryKey: ['cycles', projectId],
    queryFn: async () => {
      const { data } = await apiClient.get('/cycles/', {
        params: { project: projectId },
      });
      return data;
    },
    enabled: !!projectId,
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
  return useQuery({
    queryKey: ['cycle-progress', cycleId],
    queryFn: async () => {
      const { data } = await apiClient.get(`/cycles/${cycleId}/progress/`);
      return data as {
        ticketCount: number;
        completedCount: number;
        inProgressCount: number;
        totalPoints: number;
        completedPoints: number;
        completionRate: number;
      };
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

/** サイクル作成 */
export function useCreateCycle() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: {
      project: number;
      name: string;
      start_date: string;
      end_date: string;
      status?: string;
    }) => {
      const { data } = await apiClient.post('/cycles/', payload);
      return data as Cycle;
    },
    onSuccess: (_data, variables) => {
      void qc.invalidateQueries({ queryKey: ['cycles', variables.project] });
    },
  });
}

/** サイクル更新 */
export function useUpdateCycle() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, ...payload }: {
      id: number;
      project: number;
      name?: string;
      start_date?: string;
      end_date?: string;
      status?: string;
    }) => {
      const { data } = await apiClient.patch(`/cycles/${id}/`, payload);
      return data as Cycle;
    },
    onSuccess: (_data, variables) => {
      void qc.invalidateQueries({ queryKey: ['cycles', variables.project] });
      void qc.invalidateQueries({ queryKey: ['cycle', variables.id] });
    },
  });
}

/** サイクル完了 */
export function useCompleteCycle() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ cycleId, carryOverTo }: {
      cycleId: number;
      projectId: number;
      carryOverTo?: number;
    }) => {
      const { data } = await apiClient.post(`/cycles/${cycleId}/complete/`, {
        carry_over_to: carryOverTo,
      });
      return data;
    },
    onSuccess: (_data, variables) => {
      void qc.invalidateQueries({ queryKey: ['cycles', variables.projectId] });
      void qc.invalidateQueries({ queryKey: ['velocity', variables.projectId] });
    },
  });
}

/** サイクル削除 */
export function useDeleteCycle() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, projectId: _projectId }: { id: number; projectId: number }) => {
      await apiClient.delete(`/cycles/${id}/`);
    },
    onSuccess: (_data, variables) => {
      void qc.invalidateQueries({ queryKey: ['cycles', variables.projectId] });
    },
  });
}
