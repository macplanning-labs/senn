/**
 * syncEngine.ts — Local-first 同期エンジン
 *
 * 詳細設計 §3.5・§3.8。
 *   書き込み: 画面 → 端末内 DB（即時） → 送信キュー → pushChanges でサーバーへ
 *   読み取り: サーバー → pullEntity（差分同期 API） → 端末内 DB → 画面（liveQuery）
 *
 * - 起動: startSync(userId)。以後 30秒ごと・フォーカス時・画面に戻ったとき・オンライン復帰時に runCycle
 * - 排他: 同じタブ内では送信・同期を直列に、複数タブ間は Web Locks（navigator.locks）で1タブずつ
 */

import { db, type SyncQueueItem, type LocalReaction, openUserDb } from './db';
import { apiClient } from '../api/client';
import { registerBlobAdapter, pushBlobCreate, enqueueBlobCreate } from './blobSync';
import { customEmojiAdapter } from './adapters/customEmojiBlob';
import { hasCompletedFullSync, pullEntity, registerSyncTrigger } from './pull';
import { MAX_RETRY, pushEntityItem } from './push';
import { registerPushRequester } from './pushRequester';
import { useSyncStatus } from './syncStatusStore';
import { registerDevConsistencyCheck } from './devConsistencyCheck';

const SYNC_INTERVAL_MS = 30_000;
/** 送信順（プロジェクト作成をチケットより先に。チケットはプロジェクトに依存しうる） */
const ENTITY_ORDER: Record<SyncQueueItem['entity'], number> = {
  custom_emoji: 0,
  reaction: 1,
  project: 2,
  ticket: 3,
  wiki: 4,
};

// ── 同期状態 ──
let currentUserId: number | null = null;
let cycleRunning = false;
let rerunRequested = false;
let syncInterval: ReturnType<typeof setInterval> | null = null;
let focusListenerFn: (() => void) | null = null;
let visibilityListenerFn: (() => void) | null = null;
let onlineListenerFn: (() => void) | null = null;
/** タブ内で pushChanges を直列にするための鎖 */
let pushChain: Promise<void> = Promise.resolve();

registerBlobAdapter(customEmojiAdapter);
registerPushRequester(() => requestPush());
registerSyncTrigger(() => void runCycle());
registerDevConsistencyCheck();

/** 複数タブで同時に送信・同期しないためのロック（使えない環境ではそのまま実行） */
export async function withSyncLock<T>(fn: () => Promise<T>): Promise<T> {
  const locks = typeof navigator !== 'undefined' ? navigator.locks : undefined;
  if (locks?.request && currentUserId !== null) {
    return locks.request(`senn-sync-${currentUserId}`, fn) as Promise<T>;
  }
  return fn();
}

/** 1回分の同期: 送信 → プロジェクト取得 → チケット取得。実行中に呼ばれたら、終わってからもう1回だけ回す */
export async function runCycle(): Promise<void> {
  if (currentUserId === null) return;
  if (cycleRunning) {
    rerunRequested = true;
    return;
  }
  cycleRunning = true;
  rerunRequested = false;
  const status = useSyncStatus.getState();
  status.set({ syncing: true });
  try {
    await withSyncLock(async () => {
      await pushChangesUnlocked();
      await pullEntity('projects');
      await pullEntity('tickets');
    });
    status.set({ lastSyncAt: new Date().toISOString(), lastError: null, initialSyncDone: true });
  } catch (err) {
    status.set({ lastError: err instanceof Error ? err.message : String(err) });
    // 初回同期に失敗しても、以前のフル同期が端末にあれば画面は出せる
    if (!status.initialSyncDone && (await hasCompletedFullSync('tickets').catch(() => false))) {
      status.set({ initialSyncDone: true });
    }
  } finally {
    cycleRunning = false;
    useSyncStatus.getState().set({ syncing: false });
    await useSyncStatus.getState().refreshQueueCounts();
    if (rerunRequested) {
      rerunRequested = false;
      void runCycle();
    }
  }
}

/** 送信キューを送る（タブ間ロック付き） */
export async function pushChanges(): Promise<void> {
  await withSyncLock(() => pushChangesUnlocked());
  await useSyncStatus.getState().refreshQueueCounts();
}

/** 送信キューを送る（タブ内は直列。呼び出し側がタブ間ロックを持っている前提） */
function pushChangesUnlocked(): Promise<void> {
  const run = pushChain.then(pushQueueOnce, pushQueueOnce);
  pushChain = run.catch(() => undefined);
  return run;
}

