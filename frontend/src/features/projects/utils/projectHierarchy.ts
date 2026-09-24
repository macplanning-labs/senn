/**
 * projectHierarchy.ts — プロジェクト階層・フィルタ・グルーピング（純関数）
 *
 * Views の絞り込み・階層表示・グルーピングをブラウザ側で行う。
 * 一覧の情報だけを使い、サーバー呼び出しは不要。
 */

import type { Project } from '@/shared/hooks/useProject';

/**
 * 階層ツリーのノード
 */
export interface ProjectNode {
  project: Project;
  /** ツリーでの深さ（ルートは 0） */
  depth: number;
  /** 子ノード */
  children: ProjectNode[];
  /** 祖先を文脈として加えた場合、true */
  isContext?: boolean;
  /** 子孫を含む全プロジェクト数 */
  allDescendantCount?: number;
}

/**
 * グループ（ステータス別またはロードマップ別）
 */
export interface ProjectGroup {
  key: string;
  label: string;
  projects: Project[];
}

/**
 * フィルタオプション
 */
export interface ProjectFilters {
  mine?: boolean;
  priority?: string;
  teamId?: string;
  parentProjectId?: string; // '' = すべて、'none' = ルートのみ
  roadmapId?: string;
  relatedIds?: Set<number>; // relatedTo で取得した id 集合
}

/**
 * 親子のツリー化
 * 親が一覧に無い場合や、絞り込みで親が消えた場合はルート扱い
 */
export function buildProjectTree(projects: Project[]): ProjectNode[] {
  const byId = new Map(projects.map((p) => [p.id, p]));
  const nodes = new Map<number, ProjectNode>();

  // すべてのプロジェクトをノード化
  for (const project of projects) {
    nodes.set(project.id, {
      project,
      depth: 0,
      children: [],
    });
  }

  // 親の存在確認と深さ計算
  const roots: ProjectNode[] = [];
  for (const node of nodes.values()) {
    const parentId = node.project.parentProjectId;
    if (parentId && byId.has(parentId)) {
      const parentNode = nodes.get(parentId)!;
      parentNode.children.push(node);
      // 深さは親の深さ + 1（後で再計算）
    } else {
      roots.push(node);
    }
  }

  // 深さを計算（BFS）
  const queue: ProjectNode[] = [...roots];
  while (queue.length > 0) {
    const node = queue.shift()!;
    for (const child of node.children) {
      child.depth = node.depth + 1;
      queue.push(child);
    }
  }

  // 子ノードを名前順でソート
  const sortByName = (nodes: ProjectNode[]) => {
    nodes.sort((a, b) => a.project.name.localeCompare(b.project.name, 'ja'));
    for (const node of nodes) {
      sortByName(node.children);
    }
  };
  sortByName(roots);

  return roots;
}

/**
 * フィルタ条件に合うプロジェクトを絞り込む（AND）
 * mine, priority, teamId, parentProjectId, roadmapId, relatedIds の条件を all match
 */
export function filterProjects(
  projects: Project[],
  filters: ProjectFilters,
): Project[] {
  return projects.filter((project) => {
    // mine フィルタ
    if (filters.mine && !project.isMember) {
      return false;
    }

    // priority フィルタ
    if (filters.priority && project.priority !== filters.priority) {
      return false;
    }

    // teamId フィルタ
    if (filters.teamId) {
      const hasTeam = (project.teams ?? []).some((team) => String(team.id) === filters.teamId);
      if (!hasTeam) {
        return false;
      }
    }

    // parentProjectId フィルタ
    if (filters.parentProjectId !== undefined && filters.parentProjectId !== '') {
      if (filters.parentProjectId === 'none') {
        // ルートのみ
        if (project.parentProjectId !== undefined && project.parentProjectId !== null) {
          return false;
        }
      } else {
        // 指定の親のみ
        const parentId = parseInt(filters.parentProjectId, 10);
        if (isNaN(parentId) || project.parentProjectId !== parentId) {
          return false;
        }
      }
    }

    // roadmapId フィルタ
    if (filters.roadmapId) {
      const roadmapId = parseInt(filters.roadmapId, 10);
      if (isNaN(roadmapId) || !(project.roadmapIds ?? []).includes(roadmapId)) {
        return false;
      }
    }

    // relatedIds フィルタ
    if (filters.relatedIds && filters.relatedIds.size > 0) {
      if (!filters.relatedIds.has(project.id)) {
        return false;
      }
    }

    return true;
  });
}

/**
 * 絞り込み結果に祖先を加える
 * 祖先には isContext: true を付ける
 */
export function withAncestors(all: Project[], matched: Project[]): Project[] {
  const allMap = new Map(all.map((p) => [p.id, p]));
  const matchedSet = new Set(matched.map((p) => p.id));
  const ancestorSet = new Set<number>();

  // 各マッチプロジェクトの祖先を集める
  for (const project of matched) {
    let parentId = project.parentProjectId;
    while (parentId !== undefined && parentId !== null && !matchedSet.has(parentId)) {
      ancestorSet.add(parentId);
      const parent = allMap.get(parentId);
      if (!parent) break;
      parentId = parent.parentProjectId;
    }
  }

  // マッチ + 祖先 を返す（祖先に isContext フラグを付ける）
  const result: Project[] = [];
  const added = new Set<number>();

  // マッチを先に追加
  for (const project of matched) {
    if (!added.has(project.id)) {
      result.push(project);
      added.add(project.id);
    }
  }

  // 祖先を追加（isContext フラグ付き）
  for (const ancestorId of ancestorSet) {
    if (!added.has(ancestorId)) {
      const ancestor = allMap.get(ancestorId);
      if (ancestor) {
        result.push({ ...ancestor, isContext: true } as Project & { isContext: boolean });
        added.add(ancestorId);
      }
    }
  }

  return result;
}

