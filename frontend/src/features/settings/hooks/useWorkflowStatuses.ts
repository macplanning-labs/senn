/**
 * useWorkflowStatuses.ts — ワークフローステータス TanStack Query hooks
 */
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import type { WorkflowStatus } from '@/shared/api/types';

/** プロジェクトのワークフローステータス一覧 */
export function useWorkflowStatuses(projectId: number | undefined) {
  return useQuery<WorkflowStatus[]>({
    queryKey: ['workflow-statuses', projectId],
    queryFn: async () => {
      const { data } = await apiClient.get('/workflow-statuses/', {
        params: { project: projectId },
      });
      return (data.results ?? data) as WorkflowStatus[];
    },
    enabled: !!projectId,
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
    onSuccess: (_data, variables) => {
      void qc.invalidateQueries({ queryKey: ['workflow-statuses', variables.project] });
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
    onSuccess: (_data, variables) => {
      void qc.invalidateQueries({ queryKey: ['workflow-statuses'] });
    },
  });
}

/** ワークフローステータス削除 */
export function useDeleteWorkflowStatus() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, projectId }: { id: number; projectId: number }) => {
      await apiClient.delete(`/workflow-statuses/${id}/`);
      return { projectId };
    },
    onSuccess: (_data, variables) => {
      void qc.invalidateQueries({ queryKey: ['workflow-statuses', variables.projectId] });
    },
  });
}

/** 並び順一括更新 */
export function useReorderWorkflowStatuses() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ order, projectId }: { order: number[]; projectId: number }) => {
      await apiClient.post('/workflow-statuses/reorder/', { order });
      return { projectId };
    },
    onSuccess: (_data, variables) => {
      void qc.invalidateQueries({ queryKey: ['workflow-statuses', variables.projectId] });
    },
  });
}
