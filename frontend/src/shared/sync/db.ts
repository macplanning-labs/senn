/**
 * db.ts — Dexie.js ローカルデータベース定義
 *
 * IndexedDB にチケット・プロジェクト等をキャッシュし、
 * オフライン時もUIが動作するローカルファースト設計。
 * Linearの同期モデル（LWW: Last Write Wins）を採用。
 */

import Dexie, { type EntityTable } from 'dexie';

// ── ローカルスキーマ ──
export interface LocalTicket {
  id: number;
  ticketKey: string;
  title: string;
  description: string;
  status: string;
  priority: string;
  ticketType: string;
  assigneeId: number | null;
  projectId: number | null;
  dueDate: string | null;
  updatedAt: string;
  /** ローカル変更フラグ（サーバー未同期） */
  _dirty: boolean;
  /** 最終同期日時 */
  _syncedAt: string | null;
}

export interface LocalProject {
  id: number;
  name: string;
  prefix: string;
  description: string;
  updatedAt: string;
  _dirty: boolean;
  _syncedAt: string | null;
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
  /** リトライ回数 */
  retryCount: number;
}

// ── データベース定義 ──
const db = new Dexie('WipLocalDB') as Dexie & {
  tickets: EntityTable<LocalTicket, 'id'>;
  projects: EntityTable<LocalProject, 'id'>;
  wikiPages: EntityTable<LocalWikiPage, 'id'>;
  reactions: EntityTable<LocalReaction, 'id'>;
  customEmojis: EntityTable<LocalCustomEmoji, 'id'>;
  pendingBlobs: EntityTable<LocalPendingBlob, 'localId'>;
  syncQueue: EntityTable<SyncQueueItem, 'id'>;
};

db.version(1).stores({
  tickets: 'id, ticketKey, status, priority, projectId, assigneeId, updatedAt, _dirty',
  projects: 'id, prefix, updatedAt, _dirty',
  wikiPages: 'id, slug, category, updatedAt, _dirty',
  syncQueue: '++id, entity, entityId, operation, createdAt',
});

// v2: syncQueue に retryCount インデックスを追加（pushChanges の where('retryCount') に必要）
db.version(2).stores({
  tickets: 'id, ticketKey, status, priority, projectId, assigneeId, updatedAt, _dirty',
  projects: 'id, prefix, updatedAt, _dirty',
  wikiPages: 'id, slug, category, updatedAt, _dirty',
  syncQueue: '++id, entity, entityId, operation, createdAt, retryCount',
});

// v3: reactions テーブル追加（チケットのリアクション Local-first）
db.version(3).stores({
  tickets: 'id, ticketKey, status, priority, projectId, assigneeId, updatedAt, _dirty',
  projects: 'id, prefix, updatedAt, _dirty',
  wikiPages: 'id, slug, category, updatedAt, _dirty',
  reactions: 'id, ticketId, userId, updatedAt, _dirty',
  syncQueue: '++id, entity, entityId, operation, createdAt, retryCount',
});

// v4: customEmojis + pendingEmojiBlobs（WIPAPPDEV-000087 で出荷済み。履歴定義は消さない）
db.version(4).stores({
  tickets: 'id, ticketKey, status, priority, projectId, assigneeId, updatedAt, _dirty',
  projects: 'id, prefix, updatedAt, _dirty',
  wikiPages: 'id, slug, category, updatedAt, _dirty',
  reactions: 'id, ticketId, userId, updatedAt, _dirty',
  customEmojis: 'id, projectId, updatedAt, _dirty',
  pendingEmojiBlobs: 'localId, projectId',
  syncQueue: '++id, entity, entityId, operation, createdAt, retryCount',
});

// v5: pendingBlobs へ寄せ、旧 pending 表を廃止（WIPAPPDEV-000091）
db.version(5)
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
db.version(6)
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

export { db };
