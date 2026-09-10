/**
 * CycleList.tsx — サイクル一覧（スプリント管理）
 *
 * アクティブなサイクルを上部にプロミネント表示。
 * 計画中・完了済みはコンパクトリスト。
 */
import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useProject } from '@/shared/hooks/useProject';
import { useTeam } from '@/shared/hooks/useTeam';
import { useCycles, useCreateCycle, useDeleteCycle, useCompleteCycle } from '../hooks/useCycles';
import { VelocityChart } from './VelocityChart';
import type { Cycle } from '@/shared/api/types';
import './CycleList.css';
import { useTranslation } from 'react-i18next';

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

function todayYmd(): string {
  const now = new Date();
  const y = now.getFullYear();
  const m = String(now.getMonth() + 1).padStart(2, '0');
  const d = String(now.getDate()).padStart(2, '0');
  return `${y}-${m}-${d}`;
}

export function CycleList() {
  const { t } = useTranslation();
  const { currentProject: project } = useProject();
  const { currentTeam: team } = useTeam();
  const projectId = project?.id;
  const teamId = team?.id;
  const { data: cycles = [], isLoading } = useCycles(projectId, teamId);
  const createMutation = useCreateCycle();
  const deleteMutation = useDeleteCycle();
  const completeMutation = useCompleteCycle(projectId, teamId);
  const navigate = useNavigate();

  const [showForm, setShowForm] = useState(false);
  const [formName, setFormName] = useState('');
  const [formDescription, setFormDescription] = useState('');
  const [formStart, setFormStart] = useState('');
  const [formEnd, setFormEnd] = useState('');
  const [formTeamId, setFormTeamId] = useState<number | null>(project?.ownerTeam?.id ?? team?.id ?? null);
  const [filterTeamId, setFilterTeamId] = useState<number | null>(null);
  const [error, setError] = useState('');

  const activeCycle = cycles.find(c => c.status === 'active');
  const plannedCycles = cycles.filter(c => c.status === 'planned');
  const completedCycles = cycles.filter(c => c.status === 'completed');

  // 一覧に出現するすべてのチームを集約
  const teamSet = new Map<number, typeof cycles[0]['team']>();
  cycles.forEach(c => {
    if (c.team?.id) {
      teamSet.set(c.team.id, c.team);
    }
  });
  if (project?.ownerTeam?.id) {
    teamSet.set(project.ownerTeam.id, project.ownerTeam);
  }

  // フィルタを適用
  const filteredPlanned = filterTeamId
    ? plannedCycles.filter(c => c.team?.id === filterTeamId)
    : plannedCycles;
  const filteredCompleted = filterTeamId
    ? completedCycles.filter(c => c.team?.id === filterTeamId)
    : completedCycles;

  const today = todayYmd();
  const isActiveCycleOverdue = activeCycle && activeCycle.endDate < today;

  const handleCreate = () => {
    if (!formName || !formStart || !formEnd) {
      setError('サイクル名・開始日・終了日は必須です');
      return;
    }
    if (!project && !team) {
      setError('プロジェクトまたはチームを指定してください');
      return;
    }
    createMutation.mutate({
      project: project?.id,
      team: team?.id,
      name: formName,
      description: formDescription,
      start_date: formStart,
      end_date: formEnd,
      teamId: formTeamId || undefined,
    }, {
      onSuccess: () => {
        setShowForm(false);
        setFormName('');
        setFormDescription('');
        setFormStart('');
        setFormEnd('');
        setFormTeamId(project?.ownerTeam?.id ?? team?.id ?? null);
        setError('');
      },
      onError: (err: unknown) => {
        const axiosErr = err as { response?: { data?: Record<string, string | string[]> } };
        const detail = axiosErr.response?.data;
        if (detail) {
          const messages = Object.values(detail)
            .flat()
            .map(msg => typeof msg === 'string' ? msg : '')
            .filter(Boolean)
            .join(', ');
          setError(messages || 'サイクルの作成に失敗しました');
        } else {
          setError('サイクルの作成に失敗しました');
        }
      },
    });
  };

  const handleComplete = (cycle: Cycle) => {
    if (!project && !team) return;
    const nextPlanned = plannedCycles[0];
    completeMutation.mutate({
      cycleId: cycle.id,
      carryOverTo: nextPlanned?.id,
    });
  };

  if (isLoading) {
    return <div className="cycle-loading">{t('common.loading')}</div>;
  }

  return (
    <div className="cycle-list">
      <div className="cycle-list__header">
        <h1 className="cycle-list__title">Cycles</h1>
        <button
          className="cycle-list__add-btn"
          onClick={() => setShowForm(true)}
        >
          {t('cycle.newCycle')}
        </button>
      </div>

      {/* 作成フォーム */}
      {showForm && (
        <div className="cycle-form">
          <input
            className="cycle-form__input"
            placeholder="サイクル名（例: Sprint 14）"
            value={formName}
            onChange={e => {
              setFormName(e.target.value);
              setError('');
            }}
          />
          <textarea
            className="cycle-form__textarea"
            placeholder="説明（任意・このサイクルのゴールや注力ドメインなど）"
            value={formDescription}
            onChange={e => setFormDescription(e.target.value)}
            rows={3}
          />
          <div className="cycle-form__dates">
            <input
              type="date"
              className="cycle-form__input"
              value={formStart}
              onChange={e => {
                setFormStart(e.target.value);
                setError('');
              }}
            />
            <span className="cycle-form__separator">→</span>
            <input
              type="date"
              className="cycle-form__input"
              value={formEnd}
              onChange={e => {
                setFormEnd(e.target.value);
                setError('');
              }}
            />
          </div>
          <div className="cycle-form__team">
            <label htmlFor="cycle-team-select" style={{ fontSize: 'var(--font-size-sm)', color: 'var(--color-text-secondary)' }}>
              Team（任意）
            </label>
            <select
              id="cycle-team-select"
              className="cycle-form__input"
              value={formTeamId ?? ''}
              onChange={e => setFormTeamId(e.target.value ? parseInt(e.target.value) : null)}
            >
              <option value="">— 未選択 —</option>
              {project?.ownerTeam && (
                <option value={project.ownerTeam.id}>{project.ownerTeam.name} (PJ Owner)</option>
              )}
            </select>
          </div>
          {error && <div className="cycle-form__error">{error}</div>}
          <div className="cycle-form__actions">
            <button className="cycle-form__btn cycle-form__btn--primary" onClick={handleCreate} disabled={createMutation.isPending}>
              {createMutation.isPending ? 'Creating...' : '作成'}
            </button>
            <button className="cycle-form__btn" onClick={() => {
              setShowForm(false);
              setError('');
            }}>
              キャンセル
            </button>
          </div>
        </div>
      )}

      {/* チームフィルタ(2チーム以上のCycleが混在する場合のみ表示。Team画面では常に1チームのため出さない) */}
      {teamSet.size > 1 && (
        <div style={{ marginBottom: 'var(--space-3)', display: 'flex', gap: 'var(--space-2)', alignItems: 'center' }}>
          <label style={{ fontSize: 'var(--font-size-sm)', color: 'var(--color-text-secondary)' }}>Filter by Team:</label>
          <select
            className="cycle-form__input"
            style={{ flex: 1, maxWidth: '200px' }}
            value={filterTeamId ?? ''}
            onChange={e => setFilterTeamId(e.target.value ? parseInt(e.target.value) : null)}
          >
            <option value="">すべてのTeam</option>
            {Array.from(teamSet.values()).map(team => (
              <option key={team!.id} value={team!.id}>{team!.name}</option>
            ))}
          </select>
        </div>
      )}

      {/* アクティブサイクル */}
      {activeCycle && (
        <div className="cycle-active" data-testid="cycle-active-card" onClick={() => {
          if (project) {
            navigate(`/p/${project.prefix}/cycles/${activeCycle.id}`);
          } else if (team) {
            navigate(`/t/${team.slug}/cycles/${activeCycle.id}`);
          }
        }}>
          <div className="cycle-active__header">
            <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-2)' }}>
              <span className="cycle-active__badge" style={{ background: statusColors.active }}>
                {statusLabels.active}
              </span>
              {isActiveCycleOverdue && (
                <span style={{
                  display: 'inline-block',
                  padding: '4px 8px',
                  fontSize: 'var(--font-size-xs)',
                  fontWeight: 'var(--font-weight-semibold)',
                  color: 'white',
                  background: 'var(--color-error)',
                  borderRadius: 'var(--radius-sm)',
                }}>
                  {t('cycle.overdue')}
                </span>
              )}
            </div>
            <h2 className="cycle-active__name">{activeCycle.name}</h2>
            <span className="cycle-active__dates">
              {activeCycle.startDate} — {activeCycle.endDate}
            </span>
            {activeCycle.description && (
              <div style={{
                marginTop: 'var(--space-2)',
                color: 'var(--color-text-secondary)',
                fontSize: 'var(--font-size-sm)',
                lineHeight: 1.4,
                overflow: 'hidden',
                textOverflow: 'ellipsis',
                display: '-webkit-box',
                WebkitLineClamp: 2,
                WebkitBoxOrient: 'vertical',
              }}>
                {activeCycle.description}
              </div>
            )}
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
      {filteredPlanned.length > 0 && (
        <div className="cycle-section">
          <h3 className="cycle-section__title">計画中</h3>
          {filteredPlanned.map(cycle => (
            <CycleRow
              key={cycle.id}
              cycle={cycle}
              onNavigate={() => {
                if (project) {
                  navigate(`/p/${project.prefix}/cycles/${cycle.id}`);
                } else if (team) {
                  navigate(`/t/${team.slug}/cycles/${cycle.id}`);
                }
              }}
              onDelete={() => deleteMutation.mutate({ id: cycle.id, projectId: project?.id, teamId: team?.id })}
            />
          ))}
        </div>
      )}

      {/* ベロシティグラフ（プロジェクト版のみ） */}
      {project && project.id && <VelocityChart projectId={project.id} />}

      {/* 完了済み */}
      {filteredCompleted.length > 0 && (
        <div className="cycle-section">
          <h3 className="cycle-section__title">{t('cycle.completed')}</h3>
          {filteredCompleted.map(cycle => (
            <CycleRow
              key={cycle.id}
              cycle={cycle}
              onNavigate={() => {
                if (project) {
                  navigate(`/p/${project.prefix}/cycles/${cycle.id}`);
                } else if (team) {
                  navigate(`/t/${team.slug}/cycles/${cycle.id}`);
                }
              }}
            />
          ))}
        </div>
      )}

      {cycles.length === 0 && !showForm && (
        <div className="cycle-empty">
          <p>{t('cycle.noCycles')}</p>
          <p>「{t('cycle.newCycle')}」をクリックして最初のスプリントを作成しましょう。</p>
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
  const { t } = useTranslation();
  const pct = cycle.ticketCount > 0
    ? Math.round((cycle.completedCount / cycle.ticketCount) * 100)
    : 0;

  const today = todayYmd();
  const isOverdue = cycle.status === 'active' && cycle.endDate < today;

  return (
    <div className="cycle-row" onClick={onNavigate}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-2)' }}>
        <span className="cycle-row__status" style={{ color: statusColors[cycle.status] }}>
          {statusLabels[cycle.status]}
        </span>
        {cycle.team && (
          <span style={{
            display: 'inline-block',
            padding: '2px 6px',
            fontSize: 'var(--font-size-xs)',
            fontWeight: 'var(--font-weight-semibold)',
            color: 'white',
            background: cycle.team.color || 'var(--color-text-secondary)',
            borderRadius: 'var(--radius-sm)',
          }}>
            {cycle.team.name}
          </span>
        )}
        {isOverdue && (
          <span style={{
            display: 'inline-block',
            padding: '2px 6px',
            fontSize: 'var(--font-size-xs)',
            fontWeight: 'var(--font-weight-semibold)',
            color: 'white',
            background: 'var(--color-error)',
            borderRadius: 'var(--radius-sm)',
          }}>
            {t('cycle.overdue')}
          </span>
        )}
      </div>
      <span className="cycle-row__name">{cycle.name}</span>
      <span className="cycle-row__dates">{cycle.startDate} — {cycle.endDate}</span>
      {cycle.description && (
        <span style={{
          display: '-webkit-box' as any,
          color: 'var(--color-text-secondary)',
          fontSize: 'var(--font-size-sm)',
          lineHeight: 1.4,
          overflow: 'hidden',
          textOverflow: 'ellipsis',
          WebkitLineClamp: 1,
          WebkitBoxOrient: 'vertical',
          marginTop: '4px',
          width: '100%',
        }}>
          {cycle.description}
        </span>
      )}
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
