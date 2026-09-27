/**
 * ticketNavigation.test.ts — チケット詳細パス生成の単体テスト
 *
 * 実行: cd frontend && npm test
 */
import { describe, it, expect } from 'vitest';
import {
  buildTicketDetailPath,
  buildTicketListPath,
  buildTicketListPathForContext,
  buildTicketPathContextFromPathname,
  buildTicketShareUrl,
  buildTicketEditPath,
  detectTicketDetailOrigin,
  resolveTicketScopeFromPathname,
} from './ticketNavigation';

describe('buildTicketDetailPath', () => {
  it('cycleId 未指定時は /tickets/:ticketKey を返す', () => {
    expect(buildTicketDetailPath('DEMO', 'DEMO-000048')).toBe(
      '/project/DEMO/tickets/DEMO-000048',
    );
  });

  it('cycleId 指定時は /cycles/:cycleId/:ticketKey を返す', () => {
    expect(buildTicketDetailPath('DEMO', 'DEMO-000048', 11)).toBe(
      '/project/DEMO/cycles/11/DEMO-000048',
    );
  });

  it('projectKey に小文字 prefix をそのまま使う', () => {
    expect(buildTicketDetailPath('demo', 'DEMO-000001', 3)).toBe(
      '/project/demo/cycles/3/DEMO-000001',
    );
  });

  it('project 無し + teamSlug は /team/:teamSlug/tickets/:ticketKey', () => {
    expect(buildTicketDetailPath(null, 'DEMO-000001', undefined, 'demo-app-dev')).toBe(
      '/team/demo-app-dev/tickets/DEMO-000001',
    );
  });

  it('project ありのときは teamSlug より /project/:projectKey を使う', () => {
    expect(buildTicketDetailPath('DEMO', 'DEMO-000001', undefined, 'demo-app-dev')).toBe(
      '/project/DEMO/tickets/DEMO-000001',
    );
  });

  it('cycleId が 0 のときも cycle パスを使う', () => {
    expect(buildTicketDetailPath('DEMO', 'DEMO-000001', 0)).toBe(
      '/project/DEMO/cycles/0/DEMO-000001',
    );
  });
});

describe('buildTicketListPath', () => {
  it('cycleId 未指定時は /tickets を返す', () => {
    expect(buildTicketListPath('DEMO')).toBe('/project/DEMO/tickets');
  });

  it('cycleId 指定時は /cycles/:cycleId を返す', () => {
    expect(buildTicketListPath('DEMO', 11)).toBe('/project/DEMO/cycles/11');
  });

  it('cycleId が 0 のときも cycle パスを使う', () => {
    expect(buildTicketListPath('DEMO', 0)).toBe('/project/DEMO/cycles/0');
  });
});

describe('ナビゲーション一貫性', () => {
  it('詳細パスから一覧パスへ戻れる（cycle コンテキスト）', () => {
    const projectKey = 'DEMO';
    const cycleId = 11;
    const ticketKey = 'DEMO-000048';

    const detail = buildTicketDetailPath(projectKey, ticketKey, cycleId);
    const list = buildTicketListPath(projectKey, cycleId);

    expect(detail.startsWith(list + '/')).toBe(true);
    expect(detail).toBe(`${list}/${ticketKey}`);
  });

  it('詳細パスから一覧パスへ戻れる（tickets コンテキスト）', () => {
    const projectKey = 'DEMO';
    const ticketKey = 'DEMO-000048';

    const detail = buildTicketDetailPath(projectKey, ticketKey);
    const list = buildTicketListPath(projectKey);

    expect(detail.startsWith(list + '/')).toBe(true);
    expect(detail).toBe(`${list}/${ticketKey}`);
  });

  it('Team だけの詳細から一覧へ戻れる', () => {
    const detail = buildTicketDetailPath(null, 'DEMO-000001', undefined, 'demo-app-dev');
    const list = buildTicketListPath(null, undefined, 'demo-app-dev');
    expect(detail.startsWith(list + '/')).toBe(true);
    expect(detail).toBe(`${list}/DEMO-000001`);
  });
});

describe('buildTicketShareUrl', () => {
  const origin = 'https://senn.example.com';

  it('プロジェクト画面では /project/:key のURLになる', () => {
    expect(buildTicketShareUrl(origin, 'DEMO-000001', { projectKey: 'DEMO' })).toBe(
      'https://senn.example.com/project/DEMO/tickets/DEMO-000001',
    );
  });

  it('チーム画面(projectKey が空)では null を含まず /team/:slug のURLになる', () => {
    const url = buildTicketShareUrl(origin, 'DEMO-000001', {
      projectKey: null,
      teamSlug: 'demo-app-dev',
      ticketProjectPrefix: 'OTHER',
    });
    expect(url).toBe('https://senn.example.com/team/demo-app-dev/tickets/DEMO-000001');
    expect(url).not.toContain('null');
  });

  it('画面の文脈が無いときはチケット自身の所属を使う', () => {
    expect(
      buildTicketShareUrl(origin, 'DEMO-000001', { ticketProjectPrefix: 'OTHER', ticketTeamSlug: 'demo-app-dev' }),
    ).toBe('https://senn.example.com/project/OTHER/tickets/DEMO-000001');
    expect(buildTicketShareUrl(origin, 'DEMO-000001', { ticketTeamSlug: 'demo-app-dev' })).toBe(
      'https://senn.example.com/team/demo-app-dev/tickets/DEMO-000001',
    );
  });

  it('何も分からないときも undefined/null を含まない', () => {
    expect(buildTicketShareUrl(origin, 'DEMO-000001', {})).toBe(
      'https://senn.example.com/tickets/DEMO-000001',
    );
  });

  it('hash を付けられる(コメントリンク)', () => {
    expect(buildTicketShareUrl(origin, 'DEMO-000001', { projectKey: 'DEMO' }, 'comment-5')).toBe(
      'https://senn.example.com/project/DEMO/tickets/DEMO-000001#comment-5',
    );
  });
});

