/**
 * pull.ts — 差分同期 API（GET /api/v1/sync/{tickets|projects}/）から端末内 DB へ取り込む
 *
 * 詳細設計 §3.4。
 * - cursor なし＝フル同期。hasMore=false まで繰り返し、最後に「受け取らなかった行」を消す
 * - ローカルで未送信の変更がある行（_dirty / _pendingCreate / _deleted）は上書きしない
 * - サーバーで削除された行・見られなくなった行は、未送信の変更があっても消す（送り先が無いため）
 * - チケットは access（見られる範囲）から外れた行を消す。範囲が増えたらフル同期し直す
 */

import type { QueryClient, QueryKey } from '@tanstack/react-query';
import { apiClient } from '../api/client';
import { TICKET_DASHBOARD_INVALIDATE_KEYS } from '../utils/ticketQueryInvalidation';
import { db, type LocalTicket, type SyncAccess, type SyncMeta } from './db';
import { toLocalProject, toLocalTicket } from './ticketMapping';

export type SyncEntity = 'tickets' | 'projects';

interface SyncPage {
  changes: Record<string, unknown>[];
  deleted: Array<{ id: number; key?: string | null }>;
  access: SyncAccess;
  cursor: string;
  hasMore: boolean;
  serverTime: string;
}

const PAGE_LIMIT = 500;

/** 行の変化があったときに invalidate する、サーバー集計系のキー */
const AGGREGATE_KEYS: QueryKey[] = [
  ...TICKET_DASHBOARD_INVALIDATE_KEYS,
  ['cycle-progress'],
  ['dependency-graph'],
  ['project-activity'],
];

let queryClient: QueryClient | null = null;
/** pull 自身が invalidate している最中か（下の橋渡しで同期を起こし直さないため） */
let invalidatingFromSync = false;

/** 行（チケット・プロジェクト）を表す古いキャッシュキー。これを invalidate する処理は、サーバーへ直接書いた後の合図 */
const ROW_KEYS = new Set(['tickets', 'ticket', 'my-issues', 'projects', 'project']);

let syncTrigger: (() => void) | null = null;
let syncTimer: ReturnType<typeof setTimeout> | null = null;

/** syncEngine が「今すぐ同期」を登録する */
export function registerSyncTrigger(fn: () => void): void {
  syncTrigger = fn;
}

/**
 * QueryClient を登録する（App.tsx から）。
 * - pull で行が変わったら、サーバー集計系のキャッシュを invalidate する
 * - 逆に、画面がサーバーへ直接書いた後（添付付きの作成、プロジェクト作成、デモデータ作成など）に
 *   従来どおり ['tickets'] / ['projects'] 等を invalidate したら、それを合図に差分同期を走らせて
 *   端末内 DB にすぐ取り込む（橋渡し）
 */
export function registerSyncQueryClient(qc: QueryClient): void {
  queryClient = qc;
  const original = qc.invalidateQueries.bind(qc);
  qc.invalidateQueries = ((...args: Parameters<QueryClient['invalidateQueries']>) => {
    const head = args[0]?.queryKey?.[0];
    if (!invalidatingFromSync && typeof head === 'string' && ROW_KEYS.has(head) && syncTrigger) {
      if (syncTimer) clearTimeout(syncTimer);
      syncTimer = setTimeout(() => {
        syncTimer = null;
        syncTrigger?.();
      }, 200);
    }
    return original(...args);
  }) as QueryClient['invalidateQueries'];
}

function onRowsChanged(): void {
  if (!queryClient) return;
  invalidatingFromSync = true;
  try {
    for (const key of AGGREGATE_KEYS) {
      void queryClient.invalidateQueries({ queryKey: key });
    }
  } finally {
    invalidatingFromSync = false;
  }
}

/** access の範囲にチケット行が入るか（サーバー sync_repo::access_allows と同じ判定） */
export function isAccessible(row: Pick<LocalTicket, 'teamId' | 'projectId'>, access: SyncAccess | null | undefined): boolean {
  if (!access || access.all) return true;
  if (row.teamId == null) return false;
  if (access.teamIds.includes(row.teamId)) return true;
  return row.projectId != null && access.scopedProjects.some((s) => s.teamId === row.teamId && s.projectId === row.projectId);
}

/** 見られる範囲が増えたか（増えたチームの古い行は差分では届かないので、フル同期し直す） */
export function accessExpanded(prev: SyncAccess | null | undefined, next: SyncAccess | null | undefined): boolean {
  if (!prev || !next) return false;
  if (next.all) return !prev.all;
  if (prev.all) return false;
  const teams = new Set(prev.teamIds);
  if (next.teamIds.some((t) => !teams.has(t))) return true;
  const scoped = new Set(prev.scopedProjects.map((s) => `${s.teamId}:${s.projectId}`));
  // 以前からチーム全体が見えていたなら、そのチーム内のプロジェクト限定の権限が増えても見える範囲は増えていない
  return next.scopedProjects.some((s) => !teams.has(s.teamId) && !scoped.has(`${s.teamId}:${s.projectId}`));
}

function isGone(err: unknown): boolean {
  return (err as { response?: { status?: number } })?.response?.status === 410;
}

