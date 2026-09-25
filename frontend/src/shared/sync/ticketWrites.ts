/**
 * ticketWrites.ts — チケットの書き込み（端末内 DB へ即時反映し、送信はキュー経由で裏側で行う）
 *
 * 詳細設計 §3.6。画面からチケットを変更するときは、必ずここの関数を使う（apiClient を直接呼ばない）。
 *
 * - apiPatch: サーバーへ送る形（PATCH /tickets/{key}/ の本文。snake_case、assignees は id 配列 など）
 * - rowPatch: 画面に出す形（LocalTicket の項目。assignees は {id, username, displayName} の配列 など）
 */

import { db, type LocalTicket, type SyncQueueItem } from './db';
import { withIndexFields } from './ticketMapping';
import { requestPush } from './pushRequester';

/** POST /tickets/ の本文（サーバー TicketWriteIn と同じ snake_case） */
export interface TicketCreateBody {
  title: string;
  description?: string;
  status?: string;
  priority?: string;
  ticket_type: string;
  assignees?: number[];
  reviewers?: number[];
  category?: number | null;
  project?: number | null;
  milestone?: number | null;
  parent?: number | null;
  start_date?: string | null;
  due_date?: string | null;
  labels?: number[];
  story_points?: number | null;
  cycle?: number | null;
  team_id?: number | null;
  linked_rules?: number[];
  [key: string]: unknown;
}

function uuid(): string {
  const c = globalThis.crypto as Crypto | undefined;
  if (c?.randomUUID) return c.randomUUID();
  // 古い環境向け（RFC4122 v4 形式）
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (ch) => {
    const r = (Math.random() * 16) | 0;
    return (ch === 'x' ? r : (r & 0x3) | 0x8).toString(16);
  });
}

/** 仮キーか（作成の送信待ち） */
export function isTempTicketKey(key: string | null | undefined): boolean {
  return !!key && key.startsWith('local-');
}

let tempSeq = 0;
function newTempId(): number {
  tempSeq = (tempSeq + 1) % 1000;
  return -(Date.now() * 1000 + tempSeq);
}

function now(): string {
  return new Date().toISOString();
}

async function findByKey(ticketKey: string): Promise<LocalTicket | undefined> {
  return db.tickets.where('ticketKey').equals(ticketKey).first();
}

/** 1件分の更新をトランザクション内で行う（キューのまとめ込みを含む） */
async function applyUpdate(row: LocalTicket, apiPatch: Record<string, unknown>, rowPatch: Partial<LocalTicket>): Promise<void> {
  const updated = withIndexFields({ ...row, ...rowPatch, _dirty: true, _syncError: null });
  // updatedAt は変えない（サーバー確定前に並び順が跳ねないように）
  updated.updatedAt = row.updatedAt;
  await db.tickets.put(updated);

  const items = await db.syncQueue
    .where('entityId')
    .equals(row.id)
    .and((q) => q.entity === 'ticket')
    .toArray();

  if (row._pendingCreate) {
    // 作成がまだ送れていない → 作成の本文に合成する（更新は作らない）
    const create = items.find((q) => q.operation === 'create');
    if (create) {
      const payload = JSON.parse(create.payload) as { body: Record<string, unknown> };
      payload.body = { ...payload.body, ...apiPatch };
      await db.syncQueue.update(create.id!, { payload: JSON.stringify(payload) });
    }
    return;
  }

  const pending = items.find((q) => q.operation === 'update' && q.retryCount === 0);
  if (pending) {
    const payload = JSON.parse(pending.payload) as { key: string; patch: Record<string, unknown> };
    payload.patch = { ...payload.patch, ...apiPatch };
    await db.syncQueue.update(pending.id!, { payload: JSON.stringify(payload) });
  } else {
    const item: SyncQueueItem = {
      entity: 'ticket',
      entityId: row.id,
      operation: 'update',
      payload: JSON.stringify({ key: row.ticketKey, patch: apiPatch }),
      createdAt: now(),
      retryCount: 0,
    };
    await db.syncQueue.add(item);
  }
}

/** チケットを更新する（0ms で画面に反映し、送信は裏側） */
export async function localUpdateTicket(
  ticketKey: string,
  apiPatch: Record<string, unknown>,
  rowPatch: Partial<LocalTicket>,
): Promise<void> {
  await localUpdateTickets([ticketKey], apiPatch, rowPatch);
}

