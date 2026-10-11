/**
 * InsightsPanels.tsx - Insights の各タブの中身
 *
 * タブごとに、そのタブの API だけを useQuery で取得する(タブを開くまで呼ばない)。
 * 件数 0 のときは空の状態、失敗したときは detail とやり直しボタンを出す。
 */

import { useTranslation } from 'react-i18next';
import { useQuery } from '@tanstack/react-query';
import {
  fetchAiUsage,
  fetchCycleTime,
  fetchThroughput,
  fetchTriageTime,
  insightsErrorDetail,
  type CycleTimeSegment,
  type InsightsFilters,
  type InsightsSegment,
} from '../api';
import {
  formatDuration,
  formatPeriodLabel,
  formatTokens,
  formatUsd,
  percent,
  pivotSeries,
  type I18nDescriptor,
} from '../format';
import { BarChart, type BarChartRow } from './BarChart';

/** 空の状態・読み込み・エラーの表示 */
function PanelState({
  state,
  error,
  onRetry,
}: {
  state: 'loading' | 'error';
  error?: unknown;
  onRetry?: () => void;
}) {
  const { t } = useTranslation();
  if (state === 'loading') {
    return <p className="insights-state">{t('insights.loading')}</p>;
  }
  const detail = insightsErrorDetail(error);
  return (
    <div className="insights-state insights-state--error" role="alert">
      <p>{detail ?? t('insights.error.generic')}</p>
      {onRetry && (
        <button type="button" className="insights-button" onClick={onRetry}>
          {t('insights.error.retry')}
        </button>
      )}
    </div>
  );
}

function EmptyState() {
  const { t } = useTranslation();
  return (
    <div className="insights-state insights-state--empty">
      <h3>{t('insights.empty.title')}</h3>
      <p>{t('insights.empty.body')}</p>
    </div>
  );
}

/** 注記(除外した件数)。件数が 0 の項目は出さない */
function ExcludedNotes({ notes }: { notes: { key: string; n: number }[] }) {
  const { t } = useTranslation();
  const visible = notes.filter((note) => note.n > 0);
  if (visible.length === 0) return null;
  return (
    <ul className="insights-notes">
      {visible.map((note) => (
        <li key={note.key}>{t(note.key, { n: note.n })}</li>
      ))}
    </ul>
  );
}

/** 所要時間の表示 */
function useDurationText() {
  const { t } = useTranslation();
  return (seconds: number | null | undefined): string => {
    const d: I18nDescriptor | null = formatDuration(seconds);
    return d ? t(d.key, d.values) : t('insights.none');
  };
}

function findSegment<T extends { segment: InsightsSegment }>(items: T[], segment: InsightsSegment): T | undefined {
  return items.find((item) => item.segment === segment);
}

// ---------------------------------------------------------------
// サイクルタイム
// ---------------------------------------------------------------

