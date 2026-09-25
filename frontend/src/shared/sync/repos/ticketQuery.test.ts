/**
 * ticketQuery.test.ts — 端末側の絞り込み・並び替えがサーバーの一覧 API と一致するか（T6）
 *
 * フィクスチャはサーバー側のテストで生成する（rust/src/infrastructure/repositories/sync_repo.rs
 * generate_ticket_query_parity_fixture）。同じ行データに対するサーバーの結果（キーの並び）と比べる。
 */
import { describe, expect, it } from 'vitest';
import fixture from './__fixtures__/ticketQueryParity.json';
import { toLocalTicket } from '../ticketMapping';
import { compareTickets, queryTickets, searchTickets, toMicros, type TicketListParams } from './ticketQuery';

const rows = (fixture.tickets as Record<string, unknown>[]).map((t) => toLocalTicket(t));

describe('T6: サーバーの一覧 API と同じ結果になる', () => {
  for (const c of fixture.cases) {
    it(c.name, () => {
      const got = queryTickets(rows, c.params as TicketListParams).map((r) => r.ticketKey);
      expect(got).toEqual(c.expected);
    });
  }
});

describe('ticketQuery の細部', () => {
  it('削除待ちの行は出さない', () => {
    const r = [{ ...rows[0], _deleted: true }, rows[1]];
    expect(queryTickets(r, {}).map((x) => x.id)).toEqual([rows[1].id]);
  });

  it('マイクロ秒まで比べる', () => {
    expect(toMicros('2026-09-01T00:00:00.000001Z')).toBeGreaterThan(toMicros('2026-09-01T00:00:00Z'));
    expect(toMicros('2026-09-01T00:00:00.5Z')).toBe(toMicros('2026-09-01T00:00:00.500000Z'));
  });

  it('カンマ区切りの並び順（ガント用）', () => {
    const sorted = [...rows].sort(compareTickets('gantt_order,due_date'));
    for (let i = 1; i < sorted.length; i++) {
      expect(sorted[i - 1].gantt_order).toBeLessThanOrEqual(sorted[i].gantt_order);
    }
  });

  it('Cmd+K: キー前方一致が先、タイトル一致が後', () => {
    const key = rows[0].ticketKey;
    const res = searchTickets(rows, key.slice(0, key.length - 1), 50);
    expect(res[0].ticketKey.startsWith(key.slice(0, key.length - 1))).toBe(true);
    expect(searchTickets(rows, 'login', 50).every((r) => r.title.toLowerCase().includes('login'))).toBe(true);
    expect(searchTickets(rows, '   ')).toEqual([]);
  });
});
