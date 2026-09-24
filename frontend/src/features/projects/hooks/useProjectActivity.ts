/**
 * useProjectActivity.ts — プロジェクトの変更履歴(Activity)と進捗報告(Project Updates)
 *
 * API: GET /projects/{id}/activity/ ・ /projects/{id}/updates/ (カーソルページング。next を before に渡す)
 */

import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';

// ─── 型 ────────────────────────────────────────────

export type ProjectHealth = 'on_track' | 'at_risk' | 'off_track';
export const HEALTH_VALUES: ProjectHealth[] = ['on_track', 'at_risk', 'off_track'];

export interface ProjectActivityEvent {
  id: number;
  eventType: string;
  payload: Record<string, unknown>;
  actorId: number | null;
  actorName: string | null;
  createdAt: string;
}

export interface ProjectUpdate {
  id: number;
  projectId: number;
  health: ProjectHealth;
  body: string;
  authorId: number | null;
  authorName: string | null;
  createdAt: string;
  updatedAt: string;
}

interface Page<T> {
  results: T[];
  next: number | null;
}

export interface ProjectUpdateInput {
  health: ProjectHealth;
  body: string;
}

// ─── Query Keys ────────────────────────────────────

export const projectActivityKeys = {
  activity: (projectId: number) => ['project-activity', projectId] as const,
  updates: (projectId: number) => ['project-updates', projectId] as const,
};

// ─── Activity ──────────────────────────────────────

const ACTIVITY_PAGE_SIZE = 30;

/** 変更履歴(新しい順)。「もっと見る」で fetchNextPage を呼ぶ。 */
export function useProjectActivity(projectId: number | undefined) {
  return useInfiniteQuery({
    queryKey: projectActivityKeys.activity(projectId ?? 0),
    enabled: !!projectId,
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam }) => {
      const res = await apiClient.get<Page<ProjectActivityEvent>>(
        `/projects/${projectId}/activity/`,
        { params: { limit: ACTIVITY_PAGE_SIZE, before: pageParam } },
      );
      return res.data;
    },
    getNextPageParam: (last) => last.next ?? undefined,
  });
}

// ─── Project Updates(進捗報告) ─────────────────────

/** 直近の進捗報告(Overview 用)。 */
export function useProjectUpdates(projectId: number | undefined, limit = 3) {
  return useQuery({
    queryKey: [...projectActivityKeys.updates(projectId ?? 0), limit],
    enabled: !!projectId,
    queryFn: async () => {
      const res = await apiClient.get<Page<ProjectUpdate>>(`/projects/${projectId}/updates/`, {
        params: { limit },
      });
      return res.data;
    },
  });
}

/** 進捗の投稿・編集・削除の後は、進捗一覧と Activity を取り直す */
function useInvalidateAfterUpdate(projectId: number) {
  const queryClient = useQueryClient();
  return () => {
    void queryClient.invalidateQueries({ queryKey: projectActivityKeys.updates(projectId) });
    void queryClient.invalidateQueries({ queryKey: projectActivityKeys.activity(projectId) });
  };
}

export function usePostProjectUpdate(projectId: number) {
  const invalidate = useInvalidateAfterUpdate(projectId);
  return useMutation({
    mutationFn: async (input: ProjectUpdateInput) => {
      const res = await apiClient.post<ProjectUpdate>(`/projects/${projectId}/updates/`, input);
      return res.data;
    },
    onSuccess: invalidate,
  });
}

export function useEditProjectUpdate(projectId: number) {
  const invalidate = useInvalidateAfterUpdate(projectId);
  return useMutation({
    mutationFn: async ({ id, ...input }: ProjectUpdateInput & { id: number }) => {
      const res = await apiClient.put<ProjectUpdate>(`/projects/${projectId}/updates/${id}/`, input);
      return res.data;
    },
    onSuccess: invalidate,
  });
}

export function useDeleteProjectUpdate(projectId: number) {
  const invalidate = useInvalidateAfterUpdate(projectId);
  return useMutation({
    mutationFn: async (id: number) => {
      await apiClient.delete(`/projects/${projectId}/updates/${id}/`);
    },
    onSuccess: invalidate,
  });
}
