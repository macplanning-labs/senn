/**
 * TaskDependencyFlow.tsx — タスク依存関係フロー可視化
 */
import { useCallback, useEffect, useMemo, useState } from 'react';
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
import { useToast } from '@/shared/stores/toastStore';
import {
  useCreateDependency,
  useDeleteDependency,
  useDependencyGraph,
} from '../hooks/useDependencyGraph';
import { TaskNode, type TaskNodeData } from './TaskNode';
import './TaskDependencyFlow.css';

const nodeTypes = { task: TaskNode };

const NODE_WIDTH = 260;
const NODE_HEIGHT = 90;

// TicketForm.tsxのticket_typeセレクトと表記を統一
const TYPE_LABELS: Record<string, string> = {
  issue: '🐛 Issue',
  feature: '✨ Feature',
  improvement: '💡 Improvement',
  task: '📋 Task',
};

function layoutWithDagre(nodes: Node[], edges: Edge[]): Node[] {
  const g = new dagre.graphlib.Graph();
  g.setDefaultEdgeLabel(() => ({}));
  g.setGraph({ rankdir: 'LR', nodesep: 40, ranksep: 100 });

  nodes.forEach((n) => g.setNode(n.id, { width: NODE_WIDTH, height: NODE_HEIGHT }));
  edges.forEach((e) => g.setEdge(e.source, e.target));

  dagre.layout(g);

  return nodes.map((n) => {
    const pos = g.node(n.id);
    return {
      ...n,
      position: { x: pos.x - NODE_WIDTH / 2, y: pos.y - NODE_HEIGHT / 2 },
    };
  });
}

export function TaskDependencyFlow() {
  const { currentProject } = useProject();
  const projectId = currentProject?.id;
  const toast = useToast();

  const { data: graph, isLoading } = useDependencyGraph(projectId);
  const createDependency = useCreateDependency(projectId);
  const deleteDependency = useDeleteDependency(projectId);

  const [showIsolated, setShowIsolated] = useState(false);
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

  const layoutedNodes = useMemo(
    () => layoutWithDagre(filteredNodes, filteredEdges),
    [filteredNodes, filteredEdges],
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

  if (isLoading) {
    return <div className="task-dependency-flow__loading">読み込み中...</div>;
  }

  return (
    <div className="task-dependency-flow">
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
        fitView
      >
        <Background />
        <Controls />
        <MiniMap />
      </ReactFlow>
    </div>
  );
}
