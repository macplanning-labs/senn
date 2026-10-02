/**
 * useTeamAccess.ts — チームの参加・退出・公開区分・Owner(アクセス制御の再設計 G-1 の API)
 */
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import type { TeamSettingsPolicy, TeamVisibility } from '@/shared/api/types';

function useInvalidateTeams() {
  const queryClient = useQueryClient();
  return () => {
    void queryClient.invalidateQueries({ queryKey: ['teams'] });
  };
}

/** Join(Full Member・Public のみ。サーバーが判定する) */
export function useJoinTeam() {
  const invalidate = useInvalidateTeams();
  return useMutation({
    mutationFn: async (teamId: number) => {
      await apiClient.post(`/teams/${teamId}/join/`);
    },
    onSuccess: invalidate,
  });
}

/** 退出(Private の最後のメンバー・最後の Owner は 409) */
export function useLeaveTeam() {
  const invalidate = useInvalidateTeams();
  return useMutation({
    mutationFn: async (teamId: number) => {
      await apiClient.post(`/teams/${teamId}/leave/`);
    },
    onSuccess: invalidate,
  });
}

/** 公開区分・設定の方針の変更(Private → Public は confirm 必須) */
export function useUpdateTeamAccess() {
  const invalidate = useInvalidateTeams();
  return useMutation({
    mutationFn: async ({
      teamId,
      visibility,
      settingsPolicy,
      confirm,
    }: {
      teamId: number;
      visibility?: TeamVisibility;
      settingsPolicy?: TeamSettingsPolicy;
      confirm?: boolean;
    }) => {
      await apiClient.patch(`/teams/${teamId}/access/`, { visibility, settingsPolicy, confirm });
    },
    onSuccess: invalidate,
  });
}

/** Owner の指名・解除 */
export function useSetTeamOwner() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ teamId, userId, owner }: { teamId: number; userId: number; owner: boolean }) => {
      if (owner) {
        await apiClient.post(`/teams/${teamId}/owners/${userId}/`);
      } else {
        await apiClient.delete(`/teams/${teamId}/owners/${userId}/`);
      }
    },
    onSuccess: (_, { teamId }) => {
      void queryClient.invalidateQueries({ queryKey: ['teams', teamId, 'members'] });
      void queryClient.invalidateQueries({ queryKey: ['teams'] });
    },
  });
}
