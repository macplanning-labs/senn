/**
 * syncEngine.ts — ローカルファースト同期エンジン
 *
 * LWW (Last Write Wins) 方式の同期:
 *   1. ローカル変更 → IndexedDB に即時保存 + SyncQueue に追加
 *   2. オンライン時 → SyncQueue を順次処理してサーバーに送信
 *   3. サーバーからの最新データ → ローカルを上書き（LWW）
 *
 * Phase 2 では CRDT に移行予定。MVP では LWW で十分。
 */

import { db, type SyncQueueItem } from './db';
import { apiClient } from '../api/client';

// ── 同期状態 ──
let isSyncing = false;
let syncInterval: ReturnType<typeof setInterval> | null = null;

/**
 * サーバーからチケット一覧を取得してローカルDBに保存
 */
export async function pullTickets(): Promise<void> {
  try {
    const res = await apiClient.get<{ results: Record<string, unknown>[] }>('/tickets/', {
      params: { ordering: '-updated_at' },
    });

    const tickets = res.data.results ?? [];

    await db.transaction('rw', db.tickets, async () => {
      for (const t of tickets) {
        const existing = await db.tickets.get(t.id as number);
        // ローカルにdirtyな変更がなければ上書き
        if (!existing || !existing._dirty) {
          await db.tickets.put({
            id: t.id as number,
            ticketKey: (t.ticketKey ?? t.ticket_key ?? '') as string,
            title: (t.title ?? '') as string,
            description: (t.description ?? '') as string,
            status: (t.status ?? 'open') as string,
            priority: (t.priority ?? 'medium') as string,
            ticketType: (t.ticketType ?? t.ticket_type ?? 'issue') as string,
            assigneeId: ((t.assignees as { id: number }[]) ?? [])[0]?.id ?? null,
            projectId: (t.project as number | null) ?? null,
            dueDate: (t.dueDate ?? t.due_date ?? null) as string | null,
            updatedAt: (t.updatedAt ?? t.updated_at ?? new Date().toISOString()) as string,
            _dirty: false,
            _syncedAt: new Date().toISOString(),
          });
        }
      }
    });
  } catch {
    // オフライン時は無視（ローカルデータを使用）
  }
}

/**
 * サーバーからプロジェクト一覧を取得してローカルDBに保存
 */
export async function pullProjects(): Promise<void> {
  try {
    const res = await apiClient.get<{ results: Record<string, unknown>[] }>('/projects/');
    const projects = res.data.results ?? [];

    await db.transaction('rw', db.projects, async () => {
      for (const p of projects) {
        const existing = await db.projects.get(p.id as number);
        if (!existing || !existing._dirty) {
          await db.projects.put({
            id: p.id as number,
            name: (p.name ?? '') as string,
            prefix: (p.prefix ?? '') as string,
            description: (p.description ?? '') as string,
            updatedAt: (p.updatedAt ?? p.updated_at ?? new Date().toISOString()) as string,
            _dirty: false,
            _syncedAt: new Date().toISOString(),
          });
        }
      }
    });
  } catch {
    // オフライン時は無視
  }
}

/**
 * SyncQueue の未送信変更をサーバーに送信
 */
export async function pushChanges(): Promise<void> {
  if (isSyncing) return;
  isSyncing = true;

  try {
    const queue = await db.syncQueue
      .where('retryCount')
      .below(5) // 5回以上リトライ失敗は無視
      .toArray();

    for (const item of queue) {
      try {
        await pushSingleChange(item);
        // 成功したらキューから削除
        if (item.id) {
          await db.syncQueue.delete(item.id);
        }
        // ローカルの _dirty フラグをクリア
        await clearDirtyFlag(item.entity, item.entityId);
      } catch {
        // リトライカウントを増加
        if (item.id) {
          await db.syncQueue.update(item.id, {
            retryCount: item.retryCount + 1,
          });
        }
      }
    }
  } catch {
    // キュー取得自体の失敗（DBスキーマ不整合など）は無視
  } finally {
    isSyncing = false;
  }
}

async function pushSingleChange(item: SyncQueueItem): Promise<void> {
  const payload = JSON.parse(item.payload) as Record<string, unknown>;
  const endpoints: Record<string, string> = {
    ticket: '/tickets',
    project: '/projects',
    wiki: '/wiki',
  };
  const base = endpoints[item.entity] ?? '/tickets';

  switch (item.operation) {
    case 'create':
      await apiClient.post(`${base}/`, payload);
      break;
    case 'update':
      await apiClient.patch(`${base}/${item.entityId}/`, payload);
      break;
    case 'delete':
      await apiClient.delete(`${base}/${item.entityId}/`);
      break;
  }
}

async function clearDirtyFlag(
  entity: string,
  entityId: number,
): Promise<void> {
  const table =
    entity === 'ticket'
      ? db.tickets
      : entity === 'project'
        ? db.projects
        : db.wikiPages;

  await table.update(entityId, {
    _dirty: false,
    _syncedAt: new Date().toISOString(),
  } as Record<string, unknown>);
}

/**
 * ローカルに変更を保存し、SyncQueue に追加
 */
export async function localUpdate(
  entity: 'ticket' | 'project' | 'wiki',
  entityId: number,
  patch: Record<string, unknown>,
): Promise<void> {
  const table =
    entity === 'ticket'
      ? db.tickets
      : entity === 'project'
        ? db.projects
        : db.wikiPages;

  // ローカルDB を即時更新
  await table.update(entityId, {
    ...patch,
    _dirty: true,
    updatedAt: new Date().toISOString(),
  } as Record<string, unknown>);

  // SyncQueue に追加
  await db.syncQueue.add({
    entity,
    entityId,
    operation: 'update',
    payload: JSON.stringify(patch),
    createdAt: new Date().toISOString(),
    retryCount: 0,
  });

  // オンラインなら即時プッシュ
  if (navigator.onLine) {
    void pushChanges();
  }
}

/**
 * 定期同期の開始（30秒間隔）
 */
export function startSync(): void {
  if (syncInterval) return;

  // 初回同期
  void pullTickets();
  void pullProjects();
  void pushChanges();

  // 定期実行
  syncInterval = setInterval(() => {
    void pullTickets();
    void pullProjects();
    void pushChanges();
  }, 30_000);

  // オンライン復帰時に即時同期
  window.addEventListener('online', () => {
    void pushChanges();
    void pullTickets();
    void pullProjects();
  });
}

/**
 * 同期の停止
 */
export function stopSync(): void {
  if (syncInterval) {
    clearInterval(syncInterval);
    syncInterval = null;
  }
}
