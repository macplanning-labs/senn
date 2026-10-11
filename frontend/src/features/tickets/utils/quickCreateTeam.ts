/**
 * ワンクリック起票の起票先チームを決める。
 * チケットは必ずチームに属する(API が teamId を要求する)。
 */
export interface QuickCreateTeamChoice {
  id: number;
  archived?: boolean;
}

export interface QuickCreateTeamResolution<T extends QuickCreateTeamChoice> {
  /** 起票に使うチーム。決められなければ null */
  teamId: number | null;
  /** 利用者にチームを選んでもらう必要があるか */
  needsChoice: boolean;
  /** 選択肢(アーカイブ済みを除く) */
  choices: T[];
}

export function resolveQuickCreateTeam<T extends QuickCreateTeamChoice>(
  explicitTeamId: number | null | undefined,
  projectTeams: T[],
  selectedTeamId: number | null,
): QuickCreateTeamResolution<T> {
  if (explicitTeamId != null) {
    return { teamId: explicitTeamId, needsChoice: false, choices: [] };
  }
  const choices = projectTeams.filter((tm) => !tm.archived);
  const [only] = choices;
  if (choices.length === 1 && only) return { teamId: only.id, needsChoice: false, choices };
  const selected = choices.some((tm) => tm.id === selectedTeamId) ? selectedTeamId : null;
  return { teamId: selected, needsChoice: choices.length > 1, choices };
}
