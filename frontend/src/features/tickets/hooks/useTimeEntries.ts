/**
 * useTimeEntries.ts — タイムエントリ TanStack Query hooks
 */
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import type { TimeEntry } from '@/shared/api/types';

/** チケットの時間記録一覧 */
export function useTimeEntries(ticketId: number | undefined) {
  return useQuery<TimeEntry[]>({
    queryKey: ['time-entries', ticketId],
    queryFn: async () => {
      const { data } = await apiClient.get('/time-entries/', {
        params: { ticket: ticketId },
      });
      return (data.results ?? data) as TimeEntry[];
    },
    enabled: !!ticketId,
  });
}

/** 今日の自分の作業時間サマリー */
export function useMyTodayTime() {
  return useQuery({
    queryKey: ['time-entries', 'my-today'],
    queryFn: async () => {
      const { data } = await apiClient.get('/time-entries/my-today/');
      return data as {
        date: string;
        totalMinutes: number;
        entryCount: number;
      };
    },
  });
}

/** タイムエントリ作成（タイマー停止 or 手動入力） */
export function useCreateTimeEntry() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: {
      ticket: number;
      description?: string;
      start_time?: string;
      end_time?: string;
      duration_minutes?: number;
    }) => {
      const { data } = await apiClient.post('/time-entries/', payload);
      return data as TimeEntry;
    },
    onSuccess: (_data, variables) => {
      void qc.invalidateQueries({ queryKey: ['time-entries', variables.ticket] });
      void qc.invalidateQueries({ queryKey: ['time-entries', 'my-today'] });
      void qc.invalidateQueries({ queryKey: ['tickets'] });
      void qc.invalidateQueries({ queryKey: ['ticket'] });
    },
  });
}

/** タイムエントリ削除 */
export function useDeleteTimeEntry() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, ticketId }: { id: number; ticketId: number }) => {
      await apiClient.delete(`/time-entries/${id}/`);
      return { ticketId };
    },
    onSuccess: (_data, variables) => {
      void qc.invalidateQueries({ queryKey: ['time-entries', variables.ticketId] });
      void qc.invalidateQueries({ queryKey: ['time-entries', 'my-today'] });
      void qc.invalidateQueries({ queryKey: ['tickets'] });
    },
  });
}