export function CycleTimePanel({ filters }: { filters: InsightsFilters }) {
  const { t } = useTranslation();
  const duration = useDurationText();
  const query = useQuery({
    queryKey: ['insights', 'cycle-time', filters],
    queryFn: () => fetchCycleTime(filters),
    retry: false,
  });

  if (query.isPending) return <PanelState state="loading" />;
  if (query.isError) return <PanelState state="error" error={query.error} onRetry={() => void query.refetch()} />;

  const data = query.data;
  const segments: CycleTimeSegment[] = (['human', 'ai', 'unassigned'] as InsightsSegment[]).map(
    (segment) =>
      findSegment(data.segments, segment) ?? {
        segment,
        count: 0,
        medianSeconds: null,
        meanSeconds: null,
        p90Seconds: null,
      },
  );
  const total = segments.reduce((sum, s) => sum + s.count, 0);
  const human = findSegment(segments, 'human');
  const ai = findSegment(segments, 'ai');
  const ratio =
    human && ai && human.count > 0 && ai.count > 0 ? percent(ai.medianSeconds, human.medianSeconds) : null;

  const notes = [
    { key: 'insights.note.noStart', n: data.excluded.noStart },
    { key: 'insights.note.aiUnfinished', n: data.aiRuns.unfinished },
    { key: 'insights.note.aiError', n: data.aiRuns.error },
  ];

  return (
    <div className="insights-panel">
      <p className="insights-help">{t('insights.cycle.help')}</p>
      {total === 0 ? (
        <EmptyState />
      ) : (
        <>
          <div className="insights-cards">
            {segments.map((s) => (
              <section key={s.segment} className={`insights-card insights-card--${s.segment}`}>
                <h3 className="insights-card__title">
                  <span
                    className={`insights-chart__swatch insights-chart__swatch--${s.segment}`}
                    style={{ background: `var(--insights-${s.segment})` }}
                    aria-hidden="true"
                  />
                  {t(`insights.segment.${s.segment}`)}
                </h3>
                <p className="insights-card__count">{t('insights.cycle.count', { n: s.count })}</p>
                <p className="insights-card__median">
                  <span className="insights-card__label">{t('insights.cycle.median')}</span>
                  <strong>{duration(s.medianSeconds)}</strong>
                </p>
                <p className="insights-card__sub">
                  {t('insights.cycle.p90')}: {duration(s.p90Seconds)}
                </p>
                <p className="insights-card__sub">
                  {t('insights.cycle.mean')}: {duration(s.meanSeconds)}
                </p>
              </section>
            ))}
          </div>
          {ratio != null && <p className="insights-ratio">{t('insights.cycle.ratio', { percent: Math.round(ratio) })}</p>}
        </>
      )}
      <ExcludedNotes notes={notes} />
    </div>
  );
}

// ---------------------------------------------------------------
// トリアージ時間
// ---------------------------------------------------------------

export function TriageTimePanel({ filters }: { filters: InsightsFilters }) {
  const { t } = useTranslation();
  const duration = useDurationText();
  const query = useQuery({
    queryKey: ['insights', 'triage-time', filters],
    queryFn: () => fetchTriageTime(filters),
    retry: false,
  });

  if (query.isPending) return <PanelState state="loading" />;
  if (query.isError) return <PanelState state="error" error={query.error} onRetry={() => void query.refetch()} />;

  const data = query.data;
  const hasData = data.series.some((s) => s.count > 0) || data.waiting.count > 0;
  const pivot = pivotSeries(
    data.series,
    (periodStart, segment) => ({ periodStart, segment, count: 0, medianSeconds: null }),
  );
  const rows: BarChartRow[] = pivot.map((row) => ({
    label: formatPeriodLabel(row.periodStart),
    values: { human: row.cells.human.medianSeconds, ai: row.cells.ai.medianSeconds },
  }));

  return (
    <div className="insights-panel">
      <p className="insights-help">{t('insights.triage.help')}</p>
      {!hasData ? (
        <EmptyState />
      ) : (
        <section className="insights-section">
          <h3 className="insights-section__title">{t('insights.triage.chartTitle')}</h3>
          <BarChart
            ariaLabel={t('insights.triage.chartTitle')}
            rows={rows}
            series={[
              { key: 'human', label: t('insights.segment.human'), color: 'human' },
              { key: 'ai', label: t('insights.segment.ai'), color: 'ai' },
            ]}
            formatValue={(value) => (value == null ? t('insights.none') : duration(value))}
          />
        </section>
      )}
      <section className="insights-section">
        <h3 className="insights-section__title">{t('insights.triage.waiting.title')}</h3>
        <div className="insights-cards">
          <div className="insights-card">
            <p className="insights-card__label">{t('insights.triage.waiting.count')}</p>
            <strong className="insights-card__big">{t('insights.cycle.count', { n: data.waiting.count })}</strong>
          </div>
          <div className="insights-card">
            <p className="insights-card__label">{t('insights.triage.waiting.median')}</p>
            <strong className="insights-card__big">
              {data.waiting.medianAgeDays == null ? t('insights.none') : t('insights.days', { n: data.waiting.medianAgeDays })}
            </strong>
          </div>
          <div className="insights-card">
            <p className="insights-card__label">{t('insights.triage.waiting.max')}</p>
            <strong className="insights-card__big">
              {data.waiting.maxAgeDays == null ? t('insights.none') : t('insights.days', { n: data.waiting.maxAgeDays })}
            </strong>
          </div>
        </div>
      </section>
    </div>
  );
}

