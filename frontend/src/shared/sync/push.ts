/**
 * push.ts — 送信キュー（syncQueue）のチケット・プロジェクト項目をサーバーへ送る
 *
 * 詳細設計 §3.5・§3.7。
 *
 * キュー項目の payload:
 *   update: { key, patch, method? }   key はチケットキー／プロジェクト id。method は 'patch'（既定）か 'put'
 *   create: { body }                   idempotencyKey は項目側に持つ
 *   delete: { key }
 *
 * 大事な約束:
 * - Dexie のトランザクションの中で通信しない（トランザクションが自動で閉じてしまう）。通信 → 結果をトランザクションで反映、の順
 * - 送信中に同じ項目へ変更が追記されていたら（payload が変わっていたら）、項目は消さずに次回もう一度送る
 */

import { apiClient } from '../api/client';
import { useToastStore } from '../stores/toastStore';
import i18n from '../../i18n';
import { db, type LocalTicket, type SyncQueueItem } from './db';
import { toLocalProject, toLocalTicket } from './ticketMapping';

export const MAX_RETRY = 5;

type HttpError = { response?: { status?: number; data?: { detail?: unknown } }; message?: string };

/** 再送しても通らない失敗（4xx。401 はトークン更新で直ることがあるので除く）か */
export function classifyError(err: unknown): 'reject' | 'retry' {
  const status = (err as HttpError)?.response?.status;
  if (typeof status === 'number' && status >= 400 && status < 500 && status !== 401) return 'reject';
  return 'retry';
}

function errorMessage(err: unknown): string {
  const detail = (err as HttpError)?.response?.data?.detail;
  if (typeof detail === 'string' && detail) return detail;
  return i18n.t('sync.pushRejected', '変更を保存できませんでした。元に戻しました');
}

function isNotFound(err: unknown): boolean {
  return (err as HttpError)?.response?.status === 404;
}

// ── 仮キー → 本キーの付け替え通知（MainLayout が URL を置き換える） ──
type RemapListener = (tempKey: string, realKey: string) => void;
const remapListeners = new Set<RemapListener>();

export function onKeyRemap(listener: RemapListener): () => void {
  remapListeners.add(listener);
  return () => {
    remapListeners.delete(listener);
  };
}

export function emitKeyRemap(tempKey: string, realKey: string): void {
  for (const l of remapListeners) {
    try {
      l(tempKey, realKey);
    } catch (e) {
      console.warn('[sync] key remap listener failed', e);
    }
  }
}

interface Payload {
  key?: string | number;
  patch?: Record<string, unknown>;
  method?: 'patch' | 'put';
  body?: Record<string, unknown>;
}

function parsePayload(item: SyncQueueItem): Payload {
  try {
    return JSON.parse(item.payload) as Payload;
  } catch {
    return {};
  }
}

function endpoint(entity: 'ticket' | 'project', key?: string | number): string {
  const base = entity === 'ticket' ? '/tickets/' : '/projects/';
  return key === undefined ? base : `${base}${key}/`;
}

async function send(item: SyncQueueItem, p: Payload): Promise<Record<string, unknown> | null> {
  const entity = item.entity as 'ticket' | 'project';
  switch (item.operation) {
    case 'update': {
      const url = endpoint(entity, p.key);
      const res =
        p.method === 'put'
          ? await apiClient.put<Record<string, unknown>>(url, p.patch ?? {})
          : await apiClient.patch<Record<string, unknown>>(url, p.patch ?? {});
      return res.data ?? null;
    }
    case 'create': {
      const res = await apiClient.post<Record<string, unknown>>(endpoint(entity), p.body ?? {}, {
        headers: item.idempotencyKey ? { 'Idempotency-Key': item.idempotencyKey } : undefined,
      });
      return res.data ?? null;
    }
    case 'delete': {
      try {
        await apiClient.delete(endpoint(entity, p.key));
      } catch (err) {
        if (!isNotFound(err)) throw err; // 既に無いなら成功扱い
      }
      return null;
    }
  }
}

/**
 * キュー項目（ticket / project）を1件送る。成功・失敗の後始末（キュー項目の削除、行の更新、再送回数）まで行う。
 * 戻り値: 'done'（成功）/ 'rejected'（再送しない失敗）/ 'retry'（後で再送）
 */
export async function pushEntityItem(item: SyncQueueItem): Promise<'done' | 'rejected' | 'retry'> {
  const p = parsePayload(item);
  if (item.operation === 'create' && item.retryCount === 0 && (await cancelledBeforeSend(item))) {
    return 'done';
  }
  let data: Record<string, unknown> | null;
  try {
    data = await send(item, p);
  } catch (err) {
    if (classifyError(err) === 'reject') {
      await applyReject(item, p, err);
      return 'rejected';
    }
    const message = (err as HttpError)?.message ?? String(err);
    if (item.id !== undefined) {
      await db.syncQueue.update(item.id, { retryCount: item.retryCount + 1, lastError: message });
    }
    return 'retry';
  }

  if (item.operation === 'update') await applyUpdateSuccess(item, data);
  else if (item.operation === 'create') await applyCreateSuccess(item, p, data);
  else await applyDeleteSuccess(item);
  return 'done';
}

