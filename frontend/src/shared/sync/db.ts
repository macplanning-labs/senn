/**
 * db.ts — Dexie.js ローカルデータベース定義
 *
 * IndexedDB にチケット・プロジェクト等をキャッシュし、
 * オフライン時もUIが動作するローカルファースト設計。
 * Linearの同期モデル（LWW: Last Write Wins）を採用。
 * ユーザーごとに独立した DB インスタンスを使用。
 */

import Dexie, { type EntityTable } from 'dexie';
import { useSyncExternalStore } from 'react';
// ── サーバーの同期 DTO（GET /api/v1/sync/tickets/ の changes の1件。詳細設計 §2.3） ──
export interface SyncUser {
  id: number;
  username: string;
  email?: string;
  displayName: string;
}

export interface SyncLabel {
  id: number;
  name: string;
  color: string;
  project?: number | null;
  teamId?: number | null;
  description?: string | null;
  category?: string | null;
  isAiEnabled?: boolean;
  createdAt?: string;
}

export interface SyncTeam {
  id: number;
  name: string;
  slug: string;
  icon: string;
  color: string;
}

export interface TicketSyncDto {
  id: number;
  ticketKey: string;
  title: string;
  description: string;
  status: string;
  priority: string;
  ticketType: string;
  assignees: SyncUser[];
  reviewers: SyncUser[];
  author: SyncUser | null;
  category: { id: number; name: string; slug?: string; color: string; level?: number; parent?: number | null; sortOrder?: number } | null;
  milestone: { id: number; name: string; dueDate: string | null; description?: string; project?: number | null } | null;
  project: number | null;
  projectPrefix: string | null;
  projectName: string | null;
  parent: number | null;
  labels: SyncLabel[];
  startDate: string | null;
  dueDate: string | null;
  storyPoints: number | null;
  cycle: number | null;
  cycleName: string | null;
  team: SyncTeam | null;
  commentCount: number;
  childCount: number;
  totalTimeSpent: number;
  /** この項目だけサーバーが snake_case で返す */
  gantt_order: number;
  createdAt: string;
  updatedAt: string;
  closedAt: string | null;
}

/** GET /api/v1/sync/projects/ の changes の1件（= 一覧 API の Project ＋ updatedAt） */
export interface ProjectSyncDto {
  id: number;
  name: string;
  prefix: string;
  description: string;
  status: string;
  priority: string;
  targetEndDate: string | null;
  ticketCount: number;
  memberCount: number;
  isMember: boolean;
  teams: Array<SyncTeam & { archived?: boolean }>;
  ownerId: number | null;
  createdAt: string;
  updatedAt: string;
  cycleAutoComplete: boolean;
  cycleAutoCreateNext: boolean;
  parentProjectId: number | null;
  childCount: number;
  roadmapIds: number[];
}

export interface SyncAccess {
  all: boolean;
  teamIds: number[];
  scopedProjects: { teamId: number; projectId: number }[];
}

export interface SyncMeta {
  entity: 'tickets' | 'projects';
  cursor: string | null;
  lastFullSyncAt: string | null;
  access: SyncAccess | null;
}

// ── ローカルスキーマ ──
export interface LocalTicket extends TicketSyncDto {
  teamId: number | null; // 索引用（team?.id）
  projectId: number | null; // = project
  cycleId: number | null; // = cycle
  parentId: number | null; // = parent
  assigneeIds: number[]; // assignees.map(id)
  labelIds: number[]; // labels.map(id)
  /** ローカル変更フラグ（サーバー未同期） */
  _dirty: boolean;
  /** 最終同期日時 */
  _syncedAt: string | null;
  /** オフライン作成中（id は負数、ticketKey は 'local-<uuid>'） */
  _pendingCreate?: boolean;
  /** 削除送信待ち（UI からは見えない） */
  _deleted?: boolean;
  /** 直近の送信失敗メッセージ */
  _syncError?: string | null;
}

export interface LocalProject extends ProjectSyncDto {
  _dirty: boolean;
  _syncedAt: string | null;
  _pendingCreate?: boolean;
  _deleted?: boolean;
  _syncError?: string | null;
}

export interface LocalWikiPage {
  id: number;
  title: string;
  slug: string;
  category: string;
  content: string;
  updatedAt: string;
  _dirty: boolean;
  _syncedAt: string | null;
}

export interface LocalReaction {
  id: number;
  ticketId: number;
  userId: number;
  emojiKind: 'unicode' | 'custom';
  emojiValue: string;
  updatedAt: string;
  _dirty: boolean;
  _syncedAt: string | null;
  _localId?: string;
}

