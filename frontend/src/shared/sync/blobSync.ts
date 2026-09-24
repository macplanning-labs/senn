/**
 * blobSync.ts — Binary blob + SyncQueue の共通同期パターン
 *
 * カスタム絵文字などの binary asset に対して、ローカル ID → サーバー ID への
 * remap、multipart POST、キュー管理を共通化。
 */

import { db, type LocalPendingBlob } from './db';

export type SyncBlobEntity = 'custom_emoji';

export interface BlobCreateAdapter<TMeta extends Record<string, unknown> = Record<string, unknown>> {
  entity: SyncBlobEntity;
  localIdPrefix: string;
  buildQueuePayload: (meta: TMeta) => Record<string, unknown>;
  putLocalRow: (args: {
    localId: string;
    objectUrl: string;
    meta: TMeta;
    now: string;
  }) => Promise<void>;
  postMultipart: (args: {
    localId: string;
    pending: LocalPendingBlob;
    payload: Record<string, unknown>;
  }) => Promise<{ serverId: string; displayUrl: string; extra?: Record<string, unknown> }>;
  onRemap: (args: {
    localId: string;
    serverId: string;
    displayUrl: string;
    extra?: Record<string, unknown>;
  }) => Promise<void>;
}

const blobAdapters = new Map<SyncBlobEntity, BlobCreateAdapter>();

export function registerBlobAdapter<T extends Record<string, unknown>>(
  adapter: BlobCreateAdapter<T>,
): void {
  blobAdapters.set(adapter.entity, adapter as BlobCreateAdapter);
}

export async function enqueueBlobCreate<TMeta extends Record<string, unknown>>(
  adapter: BlobCreateAdapter<TMeta>,
  meta: TMeta,
  file: File,
): Promise<string> {
  const localId = `${adapter.localIdPrefix}${crypto.randomUUID()}`;
  const now = new Date().toISOString();
  const objectUrl = URL.createObjectURL(file);

  const scopeKey =
    adapter.entity === 'custom_emoji'
      ? String((meta as { projectId?: number }).projectId ?? 0)
      : '0';

  // putLocalRow が customEmojis 等を書くため、エンティティ表を tx に含める
  await db.transaction('rw', db.pendingBlobs, db.customEmojis, db.syncQueue, async () => {
    const pending: LocalPendingBlob = {
      localId,
      entity: adapter.entity,
      scopeKey,
      blob: file,
      mime: file.type,
      fileName: file.name,
      createdAt: now,
    };

    await db.pendingBlobs.add(pending);

    await adapter.putLocalRow({
      localId,
      objectUrl,
      meta,
      now,
    });

    const payload = adapter.buildQueuePayload(meta);
    await db.syncQueue.add({
      entity: adapter.entity,
      entityId: localId,
      operation: 'create',
      payload: JSON.stringify(payload),
      createdAt: now,
      retryCount: 0,
    });
  });

  if (navigator.onLine) {
    const { pushChanges } = await import('./syncEngine');
    void pushChanges();
  }

  return localId;
}

export async function pushBlobCreate(item: {
  entity: string;
  entityId: string | number;
  operation: string;
  payload: string;
}): Promise<void> {
  const adapter = blobAdapters.get(item.entity as SyncBlobEntity);
  if (!adapter) {
    throw new Error(`No blob adapter registered for entity: ${item.entity}`);
  }

  const localId = item.entityId as string;
  const pending = await db.pendingBlobs.get(localId);
  if (!pending) {
    throw new Error(`Pending blob not found for ${localId}`);
  }

  const payload = JSON.parse(item.payload) as Record<string, unknown>;
  const result = await adapter.postMultipart({
    localId,
    pending,
    payload,
  });

  await remapEntityId(adapter, localId, result.serverId, result.displayUrl, result.extra);
}

export async function remapEntityId(
  adapter: {
    entity: SyncBlobEntity;
    onRemap: BlobCreateAdapter['onRemap'];
  },
  localId: string,
  serverId: string,
  displayUrl: string,
  extra?: Record<string, unknown>,
): Promise<void> {
  // onRemap が customEmojis / reactions / syncQueue(reaction) を更新するため tx に含める
  await db.transaction(
    'rw',
    db.pendingBlobs,
    db.customEmojis,
    db.reactions,
    db.syncQueue,
    async () => {
      const pending = await db.pendingBlobs.get(localId);
      if (pending) {
        await db.pendingBlobs.delete(localId);
      }

      await adapter.onRemap({
        localId,
        serverId,
        displayUrl,
        extra,
      });

      const queued = await db.syncQueue.where('entity').equals(adapter.entity).toArray();
      for (const q of queued) {
        if (q.entityId === localId && q.id) {
          await db.syncQueue.update(q.id, { entityId: serverId });
        }
      }
    },
  );
}
