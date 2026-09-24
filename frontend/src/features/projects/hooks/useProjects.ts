/**
 * useProjects.ts — プロジェクト作成 Mutation Hook
 */

import { useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
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
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (data: ProjectCreateData) => {
      const res = await apiClient.post<Project>('/projects/', data);
      return res.data;
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['projects'] });
    },
  });
}
