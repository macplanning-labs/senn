/**
 * projectWrites.ts — プロジェクトの書き込み（端末内 DB へ即時反映し、送信はキュー経由）
 *
 * 詳細設計 §6。キーはプロジェクト id（エンドポイントは /projects/{id}/）。
 * PATCH で送れない項目（名前・説明・プレフィックス）は PUT（method: 'put'）で全項目を送る。
 */

import { db, MAX_RETRY, type LocalProject, type SyncQueueItem } from './db';
import { requestPush } from './pushRequester';

/** POST /projects/ の本文（サーバー ProjectWriteIn と同じ） */
export interface ProjectCreateBody {
  name: string;
  prefix: string;
  description?: string;
  priority?: string;
  teamIds: number[];
}

function uuid(): string {
  const c = globalThis.crypto as Crypto | undefined;
  if (c?.randomUUID) return c.randomUUID();
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (ch) => {
    const r = (Math.random() * 16) | 0;
    return (ch === 'x' ? r : (r & 0x3) | 0x8).toString(16);
  });
}

function now(): string {
  return new Date().toISOString();
}

/**
 * プロジェクトを更新する。
 * method='patch'（既定）は PATCH /projects/{id}/（状態・優先度・自動サイクル・親）。
 * method='put' は PUT /projects/{id}/（名前・説明など。apiPatch に全項目を入れること）。
 */
export async function localUpdateProject(
  projectId: number,
  apiPatch: Record<string, unknown>,
  rowPatch: Partial<LocalProject>,
  method: 'patch' | 'put' = 'patch',
): Promise<void> {
  let touched = false;
  await db.transaction('rw', [db.projects, db.syncQueue], async () => {
    const row = await db.projects.get(projectId);
    if (!row || row._deleted) return;
    await db.projects.put({ ...row, ...rowPatch, _dirty: true, _syncError: null, updatedAt: row.updatedAt });
    touched = true;

    const items = await db.syncQueue
      .where('entityId')
      .equals(projectId)
      .and((q) => q.entity === 'project')
      .toArray();
    if (row._pendingCreate) {
      const create = items.find((q) => q.operation === 'create');
      if (create) {
        const payload = JSON.parse(create.payload) as { body: Record<string, unknown> };
        payload.body = { ...payload.body, ...apiPatch };
        await db.syncQueue.update(create.id!, { payload: JSON.stringify(payload) });
      }
      return;
    }
    // 同じ行・同じ送り方の更新は1件にまとめる（ticketWrites と同じ理由。失敗中の項目にもまとめる）
    const pending = items
      .filter((q) => {
        if (q.operation !== 'update') return false;
        const p = JSON.parse(q.payload) as { method?: string };
        return (p.method ?? 'patch') === method;
      })
      .pop();
    if (pending) {
      const payload = JSON.parse(pending.payload) as { key: number; patch: Record<string, unknown>; method?: string };
      payload.patch = { ...payload.patch, ...apiPatch };
      await db.syncQueue.update(pending.id!, {
        payload: JSON.stringify(payload),
        nextAttemptAt: undefined,
        ...(pending.retryCount >= MAX_RETRY ? { retryCount: 0, firstFailedAt: undefined, lastError: undefined } : {}),
      });
    } else {
      const item: SyncQueueItem = {
        entity: 'project',
        entityId: projectId,
        operation: 'update',
        payload: JSON.stringify({ key: projectId, patch: apiPatch, method }),
        createdAt: now(),
        retryCount: 0,
      };
      await db.syncQueue.add(item);
    }
  });
  if (touched) requestPush();
}

/** プロジェクトを作成する（仮 id は負数。送信が通ったら push 側で本 id に付け替える） */
export async function localCreateProject(
  body: ProjectCreateBody,
  preview: Partial<LocalProject> = {},
): Promise<{ tempId: number }> {
  const tempId = -(Date.now() * 1000 + Math.floor(Math.random() * 1000));
  const t = now();
  const row: LocalProject = {
    id: tempId,
    name: body.name,
    prefix: body.prefix,
    description: body.description ?? '',
    status: 'in_progress',
    priority: body.priority ?? 'medium',
    targetEndDate: null,
    ticketCount: 0,
    memberCount: 0,
    isMember: true,
    teams: [],
    ownerId: null,
    createdAt: t,
    updatedAt: t,
    cycleAutoComplete: false,
    cycleAutoCreateNext: false,
    parentProjectId: null,
    childCount: 0,
    roadmapIds: [],
    ...preview,
    _dirty: true,
    _syncedAt: null,
    _pendingCreate: true,
    _syncError: null,
  };
  row.id = tempId;
  await db.transaction('rw', [db.projects, db.syncQueue], async () => {
    await db.projects.put(row);
    await db.syncQueue.add({
      entity: 'project',
      entityId: tempId,
      operation: 'create',
      payload: JSON.stringify({ body }),
      createdAt: t,
      retryCount: 0,
      idempotencyKey: uuid(),
    });
  });
  requestPush();
  return { tempId };
}

/** プロジェクトを削除する */
export async function localDeleteProject(projectId: number): Promise<void> {
  let queued = false;
  await db.transaction('rw', [db.projects, db.syncQueue], async () => {
    const row = await db.projects.get(projectId);
    if (!row) return;
    queued = true;
    if (row._pendingCreate) {
      await db.projects.put({ ...row, _deleted: true });
      return;
    }
    await db.syncQueue
      .where('entityId')
      .equals(projectId)
      .and((q) => q.entity === 'project' && q.operation === 'update')
      .delete();
    await db.projects.put({ ...row, _deleted: true, _dirty: true });
    await db.syncQueue.add({
      entity: 'project',
      entityId: projectId,
      operation: 'delete',
      payload: JSON.stringify({ key: projectId }),
      createdAt: now(),
      retryCount: 0,
    });
  });
  if (queued) requestPush();
}
