/**
 * applySignals.ts — 「変わった」という合図を受けて、画面のキャッシュを取り直す
 *
 * 設計: docs/design/詳細設計書_リアルタイム同期_DeltaPush.md §16
 * 端末内 DB に表を持たず、REST + react-query で動いている対象（添付・サイクル・通知・Wiki・リアクション）向け。
 * 取り直しは REST が権限を判定するので、合図が余計に届いても見えないものは見えない。
 */

import { db } from '../db';
import { invalidateFromSync, isTicketDetailLoaded } from '../pull';
import { allSignalQueryKeys, queryKeysForSignals, reactionTicketIds } from './signals';

type Signal = { entity: string; id: number };

/**
 * リアクションを取り直す。端末に無いチケット、リアクションを一度も表示していないチケットは、
 * 開いたときの取得に任せる（イベントごとに全チケット分を取りに行かない）。
 */
export async function refreshReactions(ticketId: number): Promise<void> {
  const ticket = await db.tickets.get(ticketId);
  if (!ticket || ticket._pendingCreate) return;
  const shownBefore = (await db.reactions.where('ticketId').equals(ticketId).count()) > 0;
  if (!shownBefore && !isTicketDetailLoaded(ticket.ticketKey)) return;
  // syncEngine は realtimeClient を読み込むので、循環を避けて必要なときに読む
  const { pullReactions } = await import('../syncEngine');
  await pullReactions(ticket.ticketKey, ticket.id);
}

/** 合図を反映する（トランザクション確定後に呼ぶ） */
export function applySignals(signals: Signal[]): void {
  if (signals.length === 0) return;
  const keys = queryKeysForSignals(signals);
  if (keys.length > 0) invalidateFromSync(keys);
  for (const ticketId of reactionTicketIds(signals)) {
    void refreshReactions(ticketId).catch((e) => console.warn('[realtime] reaction refresh failed', e));
  }
}

/** 取りこぼしたかもしれないとき（欠落・再同期）: 何が変わったか分からないので、対象のキャッシュをすべて取り直す */
export function applyAllSignals(): void {
  invalidateFromSync(allSignalQueryKeys());
}