/** 複数チケットに同じ変更をまとめて当てる（一括ステータス変更など）。1トランザクション */
export async function localUpdateTickets(
  ticketKeys: string[],
  apiPatch: Record<string, unknown>,
  rowPatch: Partial<LocalTicket>,
): Promise<void> {
  let touched = 0;
  await db.transaction('rw', [db.tickets, db.syncQueue], async () => {
    for (const key of ticketKeys) {
      const row = await findByKey(key);
      if (!row || row._deleted) continue;
      await applyUpdate(row, apiPatch, rowPatch);
      touched++;
    }
  });
  if (touched > 0) requestPush();
}

/**
 * チケットを作成する（オフラインでも作れる）。
 * 仮 id（負数）と仮キー（local-<uuid>）で即時に行を作り、送信が通ったら本物に付け替える（push.ts）。
 * preview には画面表示用の項目（team, assignees, labels, projectPrefix など）を渡す。
 */
export async function localCreateTicket(
  body: TicketCreateBody,
  preview: Partial<LocalTicket> = {},
): Promise<{ tempId: number; tempKey: string }> {
  const tempId = newTempId();
  const tempKey = `local-${uuid()}`;
  const t = now();
  const row = withIndexFields({
    id: tempId,
    ticketKey: tempKey,
    title: body.title,
    description: body.description ?? '',
    status: body.status ?? 'open',
    priority: body.priority ?? 'medium',
    ticketType: body.ticket_type,
    assignees: [],
    reviewers: [],
    author: null,
    category: null,
    milestone: null,
    project: body.project ?? null,
    projectPrefix: null,
    projectName: null,
    parent: body.parent ?? null,
    labels: [],
    startDate: body.start_date ?? null,
    dueDate: body.due_date ?? null,
    storyPoints: body.story_points ?? null,
    cycle: body.cycle ?? null,
    cycleName: null,
    team: null,
    commentCount: 0,
    childCount: 0,
    totalTimeSpent: 0,
    gantt_order: 0,
    createdAt: t,
    updatedAt: t,
    closedAt: null,
    ...preview,
    _dirty: true,
    _syncedAt: null,
    _pendingCreate: true,
    _syncError: null,
  } as LocalTicket);
  // preview が id / キーを上書きしないように
  row.id = tempId;
  row.ticketKey = tempKey;

  await db.transaction('rw', [db.tickets, db.syncQueue], async () => {
    await db.tickets.put(row);
    await db.syncQueue.add({
      entity: 'ticket',
      entityId: tempId,
      operation: 'create',
      payload: JSON.stringify({ body }),
      createdAt: t,
      retryCount: 0,
      idempotencyKey: uuid(),
    });
  });
  requestPush();
  return { tempId, tempKey };
}

/** チケットを削除する（画面からはすぐ消え、送信後にサーバーからも消える） */
export async function localDeleteTickets(ticketKeys: string[]): Promise<void> {
  let queued = 0;
  await db.transaction('rw', [db.tickets, db.syncQueue], async () => {
    for (const key of ticketKeys) {
      const row = await findByKey(key);
      if (!row) continue;
      const items = db.syncQueue.where('entityId').equals(row.id).and((q) => q.entity === 'ticket');
      if (row._pendingCreate) {
        // まだサーバーに無い。印だけ付け、送信前なら push 側が作成ごと取り消す。
        // 送信中だった場合は、作成が通った後に push 側が本物を削除キューに積む
        await db.tickets.put({ ...row, _deleted: true });
        queued++;
        continue;
      }
      await items.and((q) => q.operation === 'update').delete();
      await db.tickets.put({ ...row, _deleted: true, _dirty: true });
      await db.syncQueue.add({
        entity: 'ticket',
        entityId: row.id,
        operation: 'delete',
        payload: JSON.stringify({ key: row.ticketKey }),
        createdAt: now(),
        retryCount: 0,
      });
      queued++;
    }
  });
  if (queued > 0) requestPush();
}

/** コメント投稿など、行の集計値だけを手元で動かす（送信はしない。次の pull でサーバー値に揃う） */
export async function bumpTicketCounter(
  ticketKey: string,
  field: 'commentCount' | 'childCount' | 'totalTimeSpent',
  delta: number,
): Promise<void> {
  const row = await findByKey(ticketKey);
  if (!row) return;
  await db.tickets.put({ ...row, [field]: Math.max(0, (row[field] ?? 0) + delta) });
}