/** 一度も送っていない作成が、送信前に削除されていたら作成ごと取り消す */
async function cancelledBeforeSend(item: SyncQueueItem): Promise<boolean> {
  const table = item.entity === 'ticket' ? db.tickets : db.projects;
  const id = item.entityId as number;
  const row = await table.get(id);
  if (!row?._deleted) return false;
  await db.transaction('rw', [table, db.syncQueue], async () => {
    await table.delete(id);
    await db.syncQueue
      .where('entityId')
      .equals(id)
      .and((q) => q.entity === item.entity)
      .delete();
  });
  return true;
}

async function applyUpdateSuccess(item: SyncQueueItem, data: Record<string, unknown> | null): Promise<void> {
  const isTicket = item.entity === 'ticket';
  const table = isTicket ? db.tickets : db.projects;
  await db.transaction('rw', [table, db.syncQueue], async () => {
    const current = item.id !== undefined ? await db.syncQueue.get(item.id) : undefined;
    if (!current) return; // 送信中に削除操作などで置き換わった
    if (current.payload !== item.payload) {
      // 送信中に変更が追記された。行は端末の最新のまま（_dirty）にして、次回もう一度送る
      await db.syncQueue.update(current.id!, { retryCount: 0, lastError: undefined });
      return;
    }
    await db.syncQueue.delete(current.id!);
    const row = await table.get(item.entityId as number);
    if (row?._deleted) return;
    // 他に送信待ちの項目が残っていれば、行はまだ端末側が正
    const others = await db.syncQueue
      .where('entityId')
      .equals(item.entityId)
      .and((q) => q.entity === item.entity)
      .count();
    if (others > 0) return;
    if (data && typeof data.id === 'number') {
      await table.put((isTicket ? toLocalTicket(data) : toLocalProject(data)) as never);
    } else if (row) {
      await table.update(row.id, { _dirty: false, _syncError: null, _syncedAt: new Date().toISOString() } as never);
    }
  });
}

async function applyDeleteSuccess(item: SyncQueueItem): Promise<void> {
  const table = item.entity === 'ticket' ? db.tickets : db.projects;
  await db.transaction('rw', [table, db.syncQueue], async () => {
    await table.delete(item.entityId as number);
    await db.syncQueue
      .where('entityId')
      .equals(item.entityId)
      .and((q) => q.entity === item.entity)
      .delete();
  });
}

