/**
 * useProjects.ts — プロジェクト作成 Mutation Hook
 *
 * サーバー直（入力エラーをその場で出すため）。成功後に端末内 DB へ書き込む。
 */

import { useMutation } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { writeThroughProject } from '@/shared/sync/projectWrites';
import type { Project } from '@/shared/hooks/useProject';

export interface ProjectCreateData {
  name: string;
  prefix: string;
  description: string;
  priority: string;
  teamIds: number[];
}

/** プロジェクト作成 */
export function useCreateProject() {
  return useMutation({
    mutationFn: async (data: ProjectCreateData) => {
      const res = await apiClient.post<Project>('/projects/', data);
      await writeThroughProject(res.data as unknown as Record<string, unknown>);
      return res.data;
    },
  });
}
