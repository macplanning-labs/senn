/**
 * ticketMapping.ts — サーバーの応答を端末内 DB の行（LocalTicket / LocalProject）に変換する
 *
 * 詳細設計 §3.3。一覧・詳細・PATCH・作成・同期のどの応答を渡しても同じ形にする。
 * 付随データ（comments / attachments / links / linkedRules / linkedWikiPages / isWatching）は行ではないので捨てる。
 */

import type { LocalProject, LocalTicket, SyncLabel, SyncTeam, SyncUser } from './db';

type RowMeta = Partial<Pick<LocalTicket, '_dirty' | '_syncedAt' | '_pendingCreate' | '_deleted' | '_syncError'>>;

/** 行ではない（端末内 DB に入れない）項目 */
const NON_ROW_KEYS = ['comments', 'attachments', 'links', 'linkedRules', 'linkedWikiPages', 'isWatching'] as const;

type Dto = Record<string, unknown>;

function str(v: unknown, fallback = ''): string {
  return typeof v === 'string' ? v : fallback;
}
function strOrNull(v: unknown): string | null {
  return typeof v === 'string' ? v : null;
}
function num(v: unknown, fallback = 0): number {
  return typeof v === 'number' && Number.isFinite(v) ? v : fallback;
}
function numOrNull(v: unknown): number | null {
  return typeof v === 'number' && Number.isFinite(v) ? v : null;
}
function arr<T>(v: unknown): T[] {
  return Array.isArray(v) ? (v as T[]) : [];
}
function pick(dto: Dto, camel: string, snake: string): unknown {
  return dto[camel] !== undefined ? dto[camel] : dto[snake];
}

/** 索引用項目（teamId / projectId / cycleId / parentId / assigneeIds / labelIds）を作り直す */
export function withIndexFields<T extends Omit<LocalTicket, 'teamId' | 'projectId' | 'cycleId' | 'parentId' | 'assigneeIds' | 'labelIds'>>(
  row: T,
): T & Pick<LocalTicket, 'teamId' | 'projectId' | 'cycleId' | 'parentId' | 'assigneeIds' | 'labelIds'> {
  return {
    ...row,
    teamId: row.team?.id ?? null,
    projectId: row.project ?? null,
    cycleId: row.cycle ?? null,
    parentId: row.parent ?? null,
    assigneeIds: (row.assignees ?? []).map((a) => a.id),
    labelIds: (row.labels ?? []).map((l) => l.id),
  };
}

/** サーバーのチケット DTO（一覧・詳細・同期・作成応答）→ LocalTicket */
export function toLocalTicket(input: Dto, meta?: RowMeta): LocalTicket {
  const dto: Dto = { ...input };
  for (const k of NON_ROW_KEYS) delete dto[k];

  const now = new Date().toISOString();
  const team = (dto.team as SyncTeam | null | undefined) ?? null;
  const base = {
    id: num(dto.id),
    ticketKey: str(pick(dto, 'ticketKey', 'ticket_key')),
    title: str(dto.title),
    description: str(dto.description),
    status: str(dto.status, 'open'),
    priority: str(dto.priority, 'medium'),
    ticketType: str(pick(dto, 'ticketType', 'ticket_type'), 'task'),
    assignees: arr<SyncUser>(dto.assignees),
    reviewers: arr<SyncUser>(dto.reviewers),
    author: (dto.author as SyncUser | null | undefined) ?? null,
    category: (dto.category as LocalTicket['category'] | undefined) ?? null,
    milestone: (dto.milestone as LocalTicket['milestone'] | undefined) ?? null,
    project: numOrNull(dto.project),
    projectPrefix: strOrNull(pick(dto, 'projectPrefix', 'project_prefix')),
    projectName: strOrNull(pick(dto, 'projectName', 'project_name')),
    parent: numOrNull(dto.parent),
    labels: arr<SyncLabel>(dto.labels),
    startDate: strOrNull(pick(dto, 'startDate', 'start_date')),
    dueDate: strOrNull(pick(dto, 'dueDate', 'due_date')),
    storyPoints: numOrNull(pick(dto, 'storyPoints', 'story_points')),
    cycle: numOrNull(dto.cycle),
    cycleName: strOrNull(pick(dto, 'cycleName', 'cycle_name')),
    team,
    commentCount: num(pick(dto, 'commentCount', 'comment_count')),
    childCount: num(pick(dto, 'childCount', 'child_count')),
    totalTimeSpent: num(pick(dto, 'totalTimeSpent', 'total_time_spent')),
    gantt_order: num(pick(dto, 'gantt_order', 'ganttOrder')),
    createdAt: str(pick(dto, 'createdAt', 'created_at'), now),
    updatedAt: str(pick(dto, 'updatedAt', 'updated_at'), now),
    closedAt: strOrNull(pick(dto, 'closedAt', 'closed_at')),
    _dirty: meta?._dirty ?? false,
    _syncedAt: meta?._syncedAt !== undefined ? meta._syncedAt : now,
    _syncError: meta?._syncError ?? null,
    ...(meta?._pendingCreate ? { _pendingCreate: true } : {}),
    ...(meta?._deleted ? { _deleted: true } : {}),
  };
  return withIndexFields(base);
}

/** サーバーのプロジェクト DTO（一覧・詳細・同期・作成応答）→ LocalProject */
export function toLocalProject(dto: Dto, meta?: RowMeta): LocalProject {
  const now = new Date().toISOString();
  const createdAt = str(pick(dto, 'createdAt', 'created_at'), now);
  return {
    id: num(dto.id),
    name: str(dto.name),
    prefix: str(dto.prefix),
    description: str(dto.description),
    status: str(dto.status, 'in_progress'),
    priority: str(dto.priority, 'medium'),
    targetEndDate: strOrNull(pick(dto, 'targetEndDate', 'target_end_date')),
    ticketCount: num(dto.ticketCount),
    memberCount: num(dto.memberCount),
    isMember: dto.isMember === true,
    teams: arr<LocalProject['teams'][number]>(dto.teams),
    ownerId: numOrNull(dto.ownerId),
    createdAt,
    updatedAt: str(pick(dto, 'updatedAt', 'updated_at'), createdAt),
    cycleAutoComplete: dto.cycleAutoComplete === true,
    cycleAutoCreateNext: dto.cycleAutoCreateNext === true,
    parentProjectId: numOrNull(dto.parentProjectId),
    childCount: num(dto.childCount),
    roadmapIds: arr<number>(dto.roadmapIds),
    _dirty: meta?._dirty ?? false,
    _syncedAt: meta?._syncedAt !== undefined ? meta._syncedAt : now,
    _syncError: meta?._syncError ?? null,
    ...(meta?._pendingCreate ? { _pendingCreate: true } : {}),
    ...(meta?._deleted ? { _deleted: true } : {}),
  };
}

/** 行が今どの同期状態か（data-sync-state 属性用。詳細設計 §5.4） */
export function syncStateOf(row: { _dirty?: boolean; _pendingCreate?: boolean; _syncError?: string | null } | undefined | null): 'pending' | 'synced' | 'error' {
  if (!row) return 'synced';
  if (row._syncError) return 'error';
  if (row._dirty || row._pendingCreate) return 'pending';
  return 'synced';
}
