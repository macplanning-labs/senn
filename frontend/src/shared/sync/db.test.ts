/**
 * db.test.ts — 端末内 DB のユーザー分離（WIPAPPDEV-000113 / 詳細設計 §3.2・T8）
 */

import 'fake-indexeddb/auto';
import { describe, it, expect, afterEach } from 'vitest';
import Dexie from 'dexie';
import {
  db,
  getCurrentDbName,
  getDbGeneration,
  openUserDb,
  deleteCurrentUserDb,
  migrateLegacyDb,
  type LocalReaction,
} from './db';

function reaction(id: number, userId: number, dirty = true): LocalReaction {
  const now = new Date().toISOString();
  return {
    id,
    ticketId: 100,
    userId,
    emojiKind: 'unicode',
    emojiValue: '👍',
    updatedAt: now,
    _dirty: dirty,
    _syncedAt: dirty ? null : now,
  };
}

async function waitFor(cond: () => Promise<boolean>, timeoutMs = 2000): Promise<void> {
  const started = Date.now();
  while (!(await cond())) {
    if (Date.now() - started > timeoutMs) throw new Error('timeout');
    await new Promise((r) => setTimeout(r, 10));
  }
}

describe('ユーザー別 DB（WIPAPPDEV-000113）', () => {
  afterEach(async () => {
    await deleteCurrentUserDb();
    for (const name of ['senn-local-1', 'senn-local-2', 'WipLocalDB']) {
      await Dexie.delete(name);
    }
  });

  it('ユーザーごとに別の DB を開き、他ユーザーの行は見えない', async () => {
    openUserDb(1);
    expect(getCurrentDbName()).toBe('senn-local-1');
    await db.reactions.put(reaction(1, 1, false));
    expect(await db.reactions.count()).toBe(1);

    const before = getDbGeneration();
    openUserDb(2);
    expect(getDbGeneration()).toBeGreaterThan(before);
    expect(getCurrentDbName()).toBe('senn-local-2');
    expect(await db.reactions.count()).toBe(0);

    // 同じユーザーで開き直しても世代は変わらない
    const same = getDbGeneration();
    openUserDb(2);
    expect(getDbGeneration()).toBe(same);
  });

  it('deleteCurrentUserDb で DB が消え、匿名 DB に戻る', async () => {
    openUserDb(1);
    await db.reactions.put(reaction(1, 1, false));
    await deleteCurrentUserDb();
    expect(getCurrentDbName()).toBe('senn-local-anon');
    expect(await Dexie.exists('senn-local-1')).toBe(false);

    // 同じユーザーで開き直しても前のデータは残っていない
    openUserDb(1);
    expect(await db.reactions.count()).toBe(0);
  });

  it('別ユーザーでログインすると、残っていた前ユーザーの DB が消える', async () => {
    openUserDb(1);
    await db.reactions.put(reaction(1, 1, false));
    // ログアウト処理を通らずに（トークン失効）別ユーザーでログインした想定
    openUserDb(2);
    await waitFor(async () => !(await Dexie.exists('senn-local-1')));
    expect(await Dexie.exists('senn-local-1')).toBe(false);
  });

  it('旧 WipLocalDB からは本人の未送信リアクションだけを移し、旧 DB を消す', async () => {
    const legacy = new Dexie('WipLocalDB');
    legacy.version(6).stores({
      tickets: 'id, ticketKey, status, priority, projectId, assigneeId, updatedAt, _dirty',
      projects: 'id, prefix, updatedAt, _dirty',
      wikiPages: 'id, slug, category, updatedAt, _dirty',
      reactions: 'id, ticketId, userId, updatedAt, _dirty',
      customEmojis: 'id, projectId, updatedAt, _dirty',
      pendingBlobs: 'localId, entity, scopeKey',
      syncQueue: '++id, entity, entityId, operation, createdAt, retryCount',
    });
    await legacy.open();
    await legacy.table('reactions').bulkPut([reaction(-1, 1), reaction(-2, 2), reaction(3, 1, false)]);
    const q = (entityId: number) => ({
      entity: 'reaction',
      entityId,
      operation: 'create',
      payload: '{}',
      createdAt: new Date().toISOString(),
      retryCount: 0,
    });
    await legacy.table('syncQueue').bulkAdd([q(-1), q(-2)]);
    legacy.close();

    openUserDb(1); // 裏で migrateLegacyDb が走る
    await waitFor(async () => !(await Dexie.exists('WipLocalDB')));
    await migrateLegacyDb(1); // 2回目は何もしない（旧 DB が無い）

    const rows = await db.reactions.toArray();
    expect(rows.map((r) => r.id)).toEqual([-1]);
    const queue = await db.syncQueue.toArray();
    expect(queue.map((i) => i.entityId)).toEqual([-1]);
    expect(await Dexie.exists('WipLocalDB')).toBe(false);
  });
});
