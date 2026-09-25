/**
 * failedChanges.ts — 同期失敗した変更（retryCount >= MAX_RETRY）の管理
 *
 * React に依存しないロジック層。同期失敗の一覧取得・再試行・破棄を行う。
 */

import { apiClient } from '../api/client';
import { db, type SyncQueueItem } from './db';
import { toLocalTicket, toLocalProject } from './ticketMapping';
import { MAX_RETRY } from './push';
import { requestPush } from './pushRequester';
import { useSyncStatus } from './syncStatusStore';

export interface FailedChange {
  id: number;
  entity: SyncQueueItem['entity'];
  entityId: number | string;
  operation: SyncQueueItem['operation'];
  label: string;
  lastError: string | undefined;
  createdAt: string;
}

/**
 * retryCount >= MAX_RETRY の項目を createdAt 昇順で取得
 *
 * label は以下の規則で生成：
 * - ticket の update/delete: 端末内の行から「キー タイトル」
 * - ticket の create: payload.body.title
 * - project の update/delete: payload.body.name か payload.body.key
 * - project の create: payload.body.name
 * - その他: entity と entityId を結合した値
 */
export async function listFailedChanges(): Promise<FailedChange[]> {
  const queue = await db.syncQueue.where('retryCount').aboveOrEqual(MAX_RETRY).toArray();
  queue.sort((a, b) => new Date(a.createdAt).getTime() - new Date(b.createdAt).getTime());

  const results: FailedChange[] = [];

  for (const item of queue) {
    const label = await getLabel(item);
    results.push({
      id: item.id!,
      entity: item.entity,
      entityId: item.entityId,
      operation: item.operation,
      label,
      lastError: item.lastError,
      createdAt: item.createdAt,
    });
  }

  return results;
}

/**
 * ラベルを生成する内部関数
 */
async function getLabel(item: SyncQueueItem): Promise<string> {
  const payload = parsePayload(item);

  if (item.entity === 'ticket') {
    if (item.operation === 'create') {
      const title = (payload.body as Record<string, unknown> | undefined)?.title as string | undefined;
      return title || `(${item.entityId})`;
    }

    // update / delete: 端末内の行からタイトルを引く（liveQuery の中で呼ばれるので通信しない）
    const key = payload.key as string | undefined;
    const row = await db.tickets.get(item.entityId as number);
    if (row) return `${row.ticketKey} ${row.title}`;
    if (key) return key;
    return `(${item.entityId})`;
  }

  if (item.entity === 'project') {
    const body = payload.body as Record<string, unknown> | undefined;
    const name = (body?.name ?? body?.key) as string | undefined;
    return name || `(${item.entityId})`;
  }

  // wiki / reaction / custom_emoji など
  return `${item.entity} ${item.entityId}`;
}

function parsePayload(item: SyncQueueItem): Record<string, unknown> {
  try {
    return JSON.parse(item.payload) as Record<string, unknown>;
  } catch {
    return {};
  }
}

/**
 * 失敗した項目を再試行（retryCount を 0 にして送信開始）
 */
export async function retryFailedChange(id: number): Promise<void> {
  const item = await db.syncQueue.get(id);
  if (!item) return;

  await db.syncQueue.update(id, { retryCount: 0, lastError: undefined });
  requestPush();
  await useSyncStatus.getState().refreshQueueCounts();
}

/**
 * すべての失敗した項目を再試行
 */
export async function retryAllFailedChanges(): Promise<void> {
  const queue = await db.syncQueue.where('retryCount').aboveOrEqual(MAX_RETRY).toArray();
  if (queue.length === 0) return;

  const updates: Array<[number, { retryCount: number; lastError: undefined }]> = queue.map((item) => [
    item.id!,
    { retryCount: 0, lastError: undefined },
  ]);

  for (const [itemId, update] of updates) {
    await db.syncQueue.update(itemId, update);
  }

  requestPush();
  await useSyncStatus.getState().refreshQueueCounts();
}

