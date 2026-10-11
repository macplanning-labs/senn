/**
 * ai-kick/format.ts - AI Kick フォーマット・ユーティリティ
 *
 * 純粋関数のみ。UI・翻訳キーのマッピング
 */

import type { AiKickRun } from './api';

/**
 * トークン数をフォーマットする
 *
 * 1,234,567 → "1.2M"
 * 12,345 → "12k"
 * 999 → "999"
 *
 * @param n - トークン数
 * @returns フォーマット済み文字列
 */
export function formatTokens(n: number): string {
  if (n >= 1_000_000) {
    return (n / 1_000_000).toFixed(1).replace(/\.0+$/, '') + 'M';
  }
  if (n >= 1_000) {
    return (n / 1_000).toFixed(0) + 'k';
  }
  return n.toString();
}

/**
 * AI Kick 実行の状態ラベル翻訳キーを取得する
 *
 * @param run - 実行
 * @returns i18n キー
 */
export function statusLabelKey(run: AiKickRun): string {
  if (run.status === 'queued') {
    return 'aiKick.status.queued';
  }
  if (run.status === 'running') {
    return 'aiKick.status.running';
  }
  if (run.status === 'stopping') {
    return 'aiKick.status.stopping';
  }
  if (run.status === 'finished') {
    if (run.result === 'resolved') {
      return 'aiKick.status.resolved';
    }
    if (run.result === 'blocked') {
      return 'aiKick.status.blocked';
    }
    if (run.result === 'aborted') {
      return 'aiKick.status.aborted';
    }
    if (run.result === 'error') {
      return 'aiKick.status.error';
    }
  }
  return 'aiKick.status.unknown';
}

/**
 * AI Kick 実行がアクティブ（実行中・待機中）かどうか
 *
 * @param run - 実行
 * @returns アクティブならtrue
 */
export function isActive(run: AiKickRun): boolean {
  return run.status === 'queued' || run.status === 'running' || run.status === 'stopping';
}

/**
 * 優先度クラスからCSSクラス名を取得する
 *
 * @param priorityClass - 優先度クラス
 * @returns CSS クラス名
 */
export function priorityBadgeClass(priorityClass: string): string {
  switch (priorityClass) {
    case 'emergency':
      return 'ai-kick__priority-badge ai-kick__priority-badge--emergency';
    case 'high':
      return 'ai-kick__priority-badge ai-kick__priority-badge--high';
    case 'normal':
      return 'ai-kick__priority-badge ai-kick__priority-badge--normal';
    case 'low':
      return 'ai-kick__priority-badge ai-kick__priority-badge--low';
    default:
      return 'ai-kick__priority-badge';
  }
}

/**
 * AI Kick ボタンを表示すべきかどうか
 *
 * @param options - 表示条件
 * @returns 表示すべきなら true
 */
export function canShowAiKick(options: {
  projectKick?: { enabled?: boolean; systemEnabled?: boolean; systemKilled?: boolean };
  isGuest?: boolean;
}): boolean {
  if (options.isGuest) {
    return false;
  }
  if (!options.projectKick?.enabled) {
    return false;
  }
  if (!options.projectKick?.systemEnabled) {
    return false;
  }
  return true;
}

/**
 * AI Kick 実行の結果ラベル翻訳キーを取得する
 *
 * @param run - 実行
 * @returns i18n キー
 */
export function runResultLabelKey(run: AiKickRun): string {
  if (run.result === 'resolved') {
    return 'aiKick.result.resolved';
  }
  if (run.result === 'blocked') {
    return 'aiKick.result.blocked';
  }
  if (run.result === 'aborted') {
    return 'aiKick.result.aborted';
  }
  if (run.result === 'error') {
    return 'aiKick.result.error';
  }
  return 'aiKick.result.unknown';
}

/**
 * AI Kick 実行停止理由の翻訳キーを取得する
 *
 * @param stopReason - 停止理由
 * @returns i18n キー
 */
export function stopReasonLabelKey(stopReason: string | null | undefined): string {
  if (!stopReason) {
    return '';
  }
  switch (stopReason) {
    case 'per_run_tokens':
      return 'aiKick.stopReason.perRunTokens';
    case 'per_run_time':
      return 'aiKick.stopReason.perRunTime';
    case 'burn_rate':
      return 'aiKick.stopReason.burnRate';
    case 'killed':
      return 'aiKick.stopReason.killed';
    case 'user_cancel':
      return 'aiKick.stopReason.userCancel';
    case 'budget_ceiling':
      return 'aiKick.stopReason.budgetCeiling';
    case 'heartbeat_lost':
    case 'unclaimed':
    case 'expired':
      return 'aiKick.stopReason.runnerLost';
    case 'project_disabled':
      return 'aiKick.stopReason.projectDisabled';
    case 'runner_revoked':
      return 'aiKick.stopReason.runnerRevoked';
    default:
      return 'aiKick.stopReason.unknown';
  }
}
