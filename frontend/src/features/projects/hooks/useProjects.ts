/**
 * useProjects.ts — プロジェクト作成 Mutation Hook
 *
 * プロジェクト作成は localCreateProject() を使用すること（§6.4）。
 * このフックは後方互換性のためのみ。
 */

import { useMutation } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import type { Project } from '@/shared/hooks/useProject';

export interface ProjectCreateData {
  name: string;
  prefix: string;
  description: string;
  priority: string;
  teamIds: number[];
}

/** プロジェクト作成（非推奨：localCreateProject() を使用してください） */
export function useCreateProject() {
  return useMutation({
    mutationFn: async (data: ProjectCreateData) => {
      const res = await apiClient.post<Project>('/projects/', data);
      return res.data;
    },
    // invalidateQueries は不要（端末内 DB が自動更新される）
  });
}
