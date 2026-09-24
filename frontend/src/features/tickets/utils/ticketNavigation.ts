/** チケット詳細へのパスを生成 */
export function buildTicketDetailPath(
  projectKey: string | null | undefined,
  ticketKey: string,
  cycleId?: number,
  teamSlug?: string | null,
): string {
  // Team-onlyチケット（project_id なし、team_id あり）の場合
  if (!projectKey && teamSlug) {
    return `/team/${teamSlug}/tickets/${ticketKey}`;
  }
  // Project付きチケット（従来）
  if (cycleId != null && projectKey) {
    return `/project/${projectKey}/cycles/${cycleId}/${ticketKey}`;
  }
  if (projectKey) {
    return `/project/${projectKey}/tickets/${ticketKey}`;
  }
  // フォールバック（通常は到達しない）
  return `/tickets/${ticketKey}`;
}

/** チケット一覧（パネル閉じ）へのパスを生成 */
export function buildTicketListPath(
  projectKey: string | null | undefined,
  cycleId?: number,
  teamSlug?: string | null,
): string {
  // Team-onlyの場合
  if (!projectKey && teamSlug) {
    return `/team/${teamSlug}/tickets`;
  }
  // Project付きの場合（従来）
  if (cycleId != null && projectKey) {
    return `/project/${projectKey}/cycles/${cycleId}`;
  }
  if (projectKey) {
    return `/project/${projectKey}/tickets`;
  }
  // フォールバック
  return `/tickets`;
}

interface TicketPathContext {
  projectKey?: string | null;
  teamSlug?: string | null;
  ticketProjectPrefix?: string | null;
  ticketTeamSlug?: string | null;
}

/**
 * 画面の文脈(projectKey / teamSlug。チーム画面では projectKey が空)を優先し、
 * どちらも無いときだけチケット自身の所属にフォールバックする。
 */
function resolveTicketScope(ctx: TicketPathContext): {
  projectKey: string | null;
  teamSlug: string | null;
} {
  const noContext = !ctx.projectKey && !ctx.teamSlug;
  return {
    projectKey: ctx.projectKey ?? (noContext ? ctx.ticketProjectPrefix ?? null : null),
    teamSlug: ctx.teamSlug ?? (noContext ? ctx.ticketTeamSlug ?? null : null),
  };
}

/** 「リンクをコピー」用の絶対URLを生成。 */
export function buildTicketShareUrl(
  origin: string,
  ticketKey: string,
  ctx: TicketPathContext,
  hash?: string,
): string {
  const { projectKey, teamSlug } = resolveTicketScope(ctx);
  const path = buildTicketDetailPath(projectKey, ticketKey, undefined, teamSlug);
  return `${origin}${path}${hash ? `#${hash}` : ''}`;
}

/** 編集画面へのパス。プロジェクトもチームも分からないときは null。 */
export function buildTicketEditPath(ticketKey: string, ctx: TicketPathContext): string | null {
  const { projectKey, teamSlug } = resolveTicketScope(ctx);
  if (!projectKey && !teamSlug) return null;
  return `${buildTicketDetailPath(projectKey, ticketKey, undefined, teamSlug)}/edit`;
}

/** 詳細画面をどこから開いたか（URL から判定する） */
export type TicketDetailOrigin =
  | { kind: 'myIssues' }
  | { kind: 'board'; base: string }
  | { kind: 'cycle'; base: string; cycleId: string }
  | { kind: 'list' };

/**
 * 詳細画面の URL から、開いた入口を判定する。
 * `/my-issues/:id` → 自分のチケット、`/{team|project}/:x/board/:id` → ボード、
 * `/{team|project}/:x/cycles/:cycleId/:id` → サイクル、それ以外 → チケット一覧。
 */
export function detectTicketDetailOrigin(pathname: string): TicketDetailOrigin {
  if (/^\/my-issues\/[^/]+\/?$/.test(pathname)) return { kind: 'myIssues' };
  const board = pathname.match(/^(\/(?:team|project)\/[^/]+)\/board\/[^/]+\/?$/);
  if (board) return { kind: 'board', base: board[1]! };
  const cycle = pathname.match(/^(\/(?:team|project)\/[^/]+)\/cycles\/([^/]+)\/[^/]+\/?$/);
  if (cycle) return { kind: 'cycle', base: cycle[1]!, cycleId: cycle[2]! };
  return { kind: 'list' };
}