/**
 * 失敗した項目を破棄（キュー項目を削除し、行をサーバーの値に戻す）
 *
 * - ticket の update/delete: ensureTicketLocal() で最新版を取得、404 なら削除
 * - ticket の create: 仮行を削除
 * - project の update/delete: サーバーから取得、404 なら削除
 * - project の create: 仮行を削除
 * - 他（wiki / reaction / custom_emoji）: キュー項目削除のみ
 */
export async function discardFailedChange(id: number): Promise<void> {
  const item = await db.syncQueue.get(id);
  if (!item) return;

  const payload = parsePayload(item);

  if (item.entity === 'ticket') {
    if (item.operation === 'create') {
      // 仮行を削除
      await db.tickets.delete(item.entityId as number);
      await db.syncQueue.delete(id);
    } else {
      // update / delete: サーバーから取り直す
      const key = payload.key as string | undefined;
      if (key) {
        try {
          const res = await apiClient.get<Record<string, unknown>>(`/tickets/${key}/`);
          const row = toLocalTicket(res.data);
          await db.transaction('rw', [db.tickets, db.syncQueue], async () => {
            // _dirty / _deleted / _syncError を落とす
            await db.tickets.put({
              ...row,
              _dirty: false,
              _deleted: false,
              _syncError: null,
            });
            await db.syncQueue.delete(id);
          });
        } catch (err) {
          const is404 = (err as { response?: { status?: number } })?.response?.status === 404;
          if (is404) {
            // チケットが削除された
            await db.transaction('rw', [db.tickets, db.syncQueue], async () => {
              const row = await db.tickets.get(item.entityId as number);
              if (row) {
                await db.tickets.delete(item.entityId as number);
              }
              await db.syncQueue.delete(id);
            });
          } else {
            // オフライン等。DB の行をリセット（次回の pull で正しい値に戻る）
            await db.transaction('rw', [db.tickets, db.syncQueue], async () => {
              const row = await db.tickets.get(item.entityId as number);
              if (row) {
                await db.tickets.update(item.entityId as number, {
                  _dirty: false,
                  _deleted: false,
                  _syncError: null,
                });
              }
              await db.syncQueue.delete(id);
            });
          }
        }
      } else {
        // key がない場合はキュー項目削除のみ
        await db.syncQueue.delete(id);
      }
    }
  } else if (item.entity === 'project') {
    if (item.operation === 'create') {
      // 仮行を削除
      await db.projects.delete(item.entityId as number);
      await db.syncQueue.delete(id);
    } else {
      // update / delete: サーバーから取得
      try {
        const res = await apiClient.get<Record<string, unknown>>(`/projects/${item.entityId}/`);
        const row = toLocalProject(res.data);
        await db.transaction('rw', [db.projects, db.syncQueue], async () => {
          await db.projects.put({
            ...row,
            _dirty: false,
            _deleted: false,
            _syncError: null,
          });
          await db.syncQueue.delete(id);
        });
      } catch (err) {
        const is404 = (err as { response?: { status?: number } })?.response?.status === 404;
        if (is404) {
          // プロジェクトが削除された
          await db.transaction('rw', [db.projects, db.syncQueue], async () => {
            const row = await db.projects.get(item.entityId as number);
            if (row) {
              await db.projects.delete(item.entityId as number);
            }
            await db.syncQueue.delete(id);
          });
        } else {
          // オフライン等
          await db.transaction('rw', [db.projects, db.syncQueue], async () => {
            const row = await db.projects.get(item.entityId as number);
            if (row) {
              await db.projects.update(item.entityId as number, {
                _dirty: false,
                _deleted: false,
                _syncError: null,
              });
            }
            await db.syncQueue.delete(id);
          });
        }
      }
    }
  } else {
    // wiki / reaction / custom_emoji: キュー項目削除のみ
    await db.syncQueue.delete(id);
  }

  await useSyncStatus.getState().refreshQueueCounts();
}
