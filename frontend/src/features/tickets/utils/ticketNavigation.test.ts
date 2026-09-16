/**
 * ticketNavigation.test.ts — チケット詳細パス生成の単体テスト
 *
 * 実行: cd frontend && npm test
 */
import { describe, it, expect } from 'vitest';
import {
  buildTicketDetailPath,
  buildTicketListPath,
} from './ticketNavigation';

describe('buildTicketDetailPath', () => {
  it('cycleId 未指定時は /tickets/:ticketKey を返す', () => {
    expect(buildTicketDetailPath('SOPHIA', 'DEMO-000048')).toBe(
      '/project/SOPHIA/tickets/DEMO-000048',
    );
  });

  it('cycleId 指定時は /cycles/:cycleId/:ticketKey を返す', () => {
    expect(buildTicketDetailPath('SOPHIA', 'DEMO-000048', 11)).toBe(
      '/project/SOPHIA/cycles/11/DEMO-000048',
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
    expect(buildTicketDetailPath('SOPHIA', 'DEMO-000001', 0)).toBe(
      '/project/SOPHIA/cycles/0/DEMO-000001',
    );
  });
});

describe('buildTicketListPath', () => {
  it('cycleId 未指定時は /tickets を返す', () => {
    expect(buildTicketListPath('SOPHIA')).toBe('/project/SOPHIA/tickets');
  });

  it('cycleId 指定時は /cycles/:cycleId を返す', () => {
    expect(buildTicketListPath('SOPHIA', 11)).toBe('/project/SOPHIA/cycles/11');
  });

  it('cycleId が 0 のときも cycle パスを使う', () => {
    expect(buildTicketListPath('SOPHIA', 0)).toBe('/project/SOPHIA/cycles/0');
  });
});

describe('ナビゲーション一貫性', () => {
  it('詳細パスから一覧パスへ戻れる（cycle コンテキスト）', () => {
    const projectKey = 'SOPHIA';
    const cycleId = 11;
    const ticketKey = 'DEMO-000048';

    const detail = buildTicketDetailPath(projectKey, ticketKey, cycleId);
    const list = buildTicketListPath(projectKey, cycleId);

    expect(detail.startsWith(list + '/')).toBe(true);
    expect(detail).toBe(`${list}/${ticketKey}`);
  });

  it('詳細パスから一覧パスへ戻れる（tickets コンテキスト）', () => {
    const projectKey = 'SOPHIA';
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