/**
 * 子孫をカウント（子孫の件数を allDescendantCount に入れ、ツリー構造で返す）
 */
export function rollupDescendantCount(roots: ProjectNode[]): ProjectNode[] {
  const countDescendants = (node: ProjectNode): number => {
    let count = 0;
    for (const child of node.children) {
      count += 1 + countDescendants(child);
    }
    node.allDescendantCount = count;
    return count;
  };

  for (const root of roots) {
    countDescendants(root);
  }

  return roots;
}

/**
 * ステータス別またはロードマップ別でグルーピング
 */
export function groupProjects(
  projects: Project[],
  groupBy: 'status' | 'roadmap',
  roadmaps?: { id: number; name: string }[],
): ProjectGroup[] {
  if (groupBy === 'status') {
    return groupByStatus(projects);
  } else {
    return groupByRoadmap(projects, roadmaps ?? []);
  }
}

/**
 * ステータス別でグルーピング（in_progress / planned / paused / completed）
 */
function groupByStatus(projects: Project[]): ProjectGroup[] {
  const normalizeStatus = (status?: string): 'in_progress' | 'planned' | 'paused' | 'completed' => {
    if (status === 'planned' || status === 'paused' || status === 'completed') {
      return status;
    }
    return 'in_progress';
  };

  const groups: Record<string, Project[]> = {
    in_progress: [],
    planned: [],
    paused: [],
    completed: [],
  };

  for (const project of projects) {
    const key = normalizeStatus(project.status);
    groups[key]!.push(project);
  }

  const result: ProjectGroup[] = [];
  const statusKeys: Array<'in_progress' | 'planned' | 'paused' | 'completed'> = [
    'in_progress',
    'planned',
    'paused',
    'completed',
  ];
  for (const key of statusKeys) {
    const projects_ = groups[key] as Project[];
    if (projects_.length > 0) {
      result.push({ key, label: key, projects: projects_ });
    }
  }
  return result;
}

/**
 * ロードマップ別でグルーピング
 * 1つのプロジェクトが複数のロードマップに属する場合は、それぞれに表示
 * 所属なしは「未所属」グループ
 */
function groupByRoadmap(
  projects: Project[],
  roadmaps: { id: number; name: string }[],
): ProjectGroup[] {
  const groups: Record<number, Project[] | undefined> = {};
  const unassignedProjects: Project[] = [];

  for (const project of projects) {
    const roadmapIds = project.roadmapIds ?? [];
    if (roadmapIds.length === 0) {
      unassignedProjects.push(project);
    } else {
      for (const roadmapId of roadmapIds) {
        if (!groups[roadmapId]) {
          groups[roadmapId] = [];
        }
        groups[roadmapId]?.push(project);
      }
    }
  }

  const result: ProjectGroup[] = [];

  // ロードマップの順序に合わせてグループを追加
  for (const roadmap of roadmaps) {
    const groupProjects = groups[roadmap.id];
    if (groupProjects && groupProjects.length > 0) {
      result.push({
        key: String(roadmap.id),
        label: roadmap.name,
        projects: groupProjects as Project[],
      });
    }
  }

  // 未所属グループを最後に追加
  if (unassignedProjects.length > 0) {
    result.push({
      key: 'unassigned',
      label: 'unassigned',
      projects: unassignedProjects,
    });
  }

  return result;
}

/**
 * 展開状態に基づいて、表示すべきノード（フラット化）を返す
 * 折りたたまれた親の子孫は除外される
 */
export function filterVisibleNodes(
  roots: ProjectNode[],
  expandedNodeIds: Set<number>,
): ProjectNode[] {
  const result: ProjectNode[] = [];

  const traverse = (node: ProjectNode) => {
    result.push(node);
    // 親が展開状態で、子が存在すれば、子を再帰的に処理
    if (expandedNodeIds.has(node.project.id) && node.children.length > 0) {
      for (const child of node.children) {
        traverse(child);
      }
    }
  };

  for (const root of roots) {
    traverse(root);
  }

  return result;
}

/**
 * 階層表示の1行
 */
export interface HierarchyRow {
  project: Project;
  /** ツリーでの深さ(このグループ内のルートは 0) */
  depth: number;
  /** 表示対象ではなく、親として文脈のために加えた行 */
  isContext: boolean;
  /** このグループ内に表示される子がある(折りたたみできる) */
  hasChildren: boolean;
}

/**
 * 階層表示の行を作る。
 * - `shown`: このグループに表示するプロジェクト(絞り込み・グルーピング済み)
 * - `all`: 全プロジェクト(祖先を探すため)
 * - `collapsed`: 折りたたまれている親の id(その子孫は行に含めない)
 * 表示対象の祖先が `shown` に無い場合は、文脈用の行(isContext)として加える。
 */
export function buildHierarchyRows(
  all: Project[],
  shown: Project[],
  collapsed: Set<number>,
): HierarchyRow[] {
  const shownIds = new Set(shown.map((p) => p.id));
  const source = withAncestors(all, shown);
  const roots = buildProjectTree(source);
  const expanded = new Set(source.filter((p) => !collapsed.has(p.id)).map((p) => p.id));
  return filterVisibleNodes(roots, expanded).map((node) => ({
    project: node.project,
    depth: node.depth,
    isContext: !shownIds.has(node.project.id),
    hasChildren: node.children.length > 0,
  }));
}
