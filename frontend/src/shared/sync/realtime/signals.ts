/**
 * signals.ts — キャッシュ更新の合図
 *
 * 「合図(signal)」とは、端末内 DB に表を持たないエンティティの変更通知。
 * サーバーから実データではなく「変わった」という事実だけが送られ、
 * 端末側は react-query キャッシュを取り直す。
 *
 * 詳細設計: docs/design/詳細設計書_リアルタイム同期_DeltaPush.md §16
 */

import type { QueryKey } from '@tanstack/react-query';
import type { EntityName } from './packets';

/** 端末内 DB に表を持たず、REST + react-query で動いている対象 */
export type SignalEntity = 'attachment' | 'cycle' | 'notification' | 'wiki' | 'reaction';

export const SIGNAL_ENTITIES: readonly SignalEntity[] = ['attachment', 'cycle', 'notification', 'wiki', 'reaction'];

/** 文字列が SignalEntity かどうかを判定（型ガード） */
export function isSignalEntity(e: string): e is SignalEntity {
  return SIGNAL_ENTITIES.includes(e as SignalEntity);
}

/** reaction 以外の合図が取り直すキャッシュ */
export const SIGNAL_QUERY_KEYS: Record<Exclude<SignalEntity, 'reaction'>, QueryKey[]> = {
  attachment: [['ticket']],                              // 詳細の付随データ（添付を含む）
  cycle: [['cycles'], ['cycle'], ['cycle-progress']],
  notification: [['notifications'], ['unread-count']],
  wiki: [['wiki-pages'], ['wiki-page'], ['wiki']],
};

/** JSON.stringify による重複排除用ヘルパー */
function deduplicateKeys(keys: QueryKey[]): QueryKey[] {
  const seen = new Set<string>();
  const result: QueryKey[] = [];
  for (const key of keys) {
    const stringified = JSON.stringify(key);
    if (!seen.has(stringified)) {
      seen.add(stringified);
      result.push(key);
    }
  }
  return result;
}

/**
 * 合図の一覧から、取り直すキャッシュのキーを重複なしで返す（reaction は含めない）。
 * 順序は SIGNAL_QUERY_KEYS の定義順。
 * 未知のエンティティ名は無視される。
 */
export function queryKeysForSignals(signals: ReadonlyArray<{ entity: EntityName | string; id: number }>): QueryKey[] {
  const keys: QueryKey[] = [];

  // SIGNAL_ENTITIES の定義順に従ってキーを収集
  for (const entity of SIGNAL_ENTITIES) {
    if (entity !== 'reaction') {
      // signals に このエンティティがあるかチェック
      if (signals.some(s => s.entity === entity)) {
        const queryKeys = SIGNAL_QUERY_KEYS[entity as Exclude<SignalEntity, 'reaction'>];
        keys.push(...queryKeys);
      }
    }
  }

  return deduplicateKeys(keys);
}

/**
 * reaction の合図（id はチケット ID）から、リアクションを取り直すチケット ID を重複なしで返す
 */
export function reactionTicketIds(signals: ReadonlyArray<{ entity: EntityName | string; id: number }>): number[] {
  const ids = new Set<number>();

  for (const signal of signals) {
    if (signal.entity === 'reaction') {
      ids.add(signal.id);
    }
  }

  return Array.from(ids);
}

/**
 * 再接続・再同期で何が変わったか分からないとき用: すべての合図のキャッシュキー（重複なし）
 */
export function allSignalQueryKeys(): QueryKey[] {
  const keys: QueryKey[] = [];

  for (const entity of SIGNAL_ENTITIES) {
    if (entity !== 'reaction') {
      const queryKeys = SIGNAL_QUERY_KEYS[entity as Exclude<SignalEntity, 'reaction'>];
      keys.push(...queryKeys);
    }
  }

  return deduplicateKeys(keys);
}
