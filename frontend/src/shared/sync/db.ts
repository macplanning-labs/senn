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

export interface SyncQueueItem {
  id?: number;
  /** 対象エンティティ種別 */
  entity: 'ticket' | 'project' | 'wiki';
  /** 対象エンティティID */
  entityId: number;
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
  syncQueue: EntityTable<SyncQueueItem, 'id'>;
};

db.version(1).stores({
  tickets: 'id, ticketKey, status, priority, projectId, assigneeId, updatedAt, _dirty',
  projects: 'id, prefix, updatedAt, _dirty',
  wikiPages: 'id, slug, category, updatedAt, _dirty',
  syncQueue: '++id, entity, entityId, operation, createdAt',
});

export { db };
