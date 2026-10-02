/**
 * commentView.ts — 端末内のコメントを、画面が使う形（閲覧者ごとの編集可否つき）に導く
 *
 * サーバーは閲覧者ごとに canEdit を計算していたが、端末内 DB のコメントは閲覧者に依存しない事実だけを持つ
 * （リアルタイムで全員に同じ本文を配るため）。サーバーの判定（ticket_repo.rs）と同じ規則で、ここで導く。
 * 編集の可否は表示の都合にすぎず、実際の可否はサーバーが PATCH / DELETE のときに判定する。
 */
import type { LocalComment, LocalCommentUser } from './db';

export interface CommentView {
  id: number;
  body: string;
  author: LocalCommentUser;
  actingUser: LocalCommentUser | null;
  createdAt: string;
  updatedAt: string | null;
  anchorStart?: number;
  anchorEnd?: number;
  anchorQuote?: string;
  parentCommentId: number | null;
  isDeleted: boolean;
  replyCount: number;
  canEdit: boolean;
  canDelete: boolean;
}

/**
 * @param all 1つのチケットのコメント（作成日時順）
 * @param viewerId 閲覧者のユーザー ID（未ログインは null）
 * @param viewerIsAssignee 閲覧者がこのチケットの担当者か（AI エージェントのコメントを編集できる条件）
 */
export function deriveComments(all: LocalComment[], viewerId: number | null, viewerIsAssignee: boolean): CommentView[] {
  const replies = new Map<number, number>();
  for (const c of all) {
    if (c.parentCommentId !== null) replies.set(c.parentCommentId, (replies.get(c.parentCommentId) ?? 0) + 1);
  }
  return all.map((c) => {
    const isOwn = viewerId !== null && c.author.id === viewerId;
    const isActingUser = viewerId !== null && c.actingUser?.id === viewerId;
    const isAiAgentAndAssignee = c.isAiAgentAuthor && viewerIsAssignee;
    const canEdit = !c.isDeleted && (isOwn || isActingUser || isAiAgentAndAssignee);
    return {
      id: c.id,
      body: c.body,
      author: c.author,
      actingUser: c.actingUser,
      createdAt: c.createdAt,
      updatedAt: c.updatedAt,
      ...(c.anchorStart !== null ? { anchorStart: c.anchorStart } : {}),
      ...(c.anchorEnd !== null ? { anchorEnd: c.anchorEnd } : {}),
      ...(c.anchorQuote !== null ? { anchorQuote: c.anchorQuote } : {}),
      parentCommentId: c.parentCommentId,
      isDeleted: c.isDeleted,
      replyCount: replies.get(c.id) ?? 0,
      canEdit,
      canDelete: canEdit,
    };
  });
}