/** 作成が通った: 仮 id / 仮キーを本物に付け替える（詳細設計 §3.7） */
async function applyCreateSuccess(item: SyncQueueItem, p: Payload, data: Record<string, unknown> | null): Promise<void> {
  if (!data || typeof data.id !== 'number') {
    if (item.id !== undefined) await db.syncQueue.delete(item.id);
    return;
  }
  const tempId = item.entityId as number;
  const realId = data.id;

  if (item.entity === 'project') {
    await db.transaction('rw', [db.projects, db.tickets, db.syncQueue], async () => {
      const temp = await db.projects.get(tempId);
      await db.projects.delete(tempId);
      await db.projects.put(toLocalProject(data, temp?._deleted ? { _deleted: true } : undefined));
      if (item.id !== undefined) await db.syncQueue.delete(item.id);
      for (const q of await db.syncQueue.toArray()) {
        const qp = parsePayload(q);
        let dirty = false;
        if (q.entity === 'project' && q.entityId === tempId) {
          q.entityId = realId;
          if (qp.key === tempId) qp.key = realId;
          dirty = true;
        }
        if (q.entity === 'ticket' && qp.body && qp.body.project === tempId) {
          qp.body.project = realId;
          dirty = true;
        }
        if (dirty) await db.syncQueue.put({ ...q, payload: JSON.stringify(qp) });
      }
      for (const t of await db.tickets.where('projectId').equals(tempId).toArray()) {
        await db.tickets.put({ ...t, project: realId, projectId: realId, projectPrefix: (data.prefix as string) ?? t.projectPrefix });
      }
    });
    return;
  }

  const realKey = String(data.ticketKey ?? data.ticket_key ?? '');
  let tempKey = '';
  await db.transaction('rw', [db.tickets, db.syncQueue, db.reactions], async () => {
    const temp = await db.tickets.get(tempId);
    tempKey = temp?.ticketKey ?? '';
    const current = item.id !== undefined ? await db.syncQueue.get(item.id) : undefined;
    if (item.id !== undefined) await db.syncQueue.delete(item.id);
    await db.tickets.delete(tempId);

    const server = toLocalTicket(data);
    if (temp?._deleted) {
      // 作成の送信中に削除された → 本物を削除キューに積む
      await db.tickets.put({ ...server, _deleted: true, _dirty: true });
      await db.syncQueue.add({
        entity: 'ticket',
        entityId: realId,
        operation: 'delete',
        payload: JSON.stringify({ key: realKey }),
        createdAt: new Date().toISOString(),
        retryCount: 0,
      });
    } else if (current && current.payload !== item.payload) {
      // 送信中に作成内容が変わった（端末で編集された）→ 差分を更新として送る
      const latest = parsePayload(current).body ?? {};
      const sent = p.body ?? {};
      const patch: Record<string, unknown> = {};
      for (const [k, v] of Object.entries(latest)) {
        if (JSON.stringify(v) !== JSON.stringify(sent[k])) patch[k] = v;
      }
      const merged: LocalTicket = temp
        ? { ...temp, id: realId, ticketKey: realKey, _pendingCreate: undefined, _dirty: true }
        : { ...server, _dirty: true };
      delete merged._pendingCreate;
      await db.tickets.put(merged);
      if (Object.keys(patch).length > 0) {
        await db.syncQueue.add({
          entity: 'ticket',
          entityId: realId,
          operation: 'update',
          payload: JSON.stringify({ key: realKey, patch }),
          createdAt: new Date().toISOString(),
          retryCount: 0,
        });
      }
    } else {
      await db.tickets.put(server);
    }

    // 後続のキュー項目・子チケット・リアクションを付け替える
    for (const q of await db.syncQueue.toArray()) {
      const qp = parsePayload(q) as Payload & { ticketKey?: string };
      let dirty = false;
      if (q.entity === 'ticket' && q.entityId === tempId) {
        q.entityId = realId;
        dirty = true;
      }
      if (q.entity === 'ticket' && qp.key === tempKey && tempKey) {
        qp.key = realKey;
        dirty = true;
      }
      if (q.entity === 'ticket' && qp.body && qp.body.parent === tempId) {
        qp.body.parent = realId;
        dirty = true;
      }
      if (q.entity === 'ticket' && qp.patch && qp.patch.parent === tempId) {
        qp.patch.parent = realId;
        dirty = true;
      }
      if (q.entity === 'reaction' && qp.ticketKey === tempKey && tempKey) {
        qp.ticketKey = realKey;
        dirty = true;
      }
      if (dirty) await db.syncQueue.put({ ...q, payload: JSON.stringify(qp) });
    }
    for (const child of await db.tickets.where('parentId').equals(tempId).toArray()) {
      await db.tickets.put({ ...child, parent: realId, parentId: realId });
    }
    for (const r of await db.reactions.where('ticketId').equals(tempId).toArray()) {
      await db.reactions.put({ ...r, ticketId: realId });
    }
  });
  if (tempKey && realKey) emitKeyRemap(tempKey, realKey);
}

/** 4xx: 再送しない。キュー項目を消し、行をサーバーの値へ戻して知らせる */
async function applyReject(item: SyncQueueItem, p: Payload, err: unknown): Promise<void> {
  const isTicket = item.entity === 'ticket';
  const table = isTicket ? db.tickets : db.projects;
  const message = errorMessage(err);

  // 通信はトランザクションの外で
  let fresh: Record<string, unknown> | null = null;
  let freshMissing = false;
  if (item.operation !== 'create' && p.key !== undefined) {
    try {
      fresh = (await apiClient.get<Record<string, unknown>>(endpoint(item.entity as 'ticket' | 'project', p.key))).data;
    } catch (e) {
      freshMissing = isNotFound(e);
    }
  }

  await db.transaction('rw', [table, db.syncQueue], async () => {
    // この行の送信待ちはまとめて捨てる（元に戻すため）
    await db.syncQueue
      .where('entityId')
      .equals(item.entityId)
      .and((q) => q.entity === item.entity)
      .delete();
    const id = item.entityId as number;
    if (item.operation === 'create' || freshMissing) {
      await table.delete(id);
      return;
    }
    if (fresh && typeof fresh.id === 'number') {
      const row = isTicket ? toLocalTicket(fresh, { _syncError: message }) : toLocalProject(fresh, { _syncError: message });
      await table.put(row as never);
    } else {
      // 取り直せなかった（オフライン等）。次回の pull でサーバーの値に戻る
      await table.update(id, { _dirty: false, _deleted: false, _syncError: message } as never);
    }
  });

  useToastStore.getState().addToast({ type: 'error', message });
}
