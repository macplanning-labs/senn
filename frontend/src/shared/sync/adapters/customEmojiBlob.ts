/**
 * customEmojiBlob.ts — Custom emoji blob adapter
 *
 * BlobSync 共通パターンを使用したカスタム絵文字のマルチパート POST と remap ロジック。
 */

import { db } from '../db';
import { apiClient } from '../../api/client';
import type { BlobCreateAdapter } from '../blobSync';

interface CustomEmojiBlobMeta extends Record<string, unknown> {
  projectPrefix: string;
  projectId: number;
  slug: string;
  name: string;
}

export const customEmojiAdapter: BlobCreateAdapter<CustomEmojiBlobMeta> = {
  entity: 'custom_emoji',
  localIdPrefix: 'local-emoji-',

  buildQueuePayload: (meta) => ({
    projectPrefix: meta.projectPrefix,
    slug: meta.slug,
    name: meta.name,
  }),

  putLocalRow: async ({ localId, objectUrl, meta, now }) => {
    await db.customEmojis.add({
      id: localId,
      projectId: meta.projectId,
      slug: meta.slug,
      name: meta.name,
      imageUrl: objectUrl,
      updatedAt: now,
      _dirty: true,
      _syncedAt: null,
      _localId: localId,
    });
  },

  postMultipart: async ({ pending, payload }) => {
    const projectPrefix = payload.projectPrefix as string;
    const formData = new FormData();
    formData.append('file', pending.blob, pending.fileName);
    formData.append('slug', payload.slug as string);
    formData.append('name', payload.name as string);

    const res = await apiClient.post<{ id: number; imageUrl: string }>(
      `/projects/${projectPrefix}/custom-emojis/`,
      formData,
    );

    return {
      serverId: res.data.id.toString(),
      displayUrl: res.data.imageUrl,
    };
  },

  onRemap: async ({ localId, serverId, displayUrl }) => {
    const emoji = await db.customEmojis.get(localId);
    if (emoji) {
      if (emoji.imageUrl.startsWith('blob:')) {
        URL.revokeObjectURL(emoji.imageUrl);
      }
      await db.customEmojis.delete(localId);
      await db.customEmojis.put({
        ...emoji,
        id: serverId,
        imageUrl: displayUrl,
        _dirty: false,
        _syncedAt: new Date().toISOString(),
        _localId: undefined,
      });
    }

    const reactions = await db.reactions
      .filter((r) => r.emojiKind === 'custom' && r.emojiValue === localId)
      .toArray();
    for (const r of reactions) {
      await db.reactions.update(r.id, { emojiValue: serverId });
    }

    const reactionQueued = await db.syncQueue.where('entity').equals('reaction').toArray();
    for (const q of reactionQueued) {
      const p = JSON.parse(q.payload) as Record<string, unknown>;
      if (p.emojiValue === localId) {
        await db.syncQueue.update(q.id!, {
          payload: JSON.stringify({ ...p, emojiValue: serverId }),
        });
      }
    }
  },
};
