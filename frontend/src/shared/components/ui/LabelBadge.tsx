/**
 * LabelBadge.tsx — Label display badge component
 *
 * Colored pill badge for ticket labels.
 * Used in TicketTable, TicketDetailPanel, and KanbanCard.
 */

import './LabelBadge.css';

export interface Label {
  id: number;
  name: string;
  color: string;
  project?: number | null;
  createdAt?: string;
}

interface LabelBadgeProps {
  label: Label;
  size?: 'sm' | 'md';
  onRemove?: () => void;
}

export function LabelBadge({ label, size = 'sm', onRemove }: LabelBadgeProps) {
  return (
    <span
      className={`label-badge label-badge--${size}`}
      style={{
        '--label-color': label.color,
        backgroundColor: `${label.color}20`,
        color: label.color,
        borderColor: `${label.color}40`,
      } as React.CSSProperties}
      title={label.name}
    >
      <span
        className="label-badge__dot"
        style={{ backgroundColor: label.color }}
      />
      <span className="label-badge__text">{label.name}</span>
      {onRemove && (
        <button
          className="label-badge__remove"
          onClick={(e) => {
            e.stopPropagation();
            onRemove();
          }}
          aria-label={`Remove ${label.name}`}
        >
          ×
        </button>
      )}
    </span>
  );
}

interface LabelListProps {
  labels: Label[];
  max?: number;
  size?: 'sm' | 'md';
}

export function LabelList({ labels, max = 3, size = 'sm' }: LabelListProps) {
  if (!labels?.length) return null;

  const visible = labels.slice(0, max);
  const remaining = labels.length - max;

  return (
    <span className="label-list">
      {visible.map((label) => (
        <LabelBadge key={label.id} label={label} size={size} />
      ))}
      {remaining > 0 && (
        <span className="label-list__more">+{remaining}</span>
      )}
    </span>
  );
}
