/**
 * TaskDependencyFlow.tsx — タスク依存関係フロー可視化
 */
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  addEdge,
  useEdgesState,
  useNodesState,
  type Connection,
  type Edge,
  type Node,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import dagre from 'dagre';
import { isAxiosError } from 'axios';
import { useProject } from '@/shared/hooks/useProject';
import { useTeam } from '@/shared/hooks/useTeam';
import { useToast } from '@/shared/stores/toastStore';
import { TeamTabPageHeader } from '@/features/teams/components/TeamTabPageHeader';
import { IconDependency } from '@/shared/components/layout/Sidebar';
import {
  useCreateDependency,
  useDeleteDependency,
  useDependencyGraph,
  useUpdateCycleGraphPosition,
} from '../hooks/useDependencyGraph';
import { TaskNode, type TaskNodeData } from './TaskNode';
import { CycleGroupNode, type CycleGroupNodeData } from './CycleGroupNode';
import './TaskDependencyFlow.css';

const nodeTypes = { task: TaskNode, cycleGroup: CycleGroupNode };

const NODE_WIDTH = 260;
const NODE_HEIGHT = 90;
const GROUP_PADDING = { top: 36, right: 16, bottom: 16, left: 16 };
const GROUP_GAP = 32;
const CYCLE_PALETTE = ['#6366f1', '#22c55e', '#f59e0b', '#ec4899', '#06b6d4', '#a855f7'];

function cycleClusterId(cycle: number): string {
  return `cycle-${cycle}`;
}

// dagreはクラスタの外枠をタスク位置のみから算出するため、後から加える
// GROUP_PADDING(ラベル分の余白)を考慮していない。パディング込みの矩形で
// 重なりが出たクラスタをY方向に押し出して解消する。
// pinnedIds に含まれるノードは位置を移動しない。
function resolveClusterOverlaps(groupNodes: Node[], pinnedIds: Set<string>): void {
  const sorted = [...groupNodes].sort((a, b) => a.position.y - b.position.y);
  const box = (n: Node) => ({
    x: n.position.x,
    y: n.position.y,
    width: (n.style as { width?: number } | undefined)?.width ?? 0,
    height: (n.style as { height?: number } | undefined)?.height ?? 0,
  });
  const overlaps = (a: ReturnType<typeof box>, b: ReturnType<typeof box>) =>
    a.x < b.x + b.width + GROUP_GAP &&
    b.x < a.x + a.width + GROUP_GAP &&
    a.y < b.y + b.height + GROUP_GAP &&
    b.y < a.y + a.height + GROUP_GAP;

  for (let i = 1; i < sorted.length; i++) {
    const current = sorted[i];
    if (!current || pinnedIds.has(current.id)) continue; // ユーザーが手動配置した枠は動かさない
    let moved = true;
    while (moved) {
      moved = false;
      const bBox = box(current);
      for (let j = 0; j < i; j++) {
        const other = sorted[j];
        if (!other) continue;
        const aBox = box(other);
        if (overlaps(aBox, bBox)) {
          current.position.y = aBox.y + aBox.height + GROUP_GAP;
          moved = true;
          break;
        }
      }
    }
  }
}

// TicketForm.tsxのticket_typeセレクトと表記を統一
const TYPE_LABELS: Record<string, string> = {
  issue: '🐛 Issue',
  feature: '✨ Feature',
  improvement: '💡 Improvement',
  task: '📋 Task',
};

