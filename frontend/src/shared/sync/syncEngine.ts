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

import { db, type SyncQueueItem, type LocalReaction } from './db';
import { apiClient } from '../api/client';
import { registerBlobAdapter, pushBlobCreate, enqueueBlobCreate } from './blobSync';
import { customEmojiAdapter } from './adapters/customEmojiBlob';

// ── 同期状態 ──
let isSyncing = false;
let syncInterval: ReturnType<typeof setInterval> | null = null;

registerBlobAdapter(customEmojiAdapter);

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
 * 処理順: custom_emoji → reaction → その他（ticket/project/wiki）
 */
export async function pushChanges(): Promise<void> {
  if (isSyncing) return;
  isSyncing = true;

  try {
    const queue = await db.syncQueue
      .where('retryCount')
      .below(5) // 5回以上リトライ失敗は無視
      .toArray();

    // entity 種別でソート（custom_emoji を優先）
    const sorted = queue.sort((a, b) => {
      const order = { custom_emoji: 0, reaction: 1, ticket: 2, project: 3, wiki: 4 };
      return (order[a.entity as keyof typeof order] ?? 5) - (order[b.entity as keyof typeof order] ?? 5);
    });

    for (const item of sorted) {
      try {
        await pushSingleChange(item);
        // 成功したらキューから削除
        if (item.id) {
          await db.syncQueue.delete(item.id);
        }
        // custom_emoji create は remap 内で dirty クリア済み
        if (!(item.entity === 'custom_emoji' && item.operation === 'create')) {
          await clearDirtyFlag(item.entity, item.entityId);
        }
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

  if (item.entity === 'custom_emoji') {
    const projectPrefix = payload.projectPrefix as string;
    const localId = item.entityId as string;

    switch (item.operation) {
      case 'create': {
        await pushBlobCreate(item);
        break;
      }
      case 'delete':
        if (typeof localId === 'string' && !localId.startsWith('local-emoji-')) {
          await apiClient.delete(`/projects/${projectPrefix}/custom-emojis/${localId}/`);
        }
        break;
    }
  } else if (item.entity === 'reaction') {
    const ticketKey = payload.ticketKey as string;
    const reactionId = payload.reactionId as number | undefined;

    switch (item.operation) {
      case 'create': {
        const res = await apiClient.post<{ id: number }>(`/tickets/${ticketKey}/reactions/`, {
          emojiKind: payload.emojiKind,
          emojiValue: payload.emojiValue,
        });
        const serverId = res.data.id;
        const tempId = item.entityId as number;
        await db.transaction('rw', db.reactions, db.syncQueue, async () => {
          const temp = await db.reactions.get(tempId);
          if (temp) {
            await db.reactions.delete(tempId);
            await db.reactions.put({
              ...temp,
              id: serverId,
              _dirty: false,
              _syncedAt: new Date().toISOString(),
              _localId: undefined,
            });
          }
          const queued = await db.syncQueue.where('entity').equals('reaction').toArray();
          for (const q of queued) {
            if (!q.id) continue;
            const p = JSON.parse(q.payload) as Record<string, unknown>;
            if (q.operation === 'delete' && (p.reactionId === tempId || q.entityId === tempId)) {
              await db.syncQueue.update(q.id, {
                entityId: serverId,
                payload: JSON.stringify({ ...p, reactionId: serverId }),
              });
            }
          }
        });
        break;
      }
      case 'delete':
        if (reactionId != null && reactionId > 0) {
          await apiClient.delete(`/tickets/${ticketKey}/reactions/${reactionId}/`);
        }
        break;
    }
  } else {
    // 既存のエンドポイント
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
}

async function clearDirtyFlag(
  entity: string,
  entityId: number | string,
): Promise<void> {
  if (entity === 'reaction') {
    await db.reactions.update(entityId as number, {
      _dirty: false,
      _syncedAt: new Date().toISOString(),
    });
  } else if (entity === 'custom_emoji') {
    await db.customEmojis.update(entityId as string, {
      _dirty: false,
      _syncedAt: new Date().toISOString(),
    });
  } else {
    const table =
      entity === 'ticket'
        ? db.tickets
        : entity === 'project'
          ? db.projects
          : db.wikiPages;

    await table.update(entityId as number, {
      _dirty: false,
      _syncedAt: new Date().toISOString(),
    } as Record<string, unknown>);
  }
}

/**
 * リアクションをトグル（追加/削除）。ローカルDB即時更新＋SyncQueue
 */
export async function localToggleReaction(
  ticketKey: string,
  ticketId: number,
  userId: number,
  emojiKind: 'unicode' | 'custom',
  emojiValue: string,
): Promise<void> {
  // 論理キーで既存リアクションを検索
  const existing = await db.reactions
    .where('ticketId')
    .equals(ticketId)
    .and((r) => r.userId === userId && r.emojiKind === emojiKind && r.emojiValue === emojiValue)
    .first();

  const now = new Date().toISOString();

  if (existing) {
    const isUnsynced = existing.id < 0 || (existing._dirty && existing._syncedAt == null);

    await db.transaction('rw', db.reactions, db.syncQueue, async () => {
      if (isUnsynced) {
        const queued = await db.syncQueue
          .where('entity')
          .equals('reaction')
          .and((q) => q.entityId === existing.id && q.operation === 'create')
          .toArray();
        for (const q of queued) {
          if (q.id) await db.syncQueue.delete(q.id);
        }
      } else {
        await db.syncQueue.add({
          entity: 'reaction',
          entityId: existing.id,
          operation: 'delete',
          payload: JSON.stringify({
            ticketKey,
            reactionId: existing.id,
          }),
          createdAt: now,
          retryCount: 0,
        });
      }
      await db.reactions.delete(existing.id);
    });
  } else {
    const tempId = -Date.now();
    const newReaction: LocalReaction = {
      id: tempId,
      ticketId,
      userId,
      emojiKind,
      emojiValue,
      updatedAt: now,
      _dirty: true,
      _syncedAt: null,
      _localId: String(tempId),
    };

    await db.transaction('rw', db.reactions, db.syncQueue, async () => {
      await db.reactions.add(newReaction);
      await db.syncQueue.add({
        entity: 'reaction',
        entityId: tempId,
        operation: 'create',
        payload: JSON.stringify({
          ticketKey,
          emojiKind,
          emojiValue,
        }),
        createdAt: now,
        retryCount: 0,
      });
    });
  }

  // オンラインなら即時プッシュ
  if (navigator.onLine) {
    void pushChanges();
  }
}

/**
 * サーバーからリアクション一覧を取得してローカルDBに保存
 * _dirty=trueのローカル行は上書き禁止（ユーザーの未送信変更を保護）
 */
export async function pullReactions(ticketKey: string, ticketId: number): Promise<void> {
  try {
    const res = await apiClient.get<
      Record<string, unknown>[] | { results: Record<string, unknown>[] }
    >(`/tickets/${ticketKey}/reactions/`);
    const raw = res.data;
    const reactions = Array.isArray(raw) ? raw : (raw.results ?? []);

    await db.transaction('rw', db.reactions, async () => {
      const locals = await db.reactions.where('ticketId').equals(ticketId).toArray();
      const dirtyLogical = new Set(
        locals
          .filter((l) => l._dirty)
          .map((l) => `${l.userId}:${l.emojiKind}:${l.emojiValue}`),
      );
      const seenIds = new Set<number>();

      for (const r of reactions) {
        const reactionId = r.id as number;
        const userId = (r.userId ?? r.user_id ?? 0) as number;
        const emojiKind = (r.emojiKind ?? r.emoji_kind ?? 'unicode') as 'unicode' | 'custom';
        const emojiValue = (r.emojiValue ?? r.emoji_value ?? '') as string;
        const logicalKey = `${userId}:${emojiKind}:${emojiValue}`;

        if (dirtyLogical.has(logicalKey)) continue;

        const existing = await db.reactions.get(reactionId);
        if (existing?._dirty) continue;

        await db.reactions.put({
          id: reactionId,
          ticketId,
          userId,
          emojiKind,
          emojiValue,
          updatedAt: (r.updatedAt ?? r.updated_at ?? new Date().toISOString()) as string,
          _dirty: false,
          _syncedAt: new Date().toISOString(),
        });
        seenIds.add(reactionId);
      }

      for (const local of locals) {
        if (!local._dirty && local.id > 0 && !seenIds.has(local.id)) {
          await db.reactions.delete(local.id);
        }
      }
    });
  } catch {
    // オフライン時は無視
  }
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
 * 定期実行で pullReactions は呼ばない（詳細オープン・focus時に明示的に呼ぶ）
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
    // pullReactionsは呼ばない（詳細オープン・focus時に明示的に呼ぶ）
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

/**
 * カスタム絵文字アップロード（オフライン対応）
 * pendingBlobs + customEmojis + SyncQueue（blobSync.enqueueBlobCreate）
 */
export async function localUploadCustomEmoji(
  projectPrefix: string,
  projectId: number,
  file: File,
  slug: string,
  name: string,
): Promise<string> {
  return enqueueBlobCreate(
    customEmojiAdapter,
    {
      projectPrefix,
      projectId,
      slug,
      name,
    },
    file,
  );
}

/**
 * カスタム絵文字を削除（オフライン対応）
 */
export async function localDeleteCustomEmoji(
  projectPrefix: string,
  emojiId: string,
): Promise<void> {
  const now = new Date().toISOString();

  await db.transaction(
    'rw',
    db.customEmojis,
    db.pendingBlobs,
    db.syncQueue,
    async () => {
      const emoji = await db.customEmojis.get(emojiId);
      if (!emoji) return;

      if (emoji.imageUrl.startsWith('blob:')) {
        URL.revokeObjectURL(emoji.imageUrl);
      }

      await db.customEmojis.delete(emojiId);
      await db.pendingBlobs.delete(emojiId);

      const queued = await db.syncQueue
        .where('entity')
        .equals('custom_emoji')
        .toArray();
      for (const q of queued) {
        if (q.entityId === emojiId && q.id) {
          await db.syncQueue.delete(q.id);
        }
      }

      if (!emojiId.startsWith('local-emoji-')) {
        await db.syncQueue.add({
          entity: 'custom_emoji',
          entityId: emojiId,
          operation: 'delete',
          payload: JSON.stringify({ projectPrefix }),
          createdAt: now,
          retryCount: 0,
        });
      }
    },
  );

  if (navigator.onLine) {
    void pushChanges();
  }
}

/**
 * カスタム絵文字を pull（プロジェクト単位）
 * _dirty=true の行は上書き禁止
 */
export async function pullCustomEmojis(projectPrefix: string, projectId: number): Promise<void> {
  try {
    const res = await apiClient.get<
      Record<string, unknown>[] | { results: Record<string, unknown>[] }
    >(`/projects/${projectPrefix}/custom-emojis/`);
    const raw = res.data;
    const emojis = Array.isArray(raw) ? raw : (raw.results ?? []);

    await db.transaction('rw', db.customEmojis, async () => {
      const locals = await db.customEmojis.where('projectId').equals(projectId).toArray();
      const dirtySet = new Set(locals.filter((l) => l._dirty).map((l) => l.id));
      const seenIds = new Set<string>();

      for (const e of emojis) {
        const emojiId = e.id?.toString() ?? '';
        if (dirtySet.has(emojiId)) continue;

        await db.customEmojis.put({
          id: emojiId,
          projectId,
          slug: (e.slug ?? '') as string,
          name: (e.name ?? '') as string,
          imageUrl: (e.imageUrl ?? e.image_url ?? '') as string,
          updatedAt: (e.updatedAt ?? e.updated_at ?? new Date().toISOString()) as string,
          _dirty: false,
          _syncedAt: new Date().toISOString(),
        });
        seenIds.add(emojiId);
      }

      for (const local of locals) {
        if (!local._dirty && !local.id.startsWith('local-emoji-') && !seenIds.has(local.id)) {
          await db.customEmojis.delete(local.id);
        }
      }
    });
  } catch {
    // オフライン時は無視
  }
}
