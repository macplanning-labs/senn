/**
 * CycleList.tsx — サイクル一覧（スプリント管理）
 *
 * アクティブなサイクルを上部にプロミネント表示。
 * 計画中・完了済みはコンパクトリスト。
 */
import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useProject } from '@/shared/hooks/useProject';
import { useCycles, useCreateCycle, useDeleteCycle, useCompleteCycle } from '../hooks/useCycles';
import { VelocityChart } from './VelocityChart';
import type { Cycle } from '@/shared/api/types';
import './CycleList.css';

const statusLabels: Record<string, string> = {
  planned: '計画中',
  active: '進行中',
  completed: '完了',
};

const statusColors: Record<string, string> = {
  planned: 'var(--color-text-tertiary)',
  active: 'var(--color-accent)',
  completed: 'var(--color-success)',
};

export function CycleList() {
  const { currentProject: project } = useProject();
  const { data: cycles = [], isLoading } = useCycles(project?.id);
  const createMutation = useCreateCycle();
  const deleteMutation = useDeleteCycle();
  const completeMutation = useCompleteCycle();
  const navigate = useNavigate();

  const [showForm, setShowForm] = useState(false);
  const [formName, setFormName] = useState('');
  const [formStart, setFormStart] = useState('');
  const [formEnd, setFormEnd] = useState('');

  const activeCycle = cycles.find(c => c.status === 'active');
  const plannedCycles = cycles.filter(c => c.status === 'planned');
  const completedCycles = cycles.filter(c => c.status === 'completed');

  const handleCreate = () => {
    if (!project || !formName || !formStart || !formEnd) return;
    createMutation.mutate({
      project: project.id,
      name: formName,
      start_date: formStart,
      end_date: formEnd,
    }, {
      onSuccess: () => {
        setShowForm(false);
        setFormName('');
        setFormStart('');
        setFormEnd('');
      },
    });
  };

  const handleComplete = (cycle: Cycle) => {
    if (!project) return;
    const nextPlanned = plannedCycles[0];
    completeMutation.mutate({
      cycleId: cycle.id,
      projectId: project.id,
      carryOverTo: nextPlanned?.id,
    });
  };

  if (isLoading) {
    return <div className="cycle-loading">読み込み中...</div>;
  }

  return (
    <div className="cycle-list">
      <div className="cycle-list__header">
        <h1 className="cycle-list__title">Cycles</h1>
        <button
          className="cycle-list__add-btn"
          onClick={() => setShowForm(true)}
        >
          + 新しいサイクル
        </button>
      </div>

      {/* 作成フォーム */}
      {showForm && (
        <div className="cycle-form">
          <input
            className="cycle-form__input"
            placeholder="サイクル名（例: Sprint 14）"
            value={formName}
            onChange={e => setFormName(e.target.value)}
          />
          <div className="cycle-form__dates">
            <input
              type="date"
              className="cycle-form__input"
              value={formStart}
              onChange={e => setFormStart(e.target.value)}
            />
            <span className="cycle-form__separator">→</span>
            <input
              type="date"
              className="cycle-form__input"
              value={formEnd}
              onChange={e => setFormEnd(e.target.value)}
            />
          </div>
          <div className="cycle-form__actions">
            <button className="cycle-form__btn cycle-form__btn--primary" onClick={handleCreate}>
              作成
            </button>
            <button className="cycle-form__btn" onClick={() => setShowForm(false)}>
              キャンセル
            </button>
          </div>
        </div>
      )}

      {/* アクティブサイクル */}
      {activeCycle && (
        <div className="cycle-active" onClick={() => navigate(`cycles/${activeCycle.id}`)}>
          <div className="cycle-active__header">
            <span className="cycle-active__badge" style={{ background: statusColors.active }}>
              {statusLabels.active}
            </span>
            <h2 className="cycle-active__name">{activeCycle.name}</h2>
            <span className="cycle-active__dates">
              {activeCycle.startDate} — {activeCycle.endDate}
            </span>
          </div>
          <div className="cycle-active__stats">
            <div className="cycle-active__stat">
              <span className="cycle-active__stat-value">{activeCycle.completedCount}</span>
              <span className="cycle-active__stat-label">/ {activeCycle.ticketCount} 完了</span>
            </div>
            <div className="cycle-active__stat">
              <span className="cycle-active__stat-value">{activeCycle.completedPoints}</span>
              <span className="cycle-active__stat-label">/ {activeCycle.totalPoints} pt</span>
            </div>
          </div>
          <div className="cycle-active__progress">
            <div
              className="cycle-active__progress-bar"
              style={{
                width: `${activeCycle.ticketCount > 0
                  ? (activeCycle.completedCount / activeCycle.ticketCount) * 100
                  : 0}%`,
              }}
            />
          </div>
          <div className="cycle-active__actions" onClick={e => e.stopPropagation()}>
            <button
              className="cycle-form__btn cycle-form__btn--complete"
              onClick={() => handleComplete(activeCycle)}
            >
              完了にする
            </button>
          </div>
        </div>
      )}

      {/* 計画中 */}
      {plannedCycles.length > 0 && (
        <div className="cycle-section">
          <h3 className="cycle-section__title">計画中</h3>
          {plannedCycles.map(cycle => (
            <CycleRow
              key={cycle.id}
              cycle={cycle}
              onNavigate={() => navigate(`cycles/${cycle.id}`)}
              onDelete={() => project && deleteMutation.mutate({ id: cycle.id, projectId: project.id })}
            />
          ))}
        </div>
      )}

      {/* ベロシティグラフ */}
      {project && <VelocityChart projectId={project.id} />}

      {/* 完了済み */}
      {completedCycles.length > 0 && (
        <div className="cycle-section">
          <h3 className="cycle-section__title">完了済み</h3>
          {completedCycles.map(cycle => (
            <CycleRow
              key={cycle.id}
              cycle={cycle}
              onNavigate={() => navigate(`cycles/${cycle.id}`)}
            />
          ))}
        </div>
      )}

      {cycles.length === 0 && !showForm && (
        <div className="cycle-empty">
          <p>サイクルがまだありません。</p>
          <p>「+ 新しいサイクル」をクリックして最初のスプリントを作成しましょう。</p>
        </div>
      )}
    </div>
  );
}

/** サイクル行コンポーネント */
function CycleRow({
  cycle,
  onNavigate,
  onDelete,
}: {
  cycle: Cycle;
  onNavigate: () => void;
  onDelete?: () => void;
}) {
  const pct = cycle.ticketCount > 0
    ? Math.round((cycle.completedCount / cycle.ticketCount) * 100)
    : 0;

  return (
    <div className="cycle-row" onClick={onNavigate}>
      <span className="cycle-row__status" style={{ color: statusColors[cycle.status] }}>
        {statusLabels[cycle.status]}
      </span>
      <span className="cycle-row__name">{cycle.name}</span>
      <span className="cycle-row__dates">{cycle.startDate} — {cycle.endDate}</span>
      <span className="cycle-row__progress">{pct}%</span>
      <div className="cycle-row__bar">
        <div className="cycle-row__bar-fill" style={{ width: `${pct}%` }} />
      </div>
      {onDelete && (
        <button
          className="cycle-row__delete"
          onClick={e => { e.stopPropagation(); onDelete(); }}
          title="削除"
        >
          🗑
        </button>
      )}
    </div>
  );
}