export interface LocalCustomEmoji {
  id: string;
  projectId: number;
  slug: string;
  name: string;
  imageUrl: string;
  updatedAt: string;
  _dirty: boolean;
  _syncedAt: string | null;
  _localId?: string;
}

export interface LocalPendingBlob {
  localId: string;
  entity: 'custom_emoji';
  scopeKey: string;
  blob: Blob;
  mime: string;
  fileName: string;
  createdAt: string;
}

/** サーバーのエラー応答がこの回数続いたら「同期できなかった変更」として扱う */
export const MAX_RETRY = 5;

export interface SyncQueueItem {
  id?: number;
  /** 対象エンティティ種別 */
  entity: 'ticket' | 'project' | 'wiki' | 'reaction' | 'custom_emoji';
  /** 対象エンティティID */
  entityId: number | string;
  /** 操作種別 */
  operation: 'create' | 'update' | 'delete';
  /** 変更データ（JSON） */
  payload: string;
  /** キュー追加日時 */
  createdAt: string;
  /**
   * 失敗扱いまでの回数（サーバーがエラー応答を返した回数）。MAX_RETRY 以上で「同期できなかった変更」になる。
   * 通信そのものが届かなかった失敗（Network Error・タイムアウト）は数えない（送れるようになるまで待って送り直す）
   */
  retryCount: number;
  /** 失敗した回数の合計（通信エラーも含む）。次に送るまでの待ち時間（指数バックオフ）の計算に使う */
  attempts?: number;
  /** この時刻（ISO）より前には送らない。未設定ならすぐ送ってよい */
  nextAttemptAt?: string;
  /** 最初に失敗した時刻（ISO）。失敗扱いにするかの判定に使う */
  firstFailedAt?: string;
  /** 最後に送信を試みて失敗した時刻（ISO） */
  lastAttemptAt?: string;
  /** 冪等キー（create のみ） */
  idempotencyKey?: string;
  /** 直近のエラーメッセージ */
  lastError?: string;
}

// ── DB 型定義 ──
export type SennDB = ReturnType<typeof createDb>;

