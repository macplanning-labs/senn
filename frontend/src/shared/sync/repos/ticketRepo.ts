/**
 * ticketRepo.ts — 画面がチケットの「行」を読むためのフック（端末内 DB だけを見る）
 *
 * 詳細設計 §4.1。画面は apiClient / useQuery でチケットの行を取りに行かず、ここを使う。
 * 行ではない付随データ（コメント・添付など）だけは useTicketDetail が /extras/ から取る。
 */
import { useEffect, useMemo, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { apiClient } from '../../api/client';
import { db, type LocalTicket } from '../db';
import { toLocalTicket } from '../ticketMapping';
import { useSyncStatus } from '../syncStatusStore';
import { isTempTicketKey } from '../ticketWrites';
import { queryTickets, searchTickets, type TicketListParams } from './ticketQuery';
import { useLiveRows } from './useLiveRows';

// ticketQuery から re-export
export type { TicketListParams };

const EMPTY: LocalTicket[] = [];

/** 初回のフル同期が済んだか（済むまでは「行が無い」を「存在しない」と扱わない） */
export function useInitialSyncDone(): boolean {
  return useSyncStatus((s) => s.initialSyncDone);
}

// ── 1件 ────────────────────────────────────────────────────────────

/** 端末内に無いチケットをサーバーから取って入れる（同期範囲外のリンクを開いたとき等） */
export async function ensureTicketLocal(ticketKey: string): Promise<'found' | 'missing' | 'error'> {
  try {
    const res = await apiClient.get<Record<string, unknown>>(`/tickets/${ticketKey}/`);
    const row = toLocalTicket(res.data);
    const existing = await db.tickets.get(row.id);
    if (!existing?._dirty && !existing?._pendingCreate) await db.tickets.put(row);
    return 'found';
  } catch (err) {
    return (err as { response?: { status?: number } })?.response?.status === 404 ? 'missing' : 'error';
  }
}

export interface TicketRowState {
  row: LocalTicket | undefined;
  /** 行がまだ出せない（初回同期中・取得中） */
  isLoading: boolean;
  /** 存在しない（または見られない・取れない） */
  notFound: boolean;
}

type Fallback = { key: string; state: 'fetching' | 'missing' | 'error' } | null;

export function useTicketRow(ticketKey: string | null | undefined): TicketRowState {
  const initialSyncDone = useInitialSyncDone();
  const { data: row, loaded } = useLiveRows(
    async () => {
      if (!ticketKey) return undefined;
      const r = await db.tickets.where('ticketKey').equals(ticketKey).first();
      return r && !r._deleted ? r : undefined;
    },
    [ticketKey],
    undefined as LocalTicket | undefined,
  );
  const [fallback, setFallback] = useState<Fallback>(null);
  const temp = isTempTicketKey(ticketKey);
  const current = fallback && fallback.key === ticketKey ? fallback.state : null;
  // 端末内の行が今の key のものか（key を切り替えた直後は前の行が残っている）
  const rowForKey = row && row.ticketKey === ticketKey ? row : undefined;

  useEffect(() => {
    if (!ticketKey || temp || !loaded || rowForKey || !initialSyncDone || current) return;
    setFallback({ key: ticketKey, state: 'fetching' });
    void ensureTicketLocal(ticketKey).then((r) => {
      if (r !== 'found') setFallback({ key: ticketKey, state: r });
    });
  }, [ticketKey, temp, loaded, rowForKey, initialSyncDone, current]);

  if (!ticketKey) return { row: undefined, isLoading: false, notFound: false };
  if (rowForKey) return { row: rowForKey, isLoading: false, notFound: false };
  if (!loaded) return { row: undefined, isLoading: true, notFound: false };
  if (temp) return { row: undefined, isLoading: false, notFound: true };
  if (current === 'missing' || current === 'error') return { row: undefined, isLoading: false, notFound: true };
  return { row: undefined, isLoading: true, notFound: false };
}

/** 詳細画面の付随データ（行ではないもの） */
export interface TicketExtras {
  comments: unknown[];
  attachments: unknown[];
  links: unknown[];
  linkedRules: unknown[];
  linkedWikiPages: Array<{ id: number; title: string; slug: string; category: string }>;
  isWatching: boolean;
}

const EMPTY_EXTRAS: TicketExtras = {
  comments: [],
  attachments: [],
  links: [],
  linkedRules: [],
  linkedWikiPages: [],
  isWatching: false,
};

export type TicketDetailData = LocalTicket & TicketExtras;

/**
 * 詳細画面用: 行（端末内 DB）＋付随データ（GET /tickets/{key}/extras/）。
 * 付随データのキャッシュキーは従来どおり ['ticket', key]（既存の setQueryData / invalidateQueries がそのまま効く）。
 */
export function useTicketDetail(ticketKey: string | null | undefined): {
  data: TicketDetailData | undefined;
  isLoading: boolean;
  isError: boolean;
  extrasLoading: boolean;
} {
  const { row, isLoading, notFound } = useTicketRow(ticketKey);
  const extrasEnabled = !!ticketKey && !!row && !row._pendingCreate;
  const extras = useQuery<Partial<TicketExtras>>({
    queryKey: ['ticket', ticketKey],
    queryFn: async () => (await apiClient.get<TicketExtras>(`/tickets/${ticketKey}/extras/`)).data,
    enabled: extrasEnabled,
    staleTime: 30_000,
  });
  const data = useMemo<TicketDetailData | undefined>(() => {
    if (!row) return undefined;
    const e = extras.data ?? {};
    return {
      ...row,
      comments: e.comments ?? EMPTY_EXTRAS.comments,
      attachments: e.attachments ?? EMPTY_EXTRAS.attachments,
      links: e.links ?? EMPTY_EXTRAS.links,
      linkedRules: e.linkedRules ?? EMPTY_EXTRAS.linkedRules,
      linkedWikiPages: e.linkedWikiPages ?? EMPTY_EXTRAS.linkedWikiPages,
      isWatching: e.isWatching ?? EMPTY_EXTRAS.isWatching,
    };
  }, [row, extras.data]);
  return { data, isLoading, isError: notFound, extrasLoading: extrasEnabled && extras.isLoading };
}

// ── 一覧 ────────────────────────────────────────────────────────────

export interface TicketListState {
  tickets: LocalTicket[];
  totalCount: number;
  isLoading: boolean;
}

/**
 * 一覧・ボード用。条件はサーバー GET /tickets/ と同じ名前（page は無し。全件）。
 * enabled=false のあいだは空。
 */
export function useTicketList(params: TicketListParams, opts: { limit?: number; enabled?: boolean } = {}): TicketListState {
  const initialSyncDone = useInitialSyncDone();
  const enabled = opts.enabled ?? true;
  const key = JSON.stringify(params);
  const { data, loaded } = useLiveRows(
    async () => {
      if (!enabled) return EMPTY;
      // よく使う絞り込みは索引で先に絞る（残りは queryTickets が同じ意味で判定する）
      const project = params.project !== undefined && params.project !== '' ? Number(params.project) : undefined;
      const parent = params.parent !== undefined && params.parent !== '' ? Number(params.parent) : undefined;
      const base =
        project !== undefined && Number.isFinite(project)
          ? await db.tickets.where('projectId').equals(project).toArray()
          : parent !== undefined && Number.isFinite(parent)
            ? await db.tickets.where('parentId').equals(parent).toArray()
            : await db.tickets.toArray();
      return queryTickets(base, params);
    },
    [key, enabled],
    EMPTY,
  );
  const tickets = opts.limit !== undefined ? data.slice(0, opts.limit) : data;
  return {
    tickets,
    totalCount: data.length,
    isLoading: enabled && (!loaded || (!initialSyncDone && data.length === 0)),
  };
}

/** My Issues（サーバー /dashboard/my-tickets/ と同じ形・条件: 担当が自分、未完了3状態、更新日時の新しい順、50件） */
export interface MyIssueRow {
  id: number;
  ticket_key: string;
  title: string;
  status: string;
  priority: string;
  due_date: string | null;
  updated_at: string;
  project_key: string | null;
  /** 画面の同期状態表示用 */
  _row: LocalTicket;
}

export const MY_ISSUE_STATUSES = 'backlog,open,in_progress';

export function useMyIssues(userId: number | null | undefined, limit = 50): { data: MyIssueRow[]; isLoading: boolean } {
  const initialSyncDone = useInitialSyncDone();
  const { data, loaded } = useLiveRows(
    async () => {
      if (!userId) return EMPTY;
      const mine = await db.tickets.where('assigneeIds').equals(userId).toArray();
      return queryTickets(mine, { assignees: userId, status__in: MY_ISSUE_STATUSES, ordering: '-updated_at' }, limit);
    },
    [userId, limit],
    EMPTY,
  );
  const rows = useMemo(
    () =>
      data.map((t) => ({
        id: t.id,
        ticket_key: t.ticketKey,
        title: t.title,
        status: t.status,
        priority: t.priority,
        due_date: t.dueDate,
        updated_at: t.updatedAt,
        project_key: t.projectPrefix,
        _row: t,
      })),
    [data],
  );
  return { data: rows, isLoading: !!userId && (!loaded || (!initialSyncDone && data.length === 0)) };
}

/** サブチケット（サーバーの parent=… と同じく更新日時の新しい順） */
export function useSubTickets(parentId: number | null | undefined): { data: LocalTicket[]; isLoading: boolean } {
  const { tickets, isLoading } = useTicketList(parentId ? { parent: parentId } : {}, { enabled: !!parentId });
  return { data: tickets, isLoading };
}

/** ガント（期日あり、並び順 → 期日の順） */
export function useGanttTickets(scope: { projectKey?: string | null; teamSlug?: string | null }): { data: LocalTicket[]; isLoading: boolean } {
  const params: TicketListParams = {
    due_date__isnull: false,
    ordering: 'gantt_order,due_date',
    ...(scope.projectKey ? { project__prefix: scope.projectKey } : {}),
    ...(scope.teamSlug ? { team_slug: scope.teamSlug } : {}),
  };
  const { tickets, isLoading } = useTicketList(params);
  return { data: tickets, isLoading };
}

/** Cmd+K 用のチケット検索（端末内。キーとタイトルだけ。本文の全文検索はサーバー） */
export async function searchTicketsLocal(q: string, limit = 8): Promise<LocalTicket[]> {
  const rows = await db.tickets.toArray();
  return searchTickets(rows, q, limit);
}