function layoutWithDagre(
  nodes: Node<TaskNodeData>[],
  edges: Edge[],
  pinnedPositions: Map<string, { x: number; y: number }>,
): Node[] {
  const g = new dagre.graphlib.Graph({ compound: true });
  g.setDefaultEdgeLabel(() => ({}));
  g.setGraph({ rankdir: 'LR', nodesep: 40, ranksep: 100 });

  // Cycleごとのクラスタ(親)ノードを登録
  const cycleNameById = new Map<string, string>();
  const cycleIdByClusterId = new Map<string, number>();
  nodes.forEach((n) => {
    const { cycle, cycleName } = n.data;
    if (cycle != null) {
      const clusterId = cycleClusterId(cycle);
      if (!cycleNameById.has(clusterId)) {
        cycleNameById.set(clusterId, cycleName ?? `Cycle ${cycle}`);
      }
      cycleIdByClusterId.set(clusterId, cycle);
    }
  });
  cycleNameById.forEach((_label, clusterId) => g.setNode(clusterId, {}));

  nodes.forEach((n) => {
    g.setNode(n.id, { width: NODE_WIDTH, height: NODE_HEIGHT });
    const { cycle } = n.data;
    if (cycle != null) {
      g.setParent(n.id, cycleClusterId(cycle));
    }
  });
  edges.forEach((e) => g.setEdge(e.source, e.target));

  dagre.layout(g);

  // dagreが自動算出するクラスタ(親)ノード自身のbboxは、クラスタをまたぐ
  // エッジがあると子タスクを完全に内包しないことがある(dagreのcompound
  // graphの既知の制限)。実際に配置された子タスクの位置から直接クラスタの
  // 外枠を算出することで、子を必ず内包することを保証する。
  const clusterBBox = new Map<string, { x: number; y: number; width: number; height: number }>();
  cycleNameById.forEach((_label, clusterId) => {
    const memberPositions = nodes
      .filter((n) => n.data.cycle != null && cycleClusterId(n.data.cycle) === clusterId)
      .map((n) => g.node(n.id));
    const left = Math.min(...memberPositions.map((p) => p.x - NODE_WIDTH / 2));
    const right = Math.max(...memberPositions.map((p) => p.x + NODE_WIDTH / 2));
    const top = Math.min(...memberPositions.map((p) => p.y - NODE_HEIGHT / 2));
    const bottom = Math.max(...memberPositions.map((p) => p.y + NODE_HEIGHT / 2));
    clusterBBox.set(clusterId, {
      x: (left + right) / 2,
      y: (top + bottom) / 2,
      width: right - left,
      height: bottom - top,
    });
  });

  const clusterIds = Array.from(cycleNameById.keys());
  const groupNodes: Node[] = clusterIds.map((clusterId, index) => {
    const pos = clusterBBox.get(clusterId)!;
    const pinned = pinnedPositions.get(clusterId);
    return {
      id: clusterId,
      type: 'cycleGroup',
      position: pinned ?? {
        x: pos.x - pos.width / 2 - GROUP_PADDING.left,
        y: pos.y - pos.height / 2 - GROUP_PADDING.top,
      },
      style: {
        width: pos.width + GROUP_PADDING.left + GROUP_PADDING.right,
        height: pos.height + GROUP_PADDING.top + GROUP_PADDING.bottom,
      },
      data: {
        label: cycleNameById.get(clusterId) ?? '',
        color: CYCLE_PALETTE[index % CYCLE_PALETTE.length],
        cycleId: cycleIdByClusterId.get(clusterId) ?? 0,
      },
      selectable: false,
      draggable: true,
      connectable: false,
      zIndex: -1,
    };
  });

  const pinnedIds = new Set(pinnedPositions.keys());
  resolveClusterOverlaps(groupNodes, pinnedIds);

  const taskNodes: Node[] = nodes.map((n) => {
    const pos = g.node(n.id);
    const { cycle } = n.data;
    if (cycle != null) {
      const clusterId = cycleClusterId(cycle);
      const parentPos = clusterBBox.get(clusterId)!;
      const parentLeft = parentPos.x - parentPos.width / 2 - GROUP_PADDING.left;
      const parentTop = parentPos.y - parentPos.height / 2 - GROUP_PADDING.top;
      return {
        ...n,
        parentId: clusterId,
        extent: 'parent' as const,
        position: {
          x: pos.x - NODE_WIDTH / 2 - parentLeft,
          y: pos.y - NODE_HEIGHT / 2 - parentTop,
        },
      };
    }
    return { ...n, position: { x: pos.x - NODE_WIDTH / 2, y: pos.y - NODE_HEIGHT / 2 } };
  });

  return [...groupNodes, ...taskNodes];
}