describe('buildTicketEditPath', () => {
  it('プロジェクト画面ではプロジェクトの編集画面へ', () => {
    expect(buildTicketEditPath('DEMO-000001', { projectKey: 'DEMO', teamSlug: 'demo-app-dev' })).toBe(
      '/project/DEMO/tickets/DEMO-000001/edit',
    );
  });

  it('チーム画面では、チケットがプロジェクトに属していてもチームの編集画面に留まる', () => {
    expect(
      buildTicketEditPath('DEMO-000001', {
        projectKey: null,
        teamSlug: 'demo-app-dev',
        ticketProjectPrefix: 'OTHER',
      }),
    ).toBe('/team/demo-app-dev/tickets/DEMO-000001/edit');
  });

  it('画面の文脈が無いときはチケット自身の所属(プロジェクト優先)', () => {
    expect(
      buildTicketEditPath('DEMO-000001', { ticketProjectPrefix: 'OTHER', ticketTeamSlug: 'demo-app-dev' }),
    ).toBe('/project/OTHER/tickets/DEMO-000001/edit');
  });

  it('どこにも属さないときは null', () => {
    expect(buildTicketEditPath('DEMO-000001', {})).toBeNull();
  });
});

describe('buildTicketListPathForContext', () => {
  it('チーム URL から開いた詳細は project 付きチケットでもチーム一覧へ戻る', () => {
    expect(
      buildTicketListPathForContext('/team/wip-app-dev/tickets/WIPAPPDEV-000131', {
        projectKey: 'WAPP',
        teamSlug: 'wip-app-dev',
      }),
    ).toBe('/team/wip-app-dev/tickets');
  });

  it('プロジェクト URL から開いた詳細はプロジェクト一覧へ戻る', () => {
    expect(
      buildTicketListPathForContext('/project/WAPP/tickets/WIPAPPDEV-000131', {
        projectKey: 'WAPP',
        teamSlug: 'wip-app-dev',
      }),
    ).toBe('/project/WAPP/tickets');
  });
});

describe('buildTicketPathContextFromPathname', () => {
  const ticket = { projectPrefix: 'WAPP', team: { slug: 'wip-app-dev' } };

  it('チーム URL では projectKey を渡さず teamSlug のみ', () => {
    expect(
      buildTicketPathContextFromPathname('/team/wip-app-dev/tickets/WIP-1', ticket),
    ).toEqual({
      projectKey: null,
      teamSlug: 'wip-app-dev',
      ticketProjectPrefix: 'WAPP',
      ticketTeamSlug: 'wip-app-dev',
    });
  });

  it('プロジェクト URL では teamSlug を渡さず projectKey のみ', () => {
    expect(
      buildTicketPathContextFromPathname('/project/WAPP/tickets/WIP-1', ticket),
    ).toEqual({
      projectKey: 'WAPP',
      teamSlug: null,
      ticketProjectPrefix: 'WAPP',
      ticketTeamSlug: 'wip-app-dev',
    });
  });

  it('チーム URL 経由の共有リンクは team パスになる', () => {
    const ctx = buildTicketPathContextFromPathname('/team/wip-app-dev/tickets/WIP-1', ticket);
    expect(buildTicketShareUrl('https://senn.example.com', 'WIP-1', ctx)).toBe(
      'https://senn.example.com/team/wip-app-dev/tickets/WIP-1',
    );
  });
});

describe('resolveTicketScopeFromPathname', () => {
  it('チーム URL を team と判定する', () => {
    expect(resolveTicketScopeFromPathname('/team/wip-app-dev/tickets/DEMO-1')).toEqual({
      kind: 'team',
      teamSlug: 'wip-app-dev',
      projectKey: null,
    });
  });

  it('プロジェクト URL を project と判定する', () => {
    expect(resolveTicketScopeFromPathname('/project/WAPP/tickets/DEMO-1')).toEqual({
      kind: 'project',
      teamSlug: null,
      projectKey: 'WAPP',
    });
  });
});

describe('detectTicketDetailOrigin（詳細を開いた入口）', () => {
  it('自分のチケットから開いた詳細', () => {
    expect(detectTicketDetailOrigin('/my-issues/DEMO-000001')).toEqual({ kind: 'myIssues' });
  });

  it('チームのボードから開いた詳細', () => {
    expect(detectTicketDetailOrigin('/team/demo-app-dev/board/DEMO-000001')).toEqual({
      kind: 'board',
      base: '/team/demo-app-dev',
    });
  });

  it('プロジェクトのサイクル詳細から開いた詳細', () => {
    expect(detectTicketDetailOrigin('/project/DEMO/cycles/12/DEMO-000001')).toEqual({
      kind: 'cycle',
      base: '/project/DEMO',
      cycleId: '12',
    });
  });

  it('チケット一覧から開いた詳細・編集は一覧扱い', () => {
    expect(detectTicketDetailOrigin('/team/demo-app-dev/tickets/DEMO-000001')).toEqual({ kind: 'list' });
    expect(detectTicketDetailOrigin('/project/DEMO/tickets/DEMO-000001/edit')).toEqual({ kind: 'list' });
  });

  it('一覧そのもの（詳細でない）は一覧扱い', () => {
    expect(detectTicketDetailOrigin('/my-issues')).toEqual({ kind: 'list' });
    expect(detectTicketDetailOrigin('/team/demo-app-dev/board')).toEqual({ kind: 'list' });
  });
});