// ---------------------------------------------------------------
// 処理総量
// ---------------------------------------------------------------

function roundPoints(value: number | null): string {
  if (value == null) return '-';
  return String(Math.round(value * 10) / 10);
}

export function ThroughputPanel({ filters }: { filters: InsightsFilters }) {
  const { t } = useTranslation();
  const query = useQuery({
    queryKey: ['insights', 'throughput', filters],
    queryFn: () => fetchThroughput(filters),
    retry: false,
  });

  if (query.isPending) return <PanelState state="loading" />;
  if (query.isError) return <PanelState state="error" error={query.error} onRetry={() => void query.refetch()} />;

  const data = query.data;
  const hasData = data.series.some((s) => s.tickets > 0 || s.points > 0);
  const pivot = pivotSeries(data.series, (periodStart, segment) => ({
    periodStart,
    segment,
    tickets: 0,
    points: 0,
  }));
  const series = [
    { key: 'human', label: t('insights.segment.human'), color: 'human' as const },
    { key: 'ai', label: t('insights.segment.ai'), color: 'ai' as const },
    { key: 'unassigned', label: t('insights.segment.unassigned'), color: 'unassigned' as const },
  ];
  const ticketRows: BarChartRow[] = pivot.map((row) => ({
    label: formatPeriodLabel(row.periodStart),
    values: { human: row.cells.human.tickets, ai: row.cells.ai.tickets, unassigned: row.cells.unassigned.tickets },
  }));
  const pointRows: BarChartRow[] = pivot.map((row) => ({
    label: formatPeriodLabel(row.periodStart),
    values: { human: row.cells.human.points, ai: row.cells.ai.points, unassigned: row.cells.unassigned.points },
  }));

  const shareText = (value: number | null) => (value == null ? t('insights.none') : `${value}%`);

  return (
    <div className="insights-panel">
      <p className="insights-help">{t('insights.throughput.help')}</p>
      <div className="insights-cards">
        <div className="insights-card">
          <p className="insights-card__label">{t('insights.throughput.aiShareTickets')}</p>
          <strong className="insights-card__big">{shareText(data.aiShare.ticketsPercent)}</strong>
        </div>
        <div className="insights-card">
          <p className="insights-card__label">{t('insights.throughput.aiSharePoints')}</p>
          <strong className="insights-card__big">{shareText(data.aiShare.pointsPercent)}</strong>
        </div>
      </div>
      {!hasData ? (
        <EmptyState />
      ) : (
        <>
          <section className="insights-section">
            <h3 className="insights-section__title">{t('insights.throughput.tickets')}</h3>
            <BarChart
              ariaLabel={t('insights.throughput.tickets')}
              rows={ticketRows}
              series={series}
              stacked
              formatValue={(value) => (value == null ? '-' : String(value))}
            />
          </section>
          <section className="insights-section">
            <h3 className="insights-section__title">{t('insights.throughput.points')}</h3>
            <BarChart
              ariaLabel={t('insights.throughput.points')}
              rows={pointRows}
              series={series}
              stacked
              formatValue={roundPoints}
            />
          </section>
        </>
      )}
    </div>
  );
}

// ---------------------------------------------------------------
// AI 使用量
// ---------------------------------------------------------------

