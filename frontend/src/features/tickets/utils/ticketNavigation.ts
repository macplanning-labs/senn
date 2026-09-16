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
