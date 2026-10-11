/**
 * InsightsPage.tsx - Insights 画面(人と AI の作業の速さ・AI 使用量)
 *
 * 上で条件(期間・区切り・チケット種別・プロジェクト)を選び、下の 4 つのタブで見る。
 * タブを開いたときに、そのタブの API だけを呼ぶ。
 */

import '../i18n';
import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useProjects } from '@/shared/sync/repos/projectRepo';
import type { InsightsFilters, InsightsGranularity } from '../api';
import { shiftIsoDate, toIsoDate } from '../format';
import { AiUsagePanel, CycleTimePanel, ThroughputPanel, TriageTimePanel } from './InsightsPanels';
import './InsightsPage.css';

type InsightsTab = 'cycle' | 'triage' | 'throughput' | 'aiUsage';

const TABS: InsightsTab[] = ['cycle', 'triage', 'throughput', 'aiUsage'];
const PERIOD_OPTIONS = [30, 90, 180, 365];
const GRANULARITY_OPTIONS: InsightsGranularity[] = ['week', 'month'];
/** 空文字は「すべて」 */
const TICKET_TYPE_OPTIONS = ['', 'bug', 'task', 'feature'];

export function InsightsPage() {
  const { t } = useTranslation();
  const { projects } = useProjects();

  const [periodDays, setPeriodDays] = useState(90);
  const [granularity, setGranularity] = useState<InsightsGranularity>('week');
  const [ticketType, setTicketType] = useState('');
  const [projectId, setProjectId] = useState<number | null>(null);
  const [tab, setTab] = useState<InsightsTab>('cycle');

  // 期間は「今日」まで。開始は今日から periodDays 日前(API の既定と同じ 90 日を初期値にする)
  const filters: InsightsFilters = useMemo(() => {
    const to = toIsoDate(new Date());
    return {
      from: shiftIsoDate(to, -periodDays),
      to,
      projectId,
      ticketType,
      granularity,
    };
  }, [periodDays, projectId, ticketType, granularity]);

  return (
    <div className="insights-page">
      <header className="insights-header">
        <h1 className="insights-title">{t('insights.title')}</h1>
        <p className="insights-description">{t('insights.description')}</p>
      </header>

      <div className="insights-filters">
        <div className="insights-filter">
          <span className="insights-filter__label" id="insights-period-label">
            {t('insights.filter.period')}
          </span>
          <div className="insights-segmented" role="group" aria-labelledby="insights-period-label">
            {PERIOD_OPTIONS.map((days) => (
              <button
                key={days}
                type="button"
                className="insights-segmented__item"
                aria-pressed={periodDays === days}
                onClick={() => setPeriodDays(days)}
              >
                {t('insights.period.days', { n: days })}
              </button>
            ))}
          </div>
        </div>

        <div className="insights-filter">
          <span className="insights-filter__label" id="insights-granularity-label">
            {t('insights.filter.granularity')}
          </span>
          <div className="insights-segmented" role="group" aria-labelledby="insights-granularity-label">
            {GRANULARITY_OPTIONS.map((option) => (
              <button
                key={option}
                type="button"
                className="insights-segmented__item"
                aria-pressed={granularity === option}
                onClick={() => setGranularity(option)}
              >
                {t(`insights.granularity.${option}`)}
              </button>
            ))}
          </div>
        </div>

        <div className="insights-filter">
          <label className="insights-filter__label" htmlFor="insights-ticket-type">
            {t('insights.filter.ticketType')}
          </label>
          <select
            id="insights-ticket-type"
            className="insights-select"
            value={ticketType}
            onChange={(e) => setTicketType(e.target.value)}
          >
            {TICKET_TYPE_OPTIONS.map((option) => (
              <option key={option || 'all'} value={option}>
                {t(`insights.ticketType.${option || 'all'}`)}
              </option>
            ))}
          </select>
        </div>

        <div className="insights-filter">
          <label className="insights-filter__label" htmlFor="insights-project">
            {t('insights.filter.project')}
          </label>
          <select
            id="insights-project"
            className="insights-select"
            value={projectId ?? ''}
            onChange={(e) => setProjectId(e.target.value === '' ? null : Number(e.target.value))}
          >
            <option value="">{t('insights.filter.allProjects')}</option>
            {projects.map((project) => (
              <option key={project.id} value={project.id}>
                {project.name}
              </option>
            ))}
          </select>
        </div>
      </div>

      <div className="insights-tabs" role="tablist" aria-label={t('insights.title')}>
        {TABS.map((key) => (
          <button
            key={key}
            type="button"
            role="tab"
            id={`insights-tab-${key}`}
            aria-selected={tab === key}
            aria-controls="insights-tabpanel"
            className={`insights-tabs__tab${tab === key ? ' insights-tabs__tab--active' : ''}`}
            onClick={() => setTab(key)}
          >
            {t(`insights.tab.${key}`)}
          </button>
        ))}
      </div>

      <div className="insights-tabpanel" id="insights-tabpanel" role="tabpanel" aria-labelledby={`insights-tab-${tab}`}>
        {tab === 'cycle' && <CycleTimePanel filters={filters} />}
        {tab === 'triage' && <TriageTimePanel filters={filters} />}
        {tab === 'throughput' && <ThroughputPanel filters={filters} />}
        {tab === 'aiUsage' && <AiUsagePanel filters={filters} />}
      </div>
    </div>
  );
}