export function AiUsagePanel({ filters }: { filters: InsightsFilters }) {
  const { t } = useTranslation();
  const query = useQuery({
    queryKey: ['insights', 'ai-usage', filters],
    queryFn: () => fetchAiUsage(filters),
    retry: false,
  });

  if (query.isPending) return <PanelState state="loading" />;
  if (query.isError) return <PanelState state="error" error={query.error} onRetry={() => void query.refetch()} />;

  const data = query.data;
  const { totals } = data;
  const tokenOrDash = (value: number | null | undefined) => formatTokens(value) ?? t('insights.none');
  const tokenRows: { key: string; label: string; value: number }[] = [
    { key: 'input', label: t('insights.aiUsage.input'), value: totals.inputTokens },
    { key: 'output', label: t('insights.aiUsage.output'), value: totals.outputTokens },
    { key: 'cacheCreation', label: t('insights.aiUsage.cacheCreation'), value: totals.cacheCreationTokens },
    { key: 'cacheRead', label: t('insights.aiUsage.cacheRead'), value: totals.cacheReadTokens },
  ];
  const chartRows: BarChartRow[] = data.series.map((point) => ({
    label: formatPeriodLabel(point.periodStart),
    values: { weighted: point.weightedTokens },
  }));

  return (
    <div className="insights-panel">
      <p className="insights-help">{t('insights.aiUsage.help')}</p>
      {totals.runs === 0 ? (
        <EmptyState />
      ) : (
        <>
          <div className="insights-cards">
            <div className="insights-card">
              <p className="insights-card__label">{t('insights.aiUsage.runs')}</p>
              <strong className="insights-card__big">{totals.runs}</strong>
            </div>
            <div className="insights-card">
              <p className="insights-card__label">{t('insights.aiUsage.tickets')}</p>
              <strong className="insights-card__big">{totals.tickets}</strong>
            </div>
            <div className="insights-card">
              <p className="insights-card__label">{t('insights.aiUsage.weighted')}</p>
              <strong className="insights-card__big">{tokenOrDash(totals.weightedTokens)}</strong>
            </div>
            <div className="insights-card">
              <p className="insights-card__label">{t('insights.aiUsage.perRun')}</p>
              <strong className="insights-card__big">{tokenOrDash(totals.avgWeightedPerRun)}</strong>
            </div>
            <div className="insights-card">
              <p className="insights-card__label">{t('insights.aiUsage.perTicket')}</p>
              <strong className="insights-card__big">{tokenOrDash(totals.avgWeightedPerTicket)}</strong>
            </div>
            <div className="insights-card">
              <p className="insights-card__label">{t('insights.aiUsage.usd')}</p>
              <strong className="insights-card__big">{formatUsd(totals.usdEquivalent) ?? t('insights.none')}</strong>
              <p className="insights-card__sub">{t('insights.usd.note')}</p>
            </div>
          </div>

          <section className="insights-section">
            <h3 className="insights-section__title">{t('insights.aiUsage.tokens')}</h3>
            <ul className="insights-token-list">
              {tokenRows.map((row) => (
                <li key={row.key}>
                  <span>{row.label}</span>
                  <strong>{tokenOrDash(row.value)}</strong>
                </li>
              ))}
            </ul>
          </section>

          <section className="insights-section">
            <h3 className="insights-section__title">{t('insights.aiUsage.seriesTitle')}</h3>
            <BarChart
              ariaLabel={t('insights.aiUsage.seriesTitle')}
              rows={chartRows}
              series={[{ key: 'weighted', label: t('insights.aiUsage.weighted'), color: 'ai' }]}
              formatValue={(value) => tokenOrDash(value)}
            />
          </section>
        </>
      )}
      <ExcludedNotes
        notes={[
          { key: 'insights.note.usageUnfinished', n: data.excluded.unfinished },
          { key: 'insights.note.noTokens', n: data.excluded.noTokens },
        ]}
      />
    </div>
  );
}