async function pushQueueOnce(): Promise<void> {
  let queue: SyncQueueItem[];
  try {
    queue = await db.syncQueue.where('retryCount').below(MAX_RETRY).toArray();
  } catch {
    return; // DB の切り替え直後など
  }
  queue.sort((a, b) => ENTITY_ORDER[a.entity] - ENTITY_ORDER[b.entity] || (a.id ?? 0) - (b.id ?? 0));

  for (const snapshot of queue) {
    // 並べた後に、先の項目の送信（付け替え等）で内容が変わっている場合があるので読み直す
    const item = snapshot.id !== undefined ? await db.syncQueue.get(snapshot.id) : undefined;
    if (!item || item.retryCount >= MAX_RETRY) continue;

    if (item.entity === 'ticket' || item.entity === 'project') {
      await pushEntityItem(item);
      continue;
    }
    try {
      await pushSingleChange(item);
      if (item.id) await db.syncQueue.delete(item.id);
      // custom_emoji create は remap 内で dirty クリア済み
      if (!(item.entity === 'custom_emoji' && item.operation === 'create')) {
        await clearDirtyFlag(item.entity, item.entityId);
      }
    } catch (err) {
      if (item.id) {
        await db.syncQueue.update(item.id, {
          retryCount: item.retryCount + 1,
          lastError: err instanceof Error ? err.message : String(err),
        });
      }
    }
  }
}

/** リアクション・カスタム絵文字・Wiki の送信（チケット／プロジェクトは push.ts） */
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
  } else if (item.entity === 'wiki') {
    const base = '/wiki';
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

async function clearDirtyFlag(entity: string, entityId: number | string): Promise<void> {
  const syncedAt = new Date().toISOString();
  if (entity === 'reaction') {
    await db.reactions.update(entityId as number, { _dirty: false, _syncedAt: syncedAt });
  } else if (entity === 'custom_emoji') {
    await db.customEmojis.update(entityId as string, { _dirty: false, _syncedAt: syncedAt });
  } else if (entity === 'wiki') {
    await db.wikiPages.update(entityId as number, { _dirty: false, _syncedAt: syncedAt });
  }
}

/** 書き込み直後に呼ぶ: オンラインならすぐ送り、続けてチケットを取り直す（サーバー側で変わった項目を拾う） */
export function requestPush(): void {
  void useSyncStatus.getState().refreshQueueCounts();
  if (currentUserId === null) return;
  if (typeof navigator !== 'undefined' && navigator.onLine === false) return;
  void (async () => {
    try {
      await withSyncLock(async () => {
        await pushChangesUnlocked();
        await pullEntity('tickets');
      });
    } catch (err) {
      useSyncStatus.getState().set({ lastError: err instanceof Error ? err.message : String(err) });
    } finally {
      await useSyncStatus.getState().refreshQueueCounts();
    }
  })();
}

/** 同期を開始する（ログイン後に MainLayout から） */
export function startSync(userId: number): void {
  stopSync();
  currentUserId = userId;
  openUserDb(userId);

  const status = useSyncStatus.getState();
  status.set({ initialSyncDone: false, syncing: false, lastError: null, lastSyncAt: null });
  void hasCompletedFullSync('tickets')
    .then((done) => {
      if (done && currentUserId === userId) useSyncStatus.getState().set({ initialSyncDone: true });
    })
    .catch(() => undefined);

  void runCycle();
  syncInterval = setInterval(() => void runCycle(), SYNC_INTERVAL_MS);

  if (typeof window === 'undefined') return;
  focusListenerFn = () => void runCycle();
  visibilityListenerFn = () => {
    if (document.visibilityState === 'visible') void runCycle();
  };
  onlineListenerFn = () => void runCycle();
  window.addEventListener('focus', focusListenerFn);
  document.addEventListener('visibilitychange', visibilityListenerFn);
  window.addEventListener('online', onlineListenerFn);
}

/** 同期を止める（ログアウト・ユーザー切り替え時） */
export function stopSync(): void {
  if (syncInterval) {
    clearInterval(syncInterval);
    syncInterval = null;
  }
  if (typeof window !== 'undefined') {
    if (focusListenerFn) window.removeEventListener('focus', focusListenerFn);
    if (visibilityListenerFn) document.removeEventListener('visibilitychange', visibilityListenerFn);
    if (onlineListenerFn) window.removeEventListener('online', onlineListenerFn);
  }
  focusListenerFn = null;
  visibilityListenerFn = null;
  onlineListenerFn = null;
  currentUserId = null;
  rerunRequested = false;
}

/** ログアウト前: 未送信の変更を最大5秒送ってみて、残った件数を返す */
export async function prepareLogout(): Promise<{ pending: number }> {
  await Promise.race([
    pushChanges().catch(() => undefined),
    new Promise((resolve) => setTimeout(resolve, 5000)),
  ]);
  const pending = await db.syncQueue.count();
  return { pending };
}

/** 互換: 旧名 */
export const pullTickets = () => pullEntity('tickets');
export const pullProjects = () => pullEntity('projects');

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
  if (typeof navigator !== 'undefined' && navigator.onLine) {
    void requestPush();
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

  if (typeof navigator !== 'undefined' && navigator.onLine) {
    void requestPush();
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
