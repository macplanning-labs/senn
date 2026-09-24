/**
 * チケットの担当者・レビュアー・@メンション候補の取得。
 * プロジェクト付き → /users/?project=
 * チームのみ → /teams/{id}/members/ を UserOption 形に正規化
 */
import type { AxiosInstance } from 'axios';

export interface TicketUserOption {
  id: number;
  username: string;
  displayName: string;
  alias?: string | null;
}

interface TeamMemberRow {
  user: {
    id: number;
    username: string;
    displayName: string;
    alias?: string | null;
  };
}

/** プロジェクト優先。無ければチームメンバー。どちらも無ければ空配列。 */
export async function fetchTicketUserOptions(
  api: AxiosInstance,
  opts: { projectId?: number | null; teamId?: number | null },
): Promise<TicketUserOption[]> {
  const projectId = opts.projectId ?? null;
  const teamId = opts.teamId ?? null;

  if (projectId != null) {
    const res = await api.get<TicketUserOption[] | { results?: TicketUserOption[] }>('/users/', {
      params: { project: projectId },
    });
    const data = res.data;
    if (Array.isArray(data)) return data;
    return data.results ?? [];
  }

  if (teamId != null) {
    const res = await api.get<TeamMemberRow[]>(`/teams/${teamId}/members/`);
    return (res.data ?? []).map((m) => ({
      id: m.user.id,
      username: m.user.username,
      displayName: m.user.displayName,
      alias: m.user.alias ?? null,
    }));
  }

  return [];
}

export function ticketUserOptionsEnabled(opts: {
  projectId?: number | null;
  teamId?: number | null;
}): boolean {
  return opts.projectId != null || opts.teamId != null;
}
