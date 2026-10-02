/**
 * signals.test.ts — キャッシュ更新の合図テスト
 */

import { describe, it, expect } from 'vitest';
import {
  isSignalEntity,
  queryKeysForSignals,
  reactionTicketIds,
  allSignalQueryKeys,
  SIGNAL_ENTITIES,
  SIGNAL_QUERY_KEYS,
} from './signals';

describe('signals', () => {
  describe('isSignalEntity', () => {
    it('合図エンティティを正しく判定する', () => {
      expect(isSignalEntity('attachment')).toBe(true);
      expect(isSignalEntity('cycle')).toBe(true);
      expect(isSignalEntity('notification')).toBe(true);
      expect(isSignalEntity('wiki')).toBe(true);
      expect(isSignalEntity('reaction')).toBe(true);
    });

    it('合図でないエンティティを返す', () => {
      expect(isSignalEntity('ticket')).toBe(false);
      expect(isSignalEntity('comment')).toBe(false);
      expect(isSignalEntity('project')).toBe(false);
      expect(isSignalEntity('unknown')).toBe(false);
    });
  });

  describe('queryKeysForSignals', () => {
    it('attachment の重複を 1 つに集約する', () => {
      const signals = [
        { entity: 'attachment', id: 1 },
        { entity: 'attachment', id: 2 },
      ];
      const result = queryKeysForSignals(signals);
      expect(result).toEqual([['ticket']]);
    });

    it('cycle は 3 つのキーを返す', () => {
      const signals = [{ entity: 'cycle', id: 1 }];
      const result = queryKeysForSignals(signals);
      expect(result).toEqual([['cycles'], ['cycle'], ['cycle-progress']]);
    });

    it('attachment と wiki の混合は SIGNAL_ENTITIES 定義順', () => {
      const signals = [
        { entity: 'wiki', id: 1 },
        { entity: 'attachment', id: 2 },
      ];
      const result = queryKeysForSignals(signals);
      // SIGNAL_ENTITIES 定義順: attachment が先（ticket）、その後 wiki
      expect(result).toEqual([['ticket'], ['wiki-pages'], ['wiki-page'], ['wiki']]);
    });

    it('ticket/comment/project エンティティは無視される', () => {
      const signals = [
        { entity: 'ticket', id: 1 },
        { entity: 'comment', id: 2 },
        { entity: 'project', id: 3 },
      ];
      const result = queryKeysForSignals(signals);
      expect(result).toEqual([]);
    });

    it('未知のエンティティ名は無視される', () => {
      const signals = [
        { entity: 'unknown', id: 1 },
        { entity: 'attachment', id: 2 },
      ];
      const result = queryKeysForSignals(signals);
      expect(result).toEqual([['ticket']]);
    });

    it('reaction は queryKeysForSignals に含まれない', () => {
      const signals = [
        { entity: 'reaction', id: 1 },
        { entity: 'cycle', id: 2 },
      ];
      const result = queryKeysForSignals(signals);
      // reaction は無視、cycle のみ
      expect(result).toEqual([['cycles'], ['cycle'], ['cycle-progress']]);
    });

    it('複雑な混合でも重複排除される', () => {
      const signals = [
        { entity: 'attachment', id: 1 },
        { entity: 'attachment', id: 2 },
        { entity: 'notification', id: 3 },
        { entity: 'wiki', id: 4 },
        { entity: 'wiki', id: 5 },
      ];
      const result = queryKeysForSignals(signals);
      // attachment は 1 度、notification は 2 つ、wiki は 3 つ、重複なし
      expect(result).toHaveLength(6);
      expect(result).toContainEqual(['ticket']);
      expect(result).toContainEqual(['notifications']);
      expect(result).toContainEqual(['unread-count']);
      expect(result).toContainEqual(['wiki-pages']);
      expect(result).toContainEqual(['wiki-page']);
      expect(result).toContainEqual(['wiki']);
    });
  });

  describe('reactionTicketIds', () => {
    it('reaction エンティティから ticket ID を抽出する', () => {
      const signals = [
        { entity: 'reaction', id: 10 },
        { entity: 'reaction', id: 20 },
      ];
      const result = reactionTicketIds(signals);
      expect(result).toContainEqual(10);
      expect(result).toContainEqual(20);
      expect(result.length).toBe(2);
    });

    it('reaction のみを取る', () => {
      const signals = [
        { entity: 'reaction', id: 10 },
        { entity: 'attachment', id: 1 },
        { entity: 'cycle', id: 2 },
      ];
      const result = reactionTicketIds(signals);
      expect(result).toEqual([10]);
    });

    it('重複する reaction ID は一度だけ返す', () => {
      const signals = [
        { entity: 'reaction', id: 10 },
        { entity: 'reaction', id: 10 },
        { entity: 'reaction', id: 20 },
      ];
      const result = reactionTicketIds(signals);
      expect(result.length).toBe(2);
      expect(result).toContainEqual(10);
      expect(result).toContainEqual(20);
    });

    it('reaction がない場合は空配列を返す', () => {
      const signals = [
        { entity: 'attachment', id: 1 },
        { entity: 'wiki', id: 2 },
      ];
      const result = reactionTicketIds(signals);
      expect(result).toEqual([]);
    });
  });

  describe('allSignalQueryKeys', () => {
    it('すべての合図キャッシュキーを返す', () => {
      const result = allSignalQueryKeys();
      // reaction は除く
      expect(result).toContainEqual(['ticket']);         // attachment
      expect(result).toContainEqual(['cycles']);         // cycle
      expect(result).toContainEqual(['cycle']);
      expect(result).toContainEqual(['cycle-progress']);
      expect(result).toContainEqual(['notifications']);  // notification
      expect(result).toContainEqual(['unread-count']);
      expect(result).toContainEqual(['wiki-pages']);     // wiki
      expect(result).toContainEqual(['wiki-page']);
      expect(result).toContainEqual(['wiki']);
    });

    it('重複がない', () => {
      const result = allSignalQueryKeys();
      const stringified = result.map(k => JSON.stringify(k));
      const unique = new Set(stringified);
      expect(stringified.length).toBe(unique.size);
    });

    it('定義順を守る', () => {
      const result = allSignalQueryKeys();
      // SIGNAL_ENTITIES: ['attachment', 'cycle', 'notification', 'wiki', 'reaction']
      // SIGNAL_QUERY_KEYS の順序: attachment, cycle, notification, wiki
      const indexes = {
        ticket: result.findIndex(k => JSON.stringify(k) === JSON.stringify(['ticket'])),
        cycles: result.findIndex(k => JSON.stringify(k) === JSON.stringify(['cycles'])),
        notifications: result.findIndex(k => JSON.stringify(k) === JSON.stringify(['notifications'])),
        wikiPages: result.findIndex(k => JSON.stringify(k) === JSON.stringify(['wiki-pages'])),
      };
      expect(indexes.ticket < indexes.cycles).toBe(true);
      expect(indexes.cycles < indexes.notifications).toBe(true);
      expect(indexes.notifications < indexes.wikiPages).toBe(true);
    });
  });
});
