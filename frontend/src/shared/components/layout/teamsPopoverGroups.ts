import { activeTeams as filterActiveTeams, archivedTeams as filterArchivedTeams } from '@/features/teams/utils/archivedTeams';

export interface TeamForPopover {
  id: number;
  name: string;
  slug: string;
  icon: string;
  color: string;
  archivedAt?: string | null;
}

export interface TeamsPopoverGroups<T extends TeamForPopover> {
  active: T[];
  archived: T[];
}

export function buildTeamsPopoverGroups<T extends TeamForPopover>(
  teams: T[] | null | undefined,
): TeamsPopoverGroups<T> {
  if (!teams) {
    return { active: [], archived: [] };
  }

  const active = filterActiveTeams(teams);
  const archived = filterArchivedTeams(teams);

  // Sort by name in Japanese locale
  const sortByName = (a: T, b: T) => a.name.localeCompare(b.name, 'ja');

  return {
    active: active.sort(sortByName),
    archived: archived.sort(sortByName),
  };
}
