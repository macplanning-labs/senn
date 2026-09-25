/**
 * ticketQuery.ts — チケット一覧の絞り込み・並び替えを端末側で行う（React に依存しない純関数）
 *
 * 詳細設計 §4.1。サーバーの一覧 API（ticket_repo::api_find_all）と**同じ意味**にする。
 * 意味のずれはサーバーで生成したフィクスチャとの比較テスト（ticketQuery.test.ts、T6）で検出する。
 */

import type { LocalTicket } from '../db';

/** サーバー GET /tickets/ のクエリと同じ名前 */
export interface TicketListParams {
  status?: string;
  status__in?: string;
  priority?: string;
  priority__in?: string;
  assignees?: number | string;
  project?: number | string;
  project__prefix?: string;
  team_slug?: string;
  milestone?: number | string;
  cycle?: number | string;
  category?: number | string;
  labels?: number | string;
  parent?: number | string;
  parent__isnull?: boolean | string;
  due_date__gte?: string;
  due_date__lte?: string;
  due_date__isnull?: boolean | string;
  search?: string;
  /** 例: '-updated_at'。カンマ区切りで複数（'gantt_order,due_date'）も可 */
  ordering?: string;
}

type Row = Pick<
  LocalTicket,
  | 'id'
  | 'ticketKey'
  | 'title'
  | 'description'
  | 'status'
  | 'priority'
  | 'assigneeIds'
  | 'labelIds'
  | 'projectId'
  | 'projectPrefix'
  | 'team'
  | 'milestone'
  | 'cycleId'
  | 'category'
  | 'parentId'
  | 'dueDate'
  | 'createdAt'
  | 'updatedAt'
  | 'gantt_order'
> & { _deleted?: boolean };

function toInt(v: number | string | undefined | null): number | undefined {
  if (v === undefined || v === null || v === '') return undefined;
  const n = typeof v === 'number' ? v : Number(v);
  return Number.isFinite(n) ? n : undefined;
}

function toBool(v: boolean | string | undefined): boolean | undefined {
  if (v === undefined || v === '') return undefined;
  if (typeof v === 'boolean') return v;
  return v === 'true' || v === '1';
}

function splitList(v: string | undefined): string[] | undefined {
  if (v === undefined) return undefined;
  const list = v.split(',');
  return list.length > 0 ? list : undefined;
}

/** サーバーと同じく「後に書いた方が勝つ」: status__in があれば status より優先 */
function statusSet(p: TicketListParams): Set<string> | undefined {
  const list = p.status__in !== undefined ? splitList(p.status__in) : p.status !== undefined ? [p.status] : undefined;
  return list ? new Set(list) : undefined;
}

function prioritySet(p: TicketListParams): Set<string> | undefined {
  const list = p.priority__in !== undefined ? splitList(p.priority__in) : p.priority !== undefined ? [p.priority] : undefined;
  return list ? new Set(list) : undefined;
}

/** 絞り込み条件に合うか */
export function matchesTicketParams(row: Row, p: TicketListParams): boolean {
  if (row._deleted) return false;

  const statuses = statusSet(p);
  if (statuses && !statuses.has(row.status)) return false;
  const priorities = prioritySet(p);
  if (priorities && !priorities.has(row.priority)) return false;

  const assignee = toInt(p.assignees);
  if (assignee !== undefined && !row.assigneeIds.includes(assignee)) return false;
  const project = toInt(p.project);
  if (project !== undefined && row.projectId !== project) return false;
  if (p.project__prefix !== undefined && row.projectPrefix !== p.project__prefix) return false;
  if (p.team_slug !== undefined && (row.team?.slug ?? '').toLowerCase() !== p.team_slug.toLowerCase()) return false;
  const milestone = toInt(p.milestone);
  if (milestone !== undefined && (row.milestone?.id ?? null) !== milestone) return false;
  const cycle = toInt(p.cycle);
  if (cycle !== undefined && row.cycleId !== cycle) return false;
  const category = toInt(p.category);
  if (category !== undefined && (row.category?.id ?? null) !== category) return false;
  const label = toInt(p.labels);
  if (label !== undefined && !row.labelIds.includes(label)) return false;
  const parent = toInt(p.parent);
  if (parent !== undefined && row.parentId !== parent) return false;

  const parentIsNull = toBool(p.parent__isnull);
  if (parentIsNull !== undefined && (row.parentId === null) !== parentIsNull) return false;
  // 期日の比較は 'YYYY-MM-DD' の文字列比較（NULL は不一致。SQL の比較と同じ）
  if (p.due_date__gte && !(row.dueDate !== null && row.dueDate >= p.due_date__gte)) return false;
  if (p.due_date__lte && !(row.dueDate !== null && row.dueDate <= p.due_date__lte)) return false;
  const dueIsNull = toBool(p.due_date__isnull);
  if (dueIsNull !== undefined && (row.dueDate === null) !== dueIsNull) return false;

  if (p.search) {
    const q = p.search.toLowerCase();
    const hit =
      row.title.toLowerCase().includes(q) ||
      (row.description ?? '').toLowerCase().includes(q) ||
      row.ticketKey.toLowerCase().includes(q);
    if (!hit) return false;
  }
  return true;
}

