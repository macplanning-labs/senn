/**
 * commentWrites.ts — コメントの書き込み直後に、端末内 DB へ「楽観的に」反映する
 *
 * コメントの書き込み自体は今までどおりサーバーへ直接（REST）。ここでは画面を待たせないために、
 * 端末内の行を先に書き換え、失敗したら元に戻す。サーバーが確定した値（版番号つき）は、
 * リアルタイムまたは差分同期で届いて、版番号の比較で上書きされる。
 */
import { db, type LocalComment, type LocalCommentUser } from './db';

/** 元に戻す関数 */
export type Rollback = () => Promise<void>;

export interface NewComment {
  ticketKey: string;
  body: string;
  parentCommentId?: number | null;
  anchor?: { start: number; end: number; quote: string };
  author: LocalCommentUser;
}

/** 仮のコメントを入れる。id は負数（サーバーの id と衝突せず、差分同期の掃除からも外れる） */
export async function optimisticAddComment(input: NewComment): Promise<{ tempId: number; rollback: Rollback } | undefined> {
  const ticket = await db.tickets.where('ticketKey').equals(input.ticketKey).first();
  if (!ticket) return undefined;
  const tempId = -Date.now() - Math.floor(Math.random() * 1000);
  const row: LocalComment = {
    id: tempId,
    ticketId: ticket.id,
    body: input.body,
    author: input.author,
    actingUser: null,
    createdAt: new Date().toISOString(),
    updatedAt: null,
    anchorStart: input.anchor?.start ?? null,
    anchorEnd: input.anchor?.end ?? null,
    anchorQuote: input.anchor?.quote ?? null,
    parentCommentId: input.parentCommentId ?? null,
    isDeleted: false,
    isAiAgentAuthor: false,
    _syncedAt: null,
  };
  await db.comments.put(row);
  return { tempId, rollback: async () => void (await db.comments.delete(tempId)) };
}

/**
 * サーバーが id を付けた。仮の行を本物の id に置き換える（画面から一瞬消えない）。
 * 版番号はまだ無いので付けない。リアルタイム/差分同期で届く版（1以上）が上書きする。
 * 先にリアルタイムで本物の行が届いていたら、それを残す。
 */
export async function commitAddedComment(tempId: number, realId: number): Promise<void> {
  await db.transaction('rw', db.comments, async () => {
    const temp = await db.comments.get(tempId);
    await db.comments.delete(tempId);
    if (!temp) return;
    const existing = await db.comments.get(realId);
    if (!existing) await db.comments.put({ ...temp, id: realId });
  });
}

/** 本文を書き換える。戻り値は元に戻す関数（行が無ければ undefined） */
export async function optimisticEditComment(id: number, body: string): Promise<Rollback | undefined> {
  const before = await db.comments.get(id);
  if (!before) return undefined;
  await db.comments.put({ ...before, body, updatedAt: new Date().toISOString() });
  return async () => void (await db.comments.put(before));
}

/** 論理削除にする（本文は空になる）。戻り値は元に戻す関数 */
export async function optimisticDeleteComment(id: number): Promise<Rollback | undefined> {
  const before = await db.comments.get(id);
  if (!before) return undefined;
  await db.comments.put({ ...before, isDeleted: true, body: '' });
  return async () => void (await db.comments.put(before));
}
