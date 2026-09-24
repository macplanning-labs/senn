import { describe, it, expect, vi } from 'vitest';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { MemoryRouter, Routes, Route } from 'react-router-dom';

// テスト環境は node のため、DOM ではなくサーバーレンダリングした HTML を検証する
vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

vi.mock('@/shared/hooks/useProject', () => ({
  useProject: () => ({
    currentProject: { id: 1, name: 'Sample', prefix: 'ABC', status: 'active' },
    isLoading: false,
  }),
}));

// CSS は node 環境では不要
vi.mock('./ProjectLayout.css', () => ({}));

import { ProjectLayout } from './ProjectLayout';

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
          { path: '/project/:projectKey', element: createElement(ProjectLayout) },
          createElement(Route, { index: true, element: createElement('div', null, 'index') }),
          createElement(Route, { path: '*', element: createElement('div', null, 'child') }),
        ),
      ),
    ),
  );
}

/** data-testid を持つ <a> タグ全体を取り出す */
function anchor(html: string, testId: string): string {
  const m = html.match(new RegExp(`<a [^>]*data-testid="${testId}"[^>]*>`));
  if (!m) throw new Error(`${testId} が見つかりません`);
  return m[0];
}

const TABS = ['overview', 'tickets', 'dependencies', 'gantt', 'activity', 'projects'];

describe('ProjectLayout', () => {
  it('6つのタブが、正しいリンク先で表示される', () => {
    const html = render('/project/ABC');
    const expected: Record<string, string> = {
      overview: '/project/ABC',
      tickets: '/project/ABC/tickets',
      dependencies: '/project/ABC/dependencies',
      gantt: '/project/ABC/gantt',
      activity: '/project/ABC/activity',
      projects: '/project/ABC/projects',
    };
    for (const tab of TABS) {
      expect(anchor(html, `project-tab-${tab}`)).toContain(`href="${expected[tab]}"`);
    }
  });

  it('補助リンク（サイクル/Wiki/設定）が表示される', () => {
    const html = render('/project/ABC');
    expect(anchor(html, 'project-aux-cycles')).toContain('href="/project/ABC/cycles"');
    expect(anchor(html, 'project-aux-wiki')).toContain('href="/project/ABC/wiki"');
    expect(anchor(html, 'project-aux-settings')).toContain('href="/project/ABC/settings"');
  });

  it('Overview（index）では Overview タブだけがアクティブ', () => {
    const html = render('/project/ABC');
    expect(anchor(html, 'project-tab-overview')).toContain('project-layout__tab--active');
    for (const tab of TABS.filter((t) => t !== 'overview')) {
      expect(anchor(html, `project-tab-${tab}`)).not.toContain('project-layout__tab--active');
    }
  });

  it('ガントを開いているとき、ガントタブだけがアクティブでサブ切替は出ない', () => {
    const html = render('/project/ABC/gantt');
    expect(anchor(html, 'project-tab-gantt')).toContain('project-layout__tab--active');
    expect(anchor(html, 'project-tab-overview')).not.toContain('project-layout__tab--active');
    expect(anchor(html, 'project-tab-tickets')).not.toContain('project-layout__tab--active');
    expect(html).not.toContain('project-view-list');
    expect(html).not.toContain('project-view-board');
  });

  it('チケット一覧では、チケットタブがアクティブでリスト/ボード切替が出る', () => {
    const html = render('/project/ABC/tickets');
    expect(anchor(html, 'project-tab-tickets')).toContain('project-layout__tab--active');
    expect(anchor(html, 'project-view-list')).toContain('href="/project/ABC/tickets"');
    expect(anchor(html, 'project-view-board')).toContain('href="/project/ABC/board"');
    expect(anchor(html, 'project-view-list')).toContain('project-layout__view-option--active');
  });

  it('ボードでも、チケットタブがアクティブでボード側が選択状態になる', () => {
    const html = render('/project/ABC/board');
    expect(anchor(html, 'project-tab-tickets')).toContain('project-layout__tab--active');
    expect(anchor(html, 'project-view-board')).toContain('project-layout__view-option--active');
    expect(anchor(html, 'project-view-list')).not.toContain('project-layout__view-option--active');
  });

  it('チケット詳細（tickets/:id）でもチケットタブがアクティブ', () => {
    const html = render('/project/ABC/tickets/ABC-12');
    expect(anchor(html, 'project-tab-tickets')).toContain('project-layout__tab--active');
  });

  it('プロジェクトキーに "tickets" を含んでも、他タブでチケットタブが誤ってアクティブにならない', () => {
    const html = render('/project/tickets-app/gantt');
    expect(anchor(html, 'project-tab-tickets')).not.toContain('project-layout__tab--active');
    expect(html).not.toContain('project-view-list');
  });
});

describe('ProjectLayout: プロジェクト一覧へ戻る', () => {
  it('ヘッダーに、プロジェクト一覧(/projects)へ戻るリンクがある(どのタブでも)', () => {
    for (const path of ['/project/ABC', '/project/ABC/projects', '/project/ABC/activity']) {
      const html = render(path);
      const a = anchor(html, 'project-back-to-list');
      expect(a, path).toContain('href="/projects"');
      // プロジェクト名より前(見出しの上)に出る
      expect(html.indexOf('project-back-to-list'), path).toBeLessThan(html.indexOf('Sample'));
    }
  });
});