export function TaskDependencyFlow() {
  const { t } = useTranslation();
  const { currentProject } = useProject();
  const { currentTeam } = useTeam();
  const projectId = currentProject?.id;
  const teamId = currentTeam?.id;
  const toast = useToast();

  const { data: graph, isLoading } = useDependencyGraph(projectId, teamId);
  const createDependency = useCreateDependency(projectId, teamId);
  const deleteDependency = useDeleteDependency(projectId, teamId);
  const updateCyclePosition = useUpdateCycleGraphPosition();

  const [showIsolated, setShowIsolated] = useState(true);
  const [selectedType, setSelectedType] = useState('all');

  const allNodes: Node<TaskNodeData>[] = useMemo(() => {
    if (!graph) return [];
    return graph.nodes.map((n) => ({
      id: String(n.id),
      type: 'task',
      position: { x: 0, y: 0 },
      data: n as TaskNodeData,
    }));
  }, [graph]);

  const allEdges: Edge[] = useMemo(() => {
    if (!graph) return [];
    return graph.edges.map((d) => ({
      id: String(d.id),
      source: String(d.fromTask),
      target: String(d.toTask),
      label: d.dependencyType,
    }));
  }, [graph]);

  // 依存関係(エッジ)を1つも持たないノード = 孤立ノード。判定は種別フィルターの
  // 影響を受けない(データ全体での接続有無で決める)。
  const isolatedIds = useMemo(() => {
    const connected = new Set<string>();
    allEdges.forEach((e) => {
      connected.add(e.source);
      connected.add(e.target);
    });
    return new Set(allNodes.filter((n) => !connected.has(n.id)).map((n) => n.id));
  }, [allNodes, allEdges]);

  const availableTypes = useMemo(() => {
    if (!graph) return [];
    return Array.from(new Set(graph.nodes.map((n) => n.ticketType))).sort();
  }, [graph]);

  const filteredNodes = useMemo(() => {
    return allNodes.filter((n) => {
      const typeMatch = selectedType === 'all' || n.data.ticketType === selectedType;
      const isolationMatch = showIsolated || !isolatedIds.has(n.id);
      return typeMatch && isolationMatch;
    });
  }, [allNodes, selectedType, showIsolated, isolatedIds]);

  const filteredEdges = useMemo(() => {
    const visibleIds = new Set(filteredNodes.map((n) => n.id));
    return allEdges.filter((e) => visibleIds.has(e.source) && visibleIds.has(e.target));
  }, [allEdges, filteredNodes]);

  const pinnedPositions = useMemo(() => {
    const map = new Map<string, { x: number; y: number }>();
    graph?.cycles.forEach((c) => {
      if (c.graphPositionX != null && c.graphPositionY != null) {
        map.set(cycleClusterId(c.id), { x: c.graphPositionX, y: c.graphPositionY });
      }
    });
    return map;
  }, [graph]);

  const layoutedNodes = useMemo(
    () => layoutWithDagre(filteredNodes, filteredEdges, pinnedPositions),
    [filteredNodes, filteredEdges, pinnedPositions],
  );

  const [nodes, setNodes, onNodesChange] = useNodesState(layoutedNodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState(filteredEdges);

  // useNodesState/useEdgesStateは初期値をマウント時にしか取り込まないため、
  // グラフのクエリ結果(非同期)・フィルター変更後に明示的に同期する。
  useEffect(() => {
    setNodes(layoutedNodes);
  }, [layoutedNodes, setNodes]);

  useEffect(() => {
    setEdges(filteredEdges);
  }, [filteredEdges, setEdges]);

  const onConnect = useCallback(
    (connection: Connection) => {
      const sourceNode = graph?.nodes.find((n) => String(n.id) === connection.source);
      const targetNode = graph?.nodes.find((n) => String(n.id) === connection.target);
      if (!sourceNode || !targetNode) return;

      createDependency.mutate(
        { fromTicketKey: sourceNode.ticketKey, toTaskId: targetNode.id },
        {
          onSuccess: () => setEdges((eds) => addEdge(connection, eds)),
          onError: (error) => {
            const detail = isAxiosError<{ detail?: string }>(error)
              ? error.response?.data?.detail
              : undefined;
            toast.error(detail ?? '依存関係の作成に失敗しました');
          },
        },
      );
    },
    [graph, createDependency, setEdges, toast],
  );

  const onEdgeClick = useCallback(
    (_event: unknown, edge: Edge) => {
      const dep = graph?.edges.find((d) => String(d.id) === edge.id);
      if (!dep) return;
      if (!window.confirm(`「${dep.fromTaskKey} → ${dep.toTaskKey}」の依存関係を削除しますか?`)) return;
      deleteDependency.mutate(
        { ticketKey: dep.fromTaskKey, dependencyId: dep.id },
        {
          onError: () => toast.error('依存関係の削除に失敗しました'),
        },
      );
    },
    [graph, deleteDependency, toast],
  );

  const onNodeDragStop = useCallback(
    (_event: unknown, node: Node) => {
      if (node.type !== 'cycleGroup') return;
      const cycleId = (node.data as CycleGroupNodeData).cycleId;
      if (!cycleId) return;
      updateCyclePosition.mutate(
        { cycleId, x: node.position.x, y: node.position.y },
        {
          onError: () => toast.error('サイクルの位置の保存に失敗しました'),
        },
      );
    },
    [updateCyclePosition, toast],
  );

  if (isLoading) {
    return <div className="task-dependency-flow__loading">読み込み中...</div>;
  }

  return (
    <div className="task-dependency-flow">
      <TeamTabPageHeader
        icon={IconDependency}
        title={t('nav.dependencies')}
      />
      <div className="task-dependency-flow__toolbar">
        <label className="task-dependency-flow__toolbar-item">
          <input
            type="checkbox"
            checked={showIsolated}
            onChange={(e) => setShowIsolated(e.target.checked)}
          />
          孤立タスクを表示
        </label>
        <label className="task-dependency-flow__toolbar-item">
          種別
          <select
            className="task-dependency-flow__type-select"
            value={selectedType}
            onChange={(e) => setSelectedType(e.target.value)}
          >
            <option value="all">すべて</option>
            {availableTypes.map((t) => (
              <option key={t} value={t}>
                {TYPE_LABELS[t] ?? t}
              </option>
            ))}
          </select>
        </label>
      </div>
      <ReactFlow
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onConnect={onConnect}
        onEdgeClick={onEdgeClick}
        onNodeDragStop={onNodeDragStop}
        fitView
      >
        <Background />
        <Controls />
        <MiniMap />
      </ReactFlow>
    </div>
  );
}