type SortField = 'created_at' | 'updated_at' | 'due_date' | 'priority' | 'gantt_order';
const SORT_FIELDS: ReadonlySet<string> = new Set(['created_at', 'updated_at', 'due_date', 'priority', 'gantt_order']);

/** RFC3339 → マイクロ秒（PostgreSQL の timestamptz と同じ精度で比べるため。Date はミリ秒まで） */
export function toMicros(iso: string): number {
  const ms = Date.parse(iso);
  if (Number.isNaN(ms)) return 0;
  const frac = /\.(\d+)/.exec(iso)?.[1] ?? '';
  const micro = Number((frac + '000000').slice(3, 6));
  return ms * 1000 + micro;
}

function fieldValue(row: Row, field: SortField): string | number | null {
  switch (field) {
    case 'created_at':
      return toMicros(row.createdAt);
    case 'updated_at':
      return toMicros(row.updatedAt);
    case 'due_date':
      return row.dueDate;
    case 'priority':
      return row.priority;
    case 'gantt_order':
      return row.gantt_order;
  }
}

/** PostgreSQL と同じ: 昇順は NULL が最後、降順は NULL が最初 */
function compareValues(a: string | number | null, b: string | number | null, desc: boolean): number {
  if (a === b) return 0;
  if (a === null) return desc ? -1 : 1;
  if (b === null) return desc ? 1 : -1;
  const c = a < b ? -1 : 1;
  return desc ? -c : c;
}

/** ordering（サーバーと同じ名前）から比較関数を作る。未知の値は '-updated_at'。同値は id 降順 */
export function compareTickets(ordering?: string): (a: Row, b: Row) => number {
  const keys = (ordering ?? '')
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean)
    .map((s) => ({ desc: s.startsWith('-'), field: s.replace(/^-/, '') }))
    .filter((k) => SORT_FIELDS.has(k.field)) as Array<{ desc: boolean; field: SortField }>;
  const effective = keys.length > 0 ? keys : [{ desc: true, field: 'updated_at' as const }];
  return (a, b) => {
    for (const k of effective) {
      const c = compareValues(fieldValue(a, k.field), fieldValue(b, k.field), k.desc);
      if (c !== 0) return c;
    }
    return b.id - a.id;
  };
}

/** 絞り込み → 並び替え → 件数制限 */
export function queryTickets<T extends Row>(rows: T[], p: TicketListParams, limit?: number): T[] {
  const out = rows.filter((r) => matchesTicketParams(r, p)).sort(compareTickets(p.ordering));
  return limit !== undefined ? out.slice(0, limit) : out;
}

/** Cmd+K 用: キー前方一致 → キー部分一致 → タイトル部分一致 の順。本文は対象外 */
export function searchTickets<T extends Row>(rows: T[], q: string, limit = 8): T[] {
  const needle = q.trim().toLowerCase();
  if (!needle) return [];
  const rank = (r: Row): number => {
    const key = r.ticketKey.toLowerCase();
    if (key.startsWith(needle)) return 0;
    if (key.includes(needle)) return 1;
    if (r.title.toLowerCase().includes(needle)) return 2;
    return -1;
  };
  return rows
    .filter((r) => !r._deleted)
    .map((r) => ({ r, k: rank(r) }))
    .filter((x) => x.k >= 0)
    .sort((a, b) => a.k - b.k || toMicros(b.r.updatedAt) - toMicros(a.r.updatedAt) || b.r.id - a.r.id)
    .slice(0, limit)
    .map((x) => x.r);
}
