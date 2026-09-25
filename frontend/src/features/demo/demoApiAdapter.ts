/**
 * demoApiAdapter.ts — デモモード API アダプター
 *
 * URL + method に基づいて fixture を返すか、拒否する。
 * isDemoMode() 有効時の GET は常に 200 系レスポンスを返す（null にしない）。
 */

import { isDemoMode } from './demoMode';
import {
  demoUser,
  demoProject,
  demoTeam,
  demoMyTickets,
  getDemoTicketDetail,
} from './demoFixtures';

export interface AdapterRequest {
  method: 'get' | 'post' | 'patch' | 'put' | 'delete';
  url: string;
  data?: unknown;
  headers?: Record<string, string>;
}

export interface AdapterResponse {
  status: number;
  data: unknown;
}

const EMPTY_RESULTS = { results: [] as unknown[] };

function normalizePath(url: string | undefined): string {
  if (!url) {
    return '/';
  }
  const path = url.split('?')[0] ?? '';
  return path.endsWith('/') ? path : `${path}/`;
}

/**
 * デモ時の GET リクエストを fixture で返す。
 * それ以外（POST/PATCH/PUT/DELETE）は 403 で拒否する。
 */
export function handleDemoRequest(req: AdapterRequest): AdapterResponse | null {
  if (!isDemoMode()) {
    return null;
  }

  const { method, url } = req;
  const path = normalizePath(url);

  if (method !== 'get') {
    return {
      status: 403,
      data: { detail: 'Demo mode: write operations are not allowed' },
    };
  }

  if (path.includes('/auth/me/')) {
    return { status: 200, data: demoUser };
  }

  if (path.includes('/dashboard/my-tickets/')) {
    return { status: 200, data: demoMyTickets };
  }

  // 差分同期 API（Local-first。詳細設計 §3.9）— デモのチケット・プロジェクトを端末内 DB へ入れる
  const demoSyncPage = (changes: unknown[]) => ({
    status: 200,
    data: {
      changes,
      deleted: [],
      access: { all: true, teamIds: [], scopedProjects: [] },
      cursor: 'demo',
      hasMore: false,
      serverTime: new Date().toISOString(),
    },
  });
  if (path.includes('/sync/tickets/')) {
    return demoSyncPage(
      demoMyTickets
        .map((t) => getDemoTicketDetail(t.ticket_key))
        .filter((d): d is NonNullable<typeof d> => d !== null),
    );
  }
  if (path.includes('/sync/projects/')) {
    return demoSyncPage([
      {
        ...demoProject,
        status: 'in_progress',
        priority: 'medium',
        isMember: true,
        teams: [{ id: demoTeam.id, name: demoTeam.name, slug: demoTeam.slug, icon: demoTeam.icon, color: demoTeam.color }],
        createdAt: '2026-09-10T00:00:00Z',
        updatedAt: '2026-09-10T00:00:00Z',
      },
    ]);
  }

  // 付随データ API — チケット詳細の extras
  const extrasMatch = path.match(/\/tickets\/([^/?]+)\/extras\//);
  if (extrasMatch?.[1]) {
    const detail = getDemoTicketDetail(extrasMatch[1]);
    return {
      status: 200,
      data: {
        comments: detail?.comments ?? [],
        attachments: detail?.attachments ?? [],
        links: detail?.links ?? [],
        linkedRules: [],
        linkedWikiPages: (detail as any)?.linkedWikiPages ?? [],
        isWatching: false,
      },
    };
  }

  if (/\/tickets\/[^/]+\/(change-logs|git-events)\//.test(path)) {
    return { status: 200, data: [] };
  }

  const ticketMatch = path.match(/\/tickets\/([^/?]+)\//);
  if (ticketMatch?.[1] && ticketMatch[1] !== 'export') {
    const detail = getDemoTicketDetail(ticketMatch[1]);
    if (detail) {
      return { status: 200, data: detail };
    }
    return { status: 404, data: { detail: 'Not found' } };
  }

  if (path.includes('/projects/')) {
    return { status: 200, data: { results: [demoProject] } };
  }

  if (path.includes('/teams/')) {
    return { status: 200, data: { results: [demoTeam] } };
  }

  if (path.includes('/notifications/unread_count/')) {
    return { status: 200, data: { count: 0 } };
  }

  if (path.includes('/notifications/')) {
    return { status: 200, data: EMPTY_RESULTS };
  }

  if (path.includes('/search/')) {
    return { status: 200, data: EMPTY_RESULTS };
  }

  const emptyListPaths = [
    '/labels/',
    '/users/',
    '/milestones/',
    '/categories/',
    '/cycles/',
    '/settings/ai/',
    '/time-entries/',
  ];

  if (emptyListPaths.some((segment) => path.includes(segment))) {
    return { status: 200, data: EMPTY_RESULTS };
  }

  if (path.endsWith('/tickets/') || path === '/tickets/') {
    return { status: 200, data: EMPTY_RESULTS };
  }

  return { status: 200, data: EMPTY_RESULTS };
}
