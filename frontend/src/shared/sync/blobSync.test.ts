/**
 * blobSync.test.ts — Blob + SyncQueue 共通同期のテスト
 *
 * T3: 仮 ID custom reaction がキューにある状態で remap → payload.emojiValue 置換
 */

import 'fake-indexeddb/auto';
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { db } from './db';
import { remapEntityId } from './blobSync';
import { customEmojiAdapter } from './adapters/customEmojiBlob';

describe('blobSync T3 remap', () => {
  beforeEach(async () => {
    await db.delete();
    await db.open();
  });

  afterEach(async () => {
    await db.close();
  });

  it('updates reaction.emojiValue and SyncQueue payload from localId to serverId', async () => {
    const localEmojiId = 'local-emoji-12345';
    const serverEmojiId = '999';
    const projectId = 1;
    const reactionId = -1001;
    const now = new Date().toISOString();

    await db.pendingBlobs.add({
      localId: localEmojiId,
      entity: 'custom_emoji',
      scopeKey: String(projectId),
      blob: new Blob(['test'], { type: 'image/png' }),
      mime: 'image/png',
      fileName: 'test.png',
      createdAt: now,
    });

    await db.customEmojis.add({
      id: localEmojiId,
      projectId,
      slug: 'test-emoji',
      name: 'Test Emoji',
      imageUrl: 'https://example.com/pending.png',
      updatedAt: now,
      _dirty: true,
      _syncedAt: null,
      _localId: localEmojiId,
    });

    await db.reactions.add({
      id: reactionId,
      ticketId: 1,
      userId: 100,
      emojiKind: 'custom',
      emojiValue: localEmojiId,
      updatedAt: now,
      _dirty: true,
      _syncedAt: null,
    });

    await db.syncQueue.add({
      entity: 'custom_emoji',
      entityId: localEmojiId,
      operation: 'create',
      payload: JSON.stringify({
        projectPrefix: 'TEST',
        slug: 'test-emoji',
        name: 'Test Emoji',
      }),
      createdAt: now,
      retryCount: 0,
    });

    await db.syncQueue.add({
      entity: 'reaction',
      entityId: reactionId,
      operation: 'create',
      payload: JSON.stringify({
        ticketKey: 'TEST-1',
        emojiKind: 'custom',
        emojiValue: localEmojiId,
      }),
      createdAt: now,
      retryCount: 0,
    });

    await remapEntityId(
      customEmojiAdapter,
      localEmojiId,
      serverEmojiId,
      `https://example.com/emojis/${serverEmojiId}.png`,
    );

    expect(await db.pendingBlobs.get(localEmojiId)).toBeUndefined();

    const emoji = await db.customEmojis.get(serverEmojiId);
    expect(emoji).toBeDefined();
    expect(emoji?.id).toBe(serverEmojiId);
    expect(emoji?.imageUrl).toBe(`https://example.com/emojis/${serverEmojiId}.png`);
    expect(emoji?._dirty).toBe(false);
    expect(emoji?._localId).toBeUndefined();
    expect(await db.customEmojis.get(localEmojiId)).toBeUndefined();

    const reaction = await db.reactions.get(reactionId);
    expect(reaction?.emojiValue).toBe(serverEmojiId);
    // reaction 行の id は remap しない（§2 #6）
    expect(reaction?.id).toBe(reactionId);

    const reactionQueue = (await db.syncQueue.where('entity').equals('reaction').toArray()).find(
      (q) => q.operation === 'create',
    );
    expect(reactionQueue).toBeDefined();
    expect(reactionQueue?.entityId).toBe(reactionId);
    const reactionPayload = JSON.parse(reactionQueue!.payload) as Record<string, unknown>;
    expect(reactionPayload.emojiValue).toBe(serverEmojiId);

    const emojiQueue = (await db.syncQueue.where('entity').equals('custom_emoji').toArray()).find(
      (q) => q.operation === 'create',
    );
    expect(emojiQueue).toBeDefined();
    expect(emojiQueue?.entityId).toBe(serverEmojiId);
  });
});
