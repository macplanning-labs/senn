import { describe, it, expect, vi } from 'vitest';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { MemoryRouter, Routes, Route } from 'react-router-dom';

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

const mockUseTeam = vi.fn(() => ({
  currentTeam: { id: 1, name: 'Sample Team', slug: 'foo', archivedAt: null as string | null },
  isLoading: false,
}));

vi.mock('@/shared/hooks/useTeam', () => ({
  useTeam: () => mockUseTeam(),
}));

vi.mock('./TeamLayout.css', () => ({}));

import { TeamLayout } from './TeamLayout';

function render(path: string): string {
  return renderToStaticMarkup(
    createElement(
      MemoryRouter,
      { initialEntries: [path] },
      createElement(
        Routes,
        null,
        createElement(
          Route,
          { path: '/team/:teamSlug', element: createElement(TeamLayout) },
          createElement(Route, { path: '*', element: createElement('div', null, 'child') }),
        ),
      ),
    ),
  );
}

function anchor(html: string, testId: string): string {
  const m = html.match(new RegExp(`<a [^>]*data-testid="${testId}"[^>]*>`));
  if (!m) throw new Error(`${testId} が見つかりません`);
  return m[0];
}

const TABS = [
  'tickets',
  'cycles',
  'board',
  'gantt',
  'dependencies',
  'projects',
  'triage',
  'wiki',
  'settings',
] as const;

describe('TeamLayout', () => {
  it('9つのタブが、正しいリンク先で表示される', () => {
    const html = render('/team/foo/tickets');
    const expected: Record<(typeof TABS)[number], string> = {
      tickets: '/team/foo/tickets',
      cycles: '/team/foo/cycles',
      board: '/team/foo/board',
      gantt: '/team/foo/gantt',
      dependencies: '/team/foo/dependencies',
      projects: '/team/foo/projects',
      triage: '/team/foo/triage',
      wiki: '/team/foo/wiki',
      settings: '/team/foo/settings',
    };
    for (const tab of TABS) {
      expect(anchor(html, `team-tab-${tab}`)).toContain(`href="${expected[tab]}"`);
    }
  });

  it('チーム一覧へ戻るリンクは置かない（入口はサイドバーのチーム見出しの「…」）', () => {
    const html = render('/team/foo/tickets');
    expect(html).not.toContain('team-back-to-list');
    expect(html).not.toContain('href="/teams"');
  });

  it('チケット詳細（tickets/:id）でもチケットタブがアクティブ', () => {
    const html = render('/team/foo/tickets/1');
    expect(anchor(html, 'team-tab-tickets')).toContain('team-layout__tab--active');
  });

  it('ボードではボードタブがアクティブでチケットタブは非アクティブ', () => {
    const html = render('/team/foo/board');
    expect(anchor(html, 'team-tab-board')).toContain('team-layout__tab--active');
    expect(anchor(html, 'team-tab-tickets')).not.toContain('team-layout__tab--active');
  });

  it('アーカイブ済みチームではバッジが表示される', () => {
    mockUseTeam.mockReturnValueOnce({
      currentTeam: {
        id: 1,
        name: 'Archived Team',
        slug: 'foo',
        archivedAt: '2025-01-01T00:00:00Z',
      },
      isLoading: false,
    });
    const html = render('/team/foo/tickets');
    expect(html).toContain('teamArchive.badge');
    expect(html).toContain('team-layout__status-badge');
  });
});
