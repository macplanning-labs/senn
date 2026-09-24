import { useEffect, useMemo } from 'react';
import { liveQuery } from 'dexie';
import { useObservable } from '../../../shared/hooks/useObservable';
import { db, type LocalReaction } from '../../../shared/sync/db';
import { localToggleReaction, pullReactions } from '../../../shared/sync/syncEngine';

const EMPTY_REACTIONS: LocalReaction[] = [];

/**
 * チケットのリアクション情報を管理（Local-first、dirty優先マージ）
 *
 * - Dexie liveQueryで reactions を購読
 * - dirty行は常に最優先
 * - 操作 → localToggleReaction → 購読が即再描画
 * - チケット詳細オープン・window focus時に pullReactions
 */
export function useTicketReactions(ticketKey: string | null, ticketId: number | null) {
  const reactionsByTicket = useObservable(
    useMemo(
      () =>
        ticketId
          ? liveQuery(async () => {
              if (!ticketId) return [];

              // Dexie から tickets のリアクション一覧を取得
              const localReactions = await db.reactions
                .where('ticketId')
                .equals(ticketId)
                .toArray();

              // dirty優先マージ：dirty行を最優先、その後サーバー行
              const dirty = localReactions.filter((r) => r._dirty);
              const clean = localReactions.filter((r) => !r._dirty);

              // 論理キーでの衝突チェック：dirty行が勝ち
              const keys = new Set<string>();
              const merged: LocalReaction[] = [];

              for (const r of dirty) {
                const key = `${r.ticketId}:${r.userId}:${r.emojiKind}:${r.emojiValue}`;
                if (!keys.has(key)) {
                  keys.add(key);
                  merged.push(r);
                }
              }

              for (const r of clean) {
                const key = `${r.ticketId}:${r.userId}:${r.emojiKind}:${r.emojiValue}`;
                if (!keys.has(key)) {
                  keys.add(key);
                  merged.push(r);
                }
              }

              return merged;
            })
          : null,
      [ticketId],
    ),
    EMPTY_REACTIONS,
  );

  // チケット詳細オープン・window focus時に pullReactions を呼ぶ
  useEffect(() => {
    if (!ticketKey || !ticketId) return;

    const handleFocus = () => {
      void pullReactions(ticketKey, ticketId);
    };

    // 初回ロード
    void pullReactions(ticketKey, ticketId);

    // window focus時
    window.addEventListener('focus', handleFocus);

    return () => {
      window.removeEventListener('focus', handleFocus);
    };
  }, [ticketKey, ticketId]);

  return {
    reactions: reactionsByTicket,

    /**
     * リアクションをトグル（追加/削除）
     */
    async toggleReaction(
      userId: number,
      emojiKind: 'unicode' | 'custom',
      emojiValue: string,
    ): Promise<void> {
      if (!ticketKey || !ticketId) return;
      await localToggleReaction(ticketKey, ticketId, userId, emojiKind, emojiValue);
    },

    /**
     * 手動で pullReactions を実行
     */
    async refreshReactions(): Promise<void> {
      if (!ticketKey || !ticketId) return;
      await pullReactions(ticketKey, ticketId);
    },
  };
}
