/**
 * statusOptions.ts — チケットのステータス表示名と選択肢の共通化
 *
 * ステータスの表示名は画面ごとにバラバラだった（一覧は日本語の固定訳、詳細は設定名、
 * プロパティ欄は内部値 "open"、作成フォームは固定の4種類）。
 * 表示名は「ワークフロー設定の名前（英語）」に統一する。設定が取れないときは標準の英語名。
 */
import type { WorkflowStatus } from '@/shared/api/types';

export interface StatusOption {
  value: string;
  label: string;
  color: string;
}

/** 標準ワークフロー（Rust の team_repo / resource_repo の既定セットと同じ名前） */
export const DEFAULT_STATUS_OPTIONS: readonly StatusOption[] = [
  { value: 'backlog', label: 'Backlog', color: 'var(--color-status-backlog, #6b7280)' },
  { value: 'open', label: 'Todo', color: 'var(--color-status-open)' },
  { value: 'in_progress', label: 'In Progress', color: 'var(--color-status-in-progress)' },
  { value: 'resolved', label: 'Resolved', color: 'var(--color-status-resolved)' },
  { value: 'closed', label: 'Closed', color: 'var(--color-status-closed)' },
  { value: 'canceled', label: 'Cancelled', color: 'var(--color-status-canceled, #9ca3af)' },
];

/** ワークフロー設定があればその並び・名前、なければ標準の選択肢 */
export function buildStatusOptions(workflowStatuses: readonly Pick<WorkflowStatus, 'slug' | 'name' | 'color'>[] | undefined): StatusOption[] {
  if (workflowStatuses && workflowStatuses.length > 0) {
    return workflowStatuses.map((s) => ({
      value: s.slug,
      label: s.name || defaultLabel(s.slug),
      color: s.color || defaultColor(s.slug),
    }));
  }
  return [...DEFAULT_STATUS_OPTIONS];
}

/**
 * 複数プロジェクトの選択肢の和集合。value で重複排除（最初の出現の名前・色を採用）。
 * 標準の slug を標準の順で先に、その他は初出順。すべて空なら標準の選択肢。
 */
export function mergeStatusOptions(lists: readonly (readonly StatusOption[])[]): StatusOption[] {
  const seen = new Map<string, StatusOption>();
  for (const list of lists) {
    for (const o of list) {
      if (!seen.has(o.value)) seen.set(o.value, o);
    }
  }
  if (seen.size === 0) return [...DEFAULT_STATUS_OPTIONS];
  const result: StatusOption[] = [];
  for (const d of DEFAULT_STATUS_OPTIONS) {
    const o = seen.get(d.value);
    if (o) {
      result.push(o);
      seen.delete(d.value);
    }
  }
  result.push(...seen.values());
  return result;
}

/** 内部値（slug）→ 表示名。選択肢に無い値は標準名、それも無ければ内部値のまま */
export function statusLabelOf(slug: string | null | undefined, options: readonly StatusOption[]): string {
  if (!slug) return '—';
  return options.find((o) => o.value === slug)?.label ?? defaultLabel(slug);
}

function defaultLabel(slug: string): string {
  return DEFAULT_STATUS_OPTIONS.find((o) => o.value === slug)?.label ?? slug;
}

function defaultColor(slug: string): string {
  return DEFAULT_STATUS_OPTIONS.find((o) => o.value === slug)?.color ?? 'inherit';
}
