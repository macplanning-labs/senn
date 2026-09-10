/** チケット詳細へのパスを生成 */
export function buildTicketDetailPath(
  projectKey: string,
  ticketKey: string,
  cycleId?: number,
): string {
  if (cycleId != null) {
    return `/p/${projectKey}/cycles/${cycleId}/${ticketKey}`;
  }
  return `/p/${projectKey}/tickets/${ticketKey}`;
}

/** チケット一覧（パネル閉じ）へのパスを生成 */
export function buildTicketListPath(
  projectKey: string,
  cycleId?: number,
): string {
  if (cycleId != null) {
    return `/p/${projectKey}/cycles/${cycleId}`;
  }
  return `/p/${projectKey}/tickets`;
}
