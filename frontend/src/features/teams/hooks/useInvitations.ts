/**
 * useInvitations.ts — 招待の作成・一覧・取り消し(アクセス制御の再設計 フェーズ A の API)
 *
 * teamId を指定しない招待(チームの無い Full Member)は、システム管理者だけが行える(サーバーが判定する)。
 */
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';

export type InviteRole = 'full_member' | 'guest';

export interface Invitation {
  id: number;
  email: string;
  role: InviteRole;
  teamId: number | null;
  scopedProjectId: number | null;
  endDate: string | null;
  expiresAt: string;
  createdAt: string;
}

export interface CreateInvitationInput {
  email: string;
  role: InviteRole;
  teamId?: number;
  scopedProjectId?: number;
  endDate?: string;
}

export interface CreateInvitationResult {
  invitation: Invitation;
  /** メールを送った(SMTP の設定がある)か */
  emailSent: boolean;
  /** SMTP 未設定の本番以外だけ返る、受諾の画面のパス(手で渡す用) */
  inviteUrl: string | null;
}

export const invitationsKey = (teamId: number | null) => ['invitations', teamId] as const;

export function useInvitations(teamId: number | null, enabled = true) {
  return useQuery({
    queryKey: invitationsKey(teamId),
    enabled,
    queryFn: async () => {
      const { data } = await apiClient.get<{ results: Invitation[] }>('/invitations/', {
        params: teamId === null ? {} : { teamId },
      });
      return data.results;
    },
  });
}

export function useCreateInvitation(teamId: number | null) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (input: CreateInvitationInput) => {
      const { data } = await apiClient.post<CreateInvitationResult>('/invitations/', input);
      return data;
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: invitationsKey(teamId) });
    },
  });
}

export function useRevokeInvitation(teamId: number | null) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (id: number) => {
      await apiClient.post(`/invitations/${id}/revoke/`);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: invitationsKey(teamId) });
    },
  });
}
