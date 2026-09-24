import { useOptimisticMutation } from '@/shared/hooks/useOptimisticMutation';
import { apiClient } from '@/shared/api/client';
import { TICKET_DASHBOARD_INVALIDATE_KEYS } from '@/shared/utils/ticketQueryInvalidation';
import type { TicketDetailView } from '../types/ticketDetailView';

type UserValue = { id: number; username: string; displayName: string };
type LabelValue = { id: number; name: string; color: string };
type CategoryValue = { id: number; name: string; color: string };
type MilestoneValue = { id: number; name: string; dueDate: string | null };
export type CycleMutationValue = number | null | { id: number; name: string };

function extractCycleId(value: CycleMutationValue): number | null {
  if (value === null) return null;
  if (typeof value === 'object') return value.id;
  return value;
}

function extractCycleName(
  value: CycleMutationValue,
  current: TicketDetailView,
): string | null {
  if (value === null) return null;
  if (typeof value === 'object' && 'name' in value) return value.name;
  return current.cycleName;
}

export function useTicketPropertyMutations(ticketId: string) {
  const queryKey = ['ticket', ticketId];

  const statusMutation = useOptimisticMutation<void, string>({
    mutationFn: async (status) => {
      await apiClient.patch(`/tickets/${ticketId}/`, { status });
    },
    queryKey,
    updater: (currentData, status) => {
      const data = currentData as TicketDetailView | undefined;
      if (!data) return currentData;
      return { ...data, status };
    },
    invalidateKeys: TICKET_DASHBOARD_INVALIDATE_KEYS,
    errorMessage: 'ステータス変更に失敗しました。元に戻しました。',
  });

  const priorityMutation = useOptimisticMutation<void, string>({
    mutationFn: async (priority) => {
      await apiClient.patch(`/tickets/${ticketId}/`, { priority });
    },
    queryKey,
    updater: (currentData, priority) => {
      const data = currentData as TicketDetailView | undefined;
      if (!data) return currentData;
      return { ...data, priority };
    },
    invalidateKeys: [['tickets']],
    errorMessage: '優先度変更に失敗しました。元に戻しました。',
  });

  const storyPointsMutation = useOptimisticMutation<void, number | null>({
    mutationFn: async (storyPoints) => {
      await apiClient.patch(`/tickets/${ticketId}/`, { story_points: storyPoints });
    },
    queryKey,
    updater: (currentData, storyPoints) => {
      const data = currentData as TicketDetailView | undefined;
      if (!data) return currentData;
      return { ...data, storyPoints };
    },
    invalidateKeys: [['tickets']],
    errorMessage: 'ストーリーポイント変更に失敗しました。',
  });

  const startDateMutation = useOptimisticMutation<void, string | null>({
    mutationFn: async (startDate) => {
      await apiClient.patch(`/tickets/${ticketId}/`, { start_date: startDate });
    },
    queryKey,
    updater: (currentData, startDate) => {
      const data = currentData as TicketDetailView | undefined;
      if (!data) return currentData;
      return { ...data, startDate };
    },
    invalidateKeys: TICKET_DASHBOARD_INVALIDATE_KEYS,
    errorMessage: '開始日の変更に失敗しました。',
  });

  const dueDateMutation = useOptimisticMutation<void, string | null>({
    mutationFn: async (dueDate) => {
      await apiClient.patch(`/tickets/${ticketId}/`, { due_date: dueDate });
    },
    queryKey,
    updater: (currentData, dueDate) => {
      const data = currentData as TicketDetailView | undefined;
      if (!data) return currentData;
      return { ...data, dueDate };
    },
    invalidateKeys: TICKET_DASHBOARD_INVALIDATE_KEYS,
    errorMessage: '期限の変更に失敗しました。',
  });

  const cycleMutation = useOptimisticMutation<void, CycleMutationValue>({
    mutationFn: async (cycle) => {
      await apiClient.patch(`/tickets/${ticketId}/`, { cycle: extractCycleId(cycle) });
    },
    queryKey,
    updater: (currentData, cycle) => {
      const data = currentData as TicketDetailView | undefined;
      if (!data) return currentData;
      return {
        ...data,
        cycle: extractCycleId(cycle),
        cycleName: extractCycleName(cycle, data),
      };
    },
    invalidateKeys: [['tickets']],
    errorMessage: 'サイクルの変更に失敗しました。',
  });

  const categoryMutation = useOptimisticMutation<void, CategoryValue | null>({
    mutationFn: async (category) => {
      await apiClient.patch(`/tickets/${ticketId}/`, { category: category?.id ?? null });
    },
    queryKey,
    updater: (currentData, category) => {
      const data = currentData as TicketDetailView | undefined;
      if (!data) return currentData;
      return { ...data, category };
    },
    invalidateKeys: [['tickets']],
    errorMessage: 'カテゴリの変更に失敗しました。',
  });

  const milestoneMutation = useOptimisticMutation<void, MilestoneValue | null>({
    mutationFn: async (milestone) => {
      await apiClient.patch(`/tickets/${ticketId}/`, { milestone: milestone?.id ?? null });
    },
    queryKey,
    updater: (currentData, milestone) => {
      const data = currentData as TicketDetailView | undefined;
      if (!data) return currentData;
      return { ...data, milestone };
    },
    invalidateKeys: [['tickets']],
    errorMessage: 'マイルストーンの変更に失敗しました。',
  });

  const labelsMutation = useOptimisticMutation<void, LabelValue[]>({
    mutationFn: async (labels) => {
      await apiClient.patch(`/tickets/${ticketId}/`, { labels: labels.map((l) => l.id) });
    },
    queryKey,
    updater: (currentData, labels) => {
      const data = currentData as TicketDetailView | undefined;
      if (!data) return currentData;
      return { ...data, labels };
    },
    invalidateKeys: [['tickets']],
    errorMessage: 'ラベルの変更に失敗しました。',
  });

  const assigneesMutation = useOptimisticMutation<void, UserValue[]>({
    mutationFn: async (assignees) => {
      await apiClient.patch(`/tickets/${ticketId}/`, { assignees: assignees.map((a) => a.id) });
    },
    queryKey,
    updater: (currentData, assignees) => {
      const data = currentData as TicketDetailView | undefined;
      if (!data) return currentData;
      return { ...data, assignees };
    },
    invalidateKeys: [['tickets']],
    errorMessage: '担当者の変更に失敗しました。',
  });

  const reviewersMutation = useOptimisticMutation<void, UserValue[]>({
    mutationFn: async (reviewers) => {
      await apiClient.patch(`/tickets/${ticketId}/`, { reviewers: reviewers.map((r) => r.id) });
    },
    queryKey,
    updater: (currentData, reviewers) => {
      const data = currentData as TicketDetailView | undefined;
      if (!data) return currentData;
      return { ...data, reviewers };
    },
    invalidateKeys: [['tickets']],
    errorMessage: 'レビュアーの変更に失敗しました。',
  });

  return {
    status: statusMutation,
    priority: priorityMutation,
    storyPoints: storyPointsMutation,
    startDate: startDateMutation,
    dueDate: dueDateMutation,
    cycle: cycleMutation,
    category: categoryMutation,
    milestone: milestoneMutation,
    labels: labelsMutation,
    assignees: assigneesMutation,
    reviewers: reviewersMutation,
  };
}
