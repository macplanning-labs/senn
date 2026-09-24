/**
 * projectHierarchy.test.ts — テスト
 */

import { describe, it, expect } from 'vitest';
import {
  buildProjectTree,
  filterProjects,
  withAncestors,
  rollupDescendantCount,
  groupProjects,
  filterVisibleNodes,
} from './projectHierarchy';
import type { Project } from '@/shared/hooks/useProject';

const mockProject = (overrides: Partial<Project>): Project => ({
  id: 1,
  name: 'Test',
  prefix: 'TEST',
  description: '',
  isMember: true,
  ...overrides,
});

describe('projectHierarchy', () => {
  describe('buildProjectTree', () => {
    it('ツリーの深さを正しく計算する（3段まで）', () => {
      const projects: Project[] = [
        mockProject({ id: 1, name: 'Root' }),
        mockProject({ id: 2, name: 'Parent', parentProjectId: 1 }),
        mockProject({ id: 3, name: 'Child', parentProjectId: 2 }),
      ];

      const roots = buildProjectTree(projects);

      expect(roots).toHaveLength(1);
      const root = roots[0];
      expect(root).toBeDefined();
      expect(root?.project.id).toBe(1);
      expect(root?.depth).toBe(0);
      expect(root?.children).toHaveLength(1);

      const child = root?.children[0];
      expect(child).toBeDefined();
      expect(child?.project.id).toBe(2);
      expect(child?.depth).toBe(1);
      expect(child?.children).toHaveLength(1);

      const grandchild = child?.children[0];
      expect(grandchild).toBeDefined();
      expect(grandchild?.project.id).toBe(3);
      expect(grandchild?.depth).toBe(2);
    });

    it('複数のルートを扱う', () => {
      const projects: Project[] = [
        mockProject({ id: 1, name: 'Root1' }),
        mockProject({ id: 2, name: 'Root2' }),
        mockProject({ id: 3, name: 'Child of Root1', parentProjectId: 1 }),
      ];

      const roots = buildProjectTree(projects);

      expect(roots).toHaveLength(2);
      expect(roots[0]?.children).toHaveLength(1);
      expect(roots[1]?.children).toHaveLength(0);
    });

    it('親が存在しない場合、そのプロジェクトはルート扱い', () => {
      const projects: Project[] = [
        mockProject({ id: 1, name: 'Root' }),
        mockProject({ id: 2, name: 'Orphan', parentProjectId: 999 }), // 存在しない親
      ];

      const roots = buildProjectTree(projects);

      expect(roots).toHaveLength(2);
      expect(roots.some((r) => r.project.id === 2)).toBe(true);
    });

    it('子を名前順でソート', () => {
      const projects: Project[] = [
        mockProject({ id: 1, name: 'A' }),
        mockProject({ id: 2, name: 'Z', parentProjectId: 1 }),
        mockProject({ id: 3, name: 'B', parentProjectId: 1 }),
      ];

      const roots = buildProjectTree(projects);

      const root = roots[0];
      expect(root).toBeDefined();
      expect(root?.children[0]?.project.name).toBe('B');
      expect(root?.children[1]?.project.name).toBe('Z');
    });
  });

  describe('filterProjects', () => {
    it('mine フィルタ', () => {
      const projects: Project[] = [
        mockProject({ id: 1, isMember: true }),
        mockProject({ id: 2, isMember: false }),
      ];

      const result = filterProjects(projects, { mine: true });

      expect(result).toHaveLength(1);
      expect(result[0]?.id).toBe(1);
    });

    it('priority フィルタ', () => {
      const projects: Project[] = [
        mockProject({ id: 1, priority: 'high' }),
        mockProject({ id: 2, priority: 'low' }),
      ];

      const result = filterProjects(projects, { priority: 'high' });

      expect(result).toHaveLength(1);
      expect(result[0]?.id).toBe(1);
    });

    it('teamId フィルタ', () => {
      const projects: Project[] = [
        mockProject({
          id: 1,
          teams: [{ id: 10, name: 'TeamA', slug: 'team-a', icon: '', color: '', archived: false }],
        }),
        mockProject({
          id: 2,
          teams: [{ id: 20, name: 'TeamB', slug: 'team-b', icon: '', color: '', archived: false }],
        }),
      ];

      const result = filterProjects(projects, { teamId: '10' });

      expect(result).toHaveLength(1);
      expect(result[0]?.id).toBe(1);
    });

    it('parentProjectId フィルタ: "none" でルートのみ', () => {
      const projects: Project[] = [
        mockProject({ id: 1, parentProjectId: undefined }),
        mockProject({ id: 2, parentProjectId: null }),
        mockProject({ id: 3, parentProjectId: 1 }),
      ];

      const result = filterProjects(projects, { parentProjectId: 'none' });

      expect(result).toHaveLength(2);
      expect(result.map((p) => p.id)).toEqual([1, 2]);
    });

    it('parentProjectId フィルタ: 指定 id のみ', () => {
      const projects: Project[] = [
        mockProject({ id: 1 }),
        mockProject({ id: 2, parentProjectId: 1 }),
        mockProject({ id: 3, parentProjectId: 1 }),
        mockProject({ id: 4, parentProjectId: 2 }),
      ];

      const result = filterProjects(projects, { parentProjectId: '1' });

      expect(result).toHaveLength(2);
      expect(result.map((p) => p.id).sort()).toEqual([2, 3]);
    });

    it('roadmapId フィルタ', () => {
      const projects: Project[] = [
        mockProject({ id: 1, roadmapIds: [10, 20] }),
        mockProject({ id: 2, roadmapIds: [20] }),
        mockProject({ id: 3, roadmapIds: [30] }),
      ];

      const result = filterProjects(projects, { roadmapId: '20' });

      expect(result).toHaveLength(2);
      expect(result.map((p) => p.id).sort()).toEqual([1, 2]);
    });

    it('relatedIds フィルタ', () => {
      const projects: Project[] = [
        mockProject({ id: 1 }),
        mockProject({ id: 2 }),
        mockProject({ id: 3 }),
      ];

      const relatedIds = new Set([1, 3]);
      const result = filterProjects(projects, { relatedIds });

      expect(result).toHaveLength(2);
      expect(result.map((p) => p.id)).toEqual([1, 3]);
    });

    it('複数フィルタの AND', () => {
      const projects: Project[] = [
        mockProject({
          id: 1,
          isMember: true,
          priority: 'high',
          teams: [{ id: 10, name: 'A', slug: 'a', icon: '', color: '', archived: false }],
        }),
        mockProject({
          id: 2,
          isMember: true,
          priority: 'high',
          teams: [{ id: 20, name: 'B', slug: 'b', icon: '', color: '', archived: false }],
        }),
      ];

      const result = filterProjects(projects, { mine: true, priority: 'high', teamId: '10' });

      expect(result).toHaveLength(1);
      expect(result[0]?.id).toBe(1);
    });

    it('フィルタなしで全件返す', () => {
      const projects: Project[] = [
        mockProject({ id: 1 }),
        mockProject({ id: 2 }),
      ];

      const result = filterProjects(projects, {});

      expect(result).toHaveLength(2);
    });
  });

  describe('withAncestors', () => {
    it('祖先を加える（isContext フラグ付き）', () => {
      const all: Project[] = [
        mockProject({ id: 1, name: 'Root' }),
        mockProject({ id: 2, name: 'Parent', parentProjectId: 1 }),
        mockProject({ id: 3, name: 'Child', parentProjectId: 2 }),
      ];

      const matched: Project[] = [mockProject({ id: 3, name: 'Child', parentProjectId: 2 })];

      const result = withAncestors(all, matched);

      expect(result).toHaveLength(3);
      const resultMap = new Map(result.map((p) => [p.id, p as any]));

      // マッチしたものは isContext なし
      expect(resultMap.get(3)?.isContext).toBeUndefined();

      // 祖先は isContext = true
      expect(resultMap.get(2)?.isContext).toBe(true);
      expect(resultMap.get(1)?.isContext).toBe(true);
    });

    it('マッチ済みの祖先は isContext を付けない', () => {
      const all: Project[] = [
        mockProject({ id: 1, name: 'Root' }),
        mockProject({ id: 2, name: 'Parent', parentProjectId: 1 }),
        mockProject({ id: 3, name: 'Child', parentProjectId: 2 }),
      ];

      const matched: Project[] = [
        mockProject({ id: 2, name: 'Parent', parentProjectId: 1 }),
        mockProject({ id: 3, name: 'Child', parentProjectId: 2 }),
      ];

      const result = withAncestors(all, matched);

      // 2 と 3 はマッチなので isContext なし、1 は祖先なので isContext = true
      const resultMap = new Map(result.map((p) => [p.id, p as any]));
      expect(resultMap.get(2)?.isContext).toBeUndefined();
      expect(resultMap.get(3)?.isContext).toBeUndefined();
      expect(resultMap.get(1)?.isContext).toBe(true);
    });
  });

  describe('rollupDescendantCount', () => {
    it('子孫の件数を計算する', () => {
      const projects: Project[] = [
        mockProject({ id: 1, name: 'Root' }),
        mockProject({ id: 2, name: 'Child', parentProjectId: 1 }),
        mockProject({ id: 3, name: 'GrandChild', parentProjectId: 2 }),
      ];

      let roots = buildProjectTree(projects);
      roots = rollupDescendantCount(roots);

      const root = roots[0];
      expect(root).toBeDefined();
      expect(root?.allDescendantCount).toBe(2); // 2 つの子孫
      const child = root?.children[0];
      expect(child).toBeDefined();
      expect(child?.allDescendantCount).toBe(1); // 1 つの子孫
      const grandchild = child?.children[0];
      expect(grandchild).toBeDefined();
      expect(grandchild?.allDescendantCount).toBe(0); // 子孫なし
    });
  });

  describe('groupProjects', () => {
    it('ステータス別グルーピング', () => {
      const projects: Project[] = [
        mockProject({ id: 1, status: 'in_progress' }),
        mockProject({ id: 2, status: 'planned' }),
        mockProject({ id: 3, status: 'completed' }),
      ];

      const groups = groupProjects(projects, 'status');

      expect(groups).toHaveLength(3);
      expect(groups.find((g) => g.key === 'in_progress')?.projects).toHaveLength(1);
      expect(groups.find((g) => g.key === 'planned')?.projects).toHaveLength(1);
      expect(groups.find((g) => g.key === 'completed')?.projects).toHaveLength(1);
    });

    it('ロードマップ別グルーピング（複数所属は重複表示）', () => {
      const projects: Project[] = [
        mockProject({ id: 1, roadmapIds: [10, 20] }),
        mockProject({ id: 2, roadmapIds: [20] }),
        mockProject({ id: 3, roadmapIds: [] }),
      ];

      const roadmaps = [
        { id: 10, name: 'Roadmap A' },
        { id: 20, name: 'Roadmap B' },
      ];

      const groups = groupProjects(projects, 'roadmap', roadmaps);

      // 10, 20, unassigned の 3 グループ
      expect(groups).toHaveLength(3);

      const group10 = groups.find((g) => g.key === '10');
      expect(group10?.projects).toHaveLength(1);
      expect(group10?.projects?.[0]?.id).toBe(1);

      const group20 = groups.find((g) => g.key === '20');
      expect(group20?.projects).toHaveLength(2);
      expect(group20?.projects?.map((p) => p.id).sort()).toEqual([1, 2]);

      const unassigned = groups.find((g) => g.key === 'unassigned');
      expect(unassigned?.projects).toHaveLength(1);
      expect(unassigned?.projects?.[0]?.id).toBe(3);
    });

    it('所属なしが「未所属」に入る', () => {
      const projects: Project[] = [
        mockProject({ id: 1, roadmapIds: [] }),
        mockProject({ id: 2, roadmapIds: undefined }),
      ];

      const groups = groupProjects(projects, 'roadmap', []);

      expect(groups).toHaveLength(1);
      expect(groups[0]?.key).toBe('unassigned');
      expect(groups[0]?.projects).toHaveLength(2);
    });

    it('空のグループは表示しない', () => {
      const projects: Project[] = [
        mockProject({ id: 1, status: 'in_progress' }),
      ];

      const groups = groupProjects(projects, 'status');

      // in_progress のみ
      expect(groups).toHaveLength(1);
      expect(groups[0]?.key).toBe('in_progress');
    });
  });

  describe('filterVisibleNodes', () => {
    it('全て展開状態で全ノードが表示される', () => {
      const projects: Project[] = [
        mockProject({ id: 1, name: 'Root' }),
        mockProject({ id: 2, name: 'Parent', parentProjectId: 1 }),
        mockProject({ id: 3, name: 'Child', parentProjectId: 2 }),
      ];

      let roots = buildProjectTree(projects);
      const expandedNodeIds = new Set([1, 2, 3]);
      const visible = filterVisibleNodes(roots, expandedNodeIds);

      expect(visible).toHaveLength(3);
      expect(visible.map((n) => n.project.id)).toEqual([1, 2, 3]);
    });

    it('親が折りたたまれると、子孫は表示されない', () => {
      const projects: Project[] = [
        mockProject({ id: 1, name: 'Root' }),
        mockProject({ id: 2, name: 'Parent', parentProjectId: 1 }),
        mockProject({ id: 3, name: 'Child', parentProjectId: 2 }),
      ];

      let roots = buildProjectTree(projects);
      // 1 は展開、2 は折りたたみ
      const expandedNodeIds = new Set([1]);
      const visible = filterVisibleNodes(roots, expandedNodeIds);

      expect(visible).toHaveLength(2); // 1, 2 のみ
      expect(visible.map((n) => n.project.id)).toEqual([1, 2]);
    });

    it('既定は全て展開状態', () => {
      const projects: Project[] = [
        mockProject({ id: 1, name: 'Root' }),
        mockProject({ id: 2, name: 'Parent', parentProjectId: 1 }),
      ];

      let roots = buildProjectTree(projects);
      // 全て展開
      const expandedNodeIds = new Set(projects.map((p) => p.id));
      const visible = filterVisibleNodes(roots, expandedNodeIds);

      expect(visible).toHaveLength(2);
    });
  });
});
