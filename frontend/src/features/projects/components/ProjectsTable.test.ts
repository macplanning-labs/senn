/**
 * ProjectsTable.test.ts — filterVisibleNodes の折りたたみロジックテスト
 *
 * 階層表示時、展開状態に基づいて表示すべきノードを判定するロジック。
 * react-dom/server では JSX が必要なため、pure function のテストで検証。
 */

import { describe, it, expect } from 'vitest';
import { buildProjectTree, filterVisibleNodes } from '../utils/projectHierarchy';
import type { Project } from '@/shared/hooks/useProject';

const mockProject = (overrides: Partial<Project>): Project => ({
  id: 1,
  name: 'Test',
  prefix: 'TEST',
  description: '',
  isMember: true,
  ...overrides,
});

describe('ProjectsTable folding logic', () => {
  it('全て展開状態で全ノードが表示される', () => {
    const projects: Project[] = [
      mockProject({ id: 1, name: 'Root' }),
      mockProject({ id: 2, name: 'Child', parentProjectId: 1 }),
      mockProject({ id: 3, name: 'Grandchild', parentProjectId: 2 }),
    ];

    const roots = buildProjectTree(projects);
    const expandedNodeIds = new Set([1, 2, 3]);
    const visible = filterVisibleNodes(roots, expandedNodeIds);

    expect(visible).toHaveLength(3);
    expect(visible.map((n) => n.project.id)).toEqual([1, 2, 3]);
  });

  it('親（id=1）が折りたたまれると、子孫は非表示', () => {
    const projects: Project[] = [
      mockProject({ id: 1, name: 'Root' }),
      mockProject({ id: 2, name: 'Child', parentProjectId: 1 }),
      mockProject({ id: 3, name: 'Grandchild', parentProjectId: 2 }),
    ];

    const roots = buildProjectTree(projects);
    // 1 は折りたたまれている
    const expandedNodeIds = new Set([2, 3]);
    const visible = filterVisibleNodes(roots, expandedNodeIds);

    expect(visible).toHaveLength(1); // ルート（1）のみ
    expect(visible[0]?.project.id).toBe(1);
  });

  it('中間ノード（id=2）が折りたたまれると、その子孫のみ非表示', () => {
    const projects: Project[] = [
      mockProject({ id: 1, name: 'Root' }),
      mockProject({ id: 2, name: 'Child', parentProjectId: 1 }),
      mockProject({ id: 3, name: 'Grandchild', parentProjectId: 2 }),
    ];

    const roots = buildProjectTree(projects);
    // 1, 3 は展開、2 は折りたたまれている
    const expandedNodeIds = new Set([1, 3]);
    const visible = filterVisibleNodes(roots, expandedNodeIds);

    expect(visible).toHaveLength(2); // 1, 2
    expect(visible.map((n) => n.project.id)).toEqual([1, 2]);
  });

  it('複数のルートで個別に折りたたみ状態を管理', () => {
    const projects: Project[] = [
      mockProject({ id: 1, name: 'Root1' }),
      mockProject({ id: 2, name: 'Child1', parentProjectId: 1 }),
      mockProject({ id: 3, name: 'Root2' }),
      mockProject({ id: 4, name: 'Child2', parentProjectId: 3 }),
    ];

    const roots = buildProjectTree(projects);
    // Root1 は展開、Root2 は折りたたまれている
    const expandedNodeIds = new Set([1, 2]);
    const visible = filterVisibleNodes(roots, expandedNodeIds);

    expect(visible).toHaveLength(3); // 1, 2, 3
    expect(visible.map((n) => n.project.id)).toEqual([1, 2, 3]);
  });

  it('子を持たないノードの展開状態は影響しない', () => {
    const projects: Project[] = [
      mockProject({ id: 1, name: 'Root' }),
      mockProject({ id: 2, name: 'Leaf', parentProjectId: 1 }),
    ];

    const roots = buildProjectTree(projects);
    // 1 は展開されているので、子の 2 も表示される
    const expandedNodeIds = new Set([1]);
    const visible = filterVisibleNodes(roots, expandedNodeIds);

    expect(visible).toHaveLength(2); // 1, 2 両方表示
    expect(visible.map((n) => n.project.id)).toEqual([1, 2]);
  });
});
