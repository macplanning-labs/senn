/**
 * writeGateway.ts — サーバー由来の行を端末内 DB に書く唯一の入口
 *
 * 設計: docs/design/詳細設計書_リアルタイム同期_DeltaPush.md §6, §8.3
 *
 * リアルタイム（applyDelta）と差分同期（pullEntity）は、どちらもここを通して書く。
 * 経路ごとに別々に書くと「リアルタイムでは先祖返りしないのに、定期同期では先祖返りする」といった食い違いが出るため。
 * 版番号の比較・未送信の変更の保護・削除/見えなくなった行の扱い・親子の連鎖は、この関数の中にだけ書く。
 *
 * 呼び出し側のトランザクションの中で動く（SYNC_TABLES をすべて含めること）。
 * 画面への通知（トースト）と集計キャッシュの無効化は、トランザクションが確定した後に呼び出し側が行う（notifyEffects）。
 */

import Dexie from 'dexie';
import { useToastStore } from '../stores/toastStore';
import i18n from '../../i18n';
import { db, type LocalComment, type LocalProject, type LocalTicket } from './db';
import { shouldApply } from './lww';
import { toLocalComment, toLocalProject, toLocalTicket } from './ticketMapping';
import type { EntityName } from './realtime/packets';

/** 書き込みゲートウェイが扱う変更。v は差分同期が旧サーバーの応答（版番号なし）を受けたときだけ undefined */
export type GatewayChange =
  | { op: 'upsert'; entity: EntityName; id: number; v?: number; data: Record<string, unknown> }
  | { op: 'delete'; entity: EntityName; id: number; v?: number }
  | { op: 'evict'; entity: EntityName; id: number }
  | { op: 'stale'; entity: EntityName; id: number; v?: number };

export type GatewaySource = 'push' | 'pull';

export interface RemovedEffect {
  kind: 'removed';
  entity: EntityName;
  id: number;
  reason: 'delete' | 'evict';
  /** このとき捨てた送信待ちの件数 */
  droppedQueueItems: number;
}

export interface GatewayResult {
  /** 端末内 DB が実際に変わった行数 */
  changed: number;
  /** 変わった行の種別（画面側の後始末の判断に使う） */
  changedEntities: Set<EntityName>;
  /** 実データが無く、取り直しが要る行（stale。端末内 DB に表がある種別のみ） */
  stale: Array<{ entity: EntityName; id: number }>;
  /** 端末内 DB に表を持たない対象（添付・サイクル・通知など）の「変わった」という合図。キャッシュを取り直す */
  signals: Array<{ entity: EntityName; id: number }>;
  effects: RemovedEffect[];
}

/** 削除の記録を残す時間。これより古い upsert が遅れて届くことはない */
const TOMBSTONE_KEEP_MS = 10 * 60 * 1000;

/** ゲートウェイが読み書きする表。呼び出し側のトランザクションにすべて含める */
export function syncTables() {
  return [db.tickets, db.projects, db.comments, db.reactions, db.syncQueue, db.tombstones];
}

function tableOf(entity: EntityName) {
  if (entity === 'ticket') return db.tickets;
  if (entity === 'project') return db.projects;
  if (entity === 'comment') return db.comments;
  return null; // Phase 3 以降（reaction / wiki）
}

function toLocal(entity: EntityName, data: Record<string, unknown>): LocalTicket | LocalProject | LocalComment | null {
  if (entity === 'ticket') return toLocalTicket(data);
  if (entity === 'project') return toLocalProject(data);
  if (entity === 'comment') return toLocalComment(data);
  return null;
}

type Versioned = { v?: number; _dirty?: boolean; _pendingCreate?: boolean; _deleted?: boolean };

