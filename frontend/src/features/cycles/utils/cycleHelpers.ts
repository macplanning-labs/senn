import type { Cycle } from '@/shared/api/types';

/** サイクル内のチケットがすべて完了（closed / resolved）か */
export function allTicketsClosed(cycle: Cycle): boolean {
  return cycle.ticketCount > 0 && cycle.completedCount === cycle.ticketCount;
}

/**
 * 手動完了ボタンを表示できるか。
 * - active: 従来どおり（未完了チケットがあれば持ち越し先を選ぶ）
 * - planned かつ全チケット完了: 日程に関わらず完了可能
 */
export function canCompleteCycle(cycle: Cycle): boolean {
  if (cycle.status === 'completed') return false;
  if (cycle.status === 'active') return true;
  if (cycle.status === 'planned' && allTicketsClosed(cycle)) return true;
  return false;
}

/** YYYY-MM-DD の開始日 < 終了日 */
export function isValidCycleDateRange(startDate: string, endDate: string): boolean {
  return startDate < endDate;
}

type CyclePatchVariables = {
  id: number;
  project: number;
  name?: string;
  description?: string;
  start_date?: string;
  end_date?: string;
  status?: string;
  teamId?: number;
};

/** 楽観更新: API の snake_case 日付を Cycle の camelCase に反映する */
export function applyCycleOptimisticPatch(cycle: Cycle, variables: CyclePatchVariables): Cycle {
  const next: Cycle = { ...cycle };
  if (variables.name !== undefined) next.name = variables.name;
  if (variables.description !== undefined) next.description = variables.description;
  if (variables.start_date !== undefined) next.startDate = variables.start_date;
  if (variables.end_date !== undefined) next.endDate = variables.end_date;
  return next;
}
