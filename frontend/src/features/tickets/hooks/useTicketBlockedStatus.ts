import { useQuery } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { db } from '@/shared/sync/db';
import type { TaskDependency } from '@/shared/api/types';
import { classifyDependencies } from '../utils/classifyDependencies';

export const DONE_STATUSES = new Set(['resolved', 'closed']);

export async function resolveTicketStatus(ticketKey: string): Promise<string | null> {
  const local = await db.tickets.where('ticketKey').equals(ticketKey).first();
  if (local?.status) return local.status;
  try {
    const { data } = await apiClient.get<{ status?: string }>(`/tickets/${ticketKey}/`);
    return data.status ?? null;
  } catch {
    return null;
  }
}

export function useTicketBlockedStatus(
  ticketId: number | undefined,
  dependencies: TaskDependency[] | undefined,
) {
  const blockedBy = ticketId
    ? classifyDependencies(dependencies ?? [], ticketId).blockedBy
    : [];

  return useQuery({
    queryKey: [
      'ticket-blocked-status',
      ticketId,
      blockedBy.map((d) => `${d.id}:${d.fromTaskKey}`).join(','),
    ],
    queryFn: async () => {
      const blockers: Array<{ ticketKey: string; title: string; status: string | null }> = [];
      for (const dep of blockedBy) {
        const status = await resolveTicketStatus(dep.fromTaskKey);
        blockers.push({
          ticketKey: dep.fromTaskKey,
          title: dep.fromTaskTitle,
          status,
        });
      }
      const isBlocked = blockers.some(
        (b) => b.status == null || !DONE_STATUSES.has(b.status),
      );
      return { isBlocked, blockers };
    },
    enabled: !!ticketId && blockedBy.length > 0,
  });
}