function createDb(name: string) {
  const instance = new Dexie(name) as Dexie & {
    tickets: EntityTable<LocalTicket, 'id'>;
    projects: EntityTable<LocalProject, 'id'>;
    wikiPages: EntityTable<LocalWikiPage, 'id'>;
    reactions: EntityTable<LocalReaction, 'id'>;
    customEmojis: EntityTable<LocalCustomEmoji, 'id'>;
    pendingBlobs: EntityTable<LocalPendingBlob, 'localId'>;
    syncQueue: EntityTable<SyncQueueItem, 'id'>;
    syncMeta: EntityTable<SyncMeta, 'entity'>;
  };

  instance.version(1).stores({
    tickets: 'id, ticketKey, status, priority, projectId, assigneeId, updatedAt, _dirty',
    projects: 'id, prefix, updatedAt, _dirty',
    wikiPages: 'id, slug, category, updatedAt, _dirty',
    syncQueue: '++id, entity, entityId, operation, createdAt',
  });

  // v2: syncQueue に retryCount インデックスを追加（pushChanges の where('retryCount') に必要）
  instance.version(2).stores({
    tickets: 'id, ticketKey, status, priority, projectId, assigneeId, updatedAt, _dirty',
    projects: 'id, prefix, updatedAt, _dirty',
    wikiPages: 'id, slug, category, updatedAt, _dirty',
    syncQueue: '++id, entity, entityId, operation, createdAt, retryCount',
  });

  // v3: reactions テーブル追加（チケットのリアクション Local-first）
  instance.version(3).stores({
    tickets: 'id, ticketKey, status, priority, projectId, assigneeId, updatedAt, _dirty',
    projects: 'id, prefix, updatedAt, _dirty',
    wikiPages: 'id, slug, category, updatedAt, _dirty',
    reactions: 'id, ticketId, userId, updatedAt, _dirty',
    syncQueue: '++id, entity, entityId, operation, createdAt, retryCount',
  });

  // v4: customEmojis + pendingEmojiBlobs（WIPAPPDEV-000087 で出荷済み。履歴定義は消さない）
  instance.version(4).stores({
    tickets: 'id, ticketKey, status, priority, projectId, assigneeId, updatedAt, _dirty',
    projects: 'id, prefix, updatedAt, _dirty',
    wikiPages: 'id, slug, category, updatedAt, _dirty',
    reactions: 'id, ticketId, userId, updatedAt, _dirty',
    customEmojis: 'id, projectId, updatedAt, _dirty',
    pendingEmojiBlobs: 'localId, projectId',
    syncQueue: '++id, entity, entityId, operation, createdAt, retryCount',
  });

  // v5: pendingBlobs へ寄せ、旧 pending 表を廃止（WIPAPPDEV-000091）
  instance.version(5)
    .stores({
      tickets: 'id, ticketKey, status, priority, projectId, assigneeId, updatedAt, _dirty',
      projects: 'id, prefix, updatedAt, _dirty',
      wikiPages: 'id, slug, category, updatedAt, _dirty',
      reactions: 'id, ticketId, userId, updatedAt, _dirty',
      customEmojis: 'id, projectId, updatedAt, _dirty',
      pendingBlobs: 'localId, entity, scopeKey',
      syncQueue: '++id, entity, entityId, operation, createdAt, retryCount',
    })
    .upgrade(async (tx) => {
      const oldBlobs = (await tx.table('pendingEmojiBlobs').toArray()) as Array<{
        localId: string;
        projectId: number;
        blob: Blob;
        mime: string;
        fileName: string;
      }>;

      const now = new Date().toISOString();
      const newBlobs = oldBlobs.map((b) => ({
        localId: b.localId,
        entity: 'custom_emoji' as const,
        scopeKey: String(b.projectId),
        blob: b.blob,
        mime: b.mime,
        fileName: b.fileName,
        createdAt: now,
      }));

      if (newBlobs.length > 0) {
        await tx.table('pendingBlobs').bulkAdd(newBlobs);
      }
    });

  // v6: 中間スキーマ（v5 に旧 pending 表が残った場合）の掃除
  instance.version(6)
    .stores({
      tickets: 'id, ticketKey, status, priority, projectId, assigneeId, updatedAt, _dirty',
      projects: 'id, prefix, updatedAt, _dirty',
      wikiPages: 'id, slug, category, updatedAt, _dirty',
      reactions: 'id, ticketId, userId, updatedAt, _dirty',
      customEmojis: 'id, projectId, updatedAt, _dirty',
      pendingBlobs: 'localId, entity, scopeKey',
      syncQueue: '++id, entity, entityId, operation, createdAt, retryCount',
    })
    .upgrade(async (tx) => {
      try {
        const oldBlobs = (await tx.table('pendingEmojiBlobs').toArray()) as Array<{
          localId: string;
          projectId: number;
          blob: Blob;
          mime: string;
          fileName: string;
        }>;
        if (oldBlobs.length === 0) return;
        const existing = new Set(
          ((await tx.table('pendingBlobs').toArray()) as Array<{ localId: string }>).map(
            (b) => b.localId,
          ),
        );
        const now = new Date().toISOString();
        const toAdd = oldBlobs
          .filter((b) => !existing.has(b.localId))
          .map((b) => ({
            localId: b.localId,
            entity: 'custom_emoji' as const,
            scopeKey: String(b.projectId),
            blob: b.blob,
            mime: b.mime,
            fileName: b.fileName,
            createdAt: now,
          }));
        if (toAdd.length > 0) {
          await tx.table('pendingBlobs').bulkAdd(toAdd);
        }
      } catch {
        // 旧表が既に無い場合は無視
      }
    });

  // v7: LocalFirst コアエンティティ移行（チケット・プロジェクト形式変更）
  instance.version(7)
    .stores({
      tickets: 'id, ticketKey, teamId, projectId, cycleId, parentId, status, *assigneeIds, *labelIds, updatedAt, _dirty',
      projects: 'id, prefix, updatedAt, _dirty',
      wikiPages: 'id, slug, category, updatedAt, _dirty',
      reactions: 'id, ticketId, userId, updatedAt, _dirty',
      customEmojis: 'id, projectId, updatedAt, _dirty',
      pendingBlobs: 'localId, entity, scopeKey',
      syncQueue: '++id, entity, entityId, operation, createdAt, retryCount',
      syncMeta: 'entity',
    })
    .upgrade(async (tx) => {
      // v6 の形は キャッシュなので捨てる
      await tx.table('tickets').clear();
      await tx.table('projects').clear();
    });

  return instance;
}

// ── グローバル DB インスタンス ──
export let db: SennDB = createDb('senn-local-anon');
let currentDbName: string = 'senn-local-anon';
let generation = 0;
const listeners = new Set<() => void>();

/**
 * 現在の DB 名を取得
 */
export function getCurrentDbName(): string {
  return currentDbName;
}

