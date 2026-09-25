import { useEffect, useMemo } from 'react';
import { liveQuery } from 'dexie';
import { useObservable } from '../../../shared/hooks/useObservable';
import { db, useDbGeneration } from '../../../shared/sync/db';
import {
  localUploadCustomEmoji,
  localDeleteCustomEmoji,
  pullCustomEmojis,
} from '../../../shared/sync/syncEngine';

export interface CustomEmoji {
  id: string;
  projectId: number;
  slug: string;
  name: string;
  imageUrl: string;
  updatedAt: string;
  _dirty?: boolean;
}

const EMPTY_EMOJIS: CustomEmoji[] = [];

/**
 * カスタム絵文字を管理（プロジェクト単位）
 */
export function useCustomEmojis(
  projectPrefix: string | null,
  projectId: number | null,
) {
  const dbGen = useDbGeneration();
  const rawEmojis = useObservable(
    useMemo(
      () =>
        projectId
          ? liveQuery(() =>
              db.customEmojis.where('projectId').equals(projectId).toArray(),
            )
          : null,
      // dbGen: 端末内 DB がユーザー切り替えで差し替わったら購読し直す
      // eslint-disable-next-line react-hooks/exhaustive-deps
      [projectId, dbGen],
    ),
    EMPTY_EMOJIS,
  );

  const emojis = useMemo(() => {
    const sorted = [...rawEmojis].sort((a, b) => {
      const aDirty = a._dirty ? 0 : 1;
      const bDirty = b._dirty ? 0 : 1;
      return aDirty - bDirty;
    });
    return sorted as CustomEmoji[];
  }, [rawEmojis]);

  useEffect(() => {
    if (!projectPrefix || !projectId) return;

    void pullCustomEmojis(projectPrefix, projectId);

    const handleFocus = () => {
      void pullCustomEmojis(projectPrefix, projectId);
    };

    window.addEventListener('focus', handleFocus);
    return () => {
      window.removeEventListener('focus', handleFocus);
    };
  }, [projectPrefix, projectId]);

  const uploadEmoji = async (file: File, slug: string, name: string): Promise<string> => {
    if (!projectPrefix || !projectId) {
      throw new Error('Project not available');
    }
    return localUploadCustomEmoji(projectPrefix, projectId, file, slug, name);
  };

  const deleteEmoji = async (emojiId: string): Promise<void> => {
    if (!projectPrefix) {
      throw new Error('Project not available');
    }
    await localDeleteCustomEmoji(projectPrefix, emojiId);
  };

  return {
    emojis,
    uploadEmoji,
    deleteEmoji,
  };
}
