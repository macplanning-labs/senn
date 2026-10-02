/**
 * pull.ts — 差分同期 API（GET /api/v1/sync/{tickets|projects}/）から端末内 DB へ取り込む
 *
 * 詳細設計 §3.4。
 * - cursor なし＝フル同期。hasMore=false まで繰り返し、最後に「受け取らなかった行」を消す
 * - ローカルで未送信の変更がある行（_dirty / _pendingCreate / _deleted）は上書きしない
 * - サーバーで削除された行・見られなくなった行は、未送信の変更があっても消す（送り先が無いため）
 * - チケットは access（見られる範囲）から外れた行を消す。範囲が増えたらフル同期し直す
 */

import type { Table } from 'dexie';
import type { QueryClient, QueryKey } from '@tanstack/react-query';
import { apiClient } from '../api/client';
import { TICKET_DASHBOARD_INVALIDATE_KEYS } from '../utils/ticketQueryInvalidation';
import { db, type LocalTicket, type SyncAccess, type SyncMeta } from './db';
import { applyServerChanges, notifyEffects, sameRow, syncTables, type GatewayChange, type GatewayResult } from './writeGateway';

export { sameRow };

export type SyncEntity = 'tickets' | 'projects' | 'comments';

interface SyncPage {
  changes: Record<string, unknown>[];
  deleted: Array<{ id: number; key?: string | null; v?: number }>;
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
 * 画面が一時的に覚えているサーバーの応答(React Query のキャッシュ)をすべて消す。
 * アカウントが切り替わるとき(ログアウト・新しいログイン)に呼ぶ。消さないと、前の利用者の
 * チーム名などが、次の利用者に最大 staleTime の間見えてしまう(DEMO-000166)
 */
export function clearCachedQueries(): void {
  queryClient?.clear();
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

/**
 * コメントが変わった。詳細画面の付随データ（添付など。コメントの行ではないもの）を取り直す。
 * pull 自身の invalidate なので、下の橋渡し（同期を起こし直す）は止める。
 */
export function onCommentsChanged(): void {
  invalidateFromSync([['ticket']]);
}

/**
 * 同期・リアルタイムの側から、react-query のキャッシュを取り直させる。
 * 同期自身の invalidate なので、下の橋渡し（同期を起こし直す）は止める。
 */
export function invalidateFromSync(keys: QueryKey[]): void {
  if (!queryClient) return;
  invalidatingFromSync = true;
  try {
    for (const queryKey of keys) void queryClient.invalidateQueries({ queryKey });
  } finally {
    invalidatingFromSync = false;
  }
}

/** そのチケットの詳細を開いている（付随データを取得済み）か */
export function isTicketDetailLoaded(ticketKey: string): boolean {
  return !!queryClient?.getQueryState(['ticket', ticketKey]);
}

export function onRowsChanged(): void {
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

/** 見える範囲が変わったか(どちらかが無い場合は、変わっていない扱い) */
export function accessChanged(prev: SyncAccess | null | undefined, next: SyncAccess | null | undefined): boolean {
  if (!prev || !next) return false;
  if (prev.all !== next.all) return true;
  const key = (a: SyncAccess) =>
    JSON.stringify([
      [...a.teamIds].sort((x, y) => x - y),
      a.scopedProjects.map((s) => `${s.teamId}:${s.projectId}`).sort(),
    ]);
  return key(prev) !== key(next);
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

/** ゲートウェイの結果を後始末する（トランザクションの確定後）。変わった行数を返す */
function settle(result: GatewayResult): number {
  notifyEffects(result.effects);
  return result.changed;
}

/**
 * 1エンティティ分の pull。戻り値は変わった行数。
 * 呼び出し側（syncEngine）がタブ内・タブ間の排他を行う前提。
 */
export async function pullEntity(entity: SyncEntity): Promise<{ changed: number }> {
  const table: Table<{ id: number; _pendingCreate?: boolean; _dirty?: boolean; _deleted?: boolean }, number> =
    (entity === 'tickets' ? db.tickets : entity === 'projects' ? db.projects : db.comments) as never;
  const rowEntity = entity === 'tickets' ? 'ticket' : entity === 'projects' ? 'project' : 'comment';

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

    const gatewayChanges: GatewayChange[] = [];
    for (const dto of page.changes ?? []) {
      const id = dto.id as number;
      seen.add(id);
      gatewayChanges.push({ op: 'upsert', entity: rowEntity, id, v: typeof dto.v === 'number' ? dto.v : undefined, data: dto });
    }
    for (const del of page.deleted ?? []) {
      // key が null = 見られなくなっただけ（チーム移動・権限の縮小）。key がある = 削除
      gatewayChanges.push(
        del.key === null || del.key === undefined
          ? { op: 'evict', entity: rowEntity, id: del.id }
          : { op: 'delete', entity: rowEntity, id: del.id, v: del.v },
      );
    }
    const result = await db.transaction('rw', [...syncTables(), db.syncMeta], async () => {
      const applied = await applyServerChanges(gatewayChanges);
      const finished = !page.hasMore;
      await db.syncMeta.put({
        entity,
        cursor: page.cursor,
        access: page.access ?? null,
        lastFullSyncAt: full && finished ? new Date().toISOString() : (prevMeta?.lastFullSyncAt ?? null),
      });
      return applied;
    });
    changed += settle(result);
    lastAccess = page.access ?? lastAccess;

    if (!page.hasMore) break;
    cursor = page.cursor;
  }

  // フル同期の仕上げ: 受け取らなかった行は、サーバーに無い（または見られない）
  if (full) {
    const result = await db.transaction('rw', syncTables(), async () => {
      const rows = await table.toArray();
      const gone: GatewayChange[] = rows
        .filter((row) => !(seen.has(row.id) || row.id < 0 || row._pendingCreate || row._dirty || row._deleted))
        .map((row) => ({ op: 'evict', entity: rowEntity, id: row.id }));
      return applyServerChanges(gone);
    });
    changed += settle(result);
  }

  if (entity === 'tickets' && lastAccess) {
    const access = lastAccess;
    // 見られなくなったチームの行を消す（メンバーから外れた・期限切れ。行自体は変わらないので差分では届かない）
    const result = await db.transaction('rw', syncTables(), async () => {
      const rows = await db.tickets.toArray();
      const gone: GatewayChange[] = rows
        .filter((row) => !(row._pendingCreate || isAccessible(row, access)))
        .map((row) => ({ op: 'evict', entity: 'ticket', id: row.id }));
      return applyServerChanges(gone);
    });
    changed += settle(result);
    // 見られる範囲が増えた → 増えたチームの古い行を取るためフル同期し直す
    if (!full && accessExpanded(prevMeta?.access, access)) {
      await db.syncMeta.update('tickets', { cursor: null });
      // コメントも同じ範囲で見えるので、増えたチームのコメントを取るためフル同期し直す
      await db.syncMeta.update('comments', { cursor: null });
      const again = await pullEntity('tickets');
      changed += again.changed;
    }
  }

  // プロジェクト: 見える範囲が変わった(アクセス制御の再設計で、サーバーが範囲を返すようになった)→ フル同期し直す。
  // 見えなくなったプロジェクトは差分では届かないため、フル同期の仕上げ(受け取らなかった行を消す)で消す
  if (entity === 'projects' && !full && accessChanged(prevMeta?.access, lastAccess)) {
    await db.syncMeta.update('projects', { cursor: null });
    const again = await pullEntity('projects');
    changed += again.changed;
  }

  if (changed > 0) {
    if (entity === 'comments') onCommentsChanged();
    else onRowsChanged();
  }
  return { changed };
}

/** 端末内 DB に初回のフル同期が済んでいるか */
export async function hasCompletedFullSync(entity: SyncEntity): Promise<boolean> {
  const meta = await db.syncMeta.get(entity);
  return !!meta?.lastFullSyncAt;
}
