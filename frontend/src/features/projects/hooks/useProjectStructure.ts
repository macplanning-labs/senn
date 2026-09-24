/**
 * useProjectStructure.ts — プロジェクトの構造(親子・関連・ロードマップ)の取得・変更
 *
 * API: GET /projects/{id}/structure/
 *      PATCH /projects/{id}/ (parentProjectId)
 *      POST/DELETE /projects/{id}/relations/…
 *      GET/POST /roadmaps/… POST/DELETE /roadmaps/{id}/projects/…
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import type { ProjectStructure, Roadmap, RoadmapDetail } from '@/shared/api/types';
import { projectActivityKeys } from './useProjectActivity';

export const projectStructureKey = (projectId: number) => ['project-structure', projectId] as const;
export const roadmapsKey = () => ['roadmaps'] as const;
export const roadmapKey = (roadmapId: number) => ['roadmap', roadmapId] as const;

export function useProjectStructure(projectId: number | undefined) {
  return useQuery({
    queryKey: projectStructureKey(projectId ?? 0),
    enabled: !!projectId,
    staleTime: 0,
    queryFn: async () => {
      const res = await apiClient.get<ProjectStructure>(`/projects/${projectId}/structure/`);
      return res.data;
    },
  });
}

function useInvalidateAfterStructureChange(projectId: number) {
  const queryClient = useQueryClient();
  return () => {
    void queryClient.invalidateQueries({ queryKey: projectStructureKey(projectId) });
    void queryClient.invalidateQueries({ queryKey: ['projects'] });
    void queryClient.invalidateQueries({ queryKey: projectActivityKeys.activity(projectId) });
  };
}

export function useSetParent(projectId: number) {
  const invalidate = useInvalidateAfterStructureChange(projectId);
  return useMutation({
    mutationFn: async (parentProjectId: number | null) => {
      await apiClient.patch(`/projects/${projectId}/`, { parentProjectId });
    },
    onSuccess: invalidate,
  });
}

export function useAddRelation(projectId: number) {
  const invalidate = useInvalidateAfterStructureChange(projectId);
  return useMutation({
    mutationFn: async (relatedProjectId: number) => {
      await apiClient.post(`/projects/${projectId}/relations/`, { relatedProjectId });
    },
    onSuccess: invalidate,
  });
}

export function useRemoveRelation(projectId: number) {
  const invalidate = useInvalidateAfterStructureChange(projectId);
  return useMutation({
    mutationFn: async (relatedId: number) => {
      await apiClient.delete(`/projects/${projectId}/relations/${relatedId}/`);
    },
    onSuccess: invalidate,
  });
}

export function useRoadmaps() {
  return useQuery({
    queryKey: roadmapsKey(),
    queryFn: async () => {
      const res = await apiClient.get<Roadmap[]>(`/roadmaps/`);
      return res.data;
    },
  });
}

export function useCreateRoadmap() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (data: { name: string; description?: string }) => {
      const res = await apiClient.post<Roadmap>(`/roadmaps/`, data);
      return res.data;
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: roadmapsKey() });
    },
  });
}

export function useAddToRoadmap(projectId: number) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (roadmapId: number) => {
      await apiClient.post(`/roadmaps/${roadmapId}/projects/`, { projectId });
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: projectStructureKey(projectId) });
      void queryClient.invalidateQueries({ queryKey: roadmapsKey() });
      void queryClient.invalidateQueries({ queryKey: projectActivityKeys.activity(projectId) });
    },
  });
}

export function useRemoveFromRoadmap(projectId: number) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (roadmapId: number) => {
      await apiClient.delete(`/roadmaps/${roadmapId}/projects/${projectId}/`);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: projectStructureKey(projectId) });
      void queryClient.invalidateQueries({ queryKey: roadmapsKey() });
      void queryClient.invalidateQueries({ queryKey: projectActivityKeys.activity(projectId) });
    },
  });
}

export function useRoadmapDetail(roadmapId: number | undefined) {
  return useQuery({
    queryKey: roadmapKey(roadmapId ?? 0),
    enabled: !!roadmapId,
    staleTime: 0,
    queryFn: async () => {
      const res = await apiClient.get<RoadmapDetail>(`/roadmaps/${roadmapId}/`);
      return res.data;
    },
  });
}

export function useUpdateRoadmap(roadmapId: number) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (data: { name: string; description: string }) => {
      await apiClient.put(`/roadmaps/${roadmapId}/`, data);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: roadmapKey(roadmapId) });
      void queryClient.invalidateQueries({ queryKey: roadmapsKey() });
      void queryClient.invalidateQueries({ queryKey: ['projects'] });
    },
  });
}

export function useDeleteRoadmap(roadmapId: number) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async () => {
      await apiClient.delete(`/roadmaps/${roadmapId}/`);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: roadmapsKey() });
      void queryClient.invalidateQueries({ queryKey: ['projects'] });
    },
  });
}