function hasLocalChanges(row: Versioned | undefined): boolean {
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

/** 端末内の行と中身が同じか（同じ行が再送されてきても、画面の再描画・集計キャッシュの無効化を起こさない） */
export function sameRow(existing: unknown, incoming: unknown): boolean {
  return existing !== undefined && stableKey(existing) === stableKey(incoming);
}

/**
 * 行と、その子（リアクションなど）を消し、送信待ちも捨てる。
 * 順番: ①送信待ち → ②子の行 → ③本人の行。戻り値は捨てた送信待ちの件数。
 * トランザクションの中なので、途中の状態は他の処理から見えない。
 */
async function removeWithChildren(entity: EntityName, id: number): Promise<number> {
  const queueEntity = entity === 'ticket' || entity === 'project' ? entity : null;
  let dropped = 0;
  if (queueEntity) {
    dropped += await db.syncQueue
      .where('entityId')
      .equals(id)
      .and((q) => q.entity === queueEntity)
      .delete();
  }
  if (entity === 'ticket') {
    // 子: このチケットのコメント（コメントは送信待ちを持たない。書き込みはサーバーへ直接）
    await db.comments.where('ticketId').equals(id).delete();
    // 子: このチケットへのリアクション（と、その送信待ち）
    const reactionIds = await db.reactions.where('ticketId').equals(id).primaryKeys();
    if (reactionIds.length > 0) {
      dropped += await db.syncQueue
        .where('entity')
        .equals('reaction')
        .and((q) => reactionIds.includes(q.entityId as number))
        .delete();
      await db.reactions.bulkDelete(reactionIds);
    }
  }
  const table = tableOf(entity);
  if (table) await table.delete(id);
  return dropped;
}

/**
 * サーバー由来の変更を端末内 DB に書く。トランザクションの中で呼ぶこと。
 * ネットワークには触らない（stale の取り直しは呼び出し側）。
 */
export async function applyServerChanges(changes: GatewayChange[]): Promise<GatewayResult> {
  if (!Dexie.currentTransaction) throw new Error('applyServerChanges must be called inside a Dexie transaction');
  const result: GatewayResult = { changed: 0, changedEntities: new Set(), stale: [], signals: [], effects: [] };

  for (const c of changes) {
    const table = tableOf(c.entity);
    if (!table) {
      // 端末内 DB に表が無い種別: 実データは無く、「変わった」という合図だけが来る
      if (c.op === 'stale') result.signals.push({ entity: c.entity, id: c.id });
      continue;
    }
    const local = (await table.get(c.id)) as Versioned | undefined;

    switch (c.op) {
      case 'upsert': {
        const next = toLocal(c.entity, c.data);
        if (!next) break;
        if (c.v === undefined) {
          // 版番号のない応答（旧サーバー）: 未送信の変更だけ守り、中身が同じなら書かない
          if (hasLocalChanges(local) || sameRow(local, next)) break;
        } else {
          const tomb = await db.tombstones.get([c.entity, c.id]);
          if (shouldApply(local, c.v, tomb) === 'skip') break;
          if (sameRow(local, next)) break;
        }
        await table.put(next as never);
        result.changed++;
        result.changedEntities.add(c.entity);
        break;
      }
      case 'delete': {
        // 手元の版が削除の版以上なら、削除より後の状態（再作成など）なので消さない
        if (c.v !== undefined && local && (local.v ?? 0) >= c.v) break;
        const existed = !!local;
        const dropped = await removeWithChildren(c.entity, c.id);
        if (c.v !== undefined) {
          await db.tombstones.put({ entity: c.entity, id: c.id, v: c.v, at: Date.now() });
        }
        if (existed) {
          result.effects.push({ kind: 'removed', entity: c.entity, id: c.id, reason: 'delete', droppedQueueItems: dropped });
          result.changed++;
          result.changedEntities.add(c.entity);
        }
        break;
      }
      case 'evict': {
        // 見えなくなっただけ（削除ではない）。版は比べない。未送信の変更があっても消す（送り先が無いため）
        if (!local) break;
        const dropped = await removeWithChildren(c.entity, c.id);
        result.effects.push({ kind: 'removed', entity: c.entity, id: c.id, reason: 'evict', droppedQueueItems: dropped });
        result.changed++;
        result.changedEntities.add(c.entity);
        break;
      }
      case 'stale':
        result.stale.push({ entity: c.entity, id: c.id });
        break;
    }
  }

  // 古い削除記録の掃除（書き込みのたびに軽く）
  if (result.changed > 0) {
    await db.tombstones.where('at').below(Date.now() - TOMBSTONE_KEEP_MS).delete();
  }
  return result;
}

/** トランザクションが確定した後に呼ぶ: 捨てた送信待ちがあれば知らせる */
export function notifyEffects(effects: RemovedEffect[]): void {
  const lost = effects.reduce((n, e) => n + e.droppedQueueItems, 0);
  if (lost > 0) {
    useToastStore.getState().addToast({
      type: 'info',
      message: i18n.t('sync.removedWithPending', { count: lost }),
    });
  }
}