/**
 * DB 世代番号を取得（DB 切り替え時に increment）
 */
export function getDbGeneration(): number {
  return generation;
}

/**
 * DB 変更の購読者を登録し、購読解除関数を返す
 */
export function subscribeDb(cb: () => void): () => void {
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}

/**
 * DB 世代番号を React コンポーネントで監視する Hook
 * liveQuery() 使用時の依存配列に含めて DB 切り替え時に再購読する
 */
export function useDbGeneration(): number {
  return useSyncExternalStore(subscribeDb, getDbGeneration, getDbGeneration);
}

const ANON_DB_NAME = 'senn-local-anon';
const USER_DB_PREFIX = 'senn-local-';
const LEGACY_DB_NAME = 'WipLocalDB';

/**
 * ユーザーのログイン時に DB を切り替える（同期関数。呼び出し直後から db は新しい DB を指す）
 * 既に同じ DB の場合は何もしない
 */
export function openUserDb(userId: number): void {
  const newDbName = `${USER_DB_PREFIX}${userId}`;
  if (newDbName === currentDbName) {
    return;
  }

  db.close();
  db = createDb(newDbName);
  currentDbName = newDbName;
  generation++;
  notifyListeners();

  // 旧 DB からの移行と、他ユーザーの DB の掃除（失敗しても画面は動かす）
  void (async () => {
    try {
      await migrateLegacyDb(userId);
    } catch (err) {
      console.warn('[db] migrateLegacyDb failed:', err);
    }
    try {
      await purgeOtherUserDbs(newDbName);
    } catch (err) {
      console.warn('[db] purgeOtherUserDbs failed:', err);
    }
  })();
}

/**
 * ログアウト時に現在の DB を削除し、匿名 DB にリセット
 */
export async function deleteCurrentUserDb(): Promise<void> {
  const oldName = currentDbName;
  db.close();
  db = createDb(ANON_DB_NAME);
  currentDbName = ANON_DB_NAME;
  generation++;
  notifyListeners();
  await Dexie.delete(oldName);
}

/**
 * 旧 WipLocalDB（全ユーザー共用だった DB）から、このユーザー本人の未送信リアクションだけを移し、旧 DB を削除する。
 * 他の項目は前のユーザーの変更である可能性があるため移さない（別ユーザーの権限で送信されるのを防ぐ）。
 */
export async function migrateLegacyDb(userId: number): Promise<void> {
  if (!(await Dexie.exists(LEGACY_DB_NAME))) {
    return;
  }

  const legacy = new Dexie(LEGACY_DB_NAME);
  try {
    await legacy.open();
    const tableNames = new Set(legacy.tables.map((t) => t.name));

    if (tableNames.has('reactions')) {
      const reactions = (await legacy.table('reactions').toArray()) as LocalReaction[];
      const toMigrate = reactions.filter((r) => r.userId === userId && r._dirty);
      if (toMigrate.length > 0) {
        await db.reactions.bulkPut(toMigrate);
      }

      if (tableNames.has('syncQueue') && toMigrate.length > 0) {
        const queue = (await legacy.table('syncQueue').toArray()) as SyncQueueItem[];
        const migratedIds = new Set<number | string>(toMigrate.map((r) => r.id));
        for (const item of queue) {
          if (item.entity === 'reaction' && migratedIds.has(item.entityId)) {
            const { id: _omit, ...rest } = item;
            void _omit;
            await db.syncQueue.add(rest);
          }
        }
      }
    }
  } catch (err) {
    console.warn('[db] migrateLegacyDb error:', err);
  } finally {
    legacy.close();
  }
  await Dexie.delete(LEGACY_DB_NAME);
}

/**
 * 他ユーザーの DB を削除（トークン失効などでログアウト処理を通らなかった前ユーザーの残骸を消す）
 */
export async function purgeOtherUserDbs(currentName: string): Promise<void> {
  if (typeof indexedDB === 'undefined' || typeof indexedDB.databases !== 'function') {
    return;
  }

  try {
    const dbs = await indexedDB.databases();
    for (const dbInfo of dbs) {
      const name = dbInfo.name ?? '';
      if (name.startsWith(USER_DB_PREFIX) && name !== currentName && name !== ANON_DB_NAME) {
        await Dexie.delete(name);
      }
    }
  } catch (err) {
    console.warn('[db] purgeOtherUserDbs error:', err);
  }
}

/**
 * リスナーへ通知
 */
function notifyListeners(): void {
  listeners.forEach((cb) => cb());
}
