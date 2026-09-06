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

// CycleSummaryBar.tsx の COMPLETE_STATUSES と同じ基準(resolved/closed = 完了)
const DONE_STATUSES = new Set(['resolved', 'closed']);

export interface TaskNodeData extends DependencyGraphNode {
  [key: string]: unknown;
}

export function TaskNode({ data }: { data: TaskNodeData }) {
  const isDone = DONE_STATUSES.has(data.status);
  return (
    <div className={`task-node${isDone ? ' task-node--done' : ''}`}>
      <Handle type="target" position={Position.Left} />
      <div className="task-node__header">
        <span className="task-node__key">{data.ticketKey}</span>
        <span
          className="task-node__status"
          style={{ backgroundColor: statusColors[data.status] ?? statusColors.backlog }}
        >
          {isDone ? '✓ ' : ''}
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
