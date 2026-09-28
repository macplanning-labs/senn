import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import type { AxiosError } from 'axios';
import { apiClient } from '@/shared/api/client';
import type { TaskDependency } from '@/shared/api/types';
import {
  buildCreateDependencyRequest,
  type RelationCreateKind,
} from '../utils/classifyDependencies';

export function ticketDependenciesQueryKey(ticketKey: string) {
  return ['ticket-dependencies', ticketKey] as const;
}

export function useTicketDependencies(ticketKey: string | undefined) {
  return useQuery<TaskDependency[]>({
    queryKey: ticketDependenciesQueryKey(ticketKey ?? ''),
    queryFn: async () => {
      const { data } = await apiClient.get<TaskDependency[]>(
        `/tickets/${ticketKey}/dependencies/`,
      );
      return data;
    },
    enabled: !!ticketKey,
  });
}

function dependencyErrorMessage(err: unknown): string {
  const detail = (err as AxiosError<{ detail?: string }>)?.response?.data?.detail ?? '';
  if (/circular|loop/i.test(detail)) return 'circularDependency';
  if (/duplicate|unique|already/i.test(detail)) return 'duplicateRelation';
  return 'relationFailed';
}

export function useCreateTicketDependency(
  currentTicketKey: string,
  currentTicketId: number,
  projectId: number | null | undefined,
  teamId: number | undefined,
) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: {
      kind: RelationCreateKind;
      other: { id: number; ticketKey: string };
    }) => {
      const { postTicketKey, body } = buildCreateDependencyRequest(
        payload.kind,
        currentTicketKey,
        currentTicketId,
        payload.other,
      );
      const { data } = await apiClient.post<TaskDependency>(
        `/tickets/${postTicketKey}/dependencies/`,
        body,
      );
      return data;
    },
    onSuccess: (_data, variables) => {
      const { postTicketKey } = buildCreateDependencyRequest(
        variables.kind,
        currentTicketKey,
        currentTicketId,
        variables.other,
      );
      void qc.invalidateQueries({ queryKey: ticketDependenciesQueryKey(currentTicketKey) });
      if (postTicketKey !== currentTicketKey) {
        void qc.invalidateQueries({ queryKey: ticketDependenciesQueryKey(postTicketKey) });
      }
      void qc.invalidateQueries({ queryKey: ['ticket-blocked-status'] });
      void qc.invalidateQueries({ queryKey: ['dependency-graph', projectId, teamId] });
    },
    meta: { dependencyErrorMessage },
  });
}

export function useDeleteTicketDependency(
  ticketKey: string,
  projectId: number | null | undefined,
  teamId: number | undefined,
) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (dependencyId: number) => {
      await apiClient.delete(`/tickets/${ticketKey}/dependencies/${dependencyId}/`);
    },
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ticketDependenciesQueryKey(ticketKey) });
      void qc.invalidateQueries({ queryKey: ['ticket-blocked-status'] });
      void qc.invalidateQueries({ queryKey: ['dependency-graph', projectId, teamId] });
    },
  });
}

export function getRelationErrorKey(err: unknown): string {
  return dependencyErrorMessage(err);
}
