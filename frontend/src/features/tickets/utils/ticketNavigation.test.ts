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
    expect(buildTicketDetailPath('DEMO', 'DEMO-000048')).toBe(
      '/p/DEMO/tickets/DEMO-000048',
    );
  });

  it('cycleId 指定時は /cycles/:cycleId/:ticketKey を返す', () => {
    expect(buildTicketDetailPath('DEMO', 'DEMO-000048', 11)).toBe(
      '/p/DEMO/cycles/11/DEMO-000048',
    );
  });

  it('projectKey に小文字 prefix をそのまま使う', () => {
    expect(buildTicketDetailPath('acme', 'ACME-000001', 3)).toBe(
      '/p/acme/cycles/3/ACME-000001',
    );
  });

  it('cycleId が 0 のときも cycle パスを使う', () => {
    expect(buildTicketDetailPath('DEMO', 'DEMO-000001', 0)).toBe(
      '/p/DEMO/cycles/0/DEMO-000001',
    );
  });
});

describe('buildTicketListPath', () => {
  it('cycleId 未指定時は /tickets を返す', () => {
    expect(buildTicketListPath('DEMO')).toBe('/p/DEMO/tickets');
  });

  it('cycleId 指定時は /cycles/:cycleId を返す', () => {
    expect(buildTicketListPath('DEMO', 11)).toBe('/p/DEMO/cycles/11');
  });

  it('cycleId が 0 のときも cycle パスを使う', () => {
    expect(buildTicketListPath('DEMO', 0)).toBe('/p/DEMO/cycles/0');
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
});
