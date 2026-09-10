/**
 * CycleDetail.tsx — サイクル詳細ページ（タスクファースト）
 *
 * ヘッダー + ミニサマリー(CycleSummaryBar) + チケット一覧(TicketTable)をメインに配置し、
 * バーンダウンチャートはURL(?panel=chart)で開閉するサイドパネルに格納する。
 */
import { useParams, useNavigate, useSearchParams } from 'react-router-dom';
import { useState, useEffect } from 'react';
import { useCycle, useCycleProgress, useCycles, useCreateCycle, useCompleteCycle, useUpdateCycle } from '../hooks/useCycles';
import { BurndownChart } from './BurndownChart';
import { CycleSummaryBar } from './CycleSummaryBar';
import { TicketTable } from '@/features/tickets/components/TicketTable';
import { TicketDetailPanel } from '@/features/tickets/components/TicketDetailPanel';
import { usePanelResize } from '@/shared/hooks/usePanelResize';
import { useProject } from '@/shared/hooks/useProject';
import { useToastStore } from '@/shared/stores/toastStore';
import './CycleDetail.css';
import { useTranslation } from 'react-i18next';

const statusLabels: Record<string, string> = {
  planned: '計画中',
  active: '進行中',
  completed: '完了',
};

/** YYYY-MM-DD をローカル日付として日数加算（UTC 解釈のズレを避ける） */
function parseYmdParts(ymd: string): [number, number, number] {
  const parts = ymd.split('-');
  const y = Number(parts[0]);
  const m = Number(parts[1]);
  const d = Number(parts[2]);
  if (!Number.isFinite(y) || !Number.isFinite(m) || !Number.isFinite(d)) {
    throw new Error(`invalid YMD: ${ymd}`);
  }
  return [y, m, d];
}

function todayYmd(): string {
  const now = new Date();
  const y = now.getFullYear();
  const m = String(now.getMonth() + 1).padStart(2, '0');
  const d = String(now.getDate()).padStart(2, '0');
  return `${y}-${m}-${d}`;
}

function addDaysYmd(ymd: string, days: number): string {
  const [y, m, d] = parseYmdParts(ymd);
  const dt = new Date(y, m - 1, d);
  dt.setDate(dt.getDate() + days);
  const yy = dt.getFullYear();
  const mm = String(dt.getMonth() + 1).padStart(2, '0');
  const dd = String(dt.getDate()).padStart(2, '0');
  return `${yy}-${mm}-${dd}`;
}

function diffDaysYmd(start: string, end: string): number {
  const [sy, sm, sd] = parseYmdParts(start);
  const [ey, em, ed] = parseYmdParts(end);
  const a = new Date(sy, sm - 1, sd).getTime();
  const b = new Date(ey, em - 1, ed).getTime();
  return Math.round((b - a) / 86400000);
}

