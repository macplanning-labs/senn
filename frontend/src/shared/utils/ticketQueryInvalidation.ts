import type { QueryKey } from '@tanstack/react-query';

/** チケット一覧・詳細の更新後に無効化する QueryKey */
export const TICKET_LIST_INVALIDATE_KEYS: QueryKey[] = [['tickets'], ['my-issues']];

/**
 * 期限・ステータス等、ダッシュボード統計(期限超過/まもなく期限)に影響する
 * チケット更新後に無効化する QueryKey。
 */
export const TICKET_DASHBOARD_INVALIDATE_KEYS: QueryKey[] = [
  ['tickets'],
  ['my-issues'],
  ['dashboard-detail'],
  ['tickets-drilldown'],
];
