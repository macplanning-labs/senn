/**
 * ai-kick/api.ts - AI Kick API クライアント
 *
 * AI対応機能のAPI呼び出しと型定義
 */

import { apiClient } from '@/shared/api/client';
import type { AxiosError } from 'axios';
import i18n from 'i18next';

/**
 * AI Kick の警告コード
 */
export type AiKickWarningCode = 'budget_over' | 'budget_near' | 'emergency' | 'external_content';

/**
 * AI Kick の警告
 */
export interface AiKickWarning {
  code: AiKickWarningCode;
  message: string;
}

/**
 * AI Kick プレビューレスポンス
 */
export interface AiKickPreview {
  verdict: 'allow' | 'needs_confirm' | 'deny';
  warnings: AiKickWarning[];
  denied?: {
    code: string;
    message: string;
  };
  ticketKey: string;
  prompt: string;
  promptHash: string;
  priority: number;
  priorityClass: 'emergency' | 'high' | 'normal' | 'low';
  priorityLabel: string;
  blocksOpen: number;
  caps: {
    maxTurns: number;
    maxMinutes: number;
    maxWeightedTokens: number;
  } | null;
  repo: string;
  baseBranch: string;
  externalContent: boolean;
}

/**
 * AI Kick 実行リクエスト
 */
export interface AiKickRequest {
  promptHash: string;
  confirmed: boolean;
}

/**
 * AI Kick 実行レスポンス
 */
export interface AiKickRunResponse {
  runUuid: string;
  status: 'queued' | 'running' | 'stopping' | 'finished';
  priorityClass: 'emergency' | 'high' | 'normal' | 'low';
}

/**
 * AI Kick 実行結果
 */
export type AiKickRunResult = 'resolved' | 'blocked' | 'aborted' | 'error' | null;

/**
 * AI Kick 実行記録
 */
export interface AiKickRun {
  runUuid: string;
  status: 'queued' | 'running' | 'stopping' | 'finished';
  triggerType: string;
  priorityClass: 'emergency' | 'high' | 'normal' | 'low';
  queuedAt: string;
  startedAt: string | null;
  endedAt: string | null;
  result: AiKickRunResult;
  summary: string | null;
  weightedTokens: number;
  stopReason: string | null;
  errorDetail: string | null;
  /** 起動した人(表示名)。不明・未設定なら null */
  triggeredBy: { id: number; name: string } | null;
}

/**
 * AI Kick 実行一覧レスポンス
 */
export interface AiKickRunsResponse {
  runs: AiKickRun[];
}

/**
 * プロジェクトの AI Kick 設定
 */
export interface ProjectAiKick {
  enabled: boolean;
  repo: string;
  baseBranch: string;
  canEdit: boolean;
  systemEnabled: boolean;
  systemKilled: boolean;
}

/**
 * AI Kick エラーパース結果
 */
export interface ParsedAiKickError {
  status: number;
  code?: string;
  detail?: string;
  warnings?: AiKickWarning[];
}

/**
 * プロジェクトの AI Kick 設定を取得する
 *
 * @param projectId - プロジェクトID
 * @returns AI Kick 設定
 */
export async function getProjectAiKick(projectId: number): Promise<ProjectAiKick> {
  const response = await apiClient.get<ProjectAiKick>(
    `/projects/${projectId}/ai-kick/`,
  );
  return response.data;
}

/**
 * AI Kick プレビューを取得する
 *
 * @param ticketKey - チケットキー
 * @returns プレビュー
 */
export async function getAiKickPreview(ticketKey: string): Promise<AiKickPreview> {
  const response = await apiClient.post<AiKickPreview>(
    `/tickets/${ticketKey}/ai-kick/preview/`,
    undefined,
    // 生成に時間がかかるため長めにタイムアウト
    { timeout: 120_000 },
  );
  return response.data;
}

/**
 * AI Kick を実行する
 *
 * @param ticketKey - チケットキー
 * @param request - リクエスト（promptHash と confirmed フラグ）
 * @returns 実行レスポンス
 */
export async function kickAi(ticketKey: string, request: AiKickRequest): Promise<AiKickRunResponse> {
  const response = await apiClient.post<AiKickRunResponse>(
    `/tickets/${ticketKey}/ai-kick/`,
    request,
  );
  return response.data;
}

/**
 * AI Kick の実行を取り消す
 *
 * @param ticketKey - チケットキー
 * @param runUuid - 実行UUID
 * @returns 実行ステータス
 */
export async function cancelAiKickRun(ticketKey: string, runUuid: string): Promise<{ status: string }> {
  const response = await apiClient.post<{ status: string }>(
    `/tickets/${ticketKey}/ai-kick/${runUuid}/cancel/`,
  );
  return response.data;
}

/**
 * AI Kick の実行一覧を取得する
 *
 * @param ticketKey - チケットキー
 * @returns 実行一覧
 */
export async function getAiKickRuns(ticketKey: string): Promise<AiKickRunsResponse> {
  const response = await apiClient.get<AiKickRunsResponse>(
    `/tickets/${ticketKey}/ai-runs/`,
  );
  return response.data;
}

/**
 * Axios エラーから AI Kick エラー情報をパースする
 *
 * @param error - エラーオブジェクト
 * @returns パース済みエラー情報
 */
export function parseAiKickError(error: unknown): ParsedAiKickError {
  const axiosError = error as AxiosError<Record<string, any>>;

  if (!axiosError.response) {
    // ネットワークエラー
    return {
      status: 0,
      detail: i18n.t('aiKick.dialog.error.network'),
    };
  }

  const { status, data } = axiosError.response;
  const parsedError: ParsedAiKickError = {
    status,
    code: data?.code,
    detail: data?.detail,
    warnings: data?.warnings,
  };

  return parsedError;
}
