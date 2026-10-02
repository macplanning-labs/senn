/**
 * commentMapping.test.ts — LocalComment へのマッピングとインデックステスト
 */
import 'fake-indexeddb/auto';
import { describe, it, expect, beforeEach } from 'vitest';
import { db, openUserDb } from './db';
import { toLocalComment } from './ticketMapping';
import { nextUserId } from './__tests__/testUtils';

describe('toLocalComment', () => {
  describe('DTO から LocalComment へのマッピング', () => {
    const testCases = [
      {
        name: '完全なDTO（全フィールド + v）を入れて正しくマッピング',
        dto: {
          id: 1,
          ticketId: 10,
          body: 'Test comment',
          author: { id: 100, username: 'alice', displayName: 'Alice' },
          actingUser: { id: 101, username: 'bob', displayName: 'Bob' },
          createdAt: '2026-09-28T10:00:00Z',
          updatedAt: '2026-09-29T11:00:00Z',
          anchorStart: 5,
          anchorEnd: 15,
          anchorQuote: 'quoted text',
          parentCommentId: null,
          isDeleted: false,
          isAiAgentAuthor: false,
          v: 42,
        },
        expectedFields: {
          id: 1,
          ticketId: 10,
          body: 'Test comment',
          author: { id: 100, username: 'alice', displayName: 'Alice' },
          actingUser: { id: 101, username: 'bob', displayName: 'Bob' },
          createdAt: '2026-09-28T10:00:00Z',
          updatedAt: '2026-09-29T11:00:00Z',
          anchorStart: 5,
          anchorEnd: 15,
          anchorQuote: 'quoted text',
          parentCommentId: null,
          isDeleted: false,
          isAiAgentAuthor: false,
          v: 42,
        },
      },
      {
        name: 'snake_case エイリアス (ticket_id, created_at など)',
        dto: {
          id: 2,
          ticket_id: 20,
          body: 'Comment with snake_case',
          author: { id: 102, username: 'charlie', display_name: 'Charlie' },
          actingUser: null,
          created_at: '2026-09-28T12:00:00Z',
          updated_at: '2026-09-29T13:00:00Z',
          anchor_start: 10,
          anchor_end: 20,
          anchor_quote: 'snaked',
          parent_comment_id: 5,
          is_deleted: false,
          is_ai_agent_author: true,
        },
        expectedFields: {
          id: 2,
          ticketId: 20,
          body: 'Comment with snake_case',
          author: { id: 102, username: 'charlie', displayName: 'Charlie' },
          actingUser: null,
          createdAt: '2026-09-28T12:00:00Z',
          updatedAt: '2026-09-29T13:00:00Z',
          anchorStart: 10,
          anchorEnd: 20,
          anchorQuote: 'snaked',
          parentCommentId: 5,
          isDeleted: false,
          isAiAgentAuthor: true,
        },
      },
      {
        name: '削除済みコメント（body = "" でも isDeleted = true）',
        dto: {
          id: 3,
          ticketId: 30,
          body: '',
          author: { id: 103, username: 'dave', displayName: 'Dave' },
          actingUser: null,
          createdAt: '2026-09-28T14:00:00Z',
          updatedAt: null,
          anchorStart: null,
          anchorEnd: null,
          anchorQuote: null,
          parentCommentId: null,
          isDeleted: true,
          isAiAgentAuthor: false,
        },
        expectedFields: {
          id: 3,
          ticketId: 30,
          body: '',
          isDeleted: true,
        },
      },
      {
        name: '作者情報が無い場合は id:0 の ユーザーを作る',
        dto: {
          id: 4,
          ticketId: 40,
          body: 'No author',
          author: null,
          actingUser: undefined,
          createdAt: '2026-09-28T16:00:00Z',
          updatedAt: null,
          anchorStart: null,
          anchorEnd: null,
          anchorQuote: null,
          parentCommentId: null,
          isDeleted: false,
          isAiAgentAuthor: false,
        },
        expectedFields: {
          id: 4,
          author: { id: 0, username: '', displayName: '' },
          actingUser: null,
        },
      },
      {
        name: '不完全な author (displayName のみ) も落ちずに処理',
        dto: {
          id: 5,
          ticketId: 50,
          body: 'Partial author',
          author: { displayName: 'Partial User' },
          actingUser: { id: 105 },
          createdAt: '2026-09-28T18:00:00Z',
          updatedAt: null,
          anchorStart: null,
          anchorEnd: null,
          anchorQuote: null,
          parentCommentId: null,
          isDeleted: false,
          isAiAgentAuthor: false,
        },
        expectedFields: {
          author: { id: 0, username: '', displayName: 'Partial User' },
          actingUser: { id: 105, username: '', displayName: '' },
        },
      },
    ];

    testCases.forEach(({ name, dto, expectedFields }) => {
      it(name, () => {
        const result = toLocalComment(dto);
        Object.entries(expectedFields).forEach(([key, value]) => {
          expect(result[key as keyof typeof expectedFields]).toEqual(value);
        });
      });
    });

    it('canEdit/canDelete/replyCount/email/attachments は結果に含まない', () => {
      const dto = {
        id: 100,
        ticketId: 100,
        body: 'Comment',
        author: { id: 1, username: 'u', displayName: 'U' },
        actingUser: null,
        createdAt: '2026-09-28T20:00:00Z',
        updatedAt: null,
        anchorStart: null,
        anchorEnd: null,
        anchorQuote: null,
        parentCommentId: null,
        isDeleted: false,
        isAiAgentAuthor: false,
        canEdit: true,
        canDelete: true,
        replyCount: 5,
        email: 'test@example.com',
        attachments: [{ id: 1, url: 'http://example.com/file' }],
      };

      const result = toLocalComment(dto);
      expect(result).not.toHaveProperty('canEdit');
      expect(result).not.toHaveProperty('canDelete');
      expect(result).not.toHaveProperty('replyCount');
      expect(result).not.toHaveProperty('email');
      expect(result).not.toHaveProperty('attachments');
    });

    it('v が無い場合は結果に含まない', () => {
      const dto = {
        id: 101,
        ticketId: 101,
        body: 'No v',
        author: { id: 1, username: 'u', displayName: 'U' },
        actingUser: null,
        createdAt: '2026-09-28T21:00:00Z',
        updatedAt: null,
        anchorStart: null,
        anchorEnd: null,
        anchorQuote: null,
        parentCommentId: null,
        isDeleted: false,
        isAiAgentAuthor: false,
      };

      const result = toLocalComment(dto);
      expect(result).not.toHaveProperty('v');
    });

    it('v が非有限数値の場合は無視', () => {
      const dto1 = {
        id: 102,
        ticketId: 102,
        body: 'Infinity',
        author: { id: 1, username: 'u', displayName: 'U' },
        actingUser: null,
        createdAt: '2026-09-28T22:00:00Z',
        updatedAt: null,
        anchorStart: null,
        anchorEnd: null,
        anchorQuote: null,
        parentCommentId: null,
        isDeleted: false,
        isAiAgentAuthor: false,
        v: Infinity,
      };

      const result1 = toLocalComment(dto1);
      expect(result1).not.toHaveProperty('v');

      const dto2 = { ...dto1, id: 103, v: NaN };
      const result2 = toLocalComment(dto2);
      expect(result2).not.toHaveProperty('v');
    });
  });

  describe('Dexie DB インデックステスト', () => {
    beforeEach(async () => {
      openUserDb(nextUserId());
      await db.comments.clear();
    });

    it('[ticketId+createdAt] インデックスで特定チケットのコメントを時系列取得', async () => {
      // ticketId=1 のコメント3件、ticketId=2 のコメント1件を挿入
      const comment1_1 = toLocalComment({
        id: 1,
        ticketId: 1,
        body: 'first',
        author: { id: 1, username: 'u1', displayName: 'U1' },
        actingUser: null,
        createdAt: '2026-09-28T10:00:00Z',
        updatedAt: null,
        anchorStart: null,
        anchorEnd: null,
        anchorQuote: null,
        parentCommentId: null,
        isDeleted: false,
        isAiAgentAuthor: false,
      });

      const comment1_2 = toLocalComment({
        id: 2,
        ticketId: 1,
        body: 'second',
        author: { id: 2, username: 'u2', displayName: 'U2' },
        actingUser: null,
        createdAt: '2026-09-28T11:00:00Z',
        updatedAt: null,
        anchorStart: null,
        anchorEnd: null,
        anchorQuote: null,
        parentCommentId: null,
        isDeleted: false,
        isAiAgentAuthor: false,
      });

      const comment1_3 = toLocalComment({
        id: 3,
        ticketId: 1,
        body: 'third',
        author: { id: 3, username: 'u3', displayName: 'U3' },
        actingUser: null,
        createdAt: '2026-09-28T12:00:00Z',
        updatedAt: null,
        anchorStart: null,
        anchorEnd: null,
        anchorQuote: null,
        parentCommentId: null,
        isDeleted: false,
        isAiAgentAuthor: false,
      });

      const comment2_1 = toLocalComment({
        id: 4,
        ticketId: 2,
        body: 'ticket2',
        author: { id: 4, username: 'u4', displayName: 'U4' },
        actingUser: null,
        createdAt: '2026-09-28T13:00:00Z',
        updatedAt: null,
        anchorStart: null,
        anchorEnd: null,
        anchorQuote: null,
        parentCommentId: null,
        isDeleted: false,
        isAiAgentAuthor: false,
      });

      await db.comments.bulkAdd([comment1_1, comment1_2, comment1_3, comment2_1]);

      // [ticketId+createdAt] インデックスで ticketId=1 のコメントを範囲検索
      const result = await db.comments
        .where('[ticketId+createdAt]')
        .between([1, ''], [1, '￿'])
        .toArray();

      expect(result).toHaveLength(3);
      expect(result[0].id).toBe(1);
      expect(result[0].body).toBe('first');
      expect(result[1].id).toBe(2);
      expect(result[1].body).toBe('second');
      expect(result[2].id).toBe(3);
      expect(result[2].body).toBe('third');
    });

    it('ticketId インデックスで特定チケットのコメント数を数える', async () => {
      const comments = [
        toLocalComment({
          id: 10,
          ticketId: 5,
          body: 'c1',
          author: { id: 1, username: 'u1', displayName: 'U1' },
          actingUser: null,
          createdAt: '2026-09-28T10:00:00Z',
          updatedAt: null,
          anchorStart: null,
          anchorEnd: null,
          anchorQuote: null,
          parentCommentId: null,
          isDeleted: false,
          isAiAgentAuthor: false,
        }),
        toLocalComment({
          id: 11,
          ticketId: 6,
          body: 'c2',
          author: { id: 2, username: 'u2', displayName: 'U2' },
          actingUser: null,
          createdAt: '2026-09-28T11:00:00Z',
          updatedAt: null,
          anchorStart: null,
          anchorEnd: null,
          anchorQuote: null,
          parentCommentId: null,
          isDeleted: false,
          isAiAgentAuthor: false,
        }),
        toLocalComment({
          id: 12,
          ticketId: 5,
          body: 'c3',
          author: { id: 3, username: 'u3', displayName: 'U3' },
          actingUser: null,
          createdAt: '2026-09-28T12:00:00Z',
          updatedAt: null,
          anchorStart: null,
          anchorEnd: null,
          anchorQuote: null,
          parentCommentId: null,
          isDeleted: false,
          isAiAgentAuthor: false,
        }),
      ];

      await db.comments.bulkAdd(comments);

      const count2 = await db.comments.where('ticketId').equals(2).count();
      expect(count2).toBe(0);

      const count5 = await db.comments.where('ticketId').equals(5).count();
      expect(count5).toBe(2);

      const count6 = await db.comments.where('ticketId').equals(6).count();
      expect(count6).toBe(1);
    });
  });
});
