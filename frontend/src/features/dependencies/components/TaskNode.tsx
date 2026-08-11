/**
 * TaskNode.tsx — 依存関係フローのカスタムノード
 */
import { Handle, Position } from '@xyflow/react';
import type { DependencyGraphNode } from '@/shared/api/types';
import './TaskDependencyFlow.css';

// GanttChart.tsx の statusColors と同じ定義(既存の色定義に統一)
const statusColors: Record<string, string> = {
  backlog: 'var(--color-status-backlog, #6b7280)',
  open: 'var(--color-status-open)',
  in_progress: 'var(--color-status-in-progress)',
  resolved: 'var(--color-status-resolved)',
  closed: 'var(--color-status-closed)',
  canceled: 'var(--color-status-canceled, #9ca3af)',
};

export interface TaskNodeData extends DependencyGraphNode {
  [key: string]: unknown;
}

export function TaskNode({ data }: { data: TaskNodeData }) {
  return (
    <div className="task-node">
      <Handle type="target" position={Position.Left} />
      <div className="task-node__header">
        <span className="task-node__key">{data.ticketKey}</span>
        <span
          className="task-node__status"
          style={{ backgroundColor: statusColors[data.status] ?? statusColors.backlog }}
        >
          {data.status}
        </span>
      </div>
      <div className="task-node__title">{data.title}</div>
      <div className="task-node__footer">
        <span className="task-node__assignee">
          {data.assignees[0]?.displayName ?? '未割当'}
          {data.assignees.length > 1 ? ` +${data.assignees.length - 1}` : ''}
        </span>
        {data.storyPoints != null && (
          <span className="task-node__points">{data.storyPoints}pt</span>
        )}
      </div>
      <Handle type="source" position={Position.Right} />
    </div>
  );
}
