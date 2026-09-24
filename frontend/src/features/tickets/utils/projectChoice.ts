/** チケット作成画面で選べるプロジェクトの候補 */
interface ProjectWithTeams {
  id: number;
  teams?: { id: number }[];
}

/**
 * 選択中のチームが参加しているプロジェクトだけを返す。
 * チケットのチームは、そのプロジェクトの参加チームでなければDBが拒否するため、
 * 最初から選べないようにする。チーム未選択なら全プロジェクト。
 */
export function projectsForTeam<T extends ProjectWithTeams>(
  projects: T[],
  teamId: number | null,
): T[] {
  if (teamId == null) return projects;
  return projects.filter((p) => (p.teams ?? []).some((t) => t.id === teamId));
}
