/**
 * applyDelta.ts — WebSocket で届いたパケットを端末内 DB に反映する
 *
 * 設計: docs/design/詳細設計書_リアルタイム同期_DeltaPush.md §4.3, §8.3
 *
 * 1. seq の判定（重複は捨てる／飛んだら「欠落」として差分同期を1回走らせる）
 * 2. 行の書き込みは writeGateway に任せる（版番号の比較・未送信の変更の保護・削除・evict は差分同期と同じ）
 * 3. 受信位置（部屋ごとの epoch + seq）を、行の書き込みと同じトランザクションで進める
 *
 * 欠落があっても、届いたパケットの行は適用してよい（版番号で守られるので、順序が前後しても最後は同じ）。
 */

import { db } from '../db';
import { onCommentsChanged, onRowsChanged } from '../pull';
import { applyServerChanges, notifyEffects, syncTables, type GatewayChange } from '../writeGateway';
import { applySignals } from './applySignals';
import type { AccessPacket, Change, DeltaPacket, EntityName, ResyncPacket, WelcomePacket } from './packets';

/** 処理のあとに呼び出し側がすること */
export interface DeltaOutcome {
  /** 重複（捨てた） */
  duplicate: boolean;
  /** 差分同期で埋める必要がある（欠落・epoch の変更・stale） */
  needsCatchUp: boolean;
}

function toGateway(c: Change): GatewayChange {
  switch (c.op) {
    case 'upsert':
      return { op: 'upsert', entity: c.entity, id: c.id, v: c.v, data: c.data };
    case 'delete':
      return { op: 'delete', entity: c.entity, id: c.id, v: c.v };
    case 'evict':
      return { op: 'evict', entity: c.entity, id: c.id };
    case 'stale':
      return { op: 'stale', entity: c.entity, id: c.id, v: c.v };
  }
}

export async function applyDelta(p: DeltaPacket): Promise<DeltaOutcome> {
  let duplicate = false;
  let gap = false;

  const result = await db.transaction('rw', [...syncTables(), db.realtimeMeta], async () => {
    const pos = await db.realtimeMeta.get(p.room);
    if (pos && pos.epoch === p.epoch) {
      if (p.seq <= pos.seq) {
        duplicate = true;
        return null;
      }
      if (p.seq > pos.seq + 1) gap = true;
    } else {
      // 初めて見る部屋、またはサーバーが入れ替わった（epoch が違えば seq は比べられない）
      gap = true;
    }
    const applied = await applyServerChanges(p.changes.map(toGateway));
    await db.realtimeMeta.put({ room: p.room, epoch: p.epoch, seq: p.seq });
    return applied;
  });

  if (!result) return { duplicate, needsCatchUp: false };
  notifyEffects(result.effects);
  if (result.changed > 0) {
    // 行（チケット・プロジェクト）が変わったときだけ集計系を、コメントが変わったときだけ詳細の付随データを更新する
    if ([...result.changedEntities].some((e) => e !== 'comment')) onRowsChanged();
    if (result.changedEntities.has('comment')) onCommentsChanged();
  }
  applySignals(result.signals);
  return { duplicate: false, needsCatchUp: gap || result.stale.length > 0 };
}

/** 「差分同期で埋めてください」。示された seq から続ける */
export async function applyResync(p: ResyncPacket): Promise<DeltaOutcome> {
  await db.realtimeMeta.put({ room: p.room, epoch: p.epoch, seq: p.seq });
  return { duplicate: false, needsCatchUp: true };
}

/** welcome: 購読していない部屋の受信位置を捨てる（再送・追いつきはサーバーが resume に応えて送ってくる） */
export async function applyWelcome(p: WelcomePacket): Promise<void> {
  const subscribed = new Set(p.rooms.map((r) => r.room));
  const stale = (await db.realtimeMeta.toArray()).filter((m) => !subscribed.has(m.room));
  if (stale.length > 0) await db.realtimeMeta.bulkDelete(stale.map((m) => m.room));
}

/**
 * 見られる範囲が変わった。部屋の出入りに合わせて受信位置を作り直し、差分同期で埋める
 * （範囲が広がったときのフル同期のやり直しは、差分同期の側が access を見て行う）
 */
export async function applyAccess(p: AccessPacket, epoch: string | null): Promise<DeltaOutcome> {
  const subscribed = new Map(p.rooms.map((r) => [r.room, r.seq]));
  await db.transaction('rw', db.realtimeMeta, async () => {
    const current = await db.realtimeMeta.toArray();
    const drop = current.filter((m) => !subscribed.has(m.room)).map((m) => m.room);
    if (drop.length > 0) await db.realtimeMeta.bulkDelete(drop);
    if (epoch) {
      const known = new Set(current.map((m) => m.room));
      for (const [room, seq] of subscribed) {
        if (!known.has(room)) await db.realtimeMeta.put({ room, epoch, seq });
      }
    }
  });
  return { duplicate: false, needsCatchUp: true };
}

/** 受信位置から resume を作る（epoch はいちばん多い値。違う epoch の部屋はサーバーが resync を返す） */
export async function buildResume(): Promise<{ type: 'resume'; epoch: string | null; rooms: Array<{ room: string; seq: number }> }> {
  const metas = await db.realtimeMeta.toArray();
  if (metas.length === 0) return { type: 'resume', epoch: null, rooms: [] };
  const counts = new Map<string, number>();
  for (const m of metas) counts.set(m.epoch, (counts.get(m.epoch) ?? 0) + 1);
  let epoch = '';
  let best = 0;
  for (const [e, n] of counts) {
    if (n > best) {
      best = n;
      epoch = e;
    }
  }
  return {
    type: 'resume',
    epoch,
    rooms: metas.filter((m) => m.epoch === epoch).map(({ room, seq }) => ({ room, seq })),
  };
}

export type { EntityName };
