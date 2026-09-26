/**
 * useStatusOptions.ts — チケットのステータス選択肢（ワークフロー設定の名前で統一）
 *
 * プロジェクト付きチケットはプロジェクトのワークフロー、チームのみのチケットはチームのワークフローを使う。
 */
import { useMemo } from 'react';
import { useWorkflowStatuses } from '@/features/settings/hooks/useWorkflowStatuses';
import { buildStatusOptions, statusLabelOf, type StatusOption } from '../utils/statusOptions';

export function useStatusOptions(projectId?: number | null, teamId?: number | null): {
  options: StatusOption[];
  labelOf: (slug: string | null | undefined) => string;
} {
  const { data } = useWorkflowStatuses(projectId ?? undefined, projectId ? undefined : (teamId ?? undefined));
  return useMemo(() => {
    const options = buildStatusOptions(data);
    return { options, labelOf: (slug) => statusLabelOf(slug, options) };
  }, [data]);
}