export function CycleDetail() {
  const { t } = useTranslation();
  const { projectKey, teamSlug, cycleId, ticketId } = useParams<{
    projectKey: string;
    teamSlug: string;
    cycleId: string;
    ticketId?: string;
  }>();
  // Projectスコープ(/p/:projectKey/cycles/:cycleId)とTeamスコープ(/t/:teamSlug/cycles/:cycleId)の
  // どちらでマウントされたかでURLの組み立て先を切り替える(WIPAPPDEV-000045)
  const cyclesListPath = projectKey ? `/p/${projectKey}/cycles` : `/t/${teamSlug}/cycles`;
  const cycleDetailPath = (id: string) =>
    projectKey ? `/p/${projectKey}/cycles/${id}` : `/t/${teamSlug}/cycles/${id}`;
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const { currentProject } = useProject();
  const { addToast } = useToastStore();
  const { data: cycle, isLoading: cycleLoading } = useCycle(
    cycleId ? parseInt(cycleId) : undefined
  );
  const { data: progress } = useCycleProgress(
    cycleId ? parseInt(cycleId) : undefined
  );
  const { data: cycles = [] } = useCycles(currentProject?.id);
  const createMutation = useCreateCycle();
  const completeMutation = useCompleteCycle(currentProject?.id);
  const updateCycleMutation = useUpdateCycle(currentProject?.id);
  const { width: chartWidth, onResizeStart, isResizing } = usePanelResize('cycle-burndown', 420);
  const { width: panelWidth, onResizeStart: onPanelResizeStart, isResizing: isPanelResizing } = usePanelResize('cycle-ticket-detail', 380);

  const [completeDialogOpen, setCompleteDialogOpen] = useState(false);
  const [selectedCarryOver, setSelectedCarryOver] = useState<number | 'create' | null>(null);
  const [isCompleting, setIsCompleting] = useState(false);
  const [nameDraft, setNameDraft] = useState('');
  const [descriptionDraft, setDescriptionDraft] = useState('');

  useEffect(() => {
    if (cycle) {
      setNameDraft(cycle.name);
      setDescriptionDraft(cycle.description || '');
    }
  }, [cycle?.id, cycle?.name, cycle?.description]);

  const chartOpen = searchParams.get('panel') === 'chart';
  const toggleChart = () => {
    setSearchParams((prev) => {
      const next = new URLSearchParams(prev);
      if (chartOpen) {
        next.delete('panel');
      } else {
        next.set('panel', 'chart');
      }
      return next;
    }, { replace: true });
  };

  const handleClosePanel = () => {
    if (cycleId) {
      navigate(cycleDetailPath(cycleId));
    }
  };

  const today = todayYmd();
  const isOverdue = cycle && cycle.status === 'active' && cycle.endDate < today;

  const plannedCycles = cycles.filter(c => c.status === 'planned' && c.project === currentProject?.id);

  const handleCompleteClick = () => {
    const firstPlanned = plannedCycles[0];
    if (firstPlanned) {
      setSelectedCarryOver(firstPlanned.id);
    } else {
      setSelectedCarryOver('create');
    }
    setCompleteDialogOpen(true);
  };

  const handleConfirmComplete = async () => {
    if (!cycle || !currentProject || selectedCarryOver === null) return;
    setIsCompleting(true);

    try {
      let carryOverTo: number | undefined;
      if (selectedCarryOver === 'create') {
        const start = addDaysYmd(cycle.endDate, 1);
        const durationDays = diffDaysYmd(cycle.startDate, cycle.endDate);
        const end = addDaysYmd(start, durationDays > 0 ? durationDays : 14);
        const newCycle = await createMutation.mutateAsync({
          project: currentProject.id,
          name: `Cycle ${cycle.number + 1}`,
          start_date: start,
          end_date: end,
          status: 'planned',
        });
        carryOverTo = newCycle.id;
      } else {
        carryOverTo = selectedCarryOver;
      }

      await completeMutation.mutateAsync({
        cycleId: cycle.id,
        carryOverTo,
      });
      setCompleteDialogOpen(false);
      addToast({ message: t('cycle.complete'), type: 'success' });
      setTimeout(() => navigate(-1), 500);
    } catch {
      addToast({ message: t('cycle.completeFailed', '完了に失敗しました'), type: 'error' });
    } finally {
      setIsCompleting(false);
    }
  };

  if (cycleLoading || !cycle) {
    return <div className="cycle-detail__loading">{t('common.loading')}</div>;
  }

  return (
    <div className="cycle-detail" data-testid="cycle-detail-page">
      {/* ヘッダー */}
      <div className="cycle-detail__header">
        <button
          className="cycle-detail__back"
          onClick={() => navigate(cyclesListPath)}
          data-testid="cycle-detail-back"
        >
          ← 戻る
        </button>
        <input
          className="cycle-detail__title-input"
          value={nameDraft}
          onChange={(e) => setNameDraft(e.target.value)}
          onBlur={() => {
            if (!cycle) return;
            if (nameDraft.trim() && nameDraft !== cycle.name) {
              updateCycleMutation.mutate({ id: cycle.id, project: cycle.project, name: nameDraft.trim() });
            } else {
              setNameDraft(cycle.name);
            }
          }}
          data-testid="cycle-name-input"
        />
        <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-2)' }}>
          <span className="cycle-detail__badge">{statusLabels[cycle.status]}</span>
          {isOverdue && (
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
        <div className="cycle-detail__date-inputs">
          <input
            type="date"
            className="cycle-detail__date-input"
            value={cycle.startDate}
            onChange={(e) => updateCycleMutation.mutate({ id: cycle.id, project: cycle.project, start_date: e.target.value })}
            data-testid="cycle-start-date-input"
          />
          <span>—</span>
          <input
            type="date"
            className="cycle-detail__date-input"
            value={cycle.endDate}
            onChange={(e) => updateCycleMutation.mutate({ id: cycle.id, project: cycle.project, end_date: e.target.value })}
            data-testid="cycle-end-date-input"
          />
        </div>
        <button
          type="button"
          className={`cycle-detail__chart-toggle ${chartOpen ? 'cycle-detail__chart-toggle--active' : ''}`}
          onClick={toggleChart}
          data-testid="burndown-toggle"
        >
          📊 {chartOpen ? t('cycle.burndownHide') : t('cycle.burndownShow')}
        </button>
        {cycle.status === 'active' && (
          <button
            type="button"
            style={{
              padding: 'var(--space-2) var(--space-3)',
              background: 'var(--color-accent-primary)',
              color: 'white',
              border: 'none',
              borderRadius: 'var(--radius-md)',
              cursor: 'pointer',
              fontSize: 'var(--font-size-sm)',
              fontWeight: 'var(--font-weight-semibold)',
            }}
            onClick={handleCompleteClick}
          >
            {t('cycle.complete')}
          </button>
        )}
      </div>

      {/* 説明欄 */}
      <div className="cycle-detail__description">
        <textarea
          className="cycle-detail__description-input"
          placeholder="このサイクルのゴールや注力ドメインを記載..."
          value={descriptionDraft}
          onChange={(e) => setDescriptionDraft(e.target.value)}
          onBlur={() => {
            if (!cycle) return;
            if (descriptionDraft !== (cycle.description || '')) {
              updateCycleMutation.mutate({ id: cycle.id, project: cycle.project, description: descriptionDraft });
            }
          }}
          rows={3}
          data-testid="cycle-description-input"
        />
      </div>

      {/* ミニサマリー */}
      <CycleSummaryBar progress={progress} />

      {/* メイン: チケット一覧 + (開いていれば)バーンダウンパネル or 詳細パネル */}
      <div
        className={`cycle-detail__body ${ticketId ? 'cycle-detail__body--with-ticket-panel' : (chartOpen ? 'cycle-detail__body--with-panel' : '')} ${isResizing || isPanelResizing ? 'cycle-detail__body--resizing' : ''}`}
        style={
          ticketId
            ? { gridTemplateColumns: `1fr ${panelWidth}px` }
            : chartOpen
              ? { gridTemplateColumns: `1fr ${chartWidth}px` }
              : undefined
        }
      >
        <div className="cycle-detail__main">
          {cycleId && <TicketTable cycleId={parseInt(cycleId)} />}
        </div>
        {ticketId && (
          <div className="cycle-detail__ticket-panel">
            <div
              className="cycle-detail__ticket-resize-handle"
              onMouseDown={onPanelResizeStart}
            />
            <TicketDetailPanel
              ticketId={ticketId}
              onClose={handleClosePanel}
            />
          </div>
        )}
        {!ticketId && chartOpen && cycleId && (
          <div className="cycle-detail__chart-panel">
            <div
              className="cycle-detail__chart-resize-handle"
              onMouseDown={onResizeStart}
              data-testid="chart-panel-resize-handle"
            />
            <div className="cycle-detail__chart-panel-header">
              <span>{t('cycle.burndownShow')}</span>
              <button
                type="button"
                className="cycle-detail__chart-panel-close"
                onClick={toggleChart}
                aria-label={t('cycle.burndownHide')}
              >
                ×
              </button>
            </div>
            <BurndownChart cycleId={parseInt(cycleId)} />
          </div>
        )}
      </div>

      {/* 完了確認ダイアログ */}
      {completeDialogOpen && cycle && (
        <div style={{
          position: 'fixed', top: 0, left: 0, right: 0, bottom: 0,
          background: 'rgba(0, 0, 0, 0.5)', display: 'flex', alignItems: 'center', justifyContent: 'center',
          zIndex: 1000,
        }} onClick={() => !isCompleting && setCompleteDialogOpen(false)}>
          <div style={{
            background: 'var(--color-bg-primary)', borderRadius: 'var(--radius-lg)',
            padding: 'var(--space-6)', maxWidth: 400, width: '90%',
            boxShadow: 'var(--shadow-lg)',
          }} onClick={e => e.stopPropagation()}>
            <h2 style={{ marginBottom: 'var(--space-3)', color: 'var(--color-text-primary)' }}>
              {t('cycle.complete')}
            </h2>
            <p style={{ marginBottom: 'var(--space-4)', color: 'var(--color-text-secondary)', fontSize: 'var(--font-size-sm)' }}>
              {cycle.ticketCount - cycle.completedCount} {t('cycle.incompleteCount')} チケットを次の Cycle へ移します。
            </p>
            <div style={{ marginBottom: 'var(--space-4)' }}>
              <label style={{ display: 'block', marginBottom: 'var(--space-2)', color: 'var(--color-text-primary)', fontWeight: 'var(--font-weight-semibold)', fontSize: 'var(--font-size-sm)' }}>
                持ち越し先の選択:
              </label>
              <select
                value={selectedCarryOver === null ? '' : String(selectedCarryOver)}
                onChange={(e) => {
                  const val = e.target.value;
                  setSelectedCarryOver(val === 'create' ? 'create' : val ? parseInt(val) : null);
                }}
                style={{
                  width: '100%', padding: 'var(--space-2) var(--space-3)',
                  background: 'var(--color-bg-elevated)', border: '1px solid var(--color-border-default)',
                  borderRadius: 'var(--radius-md)', color: 'var(--color-text-primary)',
                  fontSize: 'var(--font-size-sm)',
                }}
                disabled={isCompleting}
              >
                <option value="">— 選択してください</option>
                {plannedCycles.map(pc => (
                  <option key={pc.id} value={pc.id}>
                    {pc.name}
                  </option>
                ))}
                <option value="create">📝 次 Cycle を作成して移す</option>
              </select>
            </div>
            <div style={{ display: 'flex', gap: 'var(--space-2)' }}>
              <button
                onClick={() => setCompleteDialogOpen(false)}
                disabled={isCompleting}
                style={{
                  flex: 1, padding: 'var(--space-2) var(--space-3)',
                  background: 'var(--color-bg-tertiary)', color: 'var(--color-text-primary)',
                  border: '1px solid var(--color-border-default)', borderRadius: 'var(--radius-md)',
                  cursor: isCompleting ? 'not-allowed' : 'pointer', fontSize: 'var(--font-size-sm)',
                  opacity: isCompleting ? 0.6 : 1,
                }}
              >
                キャンセル
              </button>
              <button
                onClick={handleConfirmComplete}
                disabled={isCompleting || selectedCarryOver === null}
                style={{
                  flex: 1, padding: 'var(--space-2) var(--space-3)',
                  background: 'var(--color-accent-primary)', color: 'white',
                  border: 'none', borderRadius: 'var(--radius-md)',
                  cursor: (isCompleting || selectedCarryOver === null) ? 'not-allowed' : 'pointer',
                  fontSize: 'var(--font-size-sm)', fontWeight: 'var(--font-weight-semibold)',
                  opacity: (isCompleting || selectedCarryOver === null) ? 0.6 : 1,
                }}
              >
                {isCompleting ? '処理中...' : '確認'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
