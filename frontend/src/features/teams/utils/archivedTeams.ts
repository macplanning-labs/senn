/**
 * archivedTeams.ts — チームのアーカイブ状態フィルタリング
 *
 * 純粋関数でアーカイブされたチームと未アーカイブチームを識別・分類
 */

/** チームアーカイブ状態を判定 */
export function isArchived(team: { archivedAt?: string | null }): boolean {
  return !!team.archivedAt;
}

/** アーカイブされていないチームのみをフィルタ */
export function activeTeams<T extends { archivedAt?: string | null }>(
  teams: T[] | null | undefined,
): T[] {
  if (!teams) return [];
  return teams.filter((team) => !isArchived(team));
}

/** アーカイブ済みチームのみをフィルタ */
export function archivedTeams<T extends { archivedAt?: string | null }>(
  teams: T[] | null | undefined,
): T[] {
  if (!teams) return [];
  return teams.filter((team) => isArchived(team));
}
