import { describe, it, expect, vi } from 'vitest';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { MemoryRouter } from 'react-router-dom';
import type { Project } from '@/shared/hooks/useProject';

// テスト環境は node のため、サーバーレンダリングした HTML を検証する
vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));
vi.mock('./ProjectsTable.css', () => ({}));

import { ProjectsTable } from './ProjectsTable';
import { buildHierarchyRows } from '../utils/projectHierarchy';

function project(id: number, over: Partial<Project> = {}): Project {
  return {
    id,
    name: `Project ${id}`,
    prefix: `P${id}`,
    description: '',
    isMember: true,
    status: 'in_progress',
    priority: 'medium',
    ...over,
  };
}

const parent = project(1, { childCount: 2 });
const child = project(2, { parentProjectId: 1, status: 'planned' });
const grandchild = project(3, { parentProjectId: 2, status: 'planned' });
const all = [parent, child, grandchild];

function render(props: { projects: Project[]; hierarchyEnabled?: boolean; allProjects?: Project[] }): string {
  return renderToStaticMarkup(createElement(MemoryRouter, null, createElement(ProjectsTable, props)));
}

describe('buildHierarchyRows', () => {
  it('表示対象の祖先が別のグループにあるとき、文脈用の行(isContext)として加える', () => {
    // 「計画中」グループ = child, grandchild。親(進行中)は別グループ
    const rows = buildHierarchyRows(all, [child, grandchild], new Set());
    expect(rows.map((r) => [r.project.id, r.depth, r.isContext])).toEqual([
      [1, 0, true],
      [2, 1, false],
      [3, 2, false],
    ]);
  });

  it('表示対象だけで親子がそろっていれば、文脈の行は出ない', () => {
    const rows = buildHierarchyRows(all, all, new Set());
    expect(rows.every((r) => !r.isContext)).toBe(true);
  });

  it('折りたたまれた親の子孫は行に含めない。親は折りたたみできる(hasChildren)', () => {
    const rows = buildHierarchyRows(all, all, new Set([1]));
    expect(rows.map((r) => r.project.id)).toEqual([1]);
    expect(rows[0]?.hasChildren).toBe(true);
  });

  it('絞り込みで外れたプロジェクトは、祖先でなければ出ない', () => {
    const rows = buildHierarchyRows(all, [parent], new Set());
    expect(rows.map((r) => r.project.id)).toEqual([1]);
  });
});

describe('ProjectsTable の階層表示(描画)', () => {
  it('親が別グループのとき、親は薄い文脈の行で出て、子はインデントされる', () => {
    const html = render({ projects: [child, grandchild], hierarchyEnabled: true, allProjects: all });
    // 文脈の行(親)
    expect(html).toMatch(/projects-table__row projects-table__row--context"[^>]*data-testid="projects-table-row-1"/);
    // 表示対象の行は文脈ではない
    expect(html).toMatch(/projects-table__row "[^>]*data-testid="projects-table-row-2"/);
    // インデント(深さ 1, 2)
    expect(html).toContain('data-depth="1"');
    expect(html).toContain('data-depth="2"');
    // 子を持つ行には折りたたみボタン(展開状態)
    expect(html).toContain('data-testid="projects-table-expand-1"');
    expect(html).toMatch(/aria-expanded="true"[^>]*data-testid="projects-table-expand-1"|data-testid="projects-table-expand-1"[^>]*aria-expanded="true"/);
    // 葉(grandchild)には折りたたみボタンが無い
    expect(html).not.toContain('data-testid="projects-table-expand-3"');
  });

  it('「子 N」は、子を持つ行にだけ出る', () => {
    const html = render({ projects: all, hierarchyEnabled: true, allProjects: all });
    expect(html).toContain('data-testid="projects-table-child-count-1"');
    expect(html).not.toContain('data-testid="projects-table-child-count-2"');
  });

  it('階層表示をオフにすると、従来どおりの平らな表(インデント・折りたたみ・「子 N」なし)', () => {
    const html = render({ projects: all });
    expect(html).not.toContain('data-depth');
    expect(html).not.toContain('projects-table-expand-');
    expect(html).not.toContain('projects-table-child-count-');
    expect(html).not.toContain('projects-table__row--context');
    for (const id of [1, 2, 3]) expect(html).toContain(`data-testid="projects-table-row-${id}"`);
  });

  it('階層表示でも、このグループに無いプロジェクトは行に出ない(全グループに全件が重複しない)', () => {
    const html = render({ projects: [child], hierarchyEnabled: true, allProjects: all });
    expect(html).not.toContain('data-testid="projects-table-row-3"');
  });
});
