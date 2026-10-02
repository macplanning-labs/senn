import { describe, expect, it } from 'vitest';
import type { LocalComment } from './db';
import { deriveComments } from './commentView';
import { toLocalComment } from './ticketMapping';
import { commentDto } from './__tests__/testUtils';

const c = (id: number, over: Record<string, unknown> = {}): LocalComment => toLocalComment(commentDto(id, 1, over));

describe('deriveComments（サーバーの canEdit 判定と同じ規則）', () => {
  it('本人のコメントは編集・削除できる。他人のは不可', () => {
    const out = deriveComments([c(1, { author: { id: 7, username: 'me', displayName: 'Me' } }), c(2)], 7, false);
    expect(out.map((x) => [x.canEdit, x.canDelete])).toEqual([[true, true], [false, false]]);
  });

  it('AI エージェント経由の実行者本人は編集できる', () => {
    const out = deriveComments([c(1, { actingUser: { id: 7, username: 'me', displayName: 'Me' } })], 7, false);
    expect(out[0].canEdit).toBe(true);
  });

  it('AI エージェントが書いたコメントは、閲覧者が担当者のときだけ編集できる', () => {
    const list = [c(1, { isAiAgentAuthor: true })];
    expect(deriveComments(list, 7, true)[0].canEdit).toBe(true);
    expect(deriveComments(list, 7, false)[0].canEdit).toBe(false);
    // AI エージェントのコメントでなければ、担当者でも他人のコメントは編集できない
    expect(deriveComments([c(1)], 7, true)[0].canEdit).toBe(false);
  });

  it('削除済みは本人でも編集できない', () => {
    const out = deriveComments([c(1, { author: { id: 7, username: 'me', displayName: 'Me' }, isDeleted: true, body: '' })], 7, false);
    expect(out[0]).toMatchObject({ canEdit: false, canDelete: false, isDeleted: true });
  });

  it('未ログイン（viewerId なし）は誰のコメントも編集できない', () => {
    expect(deriveComments([c(1, { author: { id: 0, username: '', displayName: '' } })], null, false)[0].canEdit).toBe(false);
  });

  it('返信数は端末内の子コメントから数える（削除済みの返信も含む。サーバーと同じ）', () => {
    const out = deriveComments([c(1), c(2, { parentCommentId: 1 }), c(3, { parentCommentId: 1, isDeleted: true, body: '' }), c(4)], 1, false);
    expect(out.map((x) => x.replyCount)).toEqual([2, 0, 0, 0]);
  });

  it('アンカーはあるものだけを持つ（サーバーの応答と同じ形）', () => {
    const out = deriveComments([c(1, { anchorStart: 3, anchorEnd: 9, anchorQuote: '引用' }), c(2)], 1, false);
    expect(out[0]).toMatchObject({ anchorStart: 3, anchorEnd: 9, anchorQuote: '引用' });
    expect(out[1]).not.toHaveProperty('anchorStart');
  });
});
