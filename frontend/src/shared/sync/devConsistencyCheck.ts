/**
 * devConsistencyCheck.ts — 開発用: 端末内 DB とサーバーの一覧 API の差を出す（WIPAPPDEV-000118）
 *
 * 開発ビルドでは、ブラウザのコンソールで `await window.__sennSyncCheck()` と打つと、
 * 端末内のチケットと GET /tickets/（全ページ）の結果を比べ、差をコンソールに表で出す。
 * 本番ビルドでは登録しない。
 */
import { apiClient } from '../api/client';
import { db } from './db';

export interface SyncDiff {
  onlyLocal: string[];
  onlyServer: string[];
  /** 同じキーで更新日時・ステータス・タイトルが違うもの */
  different: Array<{ key: string; field: string; local: unknown; server: unknown }>;
}

type ServerRow = { ticketKey: string; updatedAt: string; status: string; title: string };

export async function compareTicketsWithServer(): Promise<SyncDiff> {
  const server: ServerRow[] = [];
  for (let page = 1; page < 1000; page++) {
    const res = await apiClient.get<{ results: ServerRow[]; next: string | null }>('/tickets/', { params: { page } });
    server.push(...res.data.results);
    if (!res.data.next) break;
  }
  const local = (await db.tickets.toArray()).filter((t) => !t._deleted && !t._pendingCreate);
  const serverByKey = new Map(server.map((s) => [s.ticketKey, s]));
  const localByKey = new Map(local.map((l) => [l.ticketKey, l]));
  const diff: SyncDiff = {
    onlyLocal: [...localByKey.keys()].filter((k) => !serverByKey.has(k)),
    onlyServer: [...serverByKey.keys()].filter((k) => !localByKey.has(k)),
    different: [],
  };
  for (const [key, s] of serverByKey) {
    const l = localByKey.get(key);
    if (!l || l._dirty) continue;
    for (const field of ['updatedAt', 'status', 'title'] as const) {
      const lv = field === 'updatedAt' ? Date.parse(l.updatedAt) : l[field];
      const sv = field === 'updatedAt' ? Date.parse(s.updatedAt) : s[field];
      if (lv !== sv) diff.different.push({ key, field, local: l[field], server: s[field] });
    }
  }
  return diff;
}

export function registerDevConsistencyCheck(): void {
  if (!import.meta.env.DEV || typeof window === 'undefined') return;
  (window as unknown as { __sennSyncCheck: () => Promise<SyncDiff> }).__sennSyncCheck = async () => {
    const diff = await compareTicketsWithServer();
    console.info(
      `[local-first] 端末のみ ${diff.onlyLocal.length} 件 / サーバーのみ ${diff.onlyServer.length} 件 / 値の違い ${diff.different.length} 件`,
    );
    if (diff.onlyLocal.length) console.table(diff.onlyLocal);
    if (diff.onlyServer.length) console.table(diff.onlyServer);
    if (diff.different.length) console.table(diff.different);
    return diff;
  };
}
