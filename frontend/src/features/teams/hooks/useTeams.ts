/**
 * useTeams.ts — チーム管理 TanStack Query Hooks
 *
 * チームCRUD + メンバー管理のデータフェッチ・ミューテーション。
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import type { Team, TeamMembership, TeamGuest } from '@/shared/api/types';

// ─── Query Keys ────────────────────────────────────

const teamKeys = {
  all: ['teams'] as const,
  detail: (id: number) => ['teams', id] as const,
  members: (id: number) => ['teams', id, 'members'] as const,
  guests: (id: number) => ['teams', id, 'guests'] as const,
};

// ─── Queries ───────────────────────────────────────

/** チーム一覧を取得（未認証画面では enabled: false にする） */
export function useTeams(options?: { enabled?: boolean }) {
  return useQuery({
    queryKey: teamKeys.all,
    queryFn: async () => {
      const res = await apiClient.get<{ results: Team[] } | Team[]>('/teams/');
      const data = res.data;
      return Array.isArray(data) ? data : data.results;
    },
    enabled: options?.enabled ?? true,
  });
}

/** チーム詳細を取得 */
export function useTeam(id: number | null) {
  return useQuery({
    queryKey: teamKeys.detail(id!),
    queryFn: async () => {
      const res = await apiClient.get<Team>(`/teams/${id}/`);
      return res.data;
    },
    enabled: id !== null,
  });
}

/** チームのメンバー一覧を取得 */
export function useTeamMembers(teamId: number | null) {
  return useQuery({
    queryKey: teamKeys.members(teamId!),
    queryFn: async () => {
      const res = await apiClient.get<TeamMembership[]>(
        `/teams/${teamId}/members/`,
      );
      return res.data;
    },
    enabled: teamId !== null,
  });
}

/** チームのProjectゲスト一覧を取得 */
export function useTeamGuests(teamId: number | null) {
  return useQuery({
    queryKey: teamKeys.guests(teamId!),
    queryFn: async () => {
      const res = await apiClient.get<TeamGuest[]>(
        `/teams/${teamId}/guests/`,
      );
      return res.data;
    },
    enabled: teamId !== null,
  });
}

// ─── Mutations ─────────────────────────────────────

export interface TeamFormData {
  name: string;
  slug?: string;
  description?: string;
  icon?: string;
  color?: string;
  slackWebhookUrl?: string;
  isActive?: boolean;
  prefix?: string;
}

/** チーム作成 */
export function useCreateTeam() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (data: TeamFormData) => {
      const res = await apiClient.post<Team>('/teams/', data);
      return res.data;
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: teamKeys.all });
    },
  });
}

/** チーム更新 */
export function useUpdateTeam() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, data }: { id: number; data: Partial<TeamFormData> }) => {
      const res = await apiClient.put<Team>(`/teams/${id}/`, data);
      return res.data;
    },
    onSuccess: (_, { id }) => {
      void queryClient.invalidateQueries({ queryKey: teamKeys.all });
      void queryClient.invalidateQueries({ queryKey: teamKeys.detail(id) });
    },
  });
}

/** チーム削除 */
export function useDeleteTeam() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (id: number) => {
      await apiClient.delete(`/teams/${id}/`);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: teamKeys.all });
    },
  });
}

/** メンバー追加 */
export function useAddTeamMember() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({
      teamId,
      userId,
      role = 'member',
    }: {
      teamId: number;
      userId: number;
      role?: 'admin' | 'member';
    }) => {
      const res = await apiClient.post<TeamMembership>(
        `/teams/${teamId}/members/`,
        { userId, role },
      );
      return res.data;
    },
    onSuccess: (_, { teamId }) => {
      void queryClient.invalidateQueries({ queryKey: teamKeys.members(teamId) });
      void queryClient.invalidateQueries({ queryKey: teamKeys.all });
    },
  });
}

/** メンバー削除 */
export function useRemoveTeamMember() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({
      teamId,
      userId,
    }: {
      teamId: number;
      userId: number;
    }) => {
      await apiClient.delete(`/teams/${teamId}/members/${userId}/`);
    },
    onSuccess: (_, { teamId }) => {
      void queryClient.invalidateQueries({ queryKey: teamKeys.members(teamId) });
      void queryClient.invalidateQueries({ queryKey: teamKeys.all });
    },
  });
}

/** Projectゲスト追加(L2: このProjectに限定した参加) */
export function useAddTeamGuest() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({
      teamId,
      userId,
      projectId,
      endDate,
    }: {
      teamId: number;
      userId: number;
      projectId: number;
      endDate?: string | null;
    }) => {
      const res = await apiClient.post<TeamGuest[]>(
        `/teams/${teamId}/guests/`,
        { userId, projectId, endDate: endDate || null },
      );
      return res.data;
    },
    onSuccess: (_, { teamId }) => {
      void queryClient.invalidateQueries({ queryKey: teamKeys.guests(teamId) });
    },
  });
}

/** Projectゲスト削除 */
export function useRemoveTeamGuest() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({
      teamId,
      membershipId,
    }: {
      teamId: number;
      membershipId: number;
    }) => {
      await apiClient.delete(`/teams/${teamId}/guests/${membershipId}/`);
    },
    onSuccess: (_, { teamId }) => {
      void queryClient.invalidateQueries({ queryKey: teamKeys.guests(teamId) });
    },
  });
}
