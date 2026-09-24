/**
 * useProjectTeams.ts — プロジェクトの担当(参加)チームの取得・追加・除外
 *
 * API: GET/POST /projects/{id}/teams/ ・ DELETE /projects/{id}/teams/{team_id}/
 * 変更できるか・外せるか(できない理由)・追加できるチームは、サーバーが判定して返す。
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import type { TeamSummary } from '@/shared/api/types';

export interface ParticipatingTeam extends TeamSummary {
  ticketCount: number;
  cycleCount: number;
  removable: boolean;
  /** 外せない場合の理由(外せる、または変更権限が無い場合は null) */
  removeBlockedReason: string | null;
  /** アーカイブ済み(閲覧専用)か */
  archived: boolean;
}

export interface ProjectTeams {
  canManage: boolean;
  teams: ParticipatingTeam[];
  addableTeams: TeamSummary[];
}

export const projectTeamsKey = (projectId: number) => ['project-teams', projectId] as const;

export function useProjectTeams(projectId: number | undefined) {
  return useQuery({
    queryKey: projectTeamsKey(projectId ?? 0),
    enabled: !!projectId,
    // 権限(管理者への昇格など)や、追加できるチーム(新しく作った・所属したチーム)は
    // 画面を開くたびに変わり得るため、キャッシュせず、開くたびに取り直す
    staleTime: 0,
    queryFn: async () => {
      const res = await apiClient.get<ProjectTeams>(`/projects/${projectId}/teams/`);
      return res.data;
    },
  });
}

/** 変更の後は、この一覧・プロジェクト一覧(参加チームの表示)・Activity を取り直す */
function useInvalidateAfterChange(projectId: number) {
  const queryClient = useQueryClient();
  return () => {
    void queryClient.invalidateQueries({ queryKey: projectTeamsKey(projectId) });
    void queryClient.invalidateQueries({ queryKey: ['projects'] });
    void queryClient.invalidateQueries({ queryKey: ['project-activity', projectId] });
  };
}

export function useAddProjectTeam(projectId: number) {
  const invalidate = useInvalidateAfterChange(projectId);
  return useMutation({
    mutationFn: async (teamId: number) => {
      await apiClient.post(`/projects/${projectId}/teams/`, { teamId });
    },
    onSuccess: invalidate,
  });
}

export function useRemoveProjectTeam(projectId: number) {
  const invalidate = useInvalidateAfterChange(projectId);
  return useMutation({
    mutationFn: async (teamId: number) => {
      await apiClient.delete(`/projects/${projectId}/teams/${teamId}/`);
    },
    onSuccess: invalidate,
  });
}
