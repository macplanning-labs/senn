/**
 * TaskDependencyFlow.tsx — タスク依存関係フロー可視化
 */
import { useCallback, useEffect, useMemo } from 'react';
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

  const initialNodes: Node<TaskNodeData>[] = useMemo(() => {
    if (!graph) return [];
    return graph.nodes.map((n) => ({
      id: String(n.id),
      type: 'task',
      position: { x: 0, y: 0 },
      data: n,
    }));
  }, [graph]);

  const initialEdges: Edge[] = useMemo(() => {
    if (!graph) return [];
    return graph.edges.map((d) => ({
      id: String(d.id),
      source: String(d.fromTask),
      target: String(d.toTask),
      label: d.dependencyType,
    }));
  }, [graph]);

  const layoutedNodes = useMemo(
    () => layoutWithDagre(initialNodes, initialEdges),
    [initialNodes, initialEdges],
  );

  const [nodes, setNodes, onNodesChange] = useNodesState(layoutedNodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState(initialEdges);

  // useNodesState/useEdgesStateは初期値をマウント時にしか取り込まないため、
  // グラフのクエリ結果(非同期)が届いた後に明示的に同期する。
  useEffect(() => {
    setNodes(layoutedNodes);
  }, [layoutedNodes, setNodes]);

  useEffect(() => {
    setEdges(initialEdges);
  }, [initialEdges, setEdges]);

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
