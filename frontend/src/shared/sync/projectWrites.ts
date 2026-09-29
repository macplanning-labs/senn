/**
 * projectWrites.ts — プロジェクトの書き込み（端末内 DB へ即時反映し、送信はキュー経由）
 *
 * 詳細設計 §6。キーはプロジェクト id（エンドポイントは /projects/{id}/）。
 * PATCH で送れない項目（名前・説明・プレフィックス）は PUT（method: 'put'）で全項目を送る。
 */

import { db, MAX_RETRY, type LocalProject, type SyncQueueItem } from './db';
import { requestPush } from './pushRequester';
import { toLocalProject } from './ticketMapping';

function now(): string {
  return new Date().toISOString();
}

/** サーバー直で作成・取得したプロジェクトを端末内 DB へ書き込む（未送信の変更がある行は上書きしない） */
export async function writeThroughProject(dto: Record<string, unknown>): Promise<void> {
  const row = toLocalProject(dto);
  await db.transaction('rw', db.projects, async () => {
    const existing = await db.projects.get(row.id);
    if (existing?._dirty) return;
    await db.projects.put({ ...row, _dirty: false, _syncedAt: new Date().toISOString(), _syncError: null });
  });
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

