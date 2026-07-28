/**
 * useTriageRequests.ts — トリアージ依頼の React Query hooks
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import type { TriageRequest, TriageStatus } from '@/shared/api/types';

interface TriageListResponse {
  results: TriageRequest[];
  count: number;
}

export function useTriageRequests(statusFilter?: TriageStatus) {
  return useQuery<TriageRequest[]>({
    queryKey: ['triage-requests', statusFilter],
    queryFn: async () => {
      const params: Record<string, string> = {};
      if (statusFilter) params.status = statusFilter;
      const res = await apiClient.get<TriageListResponse | TriageRequest[]>(
        '/triage-requests/',
        { params },
      );
      const data = res.data;
      return Array.isArray(data) ? data : data.results;
    },
  });
}

export function useCreateTriageRequest() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (data: {
      title: string;
      description?: string;
      change_type?: string;
      change_payload?: Record<string, unknown>;
      ticket?: number | null;
    }) => {
      const res = await apiClient.post<TriageRequest>('/triage-requests/', data);
      return res.data;
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['triage-requests'] });
    },
  });
}

export function useApproveTriageRequest() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, comment, project_id }: { id: number; comment?: string; project_id?: number }) => {
      const res = await apiClient.post<TriageRequest>(
        `/triage-requests/${id}/approve/`,
        { comment: comment ?? '', project_id },
      );
      return res.data;
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['triage-requests'] });
      void queryClient.invalidateQueries({ queryKey: ['tickets'] });
    },
  });
}

export function useRejectTriageRequest() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, comment }: { id: number; comment?: string }) => {
      const res = await apiClient.post<TriageRequest>(
        `/triage-requests/${id}/reject/`,
        { comment: comment ?? '' },
      );
      return res.data;
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['triage-requests'] });
    },
  });
}