/** 行に紐づく送信待ち項目を消す（その行がもう存在しないとき） */
async function dropQueueFor(entity: 'ticket' | 'project', id: number): Promise<void> {
  await db.syncQueue
    .where('entityId')
    .equals(id)
    .and((q) => q.entity === entity)
    .delete();
}

function hasLocalChanges(row: { _dirty?: boolean; _pendingCreate?: boolean; _deleted?: boolean } | undefined): boolean {
  return !!row && (!!row._dirty || !!row._pendingCreate || !!row._deleted);
}

/** 比較用の文字列（キーの順番に依らない。_syncedAt は取り込んだ時刻なので比べない） */
function stableKey(value: unknown): string {
  return JSON.stringify(value, (k, v) =>
    k === '_syncedAt'
      ? undefined
      : v && typeof v === 'object' && !Array.isArray(v)
        ? Object.fromEntries(Object.keys(v as object).sort().map((key) => [key, (v as Record<string, unknown>)[key]]))
        : v,
  );
}

/** 端末内の行と中身が同じか（最終ページの巻き戻しで同じ行が再送されてくるため） */
export function sameRow(existing: unknown, incoming: unknown): boolean {
  return existing !== undefined && stableKey(existing) === stableKey(incoming);
}

/**
 * 1エンティティ分の pull。戻り値は変わった行数。
 * 呼び出し側（syncEngine）がタブ内・タブ間の排他を行う前提。
 */
export async function pullEntity(entity: SyncEntity): Promise<{ changed: number }> {
  const table = entity === 'tickets' ? db.tickets : db.projects;
  const queueEntity = entity === 'tickets' ? 'ticket' : 'project';
  const toLocal = entity === 'tickets' ? toLocalTicket : toLocalProject;

  const prevMeta: SyncMeta | undefined = await db.syncMeta.get(entity);
  let cursor: string | null = prevMeta?.cursor ?? null;
  let full = cursor === null;
  let retriedAfterGone = false;
  const seen = new Set<number>();
  let changed = 0;
  let lastAccess: SyncAccess | null = prevMeta?.access ?? null;

  for (;;) {
    let page: SyncPage;
    try {
      const res = await apiClient.get<SyncPage>(`/sync/${entity}/`, {
        params: { limit: PAGE_LIMIT, ...(cursor ? { cursor } : {}) },
      });
      page = res.data;
    } catch (err) {
      if (isGone(err) && !retriedAfterGone) {
        // cursor が古すぎる（削除記録の保持期間切れ）→ フル同期し直す
        retriedAfterGone = true;
        cursor = null;
        full = true;
        seen.clear();
        continue;
      }
      throw err;
    }

    await db.transaction('rw', [table, db.syncMeta, db.syncQueue], async () => {
      for (const dto of page.changes ?? []) {
        const id = dto.id as number;
        seen.add(id);
        const existing = await table.get(id);
        if (hasLocalChanges(existing)) continue;
        const next = toLocal(dto);
        // 中身が同じなら書かない（画面の再描画・集計キャッシュの invalidate を起こさない）
        if (sameRow(existing, next)) continue;
        await table.put(next as never);
        changed++;
      }
      for (const del of page.deleted ?? []) {
        if (await table.get(del.id)) {
          await table.delete(del.id);
          changed++;
        }
        await dropQueueFor(queueEntity, del.id);
      }
      const finished = !page.hasMore;
      await db.syncMeta.put({
        entity,
        cursor: page.cursor,
        access: page.access ?? null,
        lastFullSyncAt: full && finished ? new Date().toISOString() : (prevMeta?.lastFullSyncAt ?? null),
      });
    });
    lastAccess = page.access ?? lastAccess;

    if (!page.hasMore) break;
    cursor = page.cursor;
  }

  // フル同期の仕上げ: 受け取らなかった行は、サーバーに無い（または見られない）
  if (full) {
    await db.transaction('rw', [table, db.syncQueue], async () => {
      const rows = await table.toArray();
      for (const row of rows) {
        if (seen.has(row.id) || row._pendingCreate || row._dirty || row._deleted) continue;
        await table.delete(row.id);
        await dropQueueFor(queueEntity, row.id);
        changed++;
      }
    });
  }

  if (entity === 'tickets' && lastAccess) {
    const access = lastAccess;
    // 見られなくなったチームの行を消す（メンバーから外れた・期限切れ。行自体は変わらないので差分では届かない）
    await db.transaction('rw', [db.tickets, db.syncQueue], async () => {
      const rows = await db.tickets.toArray();
      for (const row of rows) {
        if (row._pendingCreate || isAccessible(row, access)) continue;
        await db.tickets.delete(row.id);
        await dropQueueFor('ticket', row.id);
        changed++;
      }
    });
    // 見られる範囲が増えた → 増えたチームの古い行を取るためフル同期し直す
    if (!full && accessExpanded(prevMeta?.access, access)) {
      await db.syncMeta.update('tickets', { cursor: null });
      const again = await pullEntity('tickets');
      changed += again.changed;
    }
  }

  if (changed > 0) onRowsChanged();
  return { changed };
}

/** 端末内 DB に初回のフル同期が済んでいるか */
export async function hasCompletedFullSync(entity: SyncEntity): Promise<boolean> {
  const meta = await db.syncMeta.get(entity);
  return !!meta?.lastFullSyncAt;
}
