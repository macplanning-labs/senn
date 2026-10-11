/**
 * insights/api.ts - Insights API クライアント
 *
 * すべて GET。クエリのキーは snake_case(バックエンドの仕様に合わせる)。
 */

import { apiClient } from '@/shared/api/client';
import type { AxiosError } from 'axios';

/** 担当の区分(人・AI・未割当) */
export type InsightsSegment = 'ai' | 'human' | 'unassigned';

/** 区切り(週・月) */
export type InsightsGranularity = 'week' | 'month';

/** 画面で選んだ条件 */
export interface InsightsFilters {
  /** YYYY-MM-DD */
  from: string;
  /** YYYY-MM-DD */
  to: string;
  projectId: number | null;
  /** 空文字はすべて */
  ticketType: string;
  granularity: InsightsGranularity;
}

/** 条件を、API のクエリ(snake_case)に変換する */
export function toInsightsParams(filters: InsightsFilters): Record<string, string | number> {
  const params: Record<string, string | number> = {
    from: filters.from,
    to: filters.to,
    granularity: filters.granularity,
  };
  if (filters.projectId != null) params.project_id = filters.projectId;
  if (filters.ticketType) params.ticket_type = filters.ticketType;
  return params;
}

// ---- サイクルタイム ----

export interface CycleTimeSegment {
  segment: InsightsSegment;
  count: number;
  medianSeconds: number | null;
  meanSeconds: number | null;
  p90Seconds: number | null;
}

export interface CycleTimeResponse {
  from: string;
  to: string;
  segments: CycleTimeSegment[];
  excluded: { noStart: number };
  aiRuns: { unfinished: number; error: number };
}

// ---- トリアージ時間 ----

export interface TriageSeriesPoint {
  periodStart: string;
  segment: InsightsSegment;
  count: number;
  medianSeconds: number | null;
}

export interface TriageTimeResponse {
  granularity: InsightsGranularity;
  series: TriageSeriesPoint[];
  waiting: {
    count: number;
    medianAgeDays: number | null;
    maxAgeDays: number | null;
  };
}

// ---- 処理総量 ----

export interface ThroughputPoint {
  periodStart: string;
  segment: InsightsSegment;
  tickets: number;
  points: number;
}

export interface ThroughputResponse {
  granularity: InsightsGranularity;
  series: ThroughputPoint[];
  aiShare: {
    ticketsPercent: number | null;
    pointsPercent: number | null;
  };
}

// ---- AI 使用量 ----

export interface AiUsageTotals {
  runs: number;
  tickets: number;
  inputTokens: number;
  outputTokens: number;
  cacheCreationTokens: number;
  cacheReadTokens: number;
  weightedTokens: number;
  /** API 料金での換算額(請求額ではない) */
  usdEquivalent: number | null;
  avgWeightedPerRun: number | null;
  avgWeightedPerTicket: number | null;
}

export interface AiUsageSeriesPoint {
  periodStart: string;
  runs: number;
  inputTokens: number;
  outputTokens: number;
  cacheCreationTokens: number;
  cacheReadTokens: number;
  weightedTokens: number;
}

export interface AiUsageResponse {
  from: string;
  to: string;
  granularity: InsightsGranularity;
  totals: AiUsageTotals;
  series: AiUsageSeriesPoint[];
  excluded: { unfinished: number; noTokens: number };
}

async function getInsights<T>(path: string, filters: InsightsFilters): Promise<T> {
  const res = await apiClient.get<T>(`/insights/${path}/`, { params: toInsightsParams(filters) });
  return res.data;
}

export function fetchCycleTime(filters: InsightsFilters): Promise<CycleTimeResponse> {
  return getInsights<CycleTimeResponse>('cycle-time', filters);
}

export function fetchTriageTime(filters: InsightsFilters): Promise<TriageTimeResponse> {
  return getInsights<TriageTimeResponse>('triage-time', filters);
}

export function fetchThroughput(filters: InsightsFilters): Promise<ThroughputResponse> {
  return getInsights<ThroughputResponse>('throughput', filters);
}

export function fetchAiUsage(filters: InsightsFilters): Promise<AiUsageResponse> {
  return getInsights<AiUsageResponse>('ai-usage', filters);
}

/**
 * API エラーの detail(日本語の文言)を取り出す。無ければ null。
 */
export function insightsErrorDetail(error: unknown): string | null {
  const detail = (error as AxiosError<{ detail?: unknown }> | undefined)?.response?.data?.detail;
  return typeof detail === 'string' && detail.length > 0 ? detail : null;
}
