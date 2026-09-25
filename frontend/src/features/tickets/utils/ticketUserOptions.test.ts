import { describe, expect, it, vi } from 'vitest';
import {
  fetchTicketUserOptions,
  ticketUserOptionsEnabled,
} from './ticketUserOptions';

function mockApi(handlers: {
  get?: (url: string, config?: { params?: Record<string, unknown> }) => Promise<{ data: unknown }>;
}) {
  return {
    get: vi.fn(handlers.get ?? (async () => ({ data: [] }))),
  } as unknown as import('axios').AxiosInstance;
}

describe('ticketUserOptionsEnabled', () => {
  it('project または team があれば true', () => {
    expect(ticketUserOptionsEnabled({ projectId: 1 })).toBe(true);
    expect(ticketUserOptionsEnabled({ teamId: 7 })).toBe(true);
    expect(ticketUserOptionsEnabled({ projectId: null, teamId: null })).toBe(false);
  });
});

describe('fetchTicketUserOptions', () => {
  it('project 指定時は /users/?project= を使う', async () => {
    const api = mockApi({
      get: async (url, config) => {
        expect(url).toBe('/users/');
        expect(config?.params).toEqual({ project: 42 });
        return {
          data: {
            results: [{ id: 1, username: 'a', displayName: 'A', alias: 'aa' }],
          },
        };
      },
    });
    const users = await fetchTicketUserOptions(api, { projectId: 42, teamId: 7 });
    expect(users).toEqual([{ id: 1, username: 'a', displayName: 'A', alias: 'aa' }]);
    expect(api.get).toHaveBeenCalledTimes(1);
  });

  it('project 無し・team ありは /teams/{id}/members/ を正規化する', async () => {
    const api = mockApi({
      get: async (url) => {
        expect(url).toBe('/teams/7/members/');
        return {
          data: [
            {
              id: 99,
              team: 7,
              role: 'member',
              joinedAt: '2026-01-01',
              user: { id: 2, username: 'bob', displayName: 'Bob' },
            },
          ],
        };
      },
    });
    const users = await fetchTicketUserOptions(api, { projectId: null, teamId: 7 });
    expect(users).toEqual([
      { id: 2, username: 'bob', displayName: 'Bob', alias: null },
    ]);
  });

  it('どちらも無ければ空配列（API を呼ばない）', async () => {
    const api = mockApi({});
    const users = await fetchTicketUserOptions(api, {});
    expect(users).toEqual([]);
    expect(api.get).not.toHaveBeenCalled();
  });

  it('複数の team members が返された場合、すべて TicketUserOption[] に変換される', async () => {
    const api = mockApi({
      get: async (url) => {
        expect(url).toBe('/teams/7/members/');
        return {
          data: [
            {
              id: 1,
              team: 7,
              role: 'admin',
              joinedAt: '2026-01-01',
              user: { id: 10, username: 'alice', displayName: 'Alice Aoki', alias: 'aa' },
            },
            {
              id: 2,
              team: 7,
              role: 'member',
              joinedAt: '2026-01-02',
              user: { id: 20, username: 'bob', displayName: 'Bob Brown', alias: 'bb' },
            },
            {
              id: 3,
              team: 7,
              role: 'member',
              joinedAt: '2026-01-03',
              user: { id: 30, username: 'charlie', displayName: 'Charlie Chen', alias: null },
            },
          ],
        };
      },
    });
    const users = await fetchTicketUserOptions(api, { projectId: null, teamId: 7 });
    expect(users).toHaveLength(3);
    expect(users).toEqual([
      { id: 10, username: 'alice', displayName: 'Alice Aoki', alias: 'aa' },
      { id: 20, username: 'bob', displayName: 'Bob Brown', alias: 'bb' },
      { id: 30, username: 'charlie', displayName: 'Charlie Chen', alias: null },
    ]);
  });

});
