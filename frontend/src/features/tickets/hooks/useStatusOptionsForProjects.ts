/**
 * useStatusOptionsForProjects.ts — 複数プロジェクトのステータス選択肢（一覧画面用）
 *
 * プロジェクトごとに useWorkflowStatuses と同じクエリ（同じキャッシュ）で取得し、
 * 和集合・プロジェクト別の選択肢・表示名・色を返す。
 */
import { useMemo } from 'react';
import { useQueries } from '@tanstack/react-query';
import type { WorkflowStatus } from '@/shared/api/types';
import { fetchWorkflowStatuses, workflowStatusesQueryKey } from '@/features/settings/hooks/useWorkflowStatuses';
import { buildStatusOptions, mergeStatusOptions, statusLabelOf, type StatusOption } from '../utils/statusOptions';

export interface StatusOptionsForProjects {
  union: StatusOption[];
  optionsFor: (projectId: number | null | undefined) => StatusOption[];
  labelOf: (slug: string | null | undefined, projectId?: number | null) => string;
  colorOf: (slug: string | null | undefined, projectId?: number | null) => string;
}

export function useStatusOptionsForProjects(projectIds: readonly number[]): StatusOptionsForProjects {
  const ids = useMemo(() => [...new Set(projectIds)].sort((a, b) => a - b), [projectIds]);
  const results = useQueries({
    queries: ids.map((id) => ({
      queryKey: workflowStatusesQueryKey(id),
      queryFn: () => fetchWorkflowStatuses(id),
    })),
  });
  const datas = results.map((r) => r.data as WorkflowStatus[] | undefined);
  // 取得結果が変わったときだけ作り直す
  const signature = ids.map((id, i) => `${id}:${datas[i]?.map((s) => `${s.slug}|${s.name}|${s.color}`).join(',') ?? ''}`).join(';');

  return useMemo(() => {
    const byProject = new Map<number, StatusOption[]>();
    ids.forEach((id, i) => byProject.set(id, buildStatusOptions(datas[i])));
    const defaults = buildStatusOptions(undefined);
    const optionsFor = (projectId: number | null | undefined) =>
      (projectId != null ? byProject.get(projectId) : undefined) ?? defaults;
    const loaded = ids.filter((_, i) => datas[i] && datas[i]!.length > 0).map((id) => byProject.get(id)!);
    const union = mergeStatusOptions(loaded);
    return {
      union,
      optionsFor,
      labelOf: (slug, projectId) => statusLabelOf(slug, optionsFor(projectId)),
      colorOf: (slug, projectId) => optionsFor(projectId).find((o) => o.value === slug)?.color ?? 'inherit',
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [signature]);
}
