/**
 * CycleGroupNode.tsx — 依存関係フローのCycleグループ枠(ラベル付き背景ボックス)
 */
import './TaskDependencyFlow.css';

export interface CycleGroupNodeData {
  label: string;
  color: string;
  cycleId: number;
  [key: string]: unknown;
}

export function CycleGroupNode({ data }: { data: CycleGroupNodeData }) {
  return (
    <div className="cycle-group-node" style={{ borderColor: data.color }}>
      <span className="cycle-group-node__label" style={{ backgroundColor: data.color }}>
        {data.label}
      </span>
    </div>
  );
}
