/**
 * useWorkflowStatuses.ts — ワークフローステータス TanStack Query hooks
 */
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import type { WorkflowStatus } from '@/shared/api/types';

export function useWorkflowStatuses(projectId?: number, teamId?: number) {
  return useQuery<WorkflowStatus[]>({
    queryKey: ['workflow-statuses', projectId ?? null, teamId ?? null],
    queryFn: async () => {
      const { data } = await apiClient.get('/workflow-statuses/', {
        params: {
          ...(projectId ? { project: projectId } : {}),
          ...(teamId ? { team: teamId } : {}),
        },
      });
      return (data.results ?? data) as WorkflowStatus[];
    },
    enabled: !!projectId || !!teamId,
  });
}

/** ワークフローステータス作成 */
export function useCreateWorkflowStatus() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: Omit<WorkflowStatus, 'id'>) => {
      const { data } = await apiClient.post('/workflow-statuses/', payload);
      return data as WorkflowStatus;
    },
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ['workflow-statuses'] });
    },
  });
}

/** ワークフローステータス更新 */
export function useUpdateWorkflowStatus() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, ...payload }: Partial<WorkflowStatus> & { id: number }) => {
      const { data } = await apiClient.patch(`/workflow-statuses/${id}/`, payload);
      return data as WorkflowStatus;
    },
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ['workflow-statuses'] });
    },
  });
}

/** ワークフローステータス削除 */
export function useDeleteWorkflowStatus() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ id }: { id: number; projectId?: number; teamId?: number }) => {
      await apiClient.delete(`/workflow-statuses/${id}/`);
    },
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ['workflow-statuses'] });
    },
  });
}

/** 並び順一括更新 */
export function useReorderWorkflowStatuses() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ order }: { order: number[]; projectId?: number; teamId?: number }) => {
      await apiClient.post('/workflow-statuses/reorder/', { order });
    },
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ['workflow-statuses'] });
    },
  });
}
